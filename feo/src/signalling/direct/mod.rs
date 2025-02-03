// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! Signalling layer with direct connection between scheduler and workers
//!
//! This module provides connector implementations which establish direct connections
//! between the scheduler and every worker.
//! The layer does not differentiate between connecting workers in other processes
//! or in the same process as the scheduler.

pub(crate) mod mpsc;
#[cfg(feature = "recording")]
pub(crate) mod recorder;
pub(crate) mod scheduler;
pub(crate) mod worker;
