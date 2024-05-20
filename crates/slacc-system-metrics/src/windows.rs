/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2024 Misskey and chocolate-pie */

use crate::SlaccStatsError;
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::NetworkManagement::IpHelper::{FreeMibTable, GetIfTable2, MIB_IF_TABLE2};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, GetDiskFreeSpaceExW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE,
    OPEN_EXISTING,
};
use windows::Win32::System::Ioctl::{DISK_PERFORMANCE, IOCTL_DISK_PERFORMANCE};
use windows::Win32::System::IO::DeviceIoControl;

#[derive(Debug, Clone)]
pub(crate) struct NetworkInformation {
    pub(crate) read_bytes: u64,
    pub(crate) write_bytes: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct DiskInformation {
    pub(crate) read_count: u64,
    pub(crate) write_count: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct DiskSpaceInformation {
    pub(crate) free_bytes: u64,
    pub(crate) total_bytes: u64,
}

pub(crate) unsafe fn get_network_info() -> Result<NetworkInformation, SlaccStatsError> {
    let mut table = std::ptr::null_mut::<MIB_IF_TABLE2>();
    GetIfTable2(&mut table).ok()?;
    let tables = std::slice::from_raw_parts((*table).Table.as_ptr(), (*table).NumEntries as usize);
    let read_bytes = tables
        .iter()
        .fold(0u64, |acc, table| acc.saturating_add(table.InOctets));
    let write_bytes = tables
        .iter()
        .fold(0u64, |acc, table| acc.saturating_add(table.OutOctets));
    FreeMibTable(table as *const ::libc::c_void);

    Ok(NetworkInformation {
        read_bytes,
        write_bytes,
    })
}

pub(crate) unsafe fn get_disk_space() -> Result<DiskSpaceInformation, SlaccStatsError> {
    let mut free_count: u64 = 0;
    let mut total_count: u64 = 0;
    GetDiskFreeSpaceExW(
        PCWSTR::null(),
        None,
        Some(&mut total_count),
        Some(&mut free_count),
    )?;
    Ok(DiskSpaceInformation {
        free_bytes: free_count,
        total_bytes: total_count,
    })
}

pub(crate) unsafe fn get_disk_io() -> Result<DiskInformation, SlaccStatsError> {
    let mut read_count: u64 = 0;
    let mut write_count: u64 = 0;
    let mut drive_count = 0;

    loop {
        let mut size: u32 = 0;
        let mut performance_data = std::mem::zeroed::<DISK_PERFORMANCE>();
        let device_name = format!(r"\\.\PhysicalDrive{}", drive_count);
        let device_name = HSTRING::from(device_name);
        let device_name_wide = PCWSTR(device_name.as_wide().as_ptr());
        let device = CreateFileW(
            device_name_wide,
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            None,
        );

        let device = match device {
            Ok(device) => device,
            Err(_) => break,
        };

        DeviceIoControl(
            device,
            IOCTL_DISK_PERFORMANCE,
            None,
            0,
            Some(&raw mut performance_data as *mut ::libc::c_void),
            std::mem::size_of::<DISK_PERFORMANCE>() as u32,
            Some(&mut size),
            None,
        )
        .inspect_err(|_| {
            let _ = CloseHandle(device);
        })?;

        read_count = read_count.saturating_add(performance_data.ReadCount as u64);
        write_count = write_count.saturating_add(performance_data.WriteCount as u64);
        drive_count += 1;
        CloseHandle(device)?;
    }

    Ok(DiskInformation {
        read_count,
        write_count,
    })
}
