// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use crate::error::Error;

pub trait Receiver<T>: Send {
    fn recv(&mut self) -> Result<T, Error>;
}

pub trait Sender<T>: Send {
    fn send(&mut self, t: T) -> Result<(), Error>;
}
