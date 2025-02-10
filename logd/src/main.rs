// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! Placeholder logging daemon that collects logs from various sources. Minimal effort implementation.

use anyhow::Error;
use feo_log::{info, LevelFilter};
use tokio::runtime;

fn main() -> Result<(), Error> {
    // Initialize the logger *without* the logd part logger.
    feo_logger::init(LevelFilter::Debug, true, false);

    info!("Starting logd");

    let logd = logd::run();

    runtime::Builder::new_current_thread()
        .enable_io()
        .build()?
        .block_on(logd)
}
