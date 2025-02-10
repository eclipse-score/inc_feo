// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use crate::activity::ActivityId;
use crate::error::Error;
use crate::error::Error::Io;
use crate::signalling::{AgentId, Receiver, Sender, Signal};
use crate::timestamp::{SyncInfo, Timestamp};
use feo_log::trace;
use mio::net::TcpStream;
use mio::{Events, Interest, Poll, Token};
use std::collections::HashMap;
use std::io::{ErrorKind, Read as _, Write};
use std::mem;
use std::os::fd::AsRawFd;

const MAX_PDU_DATA_SIZE: usize = 16;

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SignalTag {
    /// Hello message on connection expecting action trigger signals
    #[default]
    HelloTrigger,
    /// Hello message on connection that will send ready signals
    HelloReady,
    /// Sync signal message
    StartupSync,
    /// Task chain start signal message
    TaskChainStart,
    /// Task chain end signal message
    TaskChainEnd,
    /// Startup signal message
    Startup,
    /// Shutdown signal message
    Shutdown,
    /// Step signal message
    Step,
    /// Ready signal message
    Ready,
    /// RecorderReady signal message
    RecorderReady,
}

impl TryFrom<u8> for SignalTag {
    type Error = Error;

    fn try_from(v: u8) -> Result<Self> {
        let s: SignalTag = match v {
            v if v == SignalTag::HelloTrigger as u8 => SignalTag::HelloTrigger,
            v if v == SignalTag::HelloReady as u8 => SignalTag::HelloReady,
            v if v == SignalTag::StartupSync as u8 => SignalTag::StartupSync,
            v if v == SignalTag::TaskChainStart as u8 => SignalTag::TaskChainStart,
            v if v == SignalTag::TaskChainEnd as u8 => SignalTag::TaskChainEnd,
            v if v == SignalTag::Startup as u8 => SignalTag::Startup,
            v if v == SignalTag::Step as u8 => SignalTag::Step,
            v if v == SignalTag::Shutdown as u8 => SignalTag::Shutdown,
            v if v == SignalTag::Ready as u8 => SignalTag::Ready,
            v if v == SignalTag::RecorderReady as u8 => SignalTag::RecorderReady,
            _ => {
                return Err(Io((ErrorKind::InvalidData.into(), "invalid SignalPdu tag")));
            }
        };
        Ok(s)
    }
}

#[derive(Debug, Default)]
pub struct SignalPdu {
    tag: SignalTag,
    data_len: u16,
    data: [u8; MAX_PDU_DATA_SIZE],
}

pub struct MioSocketReceiver<'s, 'p, 'q> {
    stream: &'s mut TcpStream,
    poll: &'p mut Poll,
    events: &'q mut Events,
}

impl<'s, 'p, 'q> MioSocketReceiver<'s, 'p, 'q> {
    pub fn new(stream: &'s mut TcpStream, poll: &'p mut Poll, events: &'q mut Events) -> Self {
        MioSocketReceiver {
            stream,
            poll,
            events,
        }
    }

    pub fn register(&mut self, token: usize) -> std::io::Result<()> {
        self.poll
            .registry()
            .register(self.stream, Token(token), Interest::READABLE)
    }

    pub fn deregister(&mut self) -> std::io::Result<()> {
        self.poll.registry().deregister(self.stream)
    }
}

impl Drop for MioSocketReceiver<'_, '_, '_> {
    fn drop(&mut self) {
        _ = self.deregister() // errors ignored
    }
}

impl Receiver<SignalPdu> for MioSocketReceiver<'_, '_, '_> {
    fn recv(&mut self) -> Result<SignalPdu> {
        let mut pdu = SignalPdu::default();
        loop {
            if is_readable(self.stream) {
                // TODO: This will block until the PDU has been fully received => add timeout
                pdu.read(self.stream, self.poll, self.events)?;
                return Ok(pdu);
            }
            self.poll
                .poll(self.events, None)
                .map_err(|e| Io((e, "error while polling in MioSocketReceiver")))?;
        }
    }
}

pub struct MioMultiSocketReceiver<'p, 'q> {
    streams: HashMap<AgentId, TcpStream>,
    poll: &'p mut Poll,
    events: &'q mut Events,
}

