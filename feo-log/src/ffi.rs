// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use log::{Level, LevelFilter, Record};
use std::ffi::{c_char, c_int, CStr};

#[allow(non_camel_case_types)]
type feo_log_Level = ::std::os::raw::c_int;

const LEVEL_ERROR: feo_log_Level = 1;
const LEVEL_WARN: feo_log_Level = 2;
const LEVEL_INFO: feo_log_Level = 3;
const LEVEL_DEBUG: feo_log_Level = 4;
const LEVEL_TRACE: feo_log_Level = 5;

#[allow(non_camel_case_types)]
type feo_log_Level_Filter = ::std::os::raw::c_int;

const LEVEL_FILTER_OFF: feo_log_Level = 0;
const LEVEL_FILTER_ERROR: feo_log_Level = 1;
const LEVEL_FILTER_WARN: feo_log_Level = 2;
const LEVEL_FILTER_INFO: feo_log_Level = 3;
const LEVEL_FILTER_DEBUG: feo_log_Level = 4;
const LEVEL_FILTER_TRACE: feo_log_Level = 5;

#[no_mangle]
extern "C" fn __log(
    file: *const c_char,
    line: c_int,
    level: feo_log_Level,
    target: *const c_char,
    message: *const c_char,
) {
    // Map the c level to the log level
    let level = match level {
        LEVEL_ERROR => Level::Error,
        LEVEL_WARN => Level::Warn,
        LEVEL_INFO => Level::Info,
        LEVEL_DEBUG => Level::Debug,
        LEVEL_TRACE => Level::Trace,
        _ => panic!("invalid level"),
    };

    // Extract the target. This mappes to the tag of the c api
    let target = unsafe { CStr::from_ptr(target) }
        .to_str()
        .expect("invalid target");

    // Construct metadata to be used for the pre check filtering
    let metadata = log::Metadata::builder().level(level).target(target).build();

    // Check if the log would have a chance to be logged. before we do any more work.
    if !log::logger().enabled(&metadata) {
        return;
    }

    let file = unsafe { CStr::from_ptr(file) }
        .to_str()
        .expect("invalid file");
    let line = line as u32;
    let message = unsafe { CStr::from_ptr(message) }
        .to_str()
        .expect("invalid message");

    // Pass the record to the logger
    log::logger().log(
        &Record::builder()
            .level(level)
            .target(target)
            .file(Some(file))
            .line(Some(line))
            .args(format_args!("{}", message))
            .build(),
    );
}

/// Set the maximum log level
#[no_mangle]
extern "C" fn __set_max_level(level_filter: feo_log_Level_Filter) {
    let level_filter = match level_filter {
        LEVEL_FILTER_OFF => LevelFilter::Off,
        LEVEL_FILTER_ERROR => LevelFilter::Error,
        LEVEL_FILTER_WARN => LevelFilter::Warn,
        LEVEL_FILTER_INFO => LevelFilter::Info,
        LEVEL_FILTER_DEBUG => LevelFilter::Debug,
        LEVEL_FILTER_TRACE => LevelFilter::Trace,
        _ => panic!("invalid level filter"),
    };
    log::set_max_level(level_filter);
}

#[no_mangle]
extern "C" fn __max_level() -> feo_log_Level_Filter {
    match log::max_level() {
        LevelFilter::Off => LEVEL_FILTER_OFF,
        LevelFilter::Error => LEVEL_FILTER_ERROR,
        LevelFilter::Warn => LEVEL_FILTER_WARN,
        LevelFilter::Info => LEVEL_FILTER_INFO,
        LevelFilter::Debug => LEVEL_FILTER_DEBUG,
        LevelFilter::Trace => LEVEL_FILTER_TRACE,
    }
}
