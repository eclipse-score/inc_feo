// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! Basic components for mpsc channel signalling protocol

use crate::error::Error;
use crate::ids::ChannelId;
use crate::signalling::common::mpsc::primitives::{channel, Receiver, Sender};
use crate::signalling::common::signals::Signal;
use core::time::Duration;
use feo_log::{debug, error, trace};
use feo_time::Instant;
use std::collections::{HashMap, HashSet};

pub(crate) struct ProtocolSender {
    channel_id: ChannelId,
    sender: Sender<ProtocolSignal>,
}

impl ProtocolSender {
    pub fn new(channel_id: ChannelId, sender: Sender<ProtocolSignal>) -> Self {
        Self { channel_id, sender }
    }

    pub fn send(&mut self, t: ProtocolSignal) -> Result<(), Error> {
        self.sender
            .send(t)
            .map_err(|_| Error::Channel("channel closed"))
    }

    pub fn connect_receiver(&mut self, _timeout: Duration) -> Result<(), Error> {
        let protocol_signal = ProtocolSignal::Connect(self.channel_id);
        self.sender.send(protocol_signal)?;
        debug!(
            "Sent connect request signal from channel {:?}",
            self.channel_id
        );
        Ok(())
    }
}

pub(crate) struct ProtocolReceiver {
    channel_id: Option<ChannelId>,
    receiver: Receiver<ProtocolSignal>,
}

impl ProtocolReceiver {
    pub fn new(receiver: Receiver<ProtocolSignal>) -> Self {
        Self {
            channel_id: None,
            receiver,
        }
    }

    pub fn receive(&mut self, timeout: Duration) -> Result<Option<ProtocolSignal>, Error> {
        if self.channel_id.is_none() {
            panic!("ProtocolReceiver not connected")
        }
        self.receiver.receive(timeout)
    }

    pub fn connect_sender(&mut self, timeout: Duration) -> Result<(), Error> {
        let start = Instant::now();
        while self.channel_id.is_none() && start.elapsed() <= timeout {
            let protocol_signal = self.receiver.receive(timeout)?;
            self.channel_id = match protocol_signal {
                Some(ProtocolSignal::Connect(c)) => Some(c),
                Some(other) => {
                    error!("Received unexpected protocol signal {other:?}");
                    continue;
                }
                None => return Err(Error::Timeout(timeout, "connecting sender")),
            };
            trace!("Receiver channel connected: {:?}", self.channel_id);
            return Ok(());
        }
        Err(Error::Timeout(timeout, "connecting sender"))
    }
}

pub(crate) struct ProtocolMultiReceiver {
    channel_ids: HashSet<ChannelId>,
    connected: bool,
    receiver: Receiver<ProtocolSignal>,
}

impl ProtocolMultiReceiver {
    pub fn create<'s, T>(channel_ids: &'s T) -> (Self, HashMap<ChannelId, Sender<ProtocolSignal>>)
    where
        &'s T: IntoIterator<Item = &'s ChannelId>,
    {
        let (sender, receiver) = channel::<ProtocolSignal>();
        let senders: HashMap<_, _> = channel_ids
            .into_iter()
            .map(|id| (*id, sender.clone()))
            .collect();
        let channel_ids: HashSet<_> = channel_ids.into_iter().copied().collect();
        (
            Self {
                channel_ids,
                connected: false,
                receiver,
            },
            senders,
        )
    }

    pub fn receive(&mut self, timeout: Duration) -> Result<Option<ProtocolSignal>, Error> {
        if !self.connected {
            panic!("MultiProtocolReceiver not connected")
        }
        self.receiver.receive(timeout)
    }

    pub fn connect_senders(&mut self, timeout: Duration) -> Result<(), Error> {
        let mut missing = self.channel_ids.clone();
        let start = Instant::now();
        while !missing.is_empty() && start.elapsed() < timeout {
            trace!("Connecting, missing channels {missing:?}");
            let protocol_signal = self.receiver.receive(timeout)?;
            let channel_id = match protocol_signal {
                Some(ProtocolSignal::Connect(c)) => c,
                Some(other) => {
                    error!("Received unexpected protocol signal {other:?}");
                    continue;
                }
                None => return Err(Error::Timeout(timeout, "connecting senders")),
            };
            if !self.channel_ids.contains(&channel_id) {
                error!("Received connect signal from unexpected channel {channel_id:?}");
                continue;
            }
            let is_new = missing.remove(&channel_id);
            if !is_new {
                error!("Ignoring connect signal from previously connected channel {channel_id:?}");
            } else {
                trace!("Sender channel connected: {channel_id:?}");
            }
        }
        self.connected = true;
        Ok(())
    }
}

pub(crate) struct ProtocolMultiSender {
    senders: HashMap<ChannelId, Sender<ProtocolSignal>>,
}

impl ProtocolMultiSender {
    pub fn create<'s, T>(channel_ids: T) -> (Self, HashMap<ChannelId, Receiver<ProtocolSignal>>)
    where
        T: IntoIterator<Item = &'s ChannelId>,
    {
        let mut senders: HashMap<ChannelId, Sender<ProtocolSignal>> = Default::default();
        let mut receivers: HashMap<ChannelId, Receiver<ProtocolSignal>> = Default::default();
        for id in channel_ids {
            let (sender, receiver) = channel::<ProtocolSignal>();
            let p = senders.insert(*id, sender);
            let _ = receivers.insert(*id, receiver);
            assert!(p.is_none(), "duplicate channel id")
        }
        (Self { senders }, receivers)
    }

    pub fn send(&mut self, channel_id: ChannelId, signal: ProtocolSignal) -> Result<(), Error> {
        let sender = self
            .senders
            .get(&channel_id)
            .ok_or(Error::ChannelNotFound(channel_id))?;
        sender
            .sender
            .send(signal)
            .map_err(|_| Error::Channel("channel closed"))
    }

    pub fn connect_receivers(&mut self, _timeout: Duration) -> Result<(), Error> {
        for (id, sender) in self.senders.iter_mut() {
            sender.send(ProtocolSignal::Connect(*id))?
        }
        Ok(())
    }
}

/// Internal input signal specific to this signalling implementation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProtocolSignal {
    Core(Signal),
    Connect(ChannelId),
}