impl<'p, 'q> MioMultiSocketReceiver<'p, 'q> {
    pub fn new<T>(streams: T, poll: &'p mut Poll, events: &'q mut Events) -> Self
    where
        T: IntoIterator<Item = (AgentId, TcpStream)>,
    {
        // convert input to hash map
        let streams: HashMap<AgentId, TcpStream> = streams.into_iter().collect();
        MioMultiSocketReceiver {
            streams,
            poll,
            events,
        }
    }

    pub fn register(&mut self) -> std::io::Result<()> {
        for (_, stream) in self.streams.iter_mut() {
            self.poll
                .registry()
                .register(stream, Token(0), Interest::READABLE)?;
        }
        Ok(())
    }

    pub fn deregister(&mut self) -> std::io::Result<()> {
        for (_, stream) in self.streams.iter_mut() {
            self.poll.registry().deregister(stream)?;
        }
        Ok(())
    }
}

impl Drop for MioMultiSocketReceiver<'_, '_> {
    fn drop(&mut self) {
        _ = self.deregister() // errors ignored
    }
}

impl Receiver<(AgentId, SignalPdu)> for MioMultiSocketReceiver<'_, '_> {
    fn recv(&mut self) -> Result<(AgentId, SignalPdu)> {
        let mut pdu = SignalPdu::default();
        loop {
            for (agent_id, stream) in self.streams.iter_mut() {
                if is_readable(stream) {
                    // TODO: This will block until the PDU has been fully received
                    //       => add timeout, try reading other streams in parallel?
                    pdu.read(stream, self.poll, self.events)?;
                    return Ok((*agent_id, pdu));
                }
            }

            // if we did not receive data on any stream, wait until a stream gets readable
            self.poll
                .poll(self.events, None)
                .map_err(|e| Io((e, "error while polling in MioMultiSocketReceiver")))?;
        }
    }
}

/// Helper trait allowing MioSocketSender to accept a TcpStream either by value or as a mutable reference
pub trait IsTcpStreamOrMutRef: Send + Write {}
impl IsTcpStreamOrMutRef for TcpStream {}
impl IsTcpStreamOrMutRef for &mut TcpStream {}

pub struct MioSocketSender<K>
where
    K: IsTcpStreamOrMutRef,
{
    stream: K,
}

/// Signal sender based on mio::TcpStream (by value or mutable reference)
impl<K> MioSocketSender<K>
where
    K: IsTcpStreamOrMutRef,
{
    pub fn new(stream: K) -> Self {
        MioSocketSender { stream }
    }
}
impl<T: Into<SignalPdu>, K: IsTcpStreamOrMutRef> Sender<T> for MioSocketSender<K> {
    fn send(&mut self, t: T) -> Result<()> {
        let pdu = t.into();
        pdu.send(&mut self.stream)?;
        Ok(())
    }
}

pub struct MioMultiSocketSender {
    streams: HashMap<AgentId, TcpStream>,
}

impl MioMultiSocketSender {
    pub fn new<T>(streams: T) -> Self
    where
        T: IntoIterator<Item = (AgentId, TcpStream)>,
    {
        // convert input to hash map
        let streams: HashMap<AgentId, TcpStream> = streams.into_iter().collect();
        MioMultiSocketSender { streams }
    }
}

impl<T: Into<SignalPdu>> Sender<(AgentId, T)> for MioMultiSocketSender {
    fn send(&mut self, t: (AgentId, T)) -> Result<()> {
        let agent_id = t.0;
        let pdu = t.1.into();
        let stream = self
            .streams
            .get_mut(&agent_id)
            .ok_or_else(|| Io((ErrorKind::InvalidInput.into(), "unknown agent id")))?;
        pdu.send(stream)?;
        Ok(())
    }
}

impl SignalPdu {
    pub fn send(&self, writer: &mut dyn Write) -> Result<()> {
        trace!("sending {:?}", self);
        if self.data_len as usize > MAX_PDU_DATA_SIZE {
            return Err(Io((
                ErrorKind::InvalidData.into(),
                "max pdu data size exceeded",
            )));
        }

        const BUF_SIZE: usize = size_of::<SignalTag>() + size_of::<u16>();
        let mut buffer: [u8; BUF_SIZE] = [0; BUF_SIZE];

        buffer[0] = self.tag as u8;
        let len_as_bytes = u16::to_be_bytes(self.data_len);
        buffer[1..3].copy_from_slice(&len_as_bytes);

        writer
            .write_all(&buffer)
            .map_err(|e| Io((e, "failed to write pdu header")))?;
        writer
            .write_all(&self.data[0..self.data_len as usize])
            .map_err(|e| Io((e, "failed to write pdu data")))?;
        writer.flush().unwrap();

        Ok(())
    }

