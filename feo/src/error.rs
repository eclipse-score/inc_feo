// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! FEO Error implementation

/// FEO Error type
#[non_exhaustive]
#[derive(Debug)]
pub enum Error {
    Channel(&'static str),
    Io((std::io::Error, &'static str)),
}

impl std::error::Error for Error {}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Error::Channel(description) => write!(f, "Channel error, {}", description),
            Error::Io((e, description)) => write!(f, "Io error: {}, {}", description, e),
        }
    }
}
