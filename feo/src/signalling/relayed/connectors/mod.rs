// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! Signalling connectors and relays for various feo components

#[cfg(feature = "recording")]
pub(crate) mod recorder;
pub(crate) mod relays;
pub(crate) mod scheduler;
pub(crate) mod secondary;
