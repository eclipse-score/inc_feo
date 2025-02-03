// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use crate::signalling::common::socket::connection::Connection;
use crate::signalling::common::socket::{EncodeDecode, ProtocolSignal};
use core::fmt;
use core::net::SocketAddr;
use core::time::Duration;
use feo_log::{debug, info, warn};
use mio::net::{TcpListener, TcpStream, UnixListener, UnixStream};
use mio::{event, Events, Interest, Poll, Token};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;

/// Token of the listener
const LISTENER_TOKEN: Token = Token(0);

/// TCP server
pub(crate) type TcpServer = SocketServer<TcpListener>;

/// Unix socket server
pub(crate) type UnixServer = SocketServer<UnixListener>;

/// Socket server
pub(crate) struct SocketServer<L>
where
    L: Listen<ProtocolSignal>,
{
    /// Listener for incoming connections
    listener: L,
    /// [Poll] with registered sources
    poll: Poll,
    /// Map of accepted connections
    accepted_connections: HashMap<Token, Connection<L::Stream, ProtocolSignal>>,
    /// Number of accepted connections, used as ID on accept
    num_accepted_connections: usize,
}

impl<L> SocketServer<L>
where
    L: Listen<ProtocolSignal>,
{
    /// Try to receive a message or accept connections
    ///
    /// Note: The method may return early (i.e., before the timeout has expired) without
    ///       having received any data. This will happen in particular when a new connection
    ///       has been accepted.
    pub fn receive(
        &mut self,
        events: &mut Events,
        timeout: Duration,
    ) -> Option<(Token, ProtocolSignal)> {
        if let Some((token, msg)) = self.receive_on_readable_connections() {
            return Some((token, msg));
        }

        // There was no readable connection -> poll
        self.poll.poll(events, Some(timeout)).unwrap();
        for event in events.iter() {
            match event.token() {
                LISTENER_TOKEN => self.accept_connections(),
                token if self.accepted_connections.contains_key(&token) => {
                    self.accepted_connections
                        .get_mut(&token)
                        .unwrap()
                        .set_stream_readable();
                }
                other => {
                    warn!("Received readiness event for unknown token {other:?}");
                }
            }
        }

        // Check once again for any readable connection
        if let Some((token, msg)) = self.receive_on_readable_connections() {
            return Some((token, msg));
        }

        None
    }

    /// Send the message to the connection identified by `token`
    pub fn send(&mut self, token: &Token, msg: &ProtocolSignal) -> io::Result<()> {
        if let Some(connection) = self.accepted_connections.get_mut(token) {
            connection.send(msg)
        } else {
            Err(io::ErrorKind::NotFound.into())
        }
    }

    /// Accept connections on the listener
    fn accept_connections(&mut self) {
        loop {
            match self.listener.accept_connection() {
                Ok((mut connection, peer_addr)) => {
                    self.num_accepted_connections += 1;
                    let token = Token(self.num_accepted_connections);

                    self.poll
                        .registry()
                        .register(connection.stream(), token, Interest::READABLE)
                        .unwrap();

                    self.accepted_connections.insert(token, connection);

                    info!("Accepted connection from {peer_addr:?}");
                }
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => break,
                Err(err) => panic!("failed to accept connection: {err}"),
            }
        }
    }

    /// Try to receive a message
    fn receive_on_readable_connections(&mut self) -> Option<(Token, ProtocolSignal)> {
        for (token, connection) in self
            .accepted_connections
            .iter_mut()
            .filter(|(_, c)| c.is_readable())
        {
            match connection.read() {
                Ok(Some(msg)) => {
                    // Bubble up token with message
                    return Some((*token, msg));
                }
                Ok(None) => {}
                Err(e) => {
                    panic!("failed to read from connection with token {token:?}: {e}");
                }
            }
        }

        None
    }
}

impl<L> SocketServer<L>
where
    L: Listen<ProtocolSignal> + event::Source,
{
    /// Create a new instance with the given listener
    fn with_listener(mut listener: L) -> Self {
        let accepted_connections = HashMap::new();
        let num_accepted_connections = 0;

        let poll = Poll::new().unwrap();

        poll.registry()
            .register(&mut listener, LISTENER_TOKEN, Interest::READABLE)
            .unwrap();

        Self {
            listener,
            poll,
            accepted_connections,
            num_accepted_connections,
        }
    }
}

impl SocketServer<TcpListener> {
    /// Create a new instance
    pub fn new(address: SocketAddr) -> Self {
        let listener = TcpListener::bind(address).unwrap();
        Self::with_listener(listener)
    }
}

impl SocketServer<UnixListener> {
    /// Create a new instance
    pub fn new(path: &Path) -> Self {
        // Check for and remove stale socket file
        // This is a workaround until the shutdown is defined in FEO
        if path.exists() {
            debug!("Removing stale socket file {}", path.display());
            fs::remove_file(path).unwrap_or_else(|e| {
                panic!("failed to remove stale socket file {}: {e}", path.display())
            });
        }

        let listener = UnixListener::bind(path).unwrap();
        Self::with_listener(listener)
    }
}

/// Listen and accept incoming connections, yielding a [Connection] and a peer address
pub(crate) trait Listen<M>
where
    M: EncodeDecode,
{
    type Stream: io::Read + io::Write + event::Source;
    type PeerAddr: fmt::Debug;

    #[allow(clippy::type_complexity)]
    fn accept_connection(&mut self) -> io::Result<(Connection<Self::Stream, M>, Self::PeerAddr)>;
}

impl<M> Listen<M> for TcpListener
where
    M: EncodeDecode,
{
    type Stream = TcpStream;
    type PeerAddr = SocketAddr;

    fn accept_connection(&mut self) -> io::Result<(Connection<Self::Stream, M>, Self::PeerAddr)> {
        self.accept().and_then(|(stream, peer_addr)| {
            stream
                .set_nodelay(true)
                .map(|_| (Connection::<Self::Stream, M>::new(stream), peer_addr))
        })
    }
}

impl<M> Listen<M> for UnixListener
where
    M: EncodeDecode,
{
    type Stream = UnixStream;
    type PeerAddr = std::os::unix::net::SocketAddr;

    fn accept_connection(&mut self) -> io::Result<(Connection<Self::Stream, M>, Self::PeerAddr)> {
        self.accept()
            .map(|(stream, peer_addr)| (Connection::<Self::Stream, M>::new(stream), peer_addr))
    }
}
