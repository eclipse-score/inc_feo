// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! Connectors for relayed signalling using sockets and mpsc channels
//!
//! - Inter-process signalling is sockets-based (unix sockets or tcp sockets)
//! - Intra-process signalling uses mpsc channels

use crate::ids::{ActivityId, AgentId, ChannelId, RelayId, WorkerId};
use crate::signalling::common::socket::client::{TcpClient, UnixClient};
use crate::signalling::common::socket::server::{TcpServer, UnixServer};
#[cfg(feature = "recording")]
use crate::signalling::relayed::connectors::recorder;
use crate::signalling::relayed::connectors::relays::{
    PrimaryReceiveRelay, PrimarySendRelay, SecondaryReceiveRelay, SecondarySendRelay,
};
use crate::signalling::relayed::connectors::{scheduler, secondary};
use crate::signalling::relayed::interface::{Builder, IsChannel};
use crate::signalling::relayed::sockets::endpoint::HasAddress;
use crate::signalling::relayed::{mpsc, sockets};
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::net::SocketAddr;
use core::time::Duration;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

// The types to be used by client code
#[cfg(feature = "recording")]
pub(crate) type RecorderConnectorTcp = recorder::RecorderConnector<InterChannelTcp>;
pub(crate) type SchedulerConnectorTcp =
    scheduler::SchedulerConnector<InterChannelTcp, IntraChannel>;
pub(crate) type SecondaryConnectorTcp =
    secondary::SecondaryConnector<InterChannelTcp, IntraChannel>;
pub(crate) type WorkerConnector = crate::signalling::common::mpsc::WorkerConnector;
#[cfg(feature = "recording")]
pub(crate) type RecorderConnectorUnix = recorder::RecorderConnector<InterChannelUnix>;
pub(crate) type SchedulerConnectorUnix =
    scheduler::SchedulerConnector<InterChannelUnix, IntraChannel>;
pub(crate) type SecondaryConnectorUnix =
    secondary::SecondaryConnector<InterChannelUnix, IntraChannel>;

/// Mpsc-based intra-process signalling channel implementation
pub(crate) struct IntraChannel;

impl IsChannel for IntraChannel {
    type ProtocolSignal = mpsc::endpoint::ProtocolSignal;
    type Sender = mpsc::endpoint::ProtocolSender;
    type Receiver = mpsc::endpoint::ProtocolReceiver;
    type MultiSender = mpsc::endpoint::ProtocolMultiSender;
    type MultiReceiver = mpsc::endpoint::ProtocolMultiReceiver;
}

/// Tcp-socket-based inter-process signalling channel implementation
pub(crate) struct InterChannelTcp;

impl HasAddress for InterChannelTcp {
    type Address = SocketAddr;
}

impl IsChannel for InterChannelTcp {
    type ProtocolSignal = sockets::endpoint::ProtocolSignal;
    type Sender = sockets::endpoint::ProtocolSender<TcpClient>;
    type Receiver = sockets::endpoint::ProtocolReceiver<TcpClient>;
    type MultiSender = sockets::endpoint::ProtocolMultiSender<TcpServer>;
    type MultiReceiver = sockets::endpoint::ProtocolMultiReceiver<TcpServer>;
}

impl SocketChannel for InterChannelTcp {
    fn new_receiver(address: Self::Address, channel_id: ChannelId) -> Self::Receiver {
        sockets::endpoint::ProtocolReceiver::<TcpClient>::new(address, channel_id)
    }

    fn new_sender(address: Self::Address, channel_id: ChannelId) -> Self::Sender {
        sockets::endpoint::ProtocolSender::<TcpClient>::new(address, channel_id)
    }

    fn new_multi_receiver<'s, T>(channel_ids: &'s T, address: Self::Address) -> Self::MultiReceiver
    where
        &'s T: IntoIterator<Item = &'s ChannelId>,
    {
        sockets::endpoint::ProtocolMultiReceiver::<TcpServer>::new(channel_ids, address)
    }

    fn new_multi_sender<'s, T>(channel_ids: &'s T, address: Self::Address) -> Self::MultiSender
    where
        &'s T: IntoIterator<Item = &'s ChannelId>,
    {
        sockets::endpoint::ProtocolMultiSender::<TcpServer>::new(channel_ids, address)
    }
}

/// Unix-socket-based inter-process signalling channel implementation
pub(crate) struct InterChannelUnix;

impl HasAddress for InterChannelUnix {
    type Address = PathBuf;
}

impl IsChannel for InterChannelUnix {
    type ProtocolSignal = sockets::endpoint::ProtocolSignal;
    type Sender = sockets::endpoint::ProtocolSender<UnixClient>;
    type Receiver = sockets::endpoint::ProtocolReceiver<UnixClient>;
    type MultiSender = sockets::endpoint::ProtocolMultiSender<UnixServer>;
    type MultiReceiver = sockets::endpoint::ProtocolMultiReceiver<UnixServer>;
}

