// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use feo_log::{error, log, warn, Level, LevelFilter};
use std::{thread, time};

fn main() {
    feo_logger::init(LevelFilter::Trace, true, true);

    // Logs a static string on level `trace`.
    log!(Level::Trace, "Kick it");

    // Logs on level `debug` with `target` set to "hello".
    log!(
        target: "hello",
        Level::Debug,
        "You wake up late for school, man you don't want to go"
    );

    // Logs a format string on level `info`.
    log!(
        Level::Info,
        "You ask your mom, please? but she still says, {}!",
        "No"
    );

    // Logs a static string on level `warn`.
    warn!("You missed two classes");

    loop {
        // Logs a static string on level `error`.
        error!("And no homework");
        thread::sleep(time::Duration::from_secs(1));
    }
}
