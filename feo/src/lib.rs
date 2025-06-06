// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! FEO is an application framework for data- and time-driven applications in the ADAS domain.
//! The name is an abbreviation of Fixed Execution Order.
//!
//! # Activities and Task Chains
//!
//! [Activities](crate::activity::Activity) are the units of computation. This could be an algorithm which detects and extracts
//! lane information from a provided camera image. Such activities are the building blocks of a
//! task chain which is executed cyclically.
//!
//! # Communication via Topics
//!
//! Data exchange between activities is provided by [feo::com](crate::com). Each activity can be configured
//! to read and write messages to a named topic.
//!
//! # Execution of Activities
//!
//! A FEO application consist of one or more agents (processes) with one or more workers (threads)
//! per agent.
//! Each activity is statically mapped to one agent and one worker through [feo::configuration](crate::configuration).

#![no_std]
#![deny(
    clippy::std_instead_of_core,
    clippy::std_instead_of_alloc,
    clippy::alloc_instead_of_core
)]

extern crate alloc;
extern crate std;

pub mod activity;
pub mod agent;
pub mod cpp;
pub mod error;
pub mod ids;
#[cfg(feature = "recording")]
pub mod recording;
pub mod scheduler;
pub mod signalling;
mod timestamp;
pub mod topicspec;
pub mod worker;
