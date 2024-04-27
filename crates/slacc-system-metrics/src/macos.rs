/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2024 Misskey and chocolate-pie */

use core_foundation_sys::base::{kCFAllocatorDefault, kCFAllocatorNull, CFAllocatorRef, CFRelease};
use core_foundation_sys::dictionary::{
    CFDictionaryGetValueIfPresent, CFDictionaryRef, CFMutableDictionaryRef,
};
use core_foundation_sys::number::{kCFNumberSInt64Type, CFNumberGetValue, CFNumberRef};
use core_foundation_sys::string::{
    kCFStringEncodingUTF8, CFStringCreateWithCStringNoCopy, CFStringRef,
};
use std::ffi::CStr;
use std::marker::PhantomData;
use std::mem::MaybeUninit;

use crate::{ErrnoExt, SlaccStatsError};

// Based on
// https://github.com/apple-oss-distributions/IOStorageFamily/blob/0993ad6b36e85774fb0bc280e7d795f6dcbc641c/IOMedia.h#L41
// https://github.com/apple-oss-distributions/xnu/blob/1031c584a5e37aff177559b9f69dbd3c8c3fd30a/iokit/IOKit/IOKitKeys.h#L49
// https://github.com/apple-oss-distributions/IOStorageFamily/blob/0993ad6b36e85774fb0bc280e7d795f6dcbc641c/IOBlockStorageDriver.h#L35-L222
#[rustfmt::skip]
#[allow(non_upper_case_globals, dead_code)]
mod constants {
    pub(crate) const kIOMediaClass: &std::ffi::CStr = c"IOMedia";
    pub(crate) const kIOServicePlane: &std::ffi::CStr = c"IOService";
    pub(crate) const kIOBlockStorageDriverClass: &std::ffi::CStr = c"IOBlockStorageDriver";
    pub(crate) const kIOBlockStorageDriverStatisticsKey: &std::ffi::CStr = c"Statistics";
    pub(crate) const kIOBlockStorageDriverStatisticsBytesReadKey: &std::ffi::CStr = c"Bytes (Read)";
    pub(crate) const kIOBlockStorageDriverStatisticsBytesWrittenKey: &std::ffi::CStr = c"Bytes (Write)";
    pub(crate) const kIOBlockStorageDriverStatisticsReadErrorsKey: &std::ffi::CStr = c"Errors (Read)";
    pub(crate) const kIOBlockStorageDriverStatisticsWriteErrorsKey: &std::ffi::CStr = c"Errors (Write)";
    pub(crate) const kIOBlockStorageDriverStatisticsLatentReadTimeKey: &std::ffi::CStr = c"Latency Time (Read)";
    pub(crate) const kIOBlockStorageDriverStatisticsLatentWriteTimeKey: &std::ffi::CStr = c"Latency Time (Write)";
    pub(crate) const kIOBlockStorageDriverStatisticsReadsKey: &std::ffi::CStr = c"Operations (Read)";
    pub(crate) const kIOBlockStorageDriverStatisticsWritesKey: &std::ffi::CStr = c"Operations (Write)";
    pub(crate) const kIOBlockStorageDriverStatisticsReadRetriesKey: &std::ffi::CStr = c"Retries (Read)";
    pub(crate) const kIOBlockStorageDriverStatisticsWriteRetriesKey: &std::ffi::CStr = c"Retries (Write)";
    pub(crate) const kIOBlockStorageDriverStatisticsTotalReadTimeKey: &std::ffi::CStr = c"Total Time (Read)";
    pub(crate) const kIOBlockStorageDriverStatisticsTotalWriteTimeKey: &std::ffi::CStr = c"Total Time (Write)";
}

#[link(name = "IOKit", kind = "framework")]
extern "C" {
    pub(crate) static kIOMasterPortDefault: ::libc::c_uint;
    pub(crate) fn IOServiceMatching(name: *const ::libc::c_char) -> CFMutableDictionaryRef;
    pub(crate) fn IOServiceGetMatchingServices(
        master: ::libc::c_uint,
        dictionary: CFDictionaryRef,
        existing: *mut ::libc::c_uint,
    ) -> ::libc::c_int;
    pub(crate) fn IORegistryEntryGetParentEntry(
        entry: ::libc::c_uint,
        plane: *const ::libc::c_char,
        parent: *mut ::libc::c_uint,
    ) -> ::libc::c_int;
    pub(crate) fn IORegistryEntryCreateCFProperties(
        entry: ::libc::c_uint,
        properties: *mut CFMutableDictionaryRef,
        allocator: CFAllocatorRef,
        options: ::libc::c_uint,
    ) -> ::libc::c_int;
    pub(crate) fn IOObjectConformsTo(
        object: ::libc::c_uint,
        name: *const ::libc::c_char,
    ) -> ::libc::c_uint;
    pub(crate) fn IOObjectRelease(object: ::libc::c_uint) -> ::libc::c_int;
    pub(crate) fn IOIteratorNext(iterator: ::libc::c_uint) -> ::libc::c_uint;
}

