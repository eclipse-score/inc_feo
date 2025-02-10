// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

#[allow(clippy::all)]
#[rustfmt::skip]
mod perfetto {
    include!(concat!(env!("OUT_DIR"), "/perfetto.protos.rs"));
}

pub use perfetto::*;
