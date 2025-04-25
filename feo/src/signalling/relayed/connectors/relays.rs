// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! Relays transferring signals between intra- and inter-process channels

use crate::error::Error;
use crate::ids::{ActivityId, AgentId, ChannelId, WorkerId};
use crate::signalling::common::signals::Signal;
use crate::signalling::relayed::interface::{
    Builder, IsChannel, ProtocolMultiRecv, ProtocolMultiSend, ProtocolRecv, ProtocolSend,
};
use crate::timestamp;
use crate::timestamp::sync_info;
use core::time::Duration;
use feo_log::{debug, error, trace};
use std::collections::{HashMap, HashSet};
use std::thread;

/// Relay for the primary agent to receive signals from secondary agents
pub struct PrimaryReceiveRelay<Inter: IsChannel, Intra: IsChannel> {
    intra_sender_builder: Option<Builder<Intra::Sender>>,
    inter_receiver_builder: Option<Builder<Inter::MultiReceiver>>,
    timeout: Duration,
    _thread: Option<thread::JoinHandle<()>>,
}

impl<Inter: IsChannel, Intra: IsChannel> PrimaryReceiveRelay<Inter, Intra> {
    pub fn new(
        intra_sender_builder: Builder<Intra::Sender>,
        inter_receiver_builder: Builder<Inter::MultiReceiver>,
        timeout: Duration,
    ) -> Self {
        Self {
            intra_sender_builder: Some(intra_sender_builder),
            inter_receiver_builder: Some(inter_receiver_builder),
            timeout,
            _thread: None,
        }
    }

    pub fn run_and_connect(&mut self) {
        let inter_receiver_builder = self.inter_receiver_builder.take().unwrap();
        let intra_sender_builder = self.intra_sender_builder.take().unwrap();
        let timeout = self.timeout;
        let thread = thread::spawn(move || {
            Self::thread_main(inter_receiver_builder, intra_sender_builder, timeout)
        });
        self._thread = Some(thread);
    }

    fn thread_main(
        inter_receiver_builder: Builder<Inter::MultiReceiver>,
        intra_sender_builder: Builder<Intra::Sender>,
        timeout: Duration,
    ) {
        trace!("PrimaryReceiveRelay thread started");
        let mut inter_receiver = inter_receiver_builder();
        let mut intra_sender = intra_sender_builder();
        inter_receiver
            .connect_senders(timeout)
            .expect("failed to connect inter-process receiver");
        intra_sender
            .connect_receiver(timeout)
            .expect("failed to connect intra-process sender");
        trace!("PrimaryReceiveRelay connected");
        loop {
            // Receive from remote workers on inter-process receiver
            let signal = inter_receiver.receive(timeout);

            let signal = match signal {
                Ok(Some(signal)) => signal,
                Ok(None) => {
                    error!("Reception timed out");
                    continue;
                }
                Err(_) => {
                    error!("Failed to receive");
                    continue;
                }
            };

            let signal: Signal = match signal.try_into() {
                Ok(signal) => signal,
                Err(_) => {
                    error!("Received unexpected signal {signal:?}");
                    continue;
                }
            };

            // Forward onto intra-process connection
            let protocol_signal: Intra::ProtocolSignal = signal.into();
            let result = intra_sender.send(protocol_signal);
            if result.is_err() {
                error!("Failed to send signal {protocol_signal:?}");
            }
        }
    }
}

/// Relay for the primary agent to send signals to secondary agents and recorders
pub struct PrimarySendRelay<Inter: IsChannel> {
    /// Set of all remote agents (secondaries and recorders)
    remote_agents: HashSet<AgentId>,
    /// Inter-process sender
    inter_sender: Inter::MultiSender,
    /// Connecting timeout
    timeout: Duration,
}

impl<Inter: IsChannel> PrimarySendRelay<Inter> {
    pub fn new(
        remote_agents: HashSet<AgentId>,
        inter_sender: Inter::MultiSender,
        timeout: Duration,
    ) -> Self {
        Self {
            remote_agents,
            inter_sender,
            timeout,
        }
    }

    pub fn connect(&mut self) -> Result<(), Error> {
        self.inter_sender.connect_receivers(self.timeout)?;
        trace!("PrimarySendRelay connected");
        Ok(())
    }

    pub fn send_to_agent(
        &mut self,
        agent_id: AgentId,
        signal: Inter::ProtocolSignal,
    ) -> Result<(), Error> {
        let channel_id = ChannelId::Agent(agent_id);
        self.inter_sender.send(channel_id, signal)
    }

    pub fn sync_time(&mut self) -> Result<(), Error> {
        let signal = Signal::StartupSync(sync_info());

        // Send sync info to all remote agents
        for id in self.remote_agents.iter() {
            let channel_id = ChannelId::Agent(*id);
            self.inter_sender.send(channel_id, signal.into())?;
        }
        Ok(())
    }
}

/// Relay for a secondary to receive signals from the primary agent
pub struct SecondaryReceiveRelay<Inter: IsChannel, Intra: IsChannel> {
    inter_receiver_builder: Option<Builder<Inter::Receiver>>,
    intra_sender_builder: Option<Builder<Intra::MultiSender>>,
    activity_worker_map: HashMap<ActivityId, WorkerId>,
    timeout: Duration,
    _thread: Option<thread::JoinHandle<()>>,
}

