// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use crate::fmt;
use crate::record::Record;
use std::io::{self};

#[derive(Debug, Default)]
pub struct Console;

impl Console {
    pub fn write(&self, record: &Record) -> io::Result<()> {
        fmt::format(record, io::stdout())
    }
}
