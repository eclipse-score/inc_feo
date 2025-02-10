// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! FEO agents are processes.
//!
//! In each FEO application there is one primary agent and optional secondary
//! agents. The primary agent is responsible for triggering the execution of all activities distributed
//! across all agents.

pub mod primary;
pub mod secondary;
