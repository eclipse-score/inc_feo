// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use std::ffi::c_int;

#[no_mangle]
extern "C" fn __init(level_filter: c_int, console: bool, logd: bool) {
    let level_filter = match level_filter {
        0 => feo_log::LevelFilter::Off,
        1 => feo_log::LevelFilter::Error,
        2 => feo_log::LevelFilter::Warn,
        3 => feo_log::LevelFilter::Info,
        4 => feo_log::LevelFilter::Debug,
        5 => feo_log::LevelFilter::Trace,
        _ => panic!("invalid level filter"),
    };
    feo_logger::init(level_filter, console, logd);
}
