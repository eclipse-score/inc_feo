// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use crate::error::Error;
use crate::signalling::common::interface::ConnectRecorder;
use crate::signalling::common::signals::Signal;
use crate::signalling::relayed::interface::{IsChannel, ProtocolRecv, ProtocolSend};
use core::time::Duration;

/// Connector for a recorder
pub(crate) struct RecorderConnector<C: IsChannel> {
    inter_sender: C::Sender,
    inter_receiver: C::Receiver,
    timeout: Duration,
}

impl<C: IsChannel> RecorderConnector<C> {
    pub fn create(inter_sender: C::Sender, inter_receiver: C::Receiver, timeout: Duration) -> Self {
        Self {
            inter_sender,
            inter_receiver,
            timeout,
        }
    }

    fn connect_remote(&mut self) -> Result<(), Error> {
        self.inter_sender.connect_receiver(self.timeout)?;
        self.inter_receiver.connect_sender(self.timeout)?;
        Ok(())
    }

    fn receive(&mut self, timeout: Duration) -> Result<Option<Signal>, Error> {
        let signal = self.inter_receiver.receive(timeout)?;

        let signal = match signal {
            Some(s) => s,
            None => return Ok(None),
        };

        // Forward signal to the primary agent
        let core_signal = signal
            .try_into()
            .map_err(|_| Error::UnexpectedProtocolSignal)?;
        Ok(Some(core_signal))
    }

    fn send_to_scheduler(&mut self, signal: Signal) -> Result<(), Error> {
        let protocol_signal = signal.into();
        self.inter_sender.send(protocol_signal)
    }
}

impl<C: IsChannel> ConnectRecorder for RecorderConnector<C> {
    fn connect_remote(&mut self) -> Result<(), Error> {
        self.connect_remote()
    }

    fn receive(&mut self, timeout: Duration) -> Result<Option<Signal>, Error> {
        self.receive(timeout)
    }

    fn send_to_scheduler(&mut self, signal: &Signal) -> Result<(), Error> {
        self.send_to_scheduler(*signal)
    }
}
