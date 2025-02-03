// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! Communication endpoints for mpsc-based signalling

use crate::error::Error;
use crate::ids::ChannelId;
// re-exports for convenience
pub(crate) use crate::signalling::common::mpsc::endpoint::ProtocolMultiReceiver;
pub(crate) use crate::signalling::common::mpsc::endpoint::{
    ProtocolMultiSender, ProtocolReceiver, ProtocolSender, ProtocolSignal,
};
use crate::signalling::common::signals::Signal;
use crate::signalling::relayed;
use core::time::Duration;

impl relayed::interface::ProtocolSend for ProtocolSender {
    type ProtocolSignal = ProtocolSignal;

    fn send(&mut self, signal: Self::ProtocolSignal) -> Result<(), Error> {
        self.send(signal)
    }
    fn connect_receiver(&mut self, timeout: Duration) -> Result<(), Error> {
        self.connect_receiver(timeout)
    }
}

impl relayed::interface::ProtocolRecv for ProtocolReceiver {
    type ProtocolSignal = ProtocolSignal;

    fn receive(&mut self, timeout: Duration) -> Result<Option<Self::ProtocolSignal>, Error> {
        self.receive(timeout)
    }

    fn connect_sender(&mut self, timeout: Duration) -> Result<(), Error> {
        self.connect_sender(timeout)
    }
}

impl relayed::interface::ProtocolMultiSend for ProtocolMultiSender {
    type ProtocolSignal = ProtocolSignal;

    fn send(&mut self, channel_id: ChannelId, signal: Self::ProtocolSignal) -> Result<(), Error> {
        self.send(channel_id, signal)
    }

    fn connect_receivers(&mut self, timeout: Duration) -> Result<(), Error> {
        self.connect_receivers(timeout)
    }
}

impl relayed::interface::ProtocolMultiRecv for ProtocolMultiReceiver {
    type ProtocolSignal = ProtocolSignal;

    fn receive(&mut self, timeout: Duration) -> Result<Option<Self::ProtocolSignal>, Error> {
        self.receive(timeout)
    }

    fn connect_senders(&mut self, timeout: Duration) -> Result<(), Error> {
        self.connect_senders(timeout)
    }
}

impl TryFrom<ProtocolSignal> for Signal {
    type Error = Error;

    fn try_from(protocol_signal: ProtocolSignal) -> Result<Self, Self::Error> {
        match protocol_signal {
            ProtocolSignal::Core(s) => Ok(s),
            _ => Err(Error::UnexpectedProtocolSignal),
        }
    }
}

impl From<Signal> for ProtocolSignal {
    fn from(signal: Signal) -> Self {
        ProtocolSignal::Core(signal)
    }
}
