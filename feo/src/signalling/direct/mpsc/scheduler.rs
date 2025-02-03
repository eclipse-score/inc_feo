// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use crate::error::Error;
use crate::ids::{ActivityId, AgentId, ChannelId, WorkerId};
use crate::signalling::common::interface::ConnectScheduler;
use crate::signalling::common::mpsc::endpoint::{
    ProtocolMultiReceiver, ProtocolMultiSender, ProtocolReceiver, ProtocolSender, ProtocolSignal,
};
use crate::signalling::common::mpsc::primitives::{Receiver, Sender};
use crate::signalling::common::mpsc::worker::WorkerConnector;
use crate::signalling::common::mpsc::WorkerConnectorBuilder;
use crate::signalling::common::signals::Signal;
use alloc::boxed::Box;
use core::time::Duration;
use std::collections::{HashMap, HashSet};

pub(crate) struct SchedulerConnector {
    receiver: ProtocolMultiReceiver,
    sender: ProtocolMultiSender,
    peers: HashSet<ChannelId>,
    related_receivers: Option<HashMap<ChannelId, Receiver<ProtocolSignal>>>,
    related_senders: Option<HashMap<ChannelId, Sender<ProtocolSignal>>>,
    activity_worker_map: HashMap<ActivityId, WorkerId>,
}

type ChannelToSenderMap = HashMap<ChannelId, Sender<ProtocolSignal>>;
type ChannelToReceiverMap = HashMap<ChannelId, Receiver<ProtocolSignal>>;

impl SchedulerConnector {
    pub(crate) fn create(
        activity_worker_map: HashMap<ActivityId, WorkerId>,
    ) -> (Self, ChannelToSenderMap, ChannelToReceiverMap) {
        let channel_ids: HashSet<ChannelId> = activity_worker_map
            .values()
            .map(|id| ChannelId::Worker(*id))
            .collect();
        let (multi_sender, receivers) = ProtocolMultiSender::create(&channel_ids);
        let (multi_receiver, senders) = ProtocolMultiReceiver::create(&channel_ids);

        (
            Self {
                receiver: multi_receiver,
                sender: multi_sender,
                peers: channel_ids,
                activity_worker_map,
                related_receivers: None,
                related_senders: None,
            },
            senders,
            receivers,
        )
    }

    pub(crate) fn new(activity_worker_map: HashMap<ActivityId, WorkerId>) -> Self {
        let (object, related_senders, related_receivers) = Self::create(activity_worker_map);
        let Self {
            receiver,
            sender,
            peers,
            activity_worker_map,
            ..
        } = object;

        Self {
            receiver,
            sender,
            peers,
            activity_worker_map,
            related_receivers: Some(related_receivers),
            related_senders: Some(related_senders),
        }
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

    pub fn send(&mut self, channel_id: ChannelId, signal: Signal) -> Result<(), Error> {
        let worker_id = match channel_id {
            ChannelId::Activity(id) => self.activity_worker_map[&id],
            ChannelId::Worker(id) => id,
            _ => return Err(Error::ChannelNotFound(channel_id)),
        };

        let channel_id = ChannelId::Worker(worker_id);
        let protocol_signal = ProtocolSignal::Core(signal);
        self.sender.send(channel_id, protocol_signal)
    }

    pub fn worker_connector_builders(&mut self) -> HashMap<WorkerId, WorkerConnectorBuilder> {
        assert!(
            self.related_receivers.is_some(),
            "missing related senders and receivers"
        );

        // Create the actual connectors
        let receivers = self.related_receivers.take().unwrap();
        let mut senders = self.related_senders.take().unwrap();
        let connectors: HashMap<WorkerId, WorkerConnector> = receivers
            .into_iter()
            .map(|(id, r)| {
                let wid = match id {
                    ChannelId::Worker(wid) => wid,
                    _ => panic!("worker id needed to build worker connector"),
                };
                let s = senders.remove(&id).unwrap();
                let s = ProtocolSender::new(id, s);
                let r = ProtocolReceiver::new(r);
                let connector = WorkerConnector::new(s, r);
                (wid, connector)
            })
            .collect();

        // Create map of "builders" that store the connectors and release them when called
        connectors
            .into_iter()
            .map(|(id, connector)| {
                let builder = { move || connector };
                (id, Box::new(builder) as WorkerConnectorBuilder)
            })
            .collect()
    }
}

impl ConnectScheduler for SchedulerConnector {
    fn connect_remotes(&mut self) -> Result<(), Error> {
        let timeout = Duration::from_secs(60);
        self.receiver.connect_senders(timeout)?;
        self.sender.connect_receivers(timeout)?;
        Ok(())
    }

    /// Send `signal` to the recorder with `recorder_id`
    ///
    /// Mpsc-only signalling supports a single agent only.
    /// Therefore, nothing is to be done for synchronizing.
    /// This method is a no-op and always returns Ok.
    fn sync_time(&mut self) -> Result<(), Error> {
        Ok(())
    }

    fn receive(&mut self, timeout: Duration) -> Result<Option<Signal>, Error> {
        self.receive(timeout)
    }

    /// Send `signal` to the activity with `activity_id`
    fn send_to_activity(&mut self, activity_id: ActivityId, signal: &Signal) -> Result<(), Error> {
        self.send(ChannelId::Activity(activity_id), *signal)
    }

    /// Send `signal` to the recorder with `recorder_id`
    fn send_to_recorder(&mut self, _recorder_id: AgentId, _signal: &Signal) -> Result<(), Error> {
        unimplemented!("Recording not supported with mpsc channels");
    }
}