impl<Inter: IsChannel, Intra: IsChannel> SecondaryReceiveRelay<Inter, Intra> {
    pub fn new(
        activity_worker_map: HashMap<ActivityId, WorkerId>,
        intra_sender_builder: Builder<<Intra as IsChannel>::MultiSender>,
        inter_receiver_builder: Builder<<Inter as IsChannel>::Receiver>,
        timeout: Duration,
    ) -> Self {
        Self {
            inter_receiver_builder: Some(inter_receiver_builder),
            intra_sender_builder: Some(intra_sender_builder),
            activity_worker_map,
            timeout,
            _thread: None,
        }
    }

    pub fn run_and_connect(&mut self) {
        let inter_receiver_builder = self.inter_receiver_builder.take().unwrap();
        let intra_sender_builder = self.intra_sender_builder.take().unwrap();
        let activity_worker_map = self.activity_worker_map.clone();
        let timeout = self.timeout;
        let thread = thread::spawn(move || {
            Self::thread_main(
                intra_sender_builder,
                inter_receiver_builder,
                activity_worker_map,
                timeout,
            )
        });
        self._thread = Some(thread);
    }

    fn receive_helper(
        receiver: &mut Inter::Receiver,
        timeout: Duration,
    ) -> Option<Inter::ProtocolSignal> {
        let received = receiver.receive(timeout);
        match received {
            Ok(Some(s)) => Some(s),
            Ok(None) => {
                error!("Reception timed out");
                None
            }
            Err(_) => {
                error!("Failed to receive");
                None
            }
        }
    }

    fn thread_main(
        intra_sender_builder: Builder<Intra::MultiSender>,
        inter_receiver_builder: Builder<Inter::Receiver>,
        activity_worker_map: HashMap<ActivityId, WorkerId>,
        timeout: Duration,
    ) {
        trace!("SecondaryReceiveRelay thread started");
        let mut intra_sender = intra_sender_builder();
        let mut inter_receiver = inter_receiver_builder();

        intra_sender
            .connect_receivers(timeout)
            .expect("failed to connect intra-process sender");
        inter_receiver
            .connect_sender(timeout)
            .expect("failed to connect inter-process receiver");
        trace!("SecondaryReceiveRelay connected, waiting for time synchronization");

        // Wait for startup sync
        loop {
            // Receive signal on inter-process connection
            let Some(protocol_signal) = Self::receive_helper(&mut inter_receiver, timeout) else {
                continue;
            };

            let sync_info = match protocol_signal.try_into() {
                Ok(Signal::StartupSync(s)) => s,
                Ok(_) | Err(_) => {
                    error!("Unexpected signal {protocol_signal:?}");
                    continue;
                }
            };

            timestamp::initialize_from(sync_info);
            break;
        }
        debug!("Time synchronization done");

        loop {
            // Receive signal on inter-process connection
            let Some(protocol_signal) = Self::receive_helper(&mut inter_receiver, timeout) else {
                continue;
            };

            let core_signal: Signal = match protocol_signal.try_into() {
                Ok(signal) => signal,
                Err(_) => {
                    error!("Received unexpected signal {protocol_signal:?}");
                    continue;
                }
            };

            // Check signal type and extract activity ID
            let act_id = match core_signal {
                Signal::Startup((act_id, _)) => act_id,
                Signal::Step((act_id, _)) => act_id,
                Signal::Shutdown((act_id, _)) => act_id,
                other => {
                    error!("Received unexpected signal {other:?}");
                    continue;
                }
            };

            // Lookup corresponding worker id
            let Some(worker_id) = activity_worker_map.get(&act_id) else {
                error!("Received unexpected activity id {act_id:?} in {core_signal:?}");
                continue;
            };

            // Forward signal to determined worker
            let channel_id = ChannelId::Worker(*worker_id);
            let protocol_signal: Intra::ProtocolSignal = core_signal.into();
            let result = intra_sender.send(channel_id, protocol_signal);
            if result.is_err() {
                error!("Failed to send signal {protocol_signal:?}");
            }
        }
    }
}

/// Relay for a secondary to send signals to the primary agent
pub struct SecondarySendRelay<Inter: IsChannel, Intra: IsChannel> {
    inter_sender: Inter::Sender,
    intra_receiver: Intra::MultiReceiver,
    timeout: Duration,
}

impl<Inter: IsChannel, Intra: IsChannel> SecondarySendRelay<Inter, Intra> {
    pub fn new(
        inter_sender: Inter::Sender,
        intra_receiver: Intra::MultiReceiver,
        timeout: Duration,
    ) -> Self {
        Self {
            inter_sender,
            intra_receiver,
            timeout,
        }
    }

    pub fn connect(&mut self) -> Result<(), Error> {
        self.intra_receiver.connect_senders(self.timeout)?;
        self.inter_sender.connect_receiver(self.timeout)?;
        trace!("SecondarySendRelay connected");
        Ok(())
    }

    pub fn run(&mut self) {
        loop {
            // Receive signal from the intra-process receiver
            let signal = self.intra_receiver.receive(self.timeout);

            let signal = match signal {
                Ok(Some(signal)) => signal,
                Ok(None) => {
                    error!("Reception timed out");
                    continue;
                }
                Err(_) => {
                    error!("Failed to receive");
                    continue;
                }
            };

            // Forward signal to the primary agent
            let core_signal: Signal = match signal.try_into() {
                Ok(signal) => signal,
                Err(_) => {
                    error!("Received unexpected signal {signal:?}");
                    continue;
                }
            };

            let protocol_signal: Inter::ProtocolSignal = core_signal.into();
            let result = self.inter_sender.send(protocol_signal);
            if result.is_err() {
                error!("Failed to send signal {protocol_signal:?}");
            }
        }
    }
}
