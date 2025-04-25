// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use crate::error::Error;
use crate::ids::AgentId;
use crate::signalling::common::interface::ConnectRecorder;
use crate::signalling::common::signals::Signal;
use crate::signalling::common::socket::client::{SocketClient, TcpClient, UnixClient};
use crate::signalling::common::socket::ProtocolSignal;
use core::net::SocketAddr;
use core::time::Duration;
use feo_log::warn;
use mio::net::{TcpStream, UnixStream};
use mio::Events;
use std::io;
use std::path::PathBuf;

/// TCP based connector for a recorder
pub(crate) type TcpRecorderConnector = RecorderConnector<SocketAddr, TcpStream>;

/// Unix socket based connector for a recorder
pub(crate) type UnixRecorderConnector = RecorderConnector<PathBuf, UnixStream>;

/// Connector for a recorder
pub(crate) struct RecorderConnector<E, S>
where
    S: io::Read + io::Write,
{
    /// ID of the recorder
    recorder_id: AgentId,
    /// Endpoint on which the connector of the scheduler is listening
    endpoint: E,
    /// Pre-allocated events buffer
    events: Events,
    /// Wrapped socket client
    client: Option<SocketClient<S>>,
}

impl<E, S> RecorderConnector<E, S>
where
    S: io::Read + io::Write,
{
    fn receive(&mut self, timeout: Duration) -> Result<Option<Signal>, Error> {
        match self
            .client
            .as_mut()
            .expect("socket client not connected")
            .receive(&mut self.events, timeout)
        {
            Some(ProtocolSignal::Core(signal)) => Ok(Some(signal)),
            Some(signal) => {
                warn!("Received unexpected signal {signal:?}");
                Ok(None)
            }
            None => Ok(None),
        }
    }

    fn send_to_scheduler(&mut self, signal: &Signal) -> Result<(), Error> {
        self.client
            .as_mut()
            .expect("socket client not connected")
            .send(&ProtocolSignal::Core(*signal))
    }
}

impl TcpRecorderConnector {
    /// Create a new instance
    pub(crate) fn new(agent_id: AgentId, endpoint: SocketAddr) -> Self {
        Self {
            recorder_id: agent_id,
            endpoint,
            events: Events::with_capacity(32),
            client: None,
        }
    }

    fn connect_remote(&mut self) -> Result<(), Error> {
        let connect_signals = [ProtocolSignal::RecorderHello(self.recorder_id)];
        let tcp_client = TcpClient::connect(connect_signals, self.endpoint);
        self.client = Some(tcp_client);
        Ok(())
    }
}

impl UnixRecorderConnector {
    /// Create a new instance
    pub(crate) fn new(agent_id: AgentId, endpoint: PathBuf) -> Self {
        Self {
            recorder_id: agent_id,
            endpoint,
            events: Events::with_capacity(32),
            client: None,
        }
    }

    fn connect_remote(&mut self) -> Result<(), Error> {
        let connect_signals = [ProtocolSignal::RecorderHello(self.recorder_id)];
        let unix_client = UnixClient::connect(connect_signals, &self.endpoint);
        self.client = Some(unix_client);
        Ok(())
    }
}

impl ConnectRecorder for TcpRecorderConnector {
    fn connect_remote(&mut self) -> Result<(), Error> {
        self.connect_remote()
    }

    fn receive(&mut self, timeout: Duration) -> Result<Option<Signal>, Error> {
        self.receive(timeout)
    }

    fn send_to_scheduler(&mut self, signal: &Signal) -> Result<(), Error> {
        self.send_to_scheduler(signal)
    }
}

impl ConnectRecorder for UnixRecorderConnector {
    fn connect_remote(&mut self) -> Result<(), Error> {
        self.connect_remote()
    }

    fn receive(&mut self, timeout: Duration) -> Result<Option<Signal>, Error> {
        self.receive(timeout)
    }

    fn send_to_scheduler(&mut self, signal: &Signal) -> Result<(), Error> {
        self.send_to_scheduler(signal)
    }
}
