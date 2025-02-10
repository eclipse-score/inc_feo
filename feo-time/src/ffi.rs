// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use crate::SystemTime;

/// Time in seconds and nanoseconds.
#[repr(C)]
struct FeoTimeSpec {
    tv_sec: u64,
    tv_nsec: u32,
}

/// Set the clock speed factor.
#[no_mangle]
extern "C" fn feo_clock_speed(factor: i32) {
    crate::speed(factor);
}

/// Get the current time.
#[no_mangle]
extern "C" fn feo_clock_gettime(ts: *mut FeoTimeSpec) {
    debug_assert!(!ts.is_null());

    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("time error");

    unsafe {
        (*ts).tv_sec = now.as_secs();
        (*ts).tv_nsec = now.subsec_nanos();
    }
}