    pub fn read(
        &mut self,
        stream: &mut TcpStream,
        poll: &mut Poll,
        events: &mut Events,
    ) -> Result<()> {
        const BUF_SIZE: usize = size_of::<SignalTag>() + size_of::<u16>();
        let mut buffer: [u8; BUF_SIZE] = [0; BUF_SIZE];

        read_buffer(&mut buffer, stream, poll, events)
            .map_err(|e| Io((e, "failed to read SignalPdu header")))?;

        let data_len = u16::from_be_bytes(buffer[1..3].try_into().unwrap());
        if data_len as usize > MAX_PDU_DATA_SIZE {
            return Err(Io((
                ErrorKind::InvalidData.into(),
                "received PDU length exceeds buffer size",
            )));
        }

        read_buffer(&mut self.data[0..data_len as usize], stream, poll, events)
            .map_err(|e| Io((e, "failed to read SignalPdu data")))?;

        let tag: SignalTag = buffer[0].try_into()?;

        self.tag = tag;
        self.data_len = data_len;

        trace!("Received {:?}", self);

        Ok(())
    }
}

fn is_readable(stream: &TcpStream) -> bool {
    let mut buf: [u8; 1] = [0; 1];
    let result = stream.peek(&mut buf);
    result.is_ok() && result.unwrap() > 0
}

fn wait_readable(stream: &TcpStream, poll: &mut Poll, events: &mut Events) -> std::io::Result<()> {
    while !is_readable(stream) {
        poll.poll(events, None)?;
    }
    Ok(())
}

// Try to read as may bytes from the given TcpStream as needed to completely fill the given buffer
fn read_buffer(
    buffer: &mut [u8],
    stream: &mut TcpStream,
    poll: &mut Poll,
    events: &mut Events,
) -> std::io::Result<()> {
    let len = buffer.len();
    let mut total_read = 0usize;
    while total_read < len {
        // read next bytes, starting at current position up to the end of the buffer
        let num_read = match stream.read(buffer[total_read..len].as_mut()) {
            Ok(n) => n,
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                wait_readable(stream, poll, events)?;
                0usize
            }
            Err(e) => {
                return Err(e);
            }
        };

        total_read += num_read;
    }
    assert!(total_read <= len, "buffer overflow");
    assert_eq!(total_read, len, "buffer not fully read");

    Ok(())
}

fn encode_header(pdu: &mut SignalPdu, tag: SignalTag, data_len: usize) {
    assert!(
        data_len <= MAX_PDU_DATA_SIZE,
        "cannot encode data exceeding max pdu data size"
    );
    pdu.tag = tag;
    pdu.data_len = data_len as u16; // Cast is ok because of the above check for max size
}

macro_rules! decode_pdu_data {
    ($pdu:expr, $($intype:ty => $outtype: ty),+ $(,)?) => {{
        let data_len: usize = $pdu.data_len.into();
        let mut _offset: usize = 0usize;

        (
            $(
                {
                    let size: usize = mem::size_of::<$intype>();
                    assert!(_offset + size <= data_len, "failed to decode pdu: insufficient data");
                    let value: $outtype = <$intype>::from_be_bytes($pdu.data[_offset.._offset + size]
                        .try_into()
                        .map_err(|_| Io((ErrorKind::InvalidData.into(), "failed to decode pdu")))?)
                        .into();
                    _offset += size;
                    value
                }
            ),+
        )
    }}
}

impl TryFrom<&SignalPdu> for Signal {
    type Error = Error;