impl SocketChannel for InterChannelUnix {
    fn new_receiver(address: Self::Address, channel_id: ChannelId) -> Self::Receiver {
        sockets::endpoint::ProtocolReceiver::<UnixClient>::new(address, channel_id)
    }

    fn new_sender(address: Self::Address, channel_id: ChannelId) -> Self::Sender {
        sockets::endpoint::ProtocolSender::<UnixClient>::new(address, channel_id)
    }

    fn new_multi_receiver<'s, T>(channel_ids: &'s T, address: Self::Address) -> Self::MultiReceiver
    where
        &'s T: IntoIterator<Item = &'s ChannelId>,
    {
        sockets::endpoint::ProtocolMultiReceiver::<UnixServer>::new(channel_ids, address)
    }

    fn new_multi_sender<'s, T>(channel_ids: &'s T, address: Self::Address) -> Self::MultiSender
    where
        &'s T: IntoIterator<Item = &'s ChannelId>,
    {
        sockets::endpoint::ProtocolMultiSender::<UnixServer>::new(channel_ids, address)
    }
}

#[cfg(feature = "recording")]
impl<Inter: SocketChannel> recorder::RecorderConnector<Inter> {
    pub fn new(
        agent_id: AgentId,
        bind_address_senders: Inter::Address,
        bind_address_receivers: Inter::Address,
        timeout: Duration,
    ) -> Self {
        let inter_sender = Inter::new_sender(bind_address_senders, ChannelId::Agent(agent_id));
        let inter_receiver =
            Inter::new_receiver(bind_address_receivers, ChannelId::Agent(agent_id));

        Self::create(inter_sender, inter_receiver, timeout)
    }
}

impl<Inter: SocketChannel> scheduler::SchedulerConnector<Inter, IntraChannel> {
    /// Create scheduler connector for mixed signalling using sockets and mpsc channels
    ///
    /// #Arguments
    ///
    /// * `agent_id`: The id of the primary agent
    /// * `bind_address_senders`: The address to which secondary agents' senders shall connect
    /// * `bind_address_receivers`: The address to which secondary agents' receivers shall connect
    /// * `timeout`: The connection and reception timeout
    /// * `worker_agent_map`: A map of all worker-ids to the ids of the agents they reside on
    /// * `activity_worker_map`: A map of all activity-ids to the ids of the workers they are assigned to
    /// * `recorders`: A list of the expected recorders' agent id
    pub(crate) fn new(
        agent_id: AgentId,
        bind_address_senders: Inter::Address,
        bind_address_receivers: Inter::Address,
        timeout: Duration,
        worker_agent_map: HashMap<WorkerId, AgentId>,
        activity_worker_map: HashMap<ActivityId, WorkerId>,
        recorders: Vec<AgentId>,
    ) -> Self {
        let recorders: HashSet<AgentId> = recorders.into_iter().collect();

        // Determine local worker IDs
        let local_workers: HashSet<WorkerId> = worker_agent_map
            .iter()
            .filter(|(_, aid)| *aid == &agent_id)
            .map(|(wid, _)| *wid)
            .collect();

        // Determine remote agent Ids
        let remote_agents: HashSet<AgentId> = worker_agent_map
            .values()
            .copied()
            .filter(|id| id != &agent_id)
            .chain(recorders.iter().copied())
            .collect();

        // Determine channels to local workers plus one to relay
        let relay_channel = ChannelId::Relay(RelayId::new(0));
        let relay_channels = [relay_channel];
        let local_worker_channels: Vec<ChannelId> = local_workers
            .iter()
            .map(|id| ChannelId::Worker(*id))
            .collect();
        let local_channels: Vec<ChannelId> = local_worker_channels
            .iter()
            .chain(relay_channels.iter())
            .copied()
            .collect();

        // Create an intra-process receiver and an intra-process sender for receiving and
        // sending signals from and to local and remote workers
        let (intra_receiver_builder, mut local_sender_builders) =
            intra_builders::multi_receiver_builder(&local_channels);
        let intra_receiver = intra_receiver_builder();

        // Create an intra-process sender to the workers
        let (worker_sender_builder, mut worker_receiver_builders) =
            intra_builders::multi_sender_builder(&local_worker_channels);
        let worker_sender = worker_sender_builder();

        // Create an IPC relay for receiving from remote processes
        let channel_ids: Vec<ChannelId> = remote_agents
            .iter()
            .copied()
            .map(ChannelId::Agent)
            .collect();

        let relay_sender_builder = local_sender_builders.remove(&relay_channel).unwrap();
        let relay_receiver_builder =
            Inter::multi_receiver_builder(channel_ids.clone(), bind_address_senders);
        let ipc_receive_relay =
            PrimaryReceiveRelay::new(relay_sender_builder, relay_receiver_builder, timeout);

        let ipc_sender = Inter::new_multi_sender(&channel_ids, bind_address_receivers);

        let ipc_send_relay = PrimarySendRelay::new(remote_agents, ipc_sender, timeout);

        // Create worker connector builders for local workers
        let worker_connector_builders: HashMap<WorkerId, Builder<WorkerConnector>> =
            local_worker_channels
                .iter()
                .map(|id| {
                    let worker_id = match *id {
                        ChannelId::Worker(wid) => wid,
                        other => {
                            panic!("unexpected channel id {other:?}");
                        }
                    };
                    let worker_sender_builder = local_sender_builders.remove(id).unwrap();
                    let worker_receiver_builder = worker_receiver_builders.remove(id).unwrap();
                    let connector_builder = intra_builders::worker_connector_builder(
                        worker_sender_builder,
                        worker_receiver_builder,
                    );
                    (worker_id, connector_builder)
                })
                .collect();

        Self::with_fields(
            local_workers,
            intra_receiver,
            ipc_receive_relay,
            ipc_send_relay,
            worker_sender,
            timeout,
            Some(worker_connector_builders),
            activity_worker_map,
            worker_agent_map,
        )
    }
}

