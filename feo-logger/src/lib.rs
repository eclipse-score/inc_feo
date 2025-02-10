// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! The score-feo-logger
//!
//! Bare minimum logger implementation for the `score-feo` project.
//! This is placeholder.

use feo_log::{LevelFilter, Log, Metadata, Record};
use feo_time::SystemTime;
use std::fmt::Debug;
use std::io::Write;
use std::str::FromStr;
use std::{io, process};

mod console;
// TODO: hide fmt and its deps behind a feature flag: `console` and `time`.
pub mod fmt;
mod logd;
pub mod record;
mod thread;

const ENV_RUST_LOG: &str = "RUST_LOG";
const MAX_ARGS_SIZE: usize = 8 * 1024;
pub const MAX_RECORD_SIZE: usize = 8 * 1024;

/// Initialize the logger.
///
/// A valid level passed as `RUST_LOG` environment variable will `level`.
/// Enable output to `stdout` via `console`.
/// Enable output forwarding to `logd` via `logd=true`.
pub fn init(level: LevelFilter, console: bool, logd: bool) {
    let logger = Logger::new(console, logd);

    // Set the maximum log level the log subsystem will forward to this logger impl.
    feo_log::set_max_level(level_from_env().unwrap_or(level));
    // Set the logger in the global subsystem.
    feo_log::set_boxed_logger(Box::new(logger)).expect("failed to set logger")
}

/// The FEO logger.
#[derive(Debug)]
pub struct Logger {
    console: Option<console::Console>,
    logd: Option<logd::Logd>,
}

impl Logger {
    /// Create a new logger.
    pub fn new(console: bool, logd: bool) -> Self {
        let console = console.then(console::Console::default);
        let logd = logd.then(logd::Logd::default);
        Self { console, logd }
    }
}

impl Log for Logger {
    /// Check if a log message with the specified metadata would be logged.
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= feo_log::max_level()
    }

    fn log(&self, record: &Record) {
        let timestamp = SystemTime::now();
        let tgid = process::id();
        let tid = thread::id();
        // Serialize args into args buffer. This must happen without any heap allocation which is ensured
        // by using std::io::Write.
        let args_buffer = &mut [0u8; MAX_ARGS_SIZE];
        let args = {
            let mut writer = io::Cursor::new(&mut args_buffer[..]);
            write!(&mut writer, "{}", record.args()).expect("failed to format args");
            let len = writer.position() as usize;
            &args_buffer[0..len]
        };
        let level = record.level();
        let target = record.target();
        let file = record.file();
        let line = record.line();

        let record = record::Record::new(timestamp, level, target, file, line, tgid, tid, args);

        if let Some(console) = &self.console {
            console.write(&record).expect("failed to write to console");
        }

        if let Some(logd) = &self.logd {
            let _ = logd.write(&record);
        }
    }

    fn flush(&self) {}
}

/// Try to parse the log level from the environment variable `RUST_LOG`.
fn level_from_env() -> Option<LevelFilter> {
    std::env::var(ENV_RUST_LOG).ok().and_then(|s| {
        LevelFilter::from_str(&s)
            .inspect_err(|_| eprintln!("Failed to parse log level from `RUST_LOG={s}`"))
            .ok()
    })
}
