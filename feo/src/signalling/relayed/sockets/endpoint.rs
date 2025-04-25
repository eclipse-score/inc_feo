// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! Communication endpoints for socket-based signalling

use crate::error::Error;
use crate::ids::ChannelId;
use crate::signalling::common::signals::Signal;
use crate::signalling::common::socket::client::{TcpClient, UnixClient};
use crate::signalling::common::socket::server::{TcpServer, UnixServer};
// Re-use protocol signal definition from socket building blocks
pub(crate) use crate::signalling::common::socket::ProtocolSignal;
use crate::signalling::relayed;
use core::net::SocketAddr;
use core::time::Duration;
use feo_log::{debug, trace};
use feo_time::Instant;
use mio::{Events, Token};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

const EVENTS_CAPACITY: usize = 128;

/// ProtocolSender using client type C (TcpClient, UnixClient, possibly others in future)
pub(crate) type ProtocolSender<C> = ProtocolEndpoint<C>;

/// ProtocolReceiver using client type C (TcpClient, UnixClient, possibly others in future)
pub(crate) type ProtocolReceiver<C> = ProtocolEndpoint<C>;

/// ProtocolMultiSender using server type S (TcpServer, UnixServer, possibly others in future)
pub(crate) type ProtocolMultiSender<S> = ProtocolMultiEndpoint<S>;

/// ProtocolMultiReceiver using server type S (TcpServer, UnixServer, possibly others in future)
pub(crate) type ProtocolMultiReceiver<R> = ProtocolMultiEndpoint<R>;

/// Required shared socket server functionality
pub trait IsServer: HasAddress {
    fn create(address: &Self::Address) -> Self;

    fn send(&mut self, token: &Token, msg: &ProtocolSignal) -> Result<(), Error>;

    fn receive(
        &mut self,
        events: &mut Events,
        timeout: Duration,
    ) -> Option<(Token, ProtocolSignal)>;
}

/// Required shared socket client functionality
pub trait IsClient: HasAddress {
    fn send(&mut self, msg: &ProtocolSignal) -> Result<(), Error>;

    fn receive(&mut self, events: &mut Events, timeout: Duration) -> Option<ProtocolSignal>;

    fn connect(address: &Self::Address, channel_id: ChannelId) -> Self;
}

/// Trait for types having an address (Socket address, Unix path)
pub trait HasAddress {
    type Address: Send;
}

/// A single-channel sender/receiver, connecting as client of type C
pub(crate) struct ProtocolEndpoint<C: IsClient> {
    channel_id: ChannelId,
    address: C::Address,
    events: Events,
    client: Option<C>,
}

impl<C: IsClient> ProtocolEndpoint<C> {
    pub fn new(socket_addr: C::Address, channel_id: ChannelId) -> Self {
        Self {
            channel_id,
            address: socket_addr,
            events: Events::with_capacity(EVENTS_CAPACITY),
            client: None,
        }
    }

    pub fn send(&mut self, signal: ProtocolSignal) -> Result<(), Error> {
        self.client
            .as_mut()
            .expect("client not connected")
            .send(&signal)
    }

    /// Try to receive data
    ///
    /// Waits for incoming data or until the timeout has been reached.
    pub fn receive(&mut self, timeout: Duration) -> Result<Option<ProtocolSignal>, Error> {
        let start = Instant::now();
        let client = self.client.as_mut().expect("not connected");

        // in case of early return from tcp client, loop until timeout reached
        let mut elapsed = Duration::ZERO;
        while elapsed <= timeout {
            let remaining = timeout.saturating_sub(elapsed);
            if let Some(signal) = client.receive(&mut self.events, remaining) {
                return Ok(Some(signal));
            }
            elapsed = start.elapsed();
        }
        Ok(None)
    }

    pub fn connect(&mut self, _timeout: Duration) -> Result<(), Error> {
        let client = C::connect(&self.address, self.channel_id);
        self.client = Some(client);
        Ok(())
    }
}

impl<C: IsClient> HasAddress for ProtocolEndpoint<C> {
    type Address = C::Address;
}

/// A TCP multi-channel sender/receiver, acting as server of type S
pub(crate) struct ProtocolMultiEndpoint<S: IsServer> {
    channel_ids: HashSet<ChannelId>,
    events: Events,
    channel_token_map: HashMap<ChannelId, Token>,
    bind_address: S::Address,
    server: Option<S>,
}

