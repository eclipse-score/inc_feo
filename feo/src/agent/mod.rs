// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! FEO agents are processes.
//!
//! In each FEO application there is one primary agent and optional secondary
//! agents. The primary agent is responsible for triggering the execution of all activities distributed
//! across all agents.

use core::net::SocketAddr;
use std::path::PathBuf;

pub mod com_init;
pub mod direct;
pub mod relayed;

/// Node address of a connection
#[derive(Debug, Clone)]
pub enum NodeAddress {
    Tcp(SocketAddr),
    UnixSocket(PathBuf),
}