#[derive(Debug, Clone)]
pub(crate) struct DiskInformation {
    pub(crate) read_count: i64,
    pub(crate) write_count: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct NetworkInformation {
    pub(crate) read_bytes: u64,
    pub(crate) write_bytes: u64,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct MemoryInformation {
    pub(crate) used_count: u64,
    pub(crate) total_count: u64,
    pub(crate) active_count: u64,
}

#[derive(Debug)]
pub(crate) struct CFString<'a> {
    inner: CFStringRef,
    phantom: PhantomData<&'a CStr>,
}

impl<'a> TryFrom<&'a CStr> for CFString<'a> {
    type Error = SlaccStatsError;

    fn try_from(value: &'a CStr) -> Result<Self, Self::Error> {
        unsafe {
            match CFStringCreateWithCStringNoCopy(
                kCFAllocatorDefault,
                value.as_ptr(),
                kCFStringEncodingUTF8,
                kCFAllocatorNull,
            ) {
                output if !output.is_null() => Ok(Self {
                    inner: output,
                    phantom: PhantomData,
                }),
                _ => Err(SlaccStatsError::NullPointerReturnedError),
            }
        }
    }
}

impl<'a> Drop for CFString<'a> {
    fn drop(&mut self) {
        unsafe { CFRelease(self.inner as *const ::libc::c_void) };
    }
}

macro_rules! release_io {
    ($($item: expr),+) => {{
        $(IOObjectRelease($item);)+
    }};
    ($($item: expr),+ ; $($cfitem: expr),+) => {{
        $(IOObjectRelease($item);)+
        $(CFRelease($cfitem);)+
    }}
}

pub(crate) unsafe fn get_disk_io() -> Result<DiskInformation, SlaccStatsError> {
    unsafe fn take_value_as_dictionary_from_dictionary(
        key: &CStr,
        dictionary: CFDictionaryRef,
    ) -> Result<CFDictionaryRef, SlaccStatsError> {
        let dictionary_key = CFString::try_from(key)?;
        let mut dictionary_value = MaybeUninit::<CFDictionaryRef>::uninit();

        if CFDictionaryGetValueIfPresent(
            dictionary,
            dictionary_key.inner as *const ::libc::c_void,
            dictionary_value.as_mut_ptr() as *mut *const ::libc::c_void,
        ) == 0
        {
            Err(SlaccStatsError::SpecifiedKeyNotFoundError(
                key.to_string_lossy().into_owned(),
            ))
        } else {
            Ok(dictionary_value.assume_init())
        }
    }

    unsafe fn take_value_as_number_from_dictionary(
        key: &CStr,
        dictionary: CFDictionaryRef,
    ) -> Result<i64, SlaccStatsError> {
        let dictionary_key = CFString::try_from(key)?;
        let mut dictionary_value = MaybeUninit::<CFNumberRef>::uninit();

        if CFDictionaryGetValueIfPresent(
            dictionary,
            dictionary_key.inner as *const ::libc::c_void,
            dictionary_value.as_mut_ptr() as *mut *const ::libc::c_void,
        ) == 0
        {
            Err(SlaccStatsError::SpecifiedKeyNotFoundError(
                key.to_string_lossy().into_owned(),
            ))
        } else {
            let mut number: i64 = 0;
            let cf_number = dictionary_value.assume_init();
            CFNumberGetValue(
                cf_number,
                kCFNumberSInt64Type,
                &raw mut number as *mut ::libc::c_void,
            );
            Ok(number)
        }
    }

    let storage_driver_klass = constants::kIOBlockStorageDriverClass.as_ptr();
    let statistics_key = constants::kIOBlockStorageDriverStatisticsKey;
    let reads_key = constants::kIOBlockStorageDriverStatisticsReadsKey;
    let write_key = constants::kIOBlockStorageDriverStatisticsWritesKey;
    let mut iterator = MaybeUninit::<::libc::c_uint>::uninit();
    let keyword_dict = IOServiceMatching(constants::kIOMediaClass.as_ptr());
    IOServiceGetMatchingServices(kIOMasterPortDefault, keyword_dict, iterator.as_mut_ptr())
        .into_errno()?;
    let iterator = iterator.assume_init();
    let mut item = IOIteratorNext(iterator);
    let mut read_total: i64 = 0;
    let mut write_total: i64 = 0;

    while item != 0 {
        let mut parent: ::libc::c_uint = 0;
        let mut props_dictionary = MaybeUninit::<CFMutableDictionaryRef>::uninit();
        IORegistryEntryGetParentEntry(item, constants::kIOServicePlane.as_ptr(), &mut parent)
            .into_error_release(|| release_io!(item, iterator))?;

        if IOObjectConformsTo(parent, storage_driver_klass) == 0 {
            IOObjectRelease(parent);
            IOObjectRelease(item);
            item = IOIteratorNext(iterator);
            continue;
        }

        IORegistryEntryCreateCFProperties(
            parent,
            props_dictionary.as_mut_ptr(),
            kCFAllocatorDefault,
            0,
        )
        .into_error_release(|| release_io!(parent, item, iterator))?;

        let props_dictionary = props_dictionary.assume_init();
        let statistics_dictionary =
            take_value_as_dictionary_from_dictionary(statistics_key, props_dictionary)
                .inspect_err(|_| release_io!(parent,item,iterator;props_dictionary as *const _))?;
        let read_count = take_value_as_number_from_dictionary(reads_key, statistics_dictionary)
            .inspect_err(|_| release_io!(parent,item,iterator;props_dictionary as *const _))?;
        let write_count = take_value_as_number_from_dictionary(write_key, statistics_dictionary)
            .inspect_err(|_| release_io!(parent,item,iterator;props_dictionary as *const _))?;

        read_total = read_total.saturating_add(read_count);
        write_total = write_total.saturating_add(write_count);

        IOObjectRelease(parent);
        IOObjectRelease(item);
        CFRelease(props_dictionary as *const _);
        item = IOIteratorNext(iterator);
    }

    IOObjectRelease(iterator);
    Ok(DiskInformation {
        read_count: read_total,
        write_count: write_total,
    })
}

