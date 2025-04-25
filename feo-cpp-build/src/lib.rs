// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! Helpers for building C++ components for FEO

use std::path::PathBuf;

/// Build a library of C++ components from the given list of component source files
///
/// Arguments:
/// - sources: List of component source files
/// - header_dirs: List of directories to add to the `-I` include paths
/// - library: Name of the library to build
/// - root_path: Path to the repository root directory relative to the build.rs file
pub fn activity_lib(sources: &[&str], header_dirs: &[&str], library: &str, root_path: PathBuf) {
    for file in sources {
        println!("cargo::rerun-if-changed={file}");
    }

    let cpp_include_dir = cpp_include_dir().to_str().unwrap().to_owned();
    let cpp_include_dir = root_path.join(cpp_include_dir).canonicalize().unwrap();

    let mut build = cc::Build::new();
    build
        .cpp(true)
        .includes([cpp_include_dir])
        .includes(header_dirs);
    for file in sources {
        build.file(file);
    }

    build.compile(library)
}

/// Return the path to the include_directory
pub fn cpp_include_dir() -> PathBuf {
    let path = PathBuf::from(file!());
    path.parent().unwrap().join("include")
}
