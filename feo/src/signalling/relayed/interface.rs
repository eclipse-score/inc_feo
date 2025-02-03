// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use crate::error::Error;
use crate::ids::ChannelId;
use crate::signalling::common::signals::Signal;
use alloc::boxed::Box;
use core::fmt::Debug;
use core::time::Duration;

pub(crate) type Builder<T> = Box<dyn FnOnce() -> T + Send>;

/// Collection of types needed to implement a signalling channel
pub(crate) trait IsChannel: 'static {
    type ProtocolSignal: From<Signal> + TryInto<Signal> + Copy + Debug;
    type Sender: ProtocolSend<ProtocolSignal = Self::ProtocolSignal>;
    type Receiver: ProtocolRecv<ProtocolSignal = Self::ProtocolSignal>;
    type MultiSender: ProtocolMultiSend<ProtocolSignal = Self::ProtocolSignal>;
    type MultiReceiver: ProtocolMultiRecv<ProtocolSignal = Self::ProtocolSignal>;
}

pub(crate) trait ProtocolSend {
    type ProtocolSignal: From<Signal> + TryInto<Signal> + Copy + Debug;

    fn send(&mut self, signal: Self::ProtocolSignal) -> Result<(), Error>;

    fn connect_receiver(&mut self, timeout: Duration) -> Result<(), Error>;
}

pub(crate) trait ProtocolRecv {
    type ProtocolSignal: From<Signal> + TryInto<Signal> + Copy + Debug;

    fn receive(&mut self, timeout: Duration) -> Result<Option<Self::ProtocolSignal>, Error>;

    fn connect_sender(&mut self, timeout: Duration) -> Result<(), Error>;
}

pub(crate) trait ProtocolMultiRecv {
    type ProtocolSignal: From<Signal> + TryInto<Signal> + Copy + Debug;

    fn receive(&mut self, timeout: Duration) -> Result<Option<Self::ProtocolSignal>, Error>;

    fn connect_senders(&mut self, timeout: Duration) -> Result<(), Error>;
}

pub(crate) trait ProtocolMultiSend {
    type ProtocolSignal: From<Signal> + TryInto<Signal> + Copy + Debug;

    fn send(&mut self, channel_id: ChannelId, signal: Self::ProtocolSignal) -> Result<(), Error>;

    fn connect_receivers(&mut self, _timeout: Duration) -> Result<(), Error>;
}
