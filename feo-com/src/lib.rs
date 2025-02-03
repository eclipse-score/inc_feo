// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! Topic based communication

#![no_std]
#![deny(
    clippy::std_instead_of_core,
    clippy::std_instead_of_alloc,
    clippy::alloc_instead_of_core
)]

extern crate alloc;
extern crate std;

pub mod interface;
#[cfg(feature = "ipc_iceoryx2")]
pub mod iox2;
#[cfg(feature = "ipc_linux_shm")]
pub mod linux_shm;
