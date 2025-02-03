// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use std::env;
use std::path::PathBuf;

// Relative path to the feo repository root directory
static PATH_TO_REPO_ROOT: &str = "../../../";

fn main() {
    let sources = ["src/cpp/lane_assist.cpp", "src/cpp/trajec_vis.cpp"];
    let header_dirs = ["src/include/"];

    println!("cargo::rerun-if-changed=build.rs");

    let local_dir: PathBuf = env::var("CARGO_MANIFEST_DIR").unwrap().into();
    let repo_root_dir = local_dir.join(PATH_TO_REPO_ROOT);

    // Build given components into the given library
    feo_cpp_build::activity_lib(&sources, &header_dirs, "cpp_activities", repo_root_dir);
}