pub(crate) unsafe fn get_memory_info() -> Result<MemoryInformation, SlaccStatsError> {
    let mut page_size: u64 = 0;
    let mut memory_size: u64 = 0;
    let mut command = [libc::CTL_HW, libc::HW_PAGESIZE];
    let host_port = libc::mach_host_self();
    let mut statistic = std::mem::zeroed::<::libc::vm_statistics64>();
    let mut statistic_count = libc::HOST_VM_INFO64_COUNT;

    libc::host_statistics64(
        host_port,
        libc::HOST_VM_INFO64,
        &raw mut statistic as *mut i32,
        &mut statistic_count,
    )
    .into_errno()?;

    libc::sysctl(
        command.as_mut_ptr(),
        command.len() as ::libc::c_uint,
        &raw mut page_size as *mut ::libc::c_void,
        &mut std::mem::size_of::<u64>(),
        std::ptr::null_mut(),
        0,
    )
    .into_errno()?;

    command = [libc::CTL_HW, libc::HW_MEMSIZE];

    libc::sysctl(
        command.as_mut_ptr(),
        command.len() as ::libc::c_uint,
        &raw mut memory_size as *mut ::libc::c_void,
        &mut std::mem::size_of::<u64>(),
        std::ptr::null_mut(),
        0,
    )
    .into_errno()?;

    let used_count = statistic
        .active_count
        .saturating_add(statistic.wire_count)
        .saturating_add(statistic.speculative_count)
        .saturating_add(statistic.compressor_page_count);

    Ok(MemoryInformation {
        total_count: memory_size,
        used_count: (used_count as u64).saturating_mul(page_size),
        active_count: (statistic.active_count as u64).saturating_mul(page_size),
    })
}

pub(crate) unsafe fn get_network_info() -> Result<NetworkInformation, SlaccStatsError> {
    let mut length: ::libc::size_t = 0;
    let mut read_bytes: u64 = 0;
    let mut write_bytes: u64 = 0;
    let mut command = [libc::CTL_NET, libc::PF_ROUTE, 0, 0, libc::NET_RT_IFLIST2, 0];

    libc::sysctl(
        command.as_mut_ptr(),
        command.len() as ::libc::c_uint,
        std::ptr::null_mut(),
        &mut length,
        std::ptr::null_mut(),
        0,
    )
    .into_errno()?;

    let mut networks = Vec::with_capacity(length);

    libc::sysctl(
        command.as_mut_ptr(),
        command.len() as ::libc::c_uint,
        networks.as_mut_ptr(),
        &mut length,
        std::ptr::null_mut(),
        0,
    )
    .into_errno()?;

    #[allow(clippy::uninit_vec)]
    networks.set_len(length);
    let mut networks_addr = networks.as_ptr();
    let limit = networks_addr.add(length);

    while networks_addr < limit {
        let network = &*(networks_addr as *const ::libc::if_msghdr);
        if network.ifm_type == libc::RTM_IFINFO2 as u8 {
            let network = &*(networks_addr as *const ::libc::if_msghdr2);
            read_bytes = read_bytes.saturating_add(network.ifm_data.ifi_ibytes);
            write_bytes = write_bytes.saturating_add(network.ifm_data.ifi_obytes);
        }
        networks_addr = networks_addr.offset(network.ifm_msglen as isize);
    }

    Ok(NetworkInformation {
        read_bytes,
        write_bytes,
    })
}
