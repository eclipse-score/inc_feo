// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! FEO Error implementation

use crate::ids::{ActivityId, ChannelId, WorkerId};
use crate::signalling::common::signals::Signal;
use core::time::Duration;

/// FEO Error type
#[non_exhaustive]
#[derive(Debug)]
pub enum Error {
    ActivityNotFound(ActivityId),
    Channel(&'static str),
    ChannelNotFound(ChannelId),
    Io((std::io::Error, &'static str)),
    Timeout(Duration, &'static str),
    UnexpectedProtocolSignal,
    UnexpectedSignal(Signal),
    WorkerNotFound(WorkerId),
}

impl core::error::Error for Error {}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            Error::ActivityNotFound(id) => write!(f, "failed to find activity with ID {id}"),
            Error::Channel(description) => write!(f, "channel error: {description}"),
            Error::ChannelNotFound(id) => write!(f, "failed to find channel with ID {id}"),
            Error::Io((e, description)) => write!(f, "{description}: io error: {e}"),
            Error::Timeout(duration, action) => {
                write!(
                    f,
                    "timeout reached ({:0.3}s) while {action}",
                    duration.as_secs_f64()
                )
            }
            Error::UnexpectedProtocolSignal => write!(f, "received unexpected protocol signal"),
            Error::UnexpectedSignal(signal) => write!(f, "received unexpected signal {signal}"),
            Error::WorkerNotFound(id) => write!(f, "failed to find worker with ID {id}"),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::Io((err, "failed"))
    }
}
