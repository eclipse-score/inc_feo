// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

fn main() {
    println!("cargo::rerun-if-changed=src/include/activities.h");
    println!("cargo::rerun-if-changed=src/activities.cpp");
    println!("cargo::rerun-if-changed=build.rs");

    cc::Build::new()
        .cpp(true)
        .file("src/include/activities.h")
        .file("src/activities.cpp")
        .compile("libactivities_cc");
}
