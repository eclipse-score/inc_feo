// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! Tracing library for the FEO project.

/// The `subscriber` module contains the `Subscriber` struct, which is a custom `tracing` subscriber.
/// The tracing data is forward to `feo-tracer`
#[path = "subscriber.rs"]
mod feo_subscriber;
pub mod protocol;

/// Initialize tracing
pub use feo_subscriber::init;
/// Re-export of the `tracing` crate.
pub use tracing::{self, event, instrument, level_filters::LevelFilter, span, Level};
