/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2024 Misskey and chocolate-pie */

use crate::{CheckValidFd, ErrnoExt, SlaccStatsError};
use libc::{MSG_DONTWAIT, NETLINK_ROUTE, PF_NETLINK, SOCK_CLOEXEC, SOCK_RAW};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::raw::c_ulonglong;

#[repr(C)]
#[derive(Copy, Clone)]
#[allow(non_camel_case_types)]
struct rtnl_link_stats64 {
    rx_packets: c_ulonglong,
    tx_packets: c_ulonglong,
    rx_bytes: c_ulonglong,
    tx_bytes: c_ulonglong,
    rx_errors: c_ulonglong,
    tx_errors: c_ulonglong,
    rx_dropped: c_ulonglong,
    tx_dropped: c_ulonglong,
    multicast: c_ulonglong,
    collisions: c_ulonglong,
    rx_length_errors: c_ulonglong,
    rx_over_errors: c_ulonglong,
    rx_crc_errors: c_ulonglong,
    rx_frame_errors: c_ulonglong,
    rx_fifo_errors: c_ulonglong,
    rx_missed_errors: c_ulonglong,
    tx_aborted_errors: c_ulonglong,
    tx_carrier_errors: c_ulonglong,
    tx_fifo_errors: c_ulonglong,
    tx_heartbeat_errors: c_ulonglong,
    tx_window_errors: c_ulonglong,
    rx_compressed: c_ulonglong,
    tx_compressed: c_ulonglong,
    rx_nohandler: c_ulonglong,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct NetworkInformation {
    pub(crate) read_bytes: u64,
    pub(crate) write_bytes: u64,
}

#[repr(C)]
#[allow(non_camel_case_types)]
struct rtgenmsg {
    rtgen_family: libc::c_uchar,
}

#[repr(C)]
#[allow(non_camel_case_types)]
struct rtattr {
    rta_len: libc::c_ushort,
    rta_type: libc::c_ushort,
}

#[repr(C)]
#[allow(non_camel_case_types)]
struct ifinfomsg {
    ifi_family: libc::c_uchar,
    __ifi_pad: libc::c_uchar,
    ifi_type: libc::c_ushort,
    ifi_index: libc::c_int,
    ifi_flags: libc::c_uint,
    ifi_change: libc::c_uint,
}

#[repr(C)]
struct NetlinkRequest {
    message: libc::nlmsghdr,
    generation: rtgenmsg,
}

pub(crate) unsafe fn get_network_info() -> Result<NetworkInformation, SlaccStatsError> {
    let mut buffer = Vec::<u8>::with_capacity(8192 /* NLMSG_GOODSIZE */);
    let socket = libc::socket(PF_NETLINK, SOCK_RAW | SOCK_CLOEXEC, NETLINK_ROUTE).valid_fd()?;
    let socket = OwnedFd::from_raw_fd(socket);
    let mut request = std::mem::zeroed::<NetlinkRequest>();
    request.message.nlmsg_len = std::mem::size_of::<NetlinkRequest>() as u32;
    request.message.nlmsg_type = libc::RTM_GETLINK;
    request.message.nlmsg_flags = (libc::NLM_F_DUMP | libc::NLM_F_REQUEST) as u16;
    request.generation.rtgen_family = libc::AF_UNSPEC as u8;
    let socket_fd = socket.as_raw_fd();
    let request_ptr = &raw const request as *const ::libc::c_void;
    let request_size = std::mem::size_of::<NetlinkRequest>();
    libc::send(socket_fd, request_ptr, request_size, 0).into_errno2()?;
    let mut network = NetworkInformation::default();

    loop {
        let mut offset: i32 = 0;
        let mut message_ptr = buffer.as_ptr();
        let received = libc::recv(
            socket_fd,
            buffer.as_mut_ptr() as *mut ::libc::c_void,
            8192, /* NLMSG_GOODSIZE */
            MSG_DONTWAIT,
        )
        .into_errno2()?;
        buffer.set_len(received.try_into()?);
        while (received - offset) >= std::mem::size_of::<libc::nlmsghdr>() as i32 {
            let message = &*(message_ptr as *const ::libc::nlmsghdr);
            match message {
                message if (message.nlmsg_flags & libc::NLM_F_DUMP_INTR as u16) != 0 => {
                    return Err(SlaccStatsError::NetlinkFailed)
                }
                message if message.nlmsg_type == libc::NLMSG_DONE as u16 => return Ok(network),
                message if message.nlmsg_type == libc::NLMSG_ERROR as u16 => {
                    return Err(SlaccStatsError::NetlinkFailed)
                }
                message if offset + message.nlmsg_len as i32 > received => {
                    return Err(SlaccStatsError::NetlinkFailed)
                }
                message if message.nlmsg_type == libc::RTM_NEWLINK => {
                    let message_offset = std::mem::size_of::<libc::nlmsghdr>();
                    let ifinfo_size = std::mem::size_of::<ifinfomsg>();
                    let rtattr_size = std::mem::size_of::<rtattr>();
                    let mut message_offset = message_offset + ((ifinfo_size + 3) & !3);
                    while message_offset + rtattr_size <= message.nlmsg_len as usize {
                        let rtattr_message = message_ptr.add(message_offset);
                        let rtattr_message = &*(rtattr_message as *const rtattr);
                        let rtattr_needs_offset = message_offset + rtattr_message.rta_len as usize;
                        if rtattr_needs_offset <= message.nlmsg_len as usize {
                            if rtattr_message.rta_type == libc::IFLA_STATS64 {
                                let rta_data_offset = message_offset + rtattr_size;
                                let rta_data_message = message_ptr.add(rta_data_offset);
                                let rta_data_length = rtattr_message.rta_len as usize - rtattr_size;
                                let mut statistics = std::mem::zeroed::<rtnl_link_stats64>();
                                let rta_data_length =
                                    std::mem::size_of::<rtnl_link_stats64>().min(rta_data_length);
                                std::ptr::copy_nonoverlapping::<u8>(
                                    rta_data_message,
                                    &raw mut statistics as *mut u8,
                                    rta_data_length,
                                );
                                network.read_bytes += statistics.rx_bytes;
                                network.write_bytes += statistics.tx_bytes;
                            }
                        }
                        message_offset += ((rtattr_message.rta_len as usize) + 3) & !3;
                    }
                }
                _ => {}
            }
            let message_length = (message.nlmsg_len + 3) & !3;
            offset += message_length as i32;
            message_ptr = message_ptr.wrapping_offset(message_length as isize);
        }
    }
}
