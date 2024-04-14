#![feature(raw_ref_op)]
#![allow(clippy::unit_arg)]
#![allow(unsafe_op_in_unsafe_fn)]

#[cfg(feature = "napi")]
#[macro_use]
extern crate napi_derive;
#[cfg(target_os = "freebsd")]
mod freebsd;
mod imp;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;
pub use imp::*;
#[cfg(feature = "napi")]
pub use napi::bindgen_prelude::AsyncTask;

/// Retrieving disk information.
#[cfg_attr(feature = "napi", napi)]
#[cfg(feature = "nonblocking")]
pub fn get_disk_io_nonblocking() -> AsyncTask<imp::AsyncTask<DiskInformation, SlaccStatsError>> {
    AsyncTask::new(imp::AsyncTask::new(imp::get_disk_io))
}

/// Retrieving disk information.
#[cfg_attr(feature = "napi", napi)]
pub fn get_disk_io() -> Result<DiskInformation, SlaccStatsError> {
    crate::imp::get_disk_io()
}

/// Retrieving disk space information.
#[cfg_attr(feature = "napi", napi)]
#[cfg(feature = "nonblocking")]
pub fn get_disk_space_nonblocking(
) -> AsyncTask<imp::AsyncTask<DiskSpaceInformation, SlaccStatsError>> {
    AsyncTask::new(imp::AsyncTask::new(imp::get_disk_space))
}

/// Retrieving disk space information.
#[cfg_attr(feature = "napi", napi)]
pub fn get_disk_space() -> Result<DiskSpaceInformation, SlaccStatsError> {
    crate::imp::get_disk_space()
}

/// Retrieving memory information.
#[cfg_attr(feature = "napi", napi)]
#[cfg(feature = "nonblocking")]
pub fn get_memory_info_nonblocking() -> AsyncTask<imp::AsyncTask<MemoryInformation, SlaccStatsError>>
{
    AsyncTask::new(imp::AsyncTask::new(imp::get_memory_info))
}

/// Retrieving memory information.
#[cfg_attr(feature = "napi", napi)]
pub fn get_memory_info() -> Result<MemoryInformation, SlaccStatsError> {
    crate::imp::get_memory_info()
}

/// Retrieving network information.
#[cfg_attr(feature = "napi", napi)]
#[cfg(feature = "nonblocking")]
pub fn get_network_info_nonblocking(
) -> AsyncTask<imp::AsyncTask<NetworkInformation, SlaccStatsError>> {
    AsyncTask::new(imp::AsyncTask::new(imp::get_network_info))
}

/// Retrieving network information.
#[cfg_attr(feature = "napi", napi)]
pub fn get_network_info() -> Result<NetworkInformation, SlaccStatsError> {
    crate::imp::get_network_info()
}
