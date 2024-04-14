/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2024 Misskey and chocolate-pie */

use miette::Diagnostic;
#[cfg(feature = "nonblocking")]
use napi::bindgen_prelude::{ToNapiValue, TypeName};
#[cfg(feature = "nonblocking")]
use napi::{Error, Task};
#[cfg(any(target_os = "macos", target_os = "freebsd", target_os = "linux"))]
use num::cast::AsPrimitive;
#[cfg(target_os = "freebsd")]
use std::ffi::CStr;
#[cfg(feature = "nonblocking")]
use std::panic::AssertUnwindSafe;
use thiserror::Error;

#[derive(Debug, Clone, Error, Diagnostic)]
pub enum SlaccStatsError {
    #[error("{0} (kind: {1:?})")]
    #[diagnostic(code(misskey_stats::raw_error))]
    RawError(String, Option<i32>),
    /// Error was throwned from `windows` crate.
    #[cfg(windows)]
    #[error(transparent)]
    #[diagnostic(code(misskey_stats::windows_error))]
    WindowsError(#[from] ::windows::core::Error),
    /// Null pointer was returned by external function.
    #[error("Null pointer was returned by external function")]
    NullPointerReturnedError,
    #[error("Operation is not supported on this platform")]
    #[diagnostic(code(misskey_stats::not_supported))]
    NotSupportedError,
    #[diagnostic(code(misskey_stats::specified_key_notfound))]
    #[error("Specified key cannot found in this dictionary (provided key: {0})")]
    SpecifiedKeyNotFoundError(String),
    #[diagnostic(code(misskey_stats::netlink_failed))]
    #[error("Something went wrong when retrieving network information")]
    NetlinkFailed,
    #[diagnostic(code(misskey_stats::try_from_int_error))]
    #[error(transparent)]
    TryFromIntError(#[from] std::num::TryFromIntError),
}

impl SlaccStatsError {
    #[cfg(target_os = "freebsd")]
    pub(crate) unsafe fn from_ptr(pointer: *const ::libc::c_char) -> Self {
        let message = CStr::from_ptr(pointer).to_string_lossy().into_owned();
        SlaccStatsError::RawError(message, None)
    }
}

#[cfg(feature = "napi")]
impl From<SlaccStatsError> for napi::JsError {
    fn from(value: SlaccStatsError) -> Self {
        napi::Error::new(napi::Status::GenericFailure, value.to_string()).into()
    }
}

#[cfg(feature = "nonblocking")]
pub trait AsyncTaskValue: Send + ToNapiValue + TypeName + 'static {}
#[cfg(feature = "nonblocking")]
impl<T: Send + ToNapiValue + TypeName + 'static> AsyncTaskValue for T {}
#[cfg(feature = "nonblocking")]
pub struct AsyncTask<T: AsyncTaskValue, E: std::error::Error>(Box<dyn Send + Fn() -> Result<T, E>>);

#[cfg(feature = "nonblocking")]
impl<T: AsyncTaskValue, E: std::error::Error> AsyncTask<T, E> {
    pub(crate) fn new(inner: impl Send + Fn() -> Result<T, E> + 'static) -> Self {
        Self(Box::new(inner))
    }
}

#[cfg(feature = "nonblocking")]
impl<T: AsyncTaskValue, E: std::error::Error> Task for AsyncTask<T, E> {
    type JsValue = T;
    type Output = Result<T, napi::Error>;

    fn compute(&mut self) -> napi::Result<Self::Output> {
        match std::panic::catch_unwind(AssertUnwindSafe(move || match (self.0)() {
            Ok(output) => Ok(output),
            Err(error) => Err(napi::Error::from_reason(error.to_string())),
        })) {
            Ok(output) => Ok(output),
            Err(_) => Err(Error::from_reason("Uncaught panic was occurred")),
        }
    }

    #[inline(always)]
    fn resolve(&mut self, _env: napi::Env, output: Self::Output) -> napi::Result<Self::JsValue> {
        output
    }
}

#[cfg(feature = "napi")]
type Number = i64;
#[cfg(not(feature = "napi"))]
type Number = u64;

#[derive(Debug, Clone)]
#[cfg_attr(feature = "napi", napi(object))]
pub struct MemoryInformation {
    pub used_count: Number,
    pub total_count: Number,
    pub active_count: Number,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "napi", napi(object))]
pub struct DiskInformation {
    pub read_count: Number,
    pub write_count: Number,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "napi", napi(object))]
pub struct NetworkInformation {
    pub read_bytes: Number,
    pub write_bytes: Number,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "napi", napi(object))]
pub struct DiskSpaceInformation {
    pub free_bytes: Number,
    pub total_bytes: Number,
}

#[allow(dead_code)]
#[cfg(any(target_os = "macos", target_os = "freebsd", target_os = "linux"))]
pub(crate) trait ErrnoExt: AsPrimitive<i32> {
    unsafe fn into_errno(self) -> Result<(), SlaccStatsError>;
    #[cfg(target_os = "linux")]
    unsafe fn into_errno2(self) -> Result<i32, SlaccStatsError>;
    unsafe fn into_error_with(self, func: impl FnMut(i32) -> bool) -> Result<(), SlaccStatsError>;
    unsafe fn into_error_release(self, mut func: impl FnMut()) -> Result<(), SlaccStatsError> {
        self.into_errno().inspect_err(|_| {
            func();
        })
    }
}

