// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! Mpsc-only signalling module
//!
//! Mpsc-only signalling does not allow for inter-process communication. Therefore
//! it is not applicable for multi-process setups.

pub(crate) use crate::signalling::common::mpsc::worker::WorkerConnector;
use alloc::boxed::Box;

pub(crate) mod endpoint;
pub(crate) mod primitives;
pub(crate) mod worker;

pub type Builder<T> = Box<dyn FnOnce() -> T + Send>;
pub type WorkerConnectorBuilder = Builder<WorkerConnector>;