type ChannelToSenderBuilderMap<Intra> = HashMap<ChannelId, Builder<<Intra as IsChannel>::Sender>>;
type ChannelToReceiverBuilderMap<Intra> =
    HashMap<ChannelId, Builder<<Intra as IsChannel>::Receiver>>;

/// Implementation of specialized methods of SecondaryConnector for
/// mpsc-based intra-process communication and socket-based inter-process communication
impl<Inter: SocketChannel> secondary::SecondaryConnector<Inter, IntraChannel> {
    pub fn create(
        agent_id: AgentId,
        activity_worker_map: HashMap<ActivityId, WorkerId>,
        bind_address_senders: Inter::Address,
        bind_address_receivers: Inter::Address,
        timeout: Duration,
    ) -> (Self, HashMap<WorkerId, Builder<WorkerConnector>>) {
        let worker_ids: HashSet<WorkerId> = activity_worker_map.values().copied().collect();

        // Create a secondary connector and builders of the related channel endpoints for the workers
        let (secondary_connector, mut sender_builders, mut receiver_builders) =
            Self::create_connector_and_endpoint_builders(
                agent_id,
                activity_worker_map,
                bind_address_senders,
                bind_address_receivers,
                timeout,
            );

        // create the worker connector builders using the channel endpoint builders
        let connector_builders: HashMap<WorkerId, Builder<WorkerConnector>> = worker_ids
            .iter()
            .map(|wid| {
                let sender_builder = sender_builders
                    .remove(&ChannelId::Worker(*wid))
                    .unwrap_or_else(|| panic!("missing worker id {wid}"));
                let receiver_builder = receiver_builders
                    .remove(&ChannelId::Worker(*wid))
                    .unwrap_or_else(|| panic!("missing worker id {wid}"));
                let connector_builder =
                    intra_builders::worker_connector_builder(sender_builder, receiver_builder);
                (*wid, connector_builder)
            })
            .collect();
        (secondary_connector, connector_builders)
    }

    fn create_connector_and_endpoint_builders(
        agent_id: AgentId,
        activity_worker_map: HashMap<ActivityId, WorkerId>,
        bind_address_senders: Inter::Address,
        bind_address_receivers: Inter::Address,
        timeout: Duration,
    ) -> (
        Self,
        ChannelToSenderBuilderMap<IntraChannel>,
        ChannelToReceiverBuilderMap<IntraChannel>,
    ) {
        let channel_ids: HashSet<ChannelId> = activity_worker_map
            .values()
            .map(|id| ChannelId::Worker(*id))
            .collect();

        let (intra_receiver_builder, worker_sender_builders) =
            intra_builders::multi_receiver_builder(&channel_ids);
        let intra_receiver = intra_receiver_builder();
        let inter_sender = Inter::new_sender(bind_address_senders, ChannelId::Agent(agent_id));

        let local_to_ipc_relay = SecondarySendRelay::new(inter_sender, intra_receiver, timeout);

        let (intra_sender_builder, worker_receiver_builders) =
            intra_builders::multi_sender_builder(&channel_ids);
        let inter_receiver_builder =
            Inter::receiver_builder(ChannelId::Agent(agent_id), bind_address_receivers);

        let ipc_to_local_relay = SecondaryReceiveRelay::new(
            activity_worker_map,
            intra_sender_builder,
            inter_receiver_builder,
            timeout,
        );

        (
            Self::with_fields(local_to_ipc_relay, ipc_to_local_relay),
            worker_sender_builders,
            worker_receiver_builders,
        )
    }
}

