// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! The score-feo logging facade.
//!
//! Use what's proven. Use what used out there. No - we're not special.

#![no_std]
#![deny(
    clippy::std_instead_of_core,
    clippy::std_instead_of_alloc,
    clippy::alloc_instead_of_core
)]

pub use log::*;
