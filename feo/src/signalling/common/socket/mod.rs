// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! Socket signalling building blocks

use crate::ids::{ActivityId, AgentId, ChannelId, RelayId, WorkerId};
use crate::signalling::common::signals::Signal;
use crate::timestamp::{SyncInfo, Timestamp};
use std::io::{self, Write};
use std::os::fd::AsRawFd;

pub(crate) mod client;
pub(crate) mod connection;
pub(crate) mod server;

/// Trait providing encoding and decoding methods
///
/// This is used as a bound on the [connection::Connection] primitive.
pub(crate) trait EncodeDecode: Sized {
    /// Encode type to the writer
    fn encode<W: Write>(&self, w: &mut W) -> io::Result<()>;

    /// Try to decode type from the source
    fn try_decode(src: &[u8]) -> Option<(Self, usize)>;
}

/// Trait extending methods available on file descriptors
pub(crate) trait FdExt {
    fn set_nonblocking(&self) -> io::Result<()>;
}

impl<T> FdExt for T
where
    T: AsRawFd,
{
    /// Set stream nonblocking
    ///
    /// This implementation uses `libc::fcntl` directly instead of `libc::ioctl` with FIONBIO.
    /// Certain cross-platform targets have issues with FIONBIO.
    fn set_nonblocking(&self) -> io::Result<()> {
        let fd = self.as_raw_fd();

        // Safety: fd is available since T implements AsRawFd
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL, 0) };
        if flags == -1 {
            return Err(io::Error::last_os_error());
        }

        // Safety: fd is available since T implements AsRawFd and flags
        // is the valid value returned by the previous call to libc::fcntl
        let err = unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) };
        if err != 0 {
            return Err(io::Error::last_os_error());
        }

        Ok(())
    }
}

/// Protocol-specific signal type
///
/// This type wraps the core [Signal] type and adds signal variants
/// which are only needed by this particular implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProtocolSignal {
    /// Core [Signal] as known to schedulers, workers and recorders
    Core(Signal),
    /// Hello signal announcing the presence of an activity with [ActivityId]
    ActivityHello(ActivityId),
    /// Hello signal announcing the presence of a recorder with [AgentId]
    RecorderHello(AgentId),
    /// Hello signal announcing the presence of a generic peer with its [ChannelId]
    ChannelHello(ChannelId),
}

/// Encode signal data to a writer
macro_rules! encode_data {
    ($writer:expr; $type_id:expr; $( $value:expr => $type:ty ),*) => {
        {
            // Sum up the length of all data types to be written
            let length = $( core::mem::size_of::<$type>() + )* 0;

            // Write type ID and length
            $writer.write_all(&($type_id as u8).to_le_bytes())?;
            $writer.write_all(&(length as u8).to_le_bytes())?;
            // Write specified data types
            $(
                $writer.write_all(&(<$type>::from($value)).to_le_bytes())?;
            )*

        }
    };
}

/// Decode signal data from a byte slice
macro_rules! decode_data {
    // Variant with one wrapped item
    ($src:expr; $( $variant:expr ),+; $from:ty => $to:ty) => {{
        // Extract value
        const LENGTH: usize = core::mem::size_of::<$from>();
        let data: [u8; LENGTH] = (&$src[..LENGTH]).try_into().unwrap();
        let value: $to = <$from>::from_le_bytes(data).into();

        // Calculate the number of consumed bytes
        let consumed_bytes = 2 + LENGTH;

        // Recursively wrap value into enum layers
        let connector_signal = value;
        $(
            let connector_signal = $variant(connector_signal);
        )+

        Some((connector_signal, consumed_bytes))
    }};
    // Variant with a wrapped tuple
    ($src:expr; $( $variant:expr ),+; $from1:ty => $to1:ty; $from2:ty => $to2:ty) => {{
        // Extract data for both values
        const LENGTH1: usize = core::mem::size_of::<$from1>();
        const LENGTH2: usize = core::mem::size_of::<$from2>();
        let data: [u8; LENGTH1 + LENGTH2] = (&$src[..(LENGTH1 + LENGTH2)]).try_into().unwrap();

        // Extract first value
        let data1: [u8; LENGTH1] = data[0..LENGTH1].try_into().unwrap();
        let value1: $to1 = <$from1>::from_le_bytes(data1).into();

        // Extract second value
        let data2: [u8; LENGTH2] = data[LENGTH1..(LENGTH1 + LENGTH2)].try_into().unwrap();
        let value2: $to2 = <$from2>::from_le_bytes(data2).into();

        // Calculate the number of consumed bytes
        let consumed_bytes = 2 + LENGTH1 + LENGTH2;

        // Recursively wrap values into enum layers
        let connector_signal = (value1, value2);
        $(
            let connector_signal = $variant(connector_signal);
        )+

        Some((connector_signal, consumed_bytes))
    }};
}

