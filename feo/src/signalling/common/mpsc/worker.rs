// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use crate::error::Error;
use crate::signalling::common::interface::ConnectWorker;
use crate::signalling::common::mpsc::endpoint::{ProtocolReceiver, ProtocolSender, ProtocolSignal};
use crate::signalling::common::signals::Signal;
use core::time::Duration;

pub(crate) struct WorkerConnector {
    receiver: ProtocolReceiver,
    sender: ProtocolSender,
}

impl WorkerConnector {
    pub fn new(sender: ProtocolSender, receiver: ProtocolReceiver) -> Self {
        Self { receiver, sender }
    }

    pub fn receive(&mut self, timeout: Duration) -> Result<Option<Signal>, Error> {
        let protocol_signal = self.receiver.receive(timeout)?;
        if protocol_signal.is_none() {
            return Ok(None);
        }
        match protocol_signal.unwrap() {
            ProtocolSignal::Core(signal) => Ok(Some(signal)),
            _ => Err(Error::UnexpectedProtocolSignal),
        }
    }

    pub fn send(&mut self, signal: Signal) -> Result<(), Error> {
        let protocol_signal = ProtocolSignal::Core(signal);
        self.sender.send(protocol_signal)
    }
}

impl ConnectWorker for WorkerConnector {
    fn connect_remote(&mut self) -> Result<(), Error> {
        let timeout = Duration::from_secs(60);
        self.sender.connect_receiver(timeout)?;
        self.receiver.connect_sender(timeout)
    }

    fn receive(&mut self, timeout: Duration) -> Result<Option<Signal>, Error> {
        self.receive(timeout)
    }

    fn send_to_scheduler(&mut self, signal: &Signal) -> Result<(), Error> {
        self.send(*signal)
    }
}