    fn try_from(pdu: &SignalPdu) -> Result<Self> {
        // decode header and data
        trace!("Decoding {:?}", pdu);

        let signal = match pdu.tag {
            SignalTag::HelloTrigger => {
                let id = decode_pdu_data!(pdu, usize => AgentId);
                Signal::HelloTrigger(id)
            }
            SignalTag::HelloReady => {
                let id = decode_pdu_data!(pdu, usize => AgentId);
                Signal::HelloReady(id)
            }
            SignalTag::StartupSync => {
                let info = decode_pdu_data!(pdu, u64 => SyncInfo);
                Signal::StartupSync(info)
            }
            SignalTag::Ready => {
                let (id, t) = decode_pdu_data!(pdu, usize => ActivityId, u64 => Timestamp);
                Signal::Ready((id, t))
            }
            SignalTag::TaskChainStart => {
                let t = decode_pdu_data!(pdu, u64 => Timestamp);
                Signal::TaskChainStart(t)
            }
            SignalTag::TaskChainEnd => {
                let t = decode_pdu_data!(pdu, u64 => Timestamp);
                Signal::TaskChainEnd(t)
            }
            SignalTag::Startup => {
                let (id, t) = decode_pdu_data!(pdu, usize => ActivityId, u64 => Timestamp);
                Signal::Startup((id, t))
            }
            SignalTag::Step => {
                let (id, t) = decode_pdu_data!(pdu, usize => ActivityId, u64 => Timestamp);
                Signal::Step((id, t))
            }
            SignalTag::Shutdown => {
                let (id, t) = decode_pdu_data!(pdu, usize => ActivityId, u64 => Timestamp);
                Signal::Shutdown((id, t))
            }
            SignalTag::RecorderReady => {
                let (id, t) = decode_pdu_data!(pdu, usize => AgentId, u64 => Timestamp);
                Signal::RecorderReady((id, t))
            }
        };

        Ok(signal)
    }
}

impl TryFrom<SignalPdu> for Signal {
    type Error = Error;

    fn try_from(pdu: SignalPdu) -> Result<Self> {
        Self::try_from(&pdu)
    }
}

macro_rules! encode_pdu {
    ($tag:expr, $($value:expr => $type:ty),+ $(,)?) => {{
        let mut pdu = SignalPdu::default();
        let mut offset: usize = 0usize;

        $(
            let bytes = <$type>::to_be_bytes($value.into());
            pdu.data[offset..offset + bytes.len()].copy_from_slice(&bytes);
            offset += bytes.len();
        )+

        encode_header(&mut pdu, $tag, offset);
        pdu
    }}
}

impl From<&Signal> for SignalPdu {
    fn from(signal: &Signal) -> Self {
        match signal {
            Signal::HelloTrigger(id) => encode_pdu!(SignalTag::HelloTrigger, *id => usize),
            Signal::HelloReady(id) => encode_pdu!(SignalTag::HelloReady, *id => usize),
            Signal::StartupSync(sync_info) => {
                encode_pdu!(SignalTag::StartupSync, *sync_info => u64)
            }
            Signal::Ready((id, t)) => {
                encode_pdu!(SignalTag::Ready, *id => usize, *t => u64)
            }
            Signal::TaskChainStart(t) => {
                encode_pdu!(SignalTag::TaskChainStart, *t => u64)
            }
            Signal::TaskChainEnd(t) => encode_pdu!(SignalTag::TaskChainEnd, *t => u64),
            Signal::Startup((id, t)) => {
                encode_pdu!(SignalTag::Startup, *id => usize, *t => u64)
            }
            Signal::Step((id, t)) => {
                encode_pdu!(SignalTag::Step, *id => usize, *t => u64)
            }
            Signal::Shutdown((id, t)) => {
                encode_pdu!(SignalTag::Shutdown, *id => usize, *t => u64)
            }
            Signal::RecorderReady((id, t)) => {
                encode_pdu!(SignalTag::RecorderReady, *id => usize, *t => u64)
            }
        }
    }
}

impl From<Signal> for SignalPdu {
    fn from(signal: Signal) -> Self {
        Self::from(&signal)
    }
}

pub trait FdExt {
    fn make_nonblocking(&self) -> std::io::Result<()>;
}

impl<T> FdExt for T
where
    T: AsRawFd,
{
    // Implementing our own version of set_nonblocking, using libc::fcntl directly.
    // We call it make_nonblocking to avoid clashing with the existing implementation.
    // The std::net::TcpStreams' set_nonblocking method internally uses libc::ioctl
    // with FIONBIO, which turned out to behave differently cross-platform-wise
    fn make_nonblocking(&self) -> std::io::Result<()> {
        let fd = self.as_raw_fd();

        // Safety: fd is available since T implements AsRawFd
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL, 0) };
        if flags == -1 {
            return Err(std::io::Error::last_os_error());
        }

        // Safety: fd is available since T implements AsRawFd and flags
        // is the valid value returned by the previous call to libc::fcntl
        let err = unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) };
        if err != 0 {
            return Err(std::io::Error::last_os_error());
        }

        Ok(())
    }
}

type Result<T, E = Error> = std::result::Result<T, E>;