impl EncodeDecode for ProtocolSignal {
    // Protocol definition for this implementation
    //
    // - 1 byte type
    // - 1 byte length
    // - x bytes data
    //
    // [  type  | length | data .. data .. data ]

    fn encode<W: Write>(&self, w: &mut W) -> io::Result<()> {
        match self {
            // Sync
            ProtocolSignal::Core(Signal::StartupSync(sync_info)) => {
                encode_data!(w; SignalTag::CoreStartupSync; sync_info => u128);
            }

            // Recorder-related
            ProtocolSignal::Core(Signal::TaskChainStart(timestamp)) => {
                encode_data!(w; SignalTag::CoreTaskChainStart; timestamp => u128);
            }
            ProtocolSignal::Core(Signal::TaskChainEnd(timestamp)) => {
                encode_data!(w; SignalTag::CoreTaskChainEnd; timestamp => u128);
            }
            ProtocolSignal::Core(Signal::RecorderReady((agent_id, timestamp))) => {
                encode_data!(w; SignalTag::CoreRecorderReady; agent_id => u64, timestamp => u128);
            }

            // Activity-related
            ProtocolSignal::Core(Signal::Startup((activity_id, timestamp))) => {
                encode_data!(w; SignalTag::CoreStartup; activity_id => u64, timestamp => u128);
            }
            ProtocolSignal::Core(Signal::Step((activity_id, timestamp))) => {
                encode_data!(w; SignalTag::CoreStep; activity_id => u64, timestamp => u128);
            }
            ProtocolSignal::Core(Signal::Shutdown((activity_id, timestamp))) => {
                encode_data!(w; SignalTag::CoreShutdown; activity_id => u64, timestamp => u128);
            }
            ProtocolSignal::Core(Signal::Ready((activity_id, timestamp))) => {
                encode_data!(w; SignalTag::CoreReady; activity_id => u64, timestamp => u128);
            }

            // Signalling-layer signals
            ProtocolSignal::ActivityHello(worker_id) => {
                encode_data!(w; SignalTag::ConnectorActivityHello; worker_id => u64);
            }
            ProtocolSignal::RecorderHello(agent_id) => {
                encode_data!(w; SignalTag::ConnectorRecorderHello; agent_id => u64);
            }
            ProtocolSignal::ChannelHello(channel_id) => match channel_id {
                ChannelId::Activity(id) => {
                    encode_data!(w; SignalTag::ConnectorChannelActivityHello; id => u64)
                }
                ChannelId::Worker(id) => {
                    encode_data!(w; SignalTag::ConnectorChannelWorkerHello; id => u64)
                }
                ChannelId::Agent(id) => {
                    encode_data!(w; SignalTag::ConnectorChannelAgentHello; id => u64)
                }
                ChannelId::Relay(id) => {
                    encode_data!(w; SignalTag::ConnectorChannelRelayHello; id => u64)
                }
            },
        }

        w.flush()?;

        Ok(())
    }

    fn try_decode(src: &[u8]) -> Option<(Self, usize)> {
        // Return early if we do not have enough data for the protocol header
        if src.len() < 2 {
            return None;
        }

        // Extract protocol header
        let type_id = u8::from_le_bytes([src[0]]);
        let length = u8::from_le_bytes([src[1]]) as usize;

        // Return early if the full data as specified by the header (length) is not available
        if src.len() < (2 + length) {
            return None;
        }

        // Shorten `src` to `length` to prevent overreads below
        let src = &src[2..(2 + length)];

        let Ok(signal_tag) = type_id.try_into() else {
            panic!("failed to parse unknown type ID {type_id}");
        };

        use SignalTag::*;
        match signal_tag {
            // Sync
            CoreStartupSync => {
                decode_data!(src; Signal::StartupSync, ProtocolSignal::Core; u128 => SyncInfo)
            }

            // Recorder-related
            CoreTaskChainStart => {
                decode_data!(src; Signal::TaskChainStart, ProtocolSignal::Core; u128 => Timestamp)
            }
            CoreTaskChainEnd => {
                decode_data!(src; Signal::TaskChainEnd, ProtocolSignal::Core; u128 => Timestamp)
            }
            CoreRecorderReady => {
                decode_data!(src; Signal::RecorderReady, ProtocolSignal::Core; u64 => AgentId; u128 => Timestamp)
            }

            // Activity-related
            CoreStartup => {
                decode_data!(src; Signal::Startup, ProtocolSignal::Core; u64 => ActivityId; u128 => Timestamp)
            }
            CoreStep => {
                decode_data!(src; Signal::Step, ProtocolSignal::Core; u64 => ActivityId; u128 => Timestamp)
            }
            CoreShutdown => {
                decode_data!(src; Signal::Shutdown, ProtocolSignal::Core; u64 => ActivityId; u128 => Timestamp)
            }
            CoreReady => {
                decode_data!(src; Signal::Ready, ProtocolSignal::Core; u64 => ActivityId; u128 => Timestamp)
            }

            // Signalling-layer signals
            ConnectorActivityHello => {
                decode_data!(src; ProtocolSignal::ActivityHello; u64 => ActivityId)
            }
            ConnectorRecorderHello => {
                decode_data!(src; ProtocolSignal::RecorderHello; u64 => AgentId)
            }
            ConnectorChannelActivityHello => {
                decode_data!(src; ChannelId::Activity, ProtocolSignal::ChannelHello; u64 => ActivityId)
            }
            ConnectorChannelWorkerHello => {
                decode_data!(src; ChannelId::Worker, ProtocolSignal::ChannelHello; u64 => WorkerId)
            }
            ConnectorChannelAgentHello => {
                decode_data!(src; ChannelId::Agent, ProtocolSignal::ChannelHello; u64 => AgentId)
            }
            ConnectorChannelRelayHello => {
                decode_data!(src; ChannelId::Relay, ProtocolSignal::ChannelHello; u64 => RelayId)
            }
        }
    }
}

