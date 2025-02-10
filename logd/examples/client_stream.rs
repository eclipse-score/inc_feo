// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! This example demonstrates how to send log records to the logd daemon using a Unix stream socket.

use anyhow::Error;
use logd::UNIX_STREAM_PATH;
use std::io::Write;
use std::os::unix::net;
use std::{thread, time};

fn main() -> Result<(), Error> {
    let mut stream = net::UnixStream::connect(UNIX_STREAM_PATH)?;

    loop {
        let record = feo_logger::record::Record {
            timestamp: feo_time::SystemTime::now(),
            level: feo_log::Level::Info,
            target: "some::module",
            file: Some(file!()),
            line: Some(line!()),
            tgid: std::process::id(),
            tid: 19,
            args: b"hello again unix via unix stream",
        };
        let len = record.encoded_len() as u32;
        stream.write_all(&len.to_be_bytes())?;
        record.encode(&mut stream)?;

        thread::sleep(time::Duration::from_secs(1));
    }
}
