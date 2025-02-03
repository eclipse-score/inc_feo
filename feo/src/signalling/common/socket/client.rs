// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use crate::error::Error;
use crate::signalling::common::socket::connection::Connection;
use crate::signalling::common::socket::{FdExt, ProtocolSignal};
use core::net::SocketAddr;
use core::time::Duration;
use feo_log::{info, trace};
use mio::net::{TcpStream, UnixStream};
use mio::{Events, Interest, Poll, Token};
use std::path::Path;
use std::{io, thread};

/// Token of the client stream
const CLIENT_TOKEN: Token = Token(0);

/// TCP client
pub(crate) type TcpClient = SocketClient<TcpStream>;

// Unix socket client
pub(crate) type UnixClient = SocketClient<UnixStream>;

/// Socket client
pub(crate) struct SocketClient<S>
where
    S: io::Read + io::Write,
{
    /// [Poll] to register the stream of the connection on
    poll: Poll,
    /// Connection to the server
    connection: Connection<S, ProtocolSignal>,
}

impl SocketClient<TcpStream> {
    /// Connect to the scheduler on `address`, announcing ourselves with `connect_signals`
    pub(crate) fn connect(
        connect_signals: impl IntoIterator<Item = ProtocolSignal>,
        address: SocketAddr,
    ) -> Self {
        let stream = loop {
            if let Ok(stream) = std::net::TcpStream::connect(address) {
                <std::net::TcpStream as FdExt>::set_nonblocking(&stream)
                    .expect("failed to set stream non-blocking");
                break TcpStream::from_std(stream);
            }

            thread::sleep(Duration::from_millis(300));
        };
        info!("Successfully connected to {address:?}");
        stream.set_nodelay(true).unwrap();
        let mut connection = Connection::<TcpStream, ProtocolSignal>::new(stream);

        for signal in connect_signals {
            connection.send(&signal).unwrap();
            trace!("Sent message {signal:?}");
        }

        let poll = Poll::new().unwrap();
        poll.registry()
            .register(connection.stream(), CLIENT_TOKEN, Interest::READABLE)
            .unwrap();

        Self { poll, connection }
    }
}

impl SocketClient<UnixStream> {
    /// Connect to the scheduler on `path`, announcing ourselves with `connect_signals`
    pub(crate) fn connect(
        connect_signals: impl IntoIterator<Item = ProtocolSignal>,
        path: &Path,
    ) -> Self {
        let stream = loop {
            if let Ok(stream) = UnixStream::connect(path) {
                break stream;
            }

            thread::sleep(Duration::from_millis(300));
        };
        info!("Successfully connected to {path:?}");
        let mut connection = Connection::<UnixStream, ProtocolSignal>::new(stream);

        for signal in connect_signals {
            connection.send(&signal).unwrap();
            trace!("Sent message {signal:?}");
        }

        let poll = Poll::new().unwrap();
        poll.registry()
            .register(connection.stream(), CLIENT_TOKEN, Interest::READABLE)
            .unwrap();

        Self { poll, connection }
    }
}

impl<S> SocketClient<S>
where
    S: io::Read + io::Write,
{
    /// Try to receive a signal from the connection
    ///
    /// Note: The method may return early (i.e., before the timeout has expired) without
    ///       having received any data.
    pub(crate) fn receive(
        &mut self,
        events: &mut Events,
        timeout: Duration,
    ) -> Option<ProtocolSignal> {
        if self.connection.is_readable() {
            match self.connection.read() {
                Ok(Some(msg)) => return Some(msg),
                Ok(None) => {}
                Err(e) => {
                    panic!("connection to server died: {e}");
                }
            }
        }

        self.poll.poll(events, Some(timeout)).unwrap();

        for event in events.iter() {
            if event.token() == CLIENT_TOKEN {
                self.connection.set_stream_readable();
            }
        }

        if self.connection.is_readable() {
            match self.connection.read() {
                Ok(Some(msg)) => return Some(msg),
                Ok(None) => {}
                Err(e) => {
                    panic!("connection to server died: {e}");
                }
            }
        }

        None
    }

    /// Send a signal to the scheduler
    pub(crate) fn send(&mut self, msg: &ProtocolSignal) -> Result<(), Error> {
        self.connection
            .send(msg)
            .map_err(|e| Error::Io((e, "failed to send on connection")))
    }
}
