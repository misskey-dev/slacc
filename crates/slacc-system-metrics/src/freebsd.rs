/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2024 Misskey and chocolate-pie */

use crate::{ErrnoExt, SlaccStatsError};
use libc::devstat_trans_flags;

// https://github.com/ziglang/zig/blob/13a9d94a8038727469cf11b72273ce4ea6d89faa/lib/std/Target.zig#L2489-L2502
// https://github.com/ziglang/zig/blob/13a9d94a8038727469cf11b72273ce4ea6d89faa/lib/std/Target.zig#L2226-L2276
// https://github.com/llvm/llvm-project/blob/7ac7d418ac2b16fd44789dcf48e2b5d73de3e715/clang/lib/Basic/Targets/X86.h#L444-L445
// https://github.com/llvm/llvm-project/blob/7ac7d418ac2b16fd44789dcf48e2b5d73de3e715/clang/lib/Basic/Targets/X86.h#L724-L725
// https://github.com/llvm/llvm-project/blob/7ac7d418ac2b16fd44789dcf48e2b5d73de3e715/clang/lib/Basic/Targets/AArch64.cpp#L161
#[repr(C, align(16))]
#[allow(non_camel_case_types)]
pub(crate) struct f128([u8; 16]);

#[repr(C)]
#[allow(non_camel_case_types)]
pub(crate) struct statinfo {
    cp_time: [::libc::c_long; 5],
    tk_nin: ::libc::c_long,
    tk_nout: ::libc::c_long,
    dinfo: *mut ::libc::devinfo,
    snap_time: f128,
}

#[derive(Debug, Clone)]
pub(crate) struct DiskInformation {
    pub(crate) read_count: u64,
    pub(crate) write_count: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct NetworkInformation {
    pub(crate) read_bytes: u64,
    pub(crate) write_bytes: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct MemoryInformation {
    pub(crate) used_count: u64,
    pub(crate) total_count: u64,
    pub(crate) active_count: u64,
}

macro_rules! sysctlbyname {
    ($system: literal, $output: ident, $ty: ty) => {{
        let command = ::std::ffi::CString::new($system).unwrap();
        libc::sysctlbyname(
            command.as_ptr(),
            &raw mut $output as *mut ::libc::c_void,
            &mut std::mem::size_of::<$ty>(),
            std::ptr::null_mut(),
            0,
        )
    }};
}

#[link(name = "devstat")]
extern "C" {
    pub(crate) static devstat_errbuf: [::libc::c_char; 2048];
    pub(crate) fn devstat_getdevs(kd: *mut ::libc::kvm_t, stats: *mut statinfo) -> ::libc::c_int;
}

pub(crate) unsafe fn get_disk_io() -> Result<DiskInformation, SlaccStatsError> {
    let mut read_total: u64 = 0;
    let mut write_total: u64 = 0;
    let mut devinfo = std::mem::zeroed::<::libc::devinfo>();
    let mut statistic = std::mem::zeroed::<statinfo>();
    statistic.dinfo = &mut devinfo;
    devstat_getdevs(std::ptr::null_mut(), &mut statistic)
        .into_error_with(|input| input >= 0)
        .map_err(|_| SlaccStatsError::from_ptr(devstat_errbuf.as_ptr()))?;
    let devices = std::slice::from_raw_parts(devinfo.devices, devinfo.numdevs as _);
    for device in devices {
        let read_count = device.operations[devstat_trans_flags::DEVSTAT_READ as usize];
        let write_count = device.operations[devstat_trans_flags::DEVSTAT_WRITE as usize];
        read_total = read_total.saturating_add(read_count);
        write_total = write_total.saturating_add(write_count);
    }
    libc::free(devinfo.mem_ptr as *mut _);
    Ok(DiskInformation {
        read_count: read_total,
        write_count: write_total,
    })
}

pub(crate) unsafe fn get_network_info() -> Result<NetworkInformation, SlaccStatsError> {
    let mut interface_count: ::libc::c_int = 0;
    let mut read_total: u64 = 0;
    let mut write_total: u64 = 0;
    let command = [
        libc::CTL_NET,
        libc::PF_LINK,
        libc::NETLINK_GENERIC,
        libc::IFMIB_SYSTEM,
        libc::IFMIB_IFCOUNT,
    ];

    libc::sysctl(
        command.as_ptr(),
        command.len() as ::libc::c_uint,
        &raw mut interface_count as *mut ::libc::c_void,
        &mut std::mem::size_of::<::libc::c_int>(),
        std::ptr::null_mut(),
        0,
    )
    .into_errno()?;

    for index in 1..=interface_count {
        let mut data = std::mem::zeroed::<::libc::ifmibdata>();
        let command = [
            libc::CTL_NET,
            libc::PF_LINK,
            libc::NETLINK_GENERIC,
            libc::IFMIB_IFDATA,
            index,
            libc::IFDATA_GENERAL,
        ];
        libc::sysctl(
            command.as_ptr(),
            command.len() as ::libc::c_uint,
            &raw mut data as *mut ::libc::c_void,
            &mut std::mem::size_of::<::libc::ifmibdata>(),
            std::ptr::null_mut(),
            0,
        )
        .into_errno()?;
        let read_count = data.ifmd_data.ifi_ibytes;
        let write_count = data.ifmd_data.ifi_obytes;
        read_total = read_total.saturating_add(read_count);
        write_total = write_total.saturating_add(write_count);
    }

    Ok(NetworkInformation {
        read_bytes: read_total,
        write_bytes: write_total,
    })
}

pub(crate) unsafe fn get_memory_info() -> Result<MemoryInformation, SlaccStatsError> {
    let mut page_size: u64 = 0;
    let mut free_memory: u64 = 0;
    let mut total_memory: u64 = 0;
    let mut active_memory: u64 = 0;
    sysctlbyname!("hw.realmem", total_memory, u64).into_errno()?;
    sysctlbyname!("vm.stats.vm.v_page_size", page_size, u64).into_errno()?;
    sysctlbyname!("vm.stats.vm.v_free_count", free_memory, u64).into_errno()?;
    sysctlbyname!("vm.stats.vm.v_active_count", active_memory, u64).into_errno()?;
    let free_memory = free_memory.saturating_mul(page_size);
    let active_memory = active_memory.saturating_mul(page_size);
    let used_memory = total_memory.saturating_sub(free_memory);
    Ok(MemoryInformation {
        used_count: used_memory,
        active_count: active_memory,
        total_count: total_memory,
    })
}
