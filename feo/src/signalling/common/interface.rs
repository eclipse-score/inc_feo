// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use crate::error::Error;
use crate::ids::{ActivityId, AgentId};
use crate::signalling::common::signals::Signal;
use core::time::Duration;

/// Trait for the connector of a scheduler
///
/// This is used as bound of the scheduler for its connector
pub(crate) trait ConnectScheduler {
    /// Connect remote connectors of workers and recorders
    fn connect_remotes(&mut self) -> Result<(), Error>;

    /// Synchronize the time on all remotes
    fn sync_time(&mut self) -> Result<(), Error>;

    /// Try to receive a signal, returning latest after `timeout`
    fn receive(&mut self, timeout: Duration) -> Result<Option<Signal>, Error>;

    /// Send `signal` to the activity with `activity_id`
    fn send_to_activity(&mut self, activity_id: ActivityId, signal: &Signal) -> Result<(), Error>;

    /// Send `signal` to the recorder with `recorder_id`
    fn send_to_recorder(&mut self, recorder_id: AgentId, signal: &Signal) -> Result<(), Error>;
}

/// Trait for the connector of a worker
///
/// This is used as bound on workers for their connectors
pub(crate) trait ConnectWorker {
    /// Connect to the remote connector of the scheduler
    fn connect_remote(&mut self) -> Result<(), Error>;

    /// Try to receive a signal, returning latest after `timeout`
    fn receive(&mut self, timeout: Duration) -> Result<Option<Signal>, Error>;

    /// Send `signal` to the scheduler
    fn send_to_scheduler(&mut self, signal: &Signal) -> Result<(), Error>;
}

/// Connect a recorder
///
/// This is used as a bound on recorders for their connectors
#[cfg(feature = "recording")]
pub(crate) trait ConnectRecorder {
    /// Connect to the remote connector of the scheduler
    fn connect_remote(&mut self) -> Result<(), Error>;

    /// Try to receive a signal, returning latest after `timeout`
    fn receive(&mut self, timeout: Duration) -> Result<Option<Signal>, Error>;

    /// Send `signal` to the scheduler
    fn send_to_scheduler(&mut self, signal: &Signal) -> Result<(), Error>;
}
