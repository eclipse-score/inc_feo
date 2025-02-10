// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! Connect to logd and send a log record every second via seqpackets.

use anyhow::Error;
use feo_log::Level;
use feo_logger::record::Record;
use feo_time::SystemTime;
use logd::{MAX_RECORD_SIZE, UNIX_PACKET_PATH};
use socket2::{SockAddr, Socket};
use std::{process, thread, time};

fn main() -> Result<(), Error> {
    let socket = Socket::new(socket2::Domain::UNIX, socket2::Type::SEQPACKET, None)?;
    socket.connect(&SockAddr::unix(UNIX_PACKET_PATH)?)?;
    let mut buffer = Vec::with_capacity(MAX_RECORD_SIZE);

    loop {
        let record = Record {
            timestamp: SystemTime::now(),
            level: Level::Info,
            target: "some::module",
            file: Some(file!()),
            line: Some(line!()),
            tgid: process::id(),
            tid: 12,
            args: b"hello again via seqpacket",
        };
        buffer.clear();
        record.encode(&mut buffer)?;
        socket.send(&buffer)?;

        thread::sleep(time::Duration::from_secs(1));
    }
}
