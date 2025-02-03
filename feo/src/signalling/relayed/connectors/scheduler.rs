// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use crate::error::Error;
use crate::ids::{ActivityId, AgentId, ChannelId, WorkerId};
use crate::signalling::common::interface::ConnectScheduler;
// Re-export for convenience
use crate::signalling::common::mpsc::{WorkerConnector, WorkerConnectorBuilder};
use crate::signalling::common::signals::Signal;
use crate::signalling::relayed::connectors::relays::{PrimaryReceiveRelay, PrimarySendRelay};
use crate::signalling::relayed::interface::{
    Builder, IsChannel, ProtocolMultiRecv, ProtocolMultiSend,
};
use core::time::Duration;
use feo_log::debug;
use std::collections::{HashMap, HashSet};

pub(crate) struct SchedulerConnector<Inter: IsChannel, Intra: IsChannel> {
    local_workers: HashSet<WorkerId>,
    intra_receiver: Intra::MultiReceiver,
    ipc_receive_relay: PrimaryReceiveRelay<Inter, Intra>,
    ipc_send_relay: PrimarySendRelay<Inter>,
    worker_sender: Intra::MultiSender,
    timeout: Duration,
    worker_connector_builders: Option<HashMap<WorkerId, Builder<WorkerConnector>>>,
    activity_worker_map: HashMap<ActivityId, WorkerId>,
    worker_agent_map: HashMap<WorkerId, AgentId>,
}

impl<Inter: IsChannel, Intra: IsChannel> SchedulerConnector<Inter, Intra> {
    #[allow(clippy::too_many_arguments)]
    pub fn with_fields(
        local_workers: HashSet<WorkerId>,
        intra_receiver: Intra::MultiReceiver,
        ipc_receive_relay: PrimaryReceiveRelay<Inter, Intra>,
        ipc_send_relay: PrimarySendRelay<Inter>,
        worker_sender: Intra::MultiSender,
        timeout: Duration,
        worker_connector_builders: Option<HashMap<WorkerId, Builder<WorkerConnector>>>,
        activity_worker_map: HashMap<ActivityId, WorkerId>,
        worker_agent_map: HashMap<WorkerId, AgentId>,
    ) -> Self {
        Self {
            local_workers,
            intra_receiver,
            ipc_receive_relay,
            ipc_send_relay,
            worker_sender,
            timeout,
            worker_connector_builders,
            activity_worker_map,
            worker_agent_map,
        }
    }

    pub fn send_to_agent(
        &mut self,
        agent_id: AgentId,
        signal: Inter::ProtocolSignal,
    ) -> Result<(), Error> {
        self.ipc_send_relay.send_to_agent(agent_id, signal)
    }

    // Relay signal onto inter-process connector
    pub fn send_to_worker(&mut self, worker_id: WorkerId, signal: Signal) -> Result<(), Error> {
        // Forward signal to local worker or to remote agent
        if self.local_workers.contains(&worker_id) {
            self.worker_sender
                .send(ChannelId::Worker(worker_id), signal.into())
        } else {
            let Some(agent_id) = self.worker_agent_map.get(&worker_id) else {
                return Err(Error::WorkerNotFound(worker_id));
            };
            self.send_to_agent(*agent_id, signal.into())
        }
    }

    pub fn send_to_activity(
        &mut self,
        activity_id: ActivityId,
        signal: Signal,
    ) -> Result<(), Error> {
        if let Some(worker_id) = self.activity_worker_map.get(&activity_id) {
            self.send_to_worker(*worker_id, signal)
        } else {
            Err(Error::ActivityNotFound(activity_id))
        }
    }

    pub fn run_and_connect(&mut self) -> Result<(), Error> {
        debug!("Starting MixedSchedulerConnector");
        self.ipc_receive_relay.run_and_connect();
        self.ipc_send_relay.connect()?;
        self.intra_receiver.connect_senders(self.timeout)?;
        self.worker_sender.connect_receivers(self.timeout)
    }

    pub fn worker_connector_builders(&mut self) -> HashMap<WorkerId, WorkerConnectorBuilder> {
        self.worker_connector_builders.take().unwrap()
    }

    fn sync_time(&mut self) -> Result<(), Error> {
        self.ipc_send_relay.sync_time()
    }
}

impl<Inter: IsChannel, Intra: IsChannel> ConnectScheduler for SchedulerConnector<Inter, Intra> {
    fn connect_remotes(&mut self) -> Result<(), Error> {
        self.run_and_connect()
    }

    fn sync_time(&mut self) -> Result<(), Error> {
        self.sync_time()
    }

    fn receive(&mut self, timeout: Duration) -> Result<Option<Signal>, Error> {
        let received = self.intra_receiver.receive(timeout);
        let signal: Option<Signal> = match received {
            Ok(Some(s)) => Some(s.try_into().map_err(|_| Error::UnexpectedProtocolSignal)?),
            Ok(None) => None,
            Err(e) => {
                return Err(e);
            }
        };
        Ok(signal)
    }

    fn send_to_activity(&mut self, activity_id: ActivityId, signal: &Signal) -> Result<(), Error> {
        self.send_to_activity(activity_id, *signal)
    }

    fn send_to_recorder(&mut self, recorder_id: AgentId, signal: &Signal) -> Result<(), Error> {
        self.send_to_agent(recorder_id, (*signal).into())
    }
}