/// Tags for every signal to be used in encoding/decoding
#[repr(u8)]
pub(crate) enum SignalTag {
    CoreStartupSync = 1,
    CoreTaskChainStart = 11,
    CoreTaskChainEnd = 12,
    CoreRecorderReady = 13,
    CoreStartup = 21,
    CoreStep = 22,
    CoreShutdown = 23,
    CoreReady = 24,
    ConnectorActivityHello = 31,
    ConnectorRecorderHello = 32,
    ConnectorChannelActivityHello = 33,
    ConnectorChannelWorkerHello = 34,
    ConnectorChannelAgentHello = 35,
    ConnectorChannelRelayHello = 36,
}

impl TryFrom<u8> for SignalTag {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        use SignalTag::*;

        match value {
            v if v == CoreStartupSync as u8 => Ok(CoreStartupSync),
            v if v == CoreTaskChainStart as u8 => Ok(CoreTaskChainStart),
            v if v == CoreTaskChainEnd as u8 => Ok(CoreTaskChainEnd),
            v if v == CoreRecorderReady as u8 => Ok(CoreRecorderReady),
            v if v == CoreStartup as u8 => Ok(CoreStartup),
            v if v == CoreStep as u8 => Ok(CoreStep),
            v if v == CoreShutdown as u8 => Ok(CoreShutdown),
            v if v == CoreReady as u8 => Ok(CoreReady),
            v if v == ConnectorActivityHello as u8 => Ok(ConnectorActivityHello),
            v if v == ConnectorRecorderHello as u8 => Ok(ConnectorRecorderHello),
            v if v == ConnectorChannelActivityHello as u8 => Ok(ConnectorChannelActivityHello),
            v if v == ConnectorChannelWorkerHello as u8 => Ok(ConnectorChannelWorkerHello),
            v if v == ConnectorChannelAgentHello as u8 => Ok(ConnectorChannelAgentHello),
            other => Err(other),
        }
    }
}

#[test]
fn connector_signal_roundtrips() {
    use feo_time::Duration;
    let timestamp = Timestamp(Duration::from_secs(1));

    #[rustfmt::skip]
    let signals_with_consumed_bytes = [
        (ProtocolSignal::Core(Signal::TaskChainStart(timestamp)), 18),
        (ProtocolSignal::Core(Signal::TaskChainEnd(timestamp)), 18),
        (ProtocolSignal::Core(Signal::RecorderReady((AgentId::from(123), timestamp))), 26),
        (ProtocolSignal::Core(Signal::Startup((ActivityId::from(123), timestamp))), 26),
        (ProtocolSignal::Core(Signal::Step((ActivityId::from(123), timestamp))), 26),
        (ProtocolSignal::Core(Signal::Shutdown((ActivityId::from(123), timestamp))), 26),
        (ProtocolSignal::Core(Signal::Ready((ActivityId::from(123), timestamp))), 26),
        (ProtocolSignal::ActivityHello(ActivityId::from(123)), 10),
        (ProtocolSignal::RecorderHello(AgentId::from(123)), 10),
    ];

    for (signal, consumed_bytes) in signals_with_consumed_bytes {
        let mut buffer = [0; 128];
        let mut view = &mut buffer[..];

        signal.encode(&mut view).unwrap();
        let (decoded, consumed) = ProtocolSignal::try_decode(&buffer).unwrap();

        assert_eq!(decoded, signal);
        assert_eq!(consumed, consumed_bytes);
    }
}