#[cfg(target_os = "linux")]
pub(crate) trait CheckValidFd: AsPrimitive<i32> {
    fn valid_fd(self) -> Result<Self, SlaccStatsError>;
}

#[cfg(any(target_os = "macos", target_os = "freebsd", target_os = "linux"))]
impl<T: AsPrimitive<i32>> ErrnoExt for T {
    unsafe fn into_errno(self) -> Result<(), SlaccStatsError> {
        if self.as_() != 0 {
            let raw = errno::errno();
            Err(SlaccStatsError::RawError(raw.to_string(), Some(raw.0)))
        } else {
            Ok(())
        }
    }

    #[cfg(target_os = "linux")]
    unsafe fn into_errno2(self) -> Result<i32, SlaccStatsError> {
        if self.as_() < 0 {
            let raw = errno::errno();
            Err(SlaccStatsError::RawError(raw.to_string(), Some(raw.0)))
        } else {
            Ok(self.as_())
        }
    }

    unsafe fn into_error_with(
        self,
        mut func: impl FnMut(i32) -> bool,
    ) -> Result<(), SlaccStatsError> {
        if !func(self.as_()) {
            let raw = errno::errno();
            Err(SlaccStatsError::RawError(raw.to_string(), Some(raw.0)))
        } else {
            Ok(())
        }
    }
}

#[cfg(target_os = "linux")]
impl<T: AsPrimitive<i32>> CheckValidFd for T {
    fn valid_fd(self) -> Result<Self, SlaccStatsError> {
        match self.as_() >= 0 {
            true => Ok(self),
            false => Err(SlaccStatsError::NetlinkFailed),
        }
    }
}

/// Retrieving disk information.
pub(crate) fn get_disk_io() -> Result<DiskInformation, SlaccStatsError> {
    #[cfg(target_os = "macos")]
    unsafe {
        let statistic = crate::macos::get_disk_io()?;
        Ok(DiskInformation {
            read_count: statistic.read_count as _,
            write_count: statistic.write_count as _,
        })
    }
    #[cfg(target_os = "windows")]
    unsafe {
        let statistic = crate::windows::get_disk_io()?;
        Ok(DiskInformation {
            read_count: statistic.read_count as _,
            write_count: statistic.write_count as _,
        })
    }
    #[cfg(target_os = "freebsd")]
    unsafe {
        let statistic = crate::freebsd::get_disk_io()?;
        Ok(DiskInformation {
            read_count: statistic.read_count as _,
            write_count: statistic.write_count as _,
        })
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "freebsd")))]
    Err(SlaccStatsError::NotSupportedError)
}

/// Retrieving disk space information.
pub(crate) fn get_disk_space() -> Result<DiskSpaceInformation, SlaccStatsError> {
    #[cfg(target_os = "windows")]
    unsafe {
        let statistic = crate::windows::get_disk_space()?;
        Ok(DiskSpaceInformation {
            free_bytes: statistic.free_bytes as _,
            total_bytes: statistic.total_bytes as _,
        })
    }
    #[cfg(not(target_os = "windows"))]
    Err(SlaccStatsError::NotSupportedError)
}

/// Retrieving memory information.
pub(crate) fn get_memory_info() -> Result<MemoryInformation, SlaccStatsError> {
    #[cfg(target_os = "macos")]
    unsafe {
        let statistic = crate::macos::get_memory_info()?;
        Ok(MemoryInformation {
            used_count: statistic.used_count as _,
            total_count: statistic.total_count as _,
            active_count: statistic.active_count as _,
        })
    }
    #[cfg(target_os = "freebsd")]
    unsafe {
        let statistic = crate::freebsd::get_memory_info()?;
        Ok(MemoryInformation {
            used_count: statistic.used_count as _,
            total_count: statistic.total_count as _,
            active_count: statistic.active_count as _,
        })
    }
    #[cfg(not(any(target_os = "macos", target_os = "freebsd")))]
    Err(SlaccStatsError::NotSupportedError)
}

/// Retrieving network information.
pub(crate) fn get_network_info() -> Result<NetworkInformation, SlaccStatsError> {
    #[cfg(target_os = "windows")]
    unsafe {
        let statistic = crate::windows::get_network_info()?;
        Ok(NetworkInformation {
            read_bytes: statistic.read_bytes as _,
            write_bytes: statistic.write_bytes as _,
        })
    }
    #[cfg(target_os = "macos")]
    unsafe {
        let statistic = crate::macos::get_network_info()?;
        Ok(NetworkInformation {
            read_bytes: statistic.read_bytes as _,
            write_bytes: statistic.write_bytes as _,
        })
    }
    #[cfg(target_os = "freebsd")]
    unsafe {
        let statistic = crate::freebsd::get_network_info()?;
        Ok(NetworkInformation {
            read_bytes: statistic.read_bytes as _,
            write_bytes: statistic.write_bytes as _,
        })
    }
    #[cfg(target_os = "linux")]
    unsafe {
        let statistic = crate::linux::get_network_info()?;
        Ok(NetworkInformation {
            read_bytes: statistic.read_bytes as _,
            write_bytes: statistic.write_bytes as _,
        })
    }
    #[cfg(not(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "freebsd",
        target_os = "linux"
    )))]
    Err(SlaccStatsError::NotSupportedError)
}