impl<S: IsServer> ProtocolMultiEndpoint<S> {
    pub fn new<'s, T>(channel_ids: &'s T, bind_address: S::Address) -> Self
    where
        &'s T: IntoIterator<Item = &'s ChannelId>,
    {
        let channel_ids: HashSet<_> = channel_ids.into_iter().copied().collect();

        Self {
            channel_ids,
            events: Events::with_capacity(EVENTS_CAPACITY),
            channel_token_map: Default::default(),
            bind_address,
            server: None,
        }
    }

    pub fn connect(&mut self, timeout: Duration) -> Result<(), Error> {
        // Fail, if already connected
        assert!(self.server.is_none(), "already connected");

        // Create tcp server and start listening
        let server = S::create(&self.bind_address);
        self.server = Some(server);
        let server = self.server.as_mut().unwrap();

        // TODO: timeout handling
        let mut missing_peers: HashSet<ChannelId> = self.channel_ids.clone();
        while !missing_peers.is_empty() {
            trace!("Connecting missing channels {:?}", missing_peers);
            // TODO: server to reject connections from unexpected peers
            if let Some((token, signal)) = server.receive(&mut self.events, timeout) {
                match signal {
                    ProtocolSignal::ChannelHello(channel_id) => {
                        self.channel_token_map.insert(channel_id, token);
                        missing_peers.remove(&channel_id);
                        debug!("Channel connected {channel_id:?}");
                    }
                    _ => {
                        return Err(Error::UnexpectedProtocolSignal);
                    }
                }
            }
        }
        Ok(())
    }

    /// Try to receive data
    ///
    /// Waits for incoming data or until the timeout has been reached.
    pub fn receive(&mut self, timeout: Duration) -> Result<Option<ProtocolSignal>, Error> {
        let start = Instant::now();
        let server = self.server.as_mut().expect("not connected");

        // in case of early return from tcp client, loop until timeout reached
        let mut elapsed = Duration::ZERO;
        while elapsed <= timeout {
            let remaining = timeout.saturating_sub(elapsed);
            if let Some(signal) = server.receive(&mut self.events, remaining).map(|(_, s)| s) {
                return Ok(Some(signal));
            }
            elapsed = start.elapsed();
        }
        Ok(None)
    }

    pub fn send(&mut self, channel_id: ChannelId, signal: ProtocolSignal) -> Result<(), Error> {
        let token = self
            .channel_token_map
            .get(&channel_id)
            .unwrap_or_else(|| panic!("failed to find channel {channel_id:?}"));
        self.server
            .as_mut()
            .expect("not connected")
            .send(token, &signal)
    }
}

impl<S: IsServer> HasAddress for ProtocolMultiEndpoint<S> {
    type Address = S::Address;
}

impl HasAddress for TcpServer {
    type Address = SocketAddr;
}

impl IsServer for TcpServer {
    fn create(address: &Self::Address) -> Self {
        Self::new(*address)
    }

    fn send(&mut self, token: &Token, msg: &ProtocolSignal) -> Result<(), Error> {
        self.send(token, msg).map_err(|e| e.into())
    }

    fn receive(
        &mut self,
        events: &mut Events,
        timeout: Duration,
    ) -> Option<(Token, ProtocolSignal)> {
        self.receive(events, timeout)
    }
}

impl<C: IsClient> relayed::interface::ProtocolSend for ProtocolSender<C> {
    type ProtocolSignal = ProtocolSignal;

    fn send(&mut self, signal: Self::ProtocolSignal) -> Result<(), Error> {
        self.send(signal)
    }

    fn connect_receiver(&mut self, timeout: Duration) -> Result<(), Error> {
        self.connect(timeout)
    }
}

impl<C: IsClient> relayed::interface::ProtocolRecv for ProtocolReceiver<C> {
    type ProtocolSignal = ProtocolSignal;

    fn receive(&mut self, timeout: Duration) -> Result<Option<Self::ProtocolSignal>, Error> {
        self.receive(timeout)
    }

    fn connect_sender(&mut self, timeout: Duration) -> Result<(), Error> {
        self.connect(timeout)
    }
}

impl<S: IsServer> relayed::interface::ProtocolMultiSend for ProtocolMultiSender<S> {
    type ProtocolSignal = ProtocolSignal;

    fn send(&mut self, channel_id: ChannelId, signal: Self::ProtocolSignal) -> Result<(), Error> {
        self.send(channel_id, signal)
    }

    fn connect_receivers(&mut self, timeout: Duration) -> Result<(), Error> {
        self.connect(timeout)
    }
}

impl<S: IsServer> relayed::interface::ProtocolMultiRecv for ProtocolMultiReceiver<S> {
    type ProtocolSignal = ProtocolSignal;

    fn receive(&mut self, timeout: Duration) -> Result<Option<Self::ProtocolSignal>, Error> {
        self.receive(timeout)
    }

    fn connect_senders(&mut self, timeout: Duration) -> Result<(), Error> {
        self.connect(timeout)
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

impl HasAddress for UnixServer {
    type Address = PathBuf;
}

impl IsServer for UnixServer {
    fn create(address: &Self::Address) -> Self {
        Self::new(address)
    }

    fn send(&mut self, token: &Token, msg: &ProtocolSignal) -> Result<(), Error> {
        self.send(token, msg).map_err(|e| e.into())
    }

    fn receive(
        &mut self,
        events: &mut Events,
        timeout: Duration,
    ) -> Option<(Token, ProtocolSignal)> {
        self.receive(events, timeout)
    }
}

impl HasAddress for TcpClient {
    type Address = SocketAddr;
}

impl IsClient for TcpClient {
    fn send(&mut self, msg: &ProtocolSignal) -> Result<(), Error> {
        self.send(msg)
    }

    fn receive(&mut self, events: &mut Events, timeout: Duration) -> Option<ProtocolSignal> {
        self.receive(events, timeout)
    }

    fn connect(address: &Self::Address, channel_id: ChannelId) -> Self {
        Self::connect([ProtocolSignal::ChannelHello(channel_id)], *address)
    }
}

impl HasAddress for UnixClient {
    type Address = PathBuf;
}

impl IsClient for UnixClient {
    fn send(&mut self, msg: &ProtocolSignal) -> Result<(), Error> {
        self.send(msg)
    }

    fn receive(&mut self, events: &mut Events, timeout: Duration) -> Option<ProtocolSignal> {
        self.receive(events, timeout)
    }

    fn connect(address: &Self::Address, channel_id: ChannelId) -> Self {
        Self::connect([ProtocolSignal::ChannelHello(channel_id)], address)
    }
}
