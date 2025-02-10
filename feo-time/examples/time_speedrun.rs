// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use feo_log::{debug, info, LevelFilter};
use feo_time::Scaled;
use std::thread;
use std::time::Duration;

fn main() {
    feo_logger::init(LevelFilter::Debug, true, false);

    let start_instant_std = std::time::Instant::now();
    let start_instant_feo = feo_time::Instant::now();
    let start_systemtime_std = std::time::SystemTime::now();
    let start_systemtime_feo = feo_time::SystemTime::now();

    info!("Speeding up time by a factor of 2");
    feo_time::speed(2);

    for _ in 0..5 {
        debug!("Sleeping for 1 \"real\" second...");
        thread::sleep(std::time::Duration::from_secs(1));
        info!(
            "feo time since start: systemtime: {:?}, instant: {:?}",
            start_systemtime_feo.elapsed().expect("time error"),
            start_instant_feo.elapsed()
        );
        info!(
            "std time since start: systemtime: {:?}, instant: {:?}",
            start_systemtime_std.elapsed().expect("time error"),
            start_instant_std.elapsed()
        );
    }

    // Scaling duration for thread::sleep. Use `scaled()` method to get the scaled duration
    // that matches the current time speed factor and feed it into `std::thread::sleep`.
    const SLEEP_DURATION: Duration = std::time::Duration::from_secs(1);
    let sleep_duration_scaled = SLEEP_DURATION.scaled();

    for _ in 0..5 {
        debug!("Sleeping for {SLEEP_DURATION:?} (scaled: {sleep_duration_scaled:?})");
        thread::sleep(sleep_duration_scaled);
        info!(
            "feo time since start: systemtime: {:?}, instant: {:?}",
            start_systemtime_feo.elapsed().expect("time error"),
            start_instant_feo.elapsed()
        );
        info!(
            "std time since start: systemtime: {:?}, instant: {:?}",
            start_systemtime_std.elapsed().expect("time error"),
            start_instant_std.elapsed()
        );
    }
}
