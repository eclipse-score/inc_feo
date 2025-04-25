// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use crate::signalling::common::socket::EncodeDecode;
use core::marker::PhantomData;
use feo_log::trace;
use mio::net::{TcpStream, UnixStream};
use std::io::{self, Cursor};

/// Size of the buffer within a connection
const BUFFER_SIZE: usize = 128;

/// Wrapper around a stream to facilitate reading messages
pub(crate) struct Connection<S, M>
where
    S: io::Read + io::Write,
    M: EncodeDecode,
{
    /// Wrapped stream
    stream: S,
    /// Buffer to read to
    recv_buffer: [u8; BUFFER_SIZE],
    /// Index at which the next message starts
    recv_begin: usize,
    /// Index until which we have written received data
    recv_end: usize,
    /// Buffer to use during sending
    send_buffer: [u8; BUFFER_SIZE],
    /// Flag whether the stream might be readable
    stream_readable: bool,
    /// Flag whether the buffer might contain a parsable message
    buffer_readable: bool,
    /// Message type which this [Connection] encodes/parses
    _message: PhantomData<M>,
}

impl<M: EncodeDecode> Connection<TcpStream, M> {
    /// Wrap a [TcpStream] in a [Connection]
    pub(crate) fn new(stream: TcpStream) -> Self {
        Self {
            stream,
            recv_buffer: [0; BUFFER_SIZE],
            recv_begin: 0,
            recv_end: 0,
            send_buffer: [0; BUFFER_SIZE],
            stream_readable: false,
            buffer_readable: false,
            _message: PhantomData,
        }
    }
}

impl<M: EncodeDecode> Connection<UnixStream, M> {
    /// Wrap a [UnixStream] in a [Connection]
    pub(crate) fn new(stream: UnixStream) -> Self {
        Self {
            stream,
            recv_buffer: [0; BUFFER_SIZE],
            recv_begin: 0,
            recv_end: 0,
            send_buffer: [0; BUFFER_SIZE],
            stream_readable: false,
            buffer_readable: false,
            _message: PhantomData,
        }
    }
}

impl<S, M> Connection<S, M>
where
    S: io::Read + io::Write,
    M: EncodeDecode,
{
    /// Try to read from this connection
    pub(crate) fn read(&mut self) -> io::Result<Option<M>> {
        if self.buffer_readable {
            if let Some(msg) = self.parse_from_buffer() {
                return Ok(Some(msg));
            }
        }

        if self.stream_readable {
            self.read_from_stream()?;
        }

        if self.buffer_readable {
            if let Some(msg) = self.parse_from_buffer() {
                return Ok(Some(msg));
            }
        }

        Ok(None)
    }

    /// Send a [Message] through the stream
    pub(crate) fn send(&mut self, msg: &M) -> io::Result<()> {
        // Encode message to buffer.
        // Buffered writes reduce the signalling overhead by 33% through less fragmentation.
        let mut writer = Cursor::new(&mut self.send_buffer[..]);
        msg.encode(&mut writer)?;
        let end_idx = writer.position() as usize;

        // Write the encoded data to the stream.
        self.stream.write_all(&self.send_buffer[..end_idx])?;
        self.stream.flush()?;

        Ok(())
    }

    /// Expose inner stream to de/register it with a [mio::Poll]
    pub(crate) fn stream(&mut self) -> &mut S {
        &mut self.stream
    }

    /// Set the stream as readable
    ///
    /// This is done when the poll returns a readable event for the underlying stream.
    pub(crate) fn set_stream_readable(&mut self) {
        self.stream_readable = true;
    }

    /// Return if this connection is readable
    pub(crate) fn is_readable(&self) -> bool {
        self.stream_readable || self.buffer_readable
    }

    /// Try to read from the stream
    fn read_from_stream(&mut self) -> io::Result<()> {
        loop {
            match self.stream.read(&mut self.recv_buffer[self.recv_end..]) {
                Ok(0) => {
                    trace!("Read zero bytes");
                    return Err(io::ErrorKind::ConnectionReset.into());
                }
                Ok(n) => {
                    trace!("Read {n} bytes");
                    self.recv_end += n;
                    self.buffer_readable = true;

                    if self.recv_end == self.recv_buffer.len() {
                        return Ok(());
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    trace!("Received WouldBlock");
                    self.stream_readable = false;
                    return Ok(());
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }
    }

    /// Try to parse a message from the buffer
    fn parse_from_buffer(&mut self) -> Option<M> {
        match M::try_decode(&self.recv_buffer[self.recv_begin..self.recv_end]) {
            Some((msg, consumed_bytes)) => {
                self.recv_begin += consumed_bytes;

                // TODO: Optimize
                // We can pass two const generics:
                // - BUFFER_SIZE
                // - MAX_MSG_SIZE
                // Then we can update the logic to shift the buffer
                // only when there is less than one maximum message space left.
                if self.recv_begin > BUFFER_SIZE / 2 {
                    self.shift_buffer();
                }

                Some(msg)
            }
            None => {
                self.buffer_readable = false;
                None
            }
        }
    }

    /// Copy the remaining, not yet parsed content to the beginning of the buffer, updating indices
    fn shift_buffer(&mut self) {
        trace!("Shifting buffer");

        let remaining = self.recv_end - self.recv_begin;
        let mut tmp: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE];

        tmp[0..remaining].copy_from_slice(&self.recv_buffer[self.recv_begin..self.recv_end]);
        self.recv_buffer[0..remaining].copy_from_slice(&tmp[0..remaining]);

        self.recv_end = remaining;
        self.recv_begin = 0;
    }
}
