// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! Agent implementation for direct scheduler-to-worker signalling
//!
//! The scheduler connects directly to each worker, independent of whether
//! the worker runs in the same process or another process.

pub mod primary;
pub mod primary_mpsc;
#[cfg(feature = "recording")]
pub mod recorder;
pub mod secondary;
