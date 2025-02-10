// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

fn main() -> std::io::Result<()> {
    prost_build::compile_protos(&["protos/perfetto_trace.proto"], &["protos"]).map(drop)
}