pub(crate) trait SocketChannel: IsChannel + HasAddress {
    fn new_receiver(address: Self::Address, channel_id: ChannelId) -> Self::Receiver;

    fn new_sender(address: Self::Address, channel_id: ChannelId) -> Self::Sender;

    fn new_multi_receiver<'s, T>(channel_ids: &'s T, address: Self::Address) -> Self::MultiReceiver
    where
        &'s T: IntoIterator<Item = &'s ChannelId>;

    fn new_multi_sender<'s, T>(channel_ids: &'s T, address: Self::Address) -> Self::MultiSender
    where
        &'s T: IntoIterator<Item = &'s ChannelId>;

    /// Returns a builder of a ProtocolMultiReceiver
    fn multi_receiver_builder<T>(
        channel_ids: T,
        address: Self::Address,
    ) -> Builder<Self::MultiReceiver>
    where
        T: IntoIterator<Item = ChannelId> + Send,
    {
        let cids: Vec<ChannelId> = channel_ids.into_iter().collect();
        Box::new(move || Self::new_multi_receiver(&cids, address)) as Builder<Self::MultiReceiver>
    }

    // Returns a builder of a receiver
    fn receiver_builder(channel_id: ChannelId, address: Self::Address) -> Builder<Self::Receiver> {
        Box::new(move || Self::new_receiver(address, channel_id))
    }
}

mod intra_builders {
    use super::*;

    /// Returns a builder of a ProtocolMultiReceiver and builders of corresponding ProtocolSenders
    pub fn multi_receiver_builder<'s, T>(
        channel_ids: &'s T,
    ) -> (
        Builder<<IntraChannel as IsChannel>::MultiReceiver>,
        ChannelToSenderBuilderMap<IntraChannel>,
    )
    where
        &'s T: IntoIterator<Item = &'s ChannelId>,
    {
        // As mpsc senders and receivers do implement the Send trait, we can simply create
        // the target objects here and move them into the builder objects
        let (receiver, senders) = <IntraChannel as IsChannel>::MultiReceiver::create(channel_ids);
        let receiver_builder =
            Box::new(move || receiver) as Builder<<IntraChannel as IsChannel>::MultiReceiver>;
        let sender_builders: HashMap<ChannelId, Builder<<IntraChannel as IsChannel>::Sender>> =
            senders
                .into_iter()
                .map(|(id, sender)| (id, <IntraChannel as IsChannel>::Sender::new(id, sender)))
                .map(|(id, sender)| {
                    (
                        id,
                        Box::new(move || sender) as Builder<<IntraChannel as IsChannel>::Sender>,
                    )
                })
                .collect();

        (receiver_builder, sender_builders)
    }

    /// Returns a builder of a ProtocolMultiSender and builders of corresponding ProtocolReceivers
    pub(crate) fn multi_sender_builder<'s, T>(
        channel_ids: &'s T,
    ) -> (
        Builder<<IntraChannel as IsChannel>::MultiSender>,
        ChannelToReceiverBuilderMap<IntraChannel>,
    )
    where
        &'s T: IntoIterator<Item = &'s ChannelId>,
    {
        // As mpsc senders and receivers *do* implement the Send trait, we can simply create
        // the target objects here and move them into the builder objects
        let (sender, receivers) = <IntraChannel as IsChannel>::MultiSender::create(channel_ids);
        let sender_builder =
            Box::new(move || sender) as Builder<<IntraChannel as IsChannel>::MultiSender>;
        let receiver_builders: HashMap<ChannelId, Builder<<IntraChannel as IsChannel>::Receiver>> =
            receivers
                .into_iter()
                .map(|(id, receiver)| (id, <IntraChannel as IsChannel>::Receiver::new(receiver)))
                .map(|(id, receiver)| {
                    (
                        id,
                        Box::new(move || receiver)
                            as Builder<<IntraChannel as IsChannel>::Receiver>,
                    )
                })
                .collect();

        (sender_builder, receiver_builders)
    }

    /// Returns a builder of a worker connector
    pub(crate) fn worker_connector_builder(
        sender_builder: Builder<<IntraChannel as IsChannel>::Sender>,
        receiver_builder: Builder<<IntraChannel as IsChannel>::Receiver>,
    ) -> Builder<WorkerConnector> {
        // As mpsc senders and receivers *do* implement the Send trait, we can simply create
        // the target objects here and move them into the builder objects
        let sender = sender_builder();
        let receiver = receiver_builder();
        let builder = move || WorkerConnector::new(sender, receiver);
        Box::new(builder) as Builder<WorkerConnector>
    }
}
