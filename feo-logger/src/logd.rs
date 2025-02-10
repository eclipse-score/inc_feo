// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use crate::record::Record;
use crate::MAX_RECORD_SIZE;
use libc::{sockaddr_un, AF_UNIX};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::Mutex;
use std::{io, mem};

pub const UNIX_PACKET_PATH: &str = "/tmp/logd.sock";

/// Simple connector to logd. Uses pure libc. Doesn't do any reconnects etc. Needs lots of checks and love.
#[derive(Debug, Default)]
pub struct Logd {
    socket: Mutex<Option<OwnedFd>>,
}

impl Logd {
    pub fn write(&self, record: &Record) -> io::Result<()> {
        if record.encoded_len() > MAX_RECORD_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "record too large to encode",
            ));
        }

        // TODO: this can be optimized. Use MaybeUninit and write the record directly to the buffer.
        let mut writer = io::Cursor::new([0u8; MAX_RECORD_SIZE]);

        // Encode
        let len = record.encode(&mut writer)?;

        let buffer = writer.into_inner();

        let mut guard = self.socket.lock().unwrap();

        // Reconnect if needed
        if guard.is_none() {
            // Connect
            match Self::connect() {
                Ok(connection) => *guard = Some(connection),
                Err(_) => return Ok(()),
            };
        }

        let socket = guard.as_mut().unwrap();

        // Safety: buffer is a valid buffer with the correct length
        let ret = unsafe { libc::send(socket.as_raw_fd(), buffer.as_ptr().cast(), len, 0) };
        if ret != len as isize {
            guard.take();
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn connect() -> io::Result<OwnedFd> {
        // Create a seqpacket socket
        let socket = unsafe { libc::socket(AF_UNIX, libc::SOCK_SEQPACKET, 0) };
        assert!(socket >= 0, "socket failed");
        // Wrap the socket in a OwnedFd
        // Safety: socket is a valid file descriptor. Connect result is checked.
        let socket = unsafe { OwnedFd::from_raw_fd(socket) };

        // Prepare the sockaddr
        let bytes = UNIX_PACKET_PATH.as_bytes();

        // Safety: sockaddr_un is a C struct with no padding
        let mut sockaddr = unsafe { mem::MaybeUninit::<sockaddr_un>::zeroed().assume_init() };

        sockaddr.sun_family = AF_UNIX as u16;

        let path = (&mut sockaddr.sun_path.as_mut_slice()[0..bytes.len()]) as *mut _ as *mut [u8];
        // Safety: path is a valid pointer to a slice of the correct length
        let path = unsafe { &mut *path };
        path.clone_from_slice(bytes);
        let sockaddr = &mut sockaddr as *mut _ as *mut libc::sockaddr;
        let addr_len = mem::size_of::<sockaddr_un>() as libc::socklen_t;

        // Connect
        // Safety: sockaddr is a valid pointer to a sockaddr_un. addr_len is the correct size.
        let ret = unsafe { libc::connect(socket.as_raw_fd(), sockaddr, addr_len) };
        if ret != 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(socket)
        }
    }
}
