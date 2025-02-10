// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use crate::protocol::{TraceData, TracePacket, MAX_PACKET_SIZE};
use feo_log::{trace, warn};
use libc::{sockaddr_un, AF_UNIX};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::{atomic, Mutex};
use std::{io, mem};
use tracing::level_filters::LevelFilter;
use tracing::span;
use tracing::subscriber::set_global_default;
use tracing_serde_structured::AsSerde;

pub const UNIX_PACKET_PATH: &str = "/tmp/feo-tracer.sock";

/// Initialize the tracing subscriber with the given level
pub fn init(level: LevelFilter) {
    let subscriber = Subscriber {
        max_level: level,
        tracer: Mutex::new(None),
    };
    set_global_default(subscriber).expect("setting tracing default failed");
}

/// A subscriber that sends trace data to the feo-tracer via seqpacket and postcard serialized data.
/// See the `TraceData` and `TracePacket` types for the data format.
struct Subscriber {
    max_level: LevelFilter,
    tracer: Mutex<Option<OwnedFd>>,
}

impl Subscriber {
    /// Generate a new span id
    fn new_span_id(&self) -> span::Id {
        /// Next span id. This is a global counter. Span ids must not be 0.
        static NEXT_ID: atomic::AtomicU64 = atomic::AtomicU64::new(1);

        // Generate next span id
        let id = NEXT_ID.fetch_add(1, atomic::Ordering::Relaxed);

        span::Id::from_u64(id)
    }

    // Send a value to the tracer
    fn send(&self, packet: TracePacket<'_>) {
        let mut guard = self.tracer.lock().unwrap();

        if guard.is_none() {
            // Connect
            match connect() {
                Ok(connection) => *guard = Some(connection),
                Err(e) => {
                    trace!("Failed to connect to feo-tracer: {:?}. Discarding value", e);
                    return;
                }
            };
        }

        let socket = guard.as_mut().unwrap();

        let message = postcard::to_vec::<_, MAX_PACKET_SIZE>(&packet).expect("failed to serialize"); // TODO throw?

        // Note: Seqpacket writes write all data or fail. No need to loop around and check for partial writes.
        let fd = socket.as_raw_fd();
        let buf = message.as_ptr() as *const libc::c_void;
        let len = message.len();
        // Safety: buf is a valid pointer to a buffer of the correct length
        let ret = unsafe { libc::send(fd, buf, len, 0) };
        if ret < 0 {
            let error = io::Error::last_os_error();
            warn!("Failed to send to feo-tracer: {error:?}");
            guard.take();
        }
    }
}

impl tracing::Subscriber for Subscriber {
    fn enabled(&self, metadata: &tracing::Metadata<'_>) -> bool {
        // A span or event is enabled if it is at or below the configured
        // maximum level.
        metadata.level() <= &self.max_level
    }

    fn max_level_hint(&self) -> Option<LevelFilter> {
        Some(self.max_level)
    }

    fn new_span(&self, span: &span::Attributes) -> span::Id {
        let id = self.new_span_id();
        let trace_data = TraceData::NewSpan {
            id: id.into_u64(),
            attributes: span.as_serde(),
        };
        let trace_packet = TracePacket::now_with_data(trace_data);
        self.send(trace_packet);
        id
    }

    fn record(&self, span: &span::Id, values: &span::Record) {
        let trace_data = TraceData::Record {
            span: span.into_u64(),
            values: values.as_serde(),
        };
        let trace_packet = TracePacket::now_with_data(trace_data);
        self.send(trace_packet);
    }

    fn event(&self, event: &tracing::Event) {
        let trace_data = TraceData::Event {
            parent_span: self.current_span().id().map(|id| id.into_u64()),
            event: event.as_serde(),
        };
        let trace_packet = TracePacket::now_with_data(trace_data);
        self.send(trace_packet);
    }

    fn enter(&self, span: &span::Id) {
        let trace_data = TraceData::Enter {
            span: span.into_u64(),
        };
        let trace_packet = TracePacket::now_with_data(trace_data);
        self.send(trace_packet);
    }

    fn exit(&self, span: &span::Id) {
        let trace_data = TraceData::Exit {
            span: span.into_u64(),
        };
        let trace_packet = TracePacket::now_with_data(trace_data);
        self.send(trace_packet);
    }

    fn record_follows_from(&self, _span: &span::Id, _follows: &span::Id) {}
}

fn connect() -> io::Result<OwnedFd> {
    // Create a seqpacket socket
    let socket = unsafe { libc::socket(AF_UNIX, libc::SOCK_SEQPACKET, 0) };
    assert!(socket >= 0, "socket failed");
    // Wrap the socket in a OwnedFd
    // Safety: socket is a valid file descriptor. Connect result is checked.
    let fd = unsafe { OwnedFd::from_raw_fd(socket) };

    // Prepare the sockaddr
    let bytes = UNIX_PACKET_PATH.as_bytes();

    // Safety: sockaddr_un is a C struct with no padding
    let mut sockaddr = unsafe { mem::MaybeUninit::<sockaddr_un>::zeroed().assume_init() };

    sockaddr.sun_family = AF_UNIX as u16;

    let path = (&mut sockaddr.sun_path.as_mut_slice()[0..bytes.len()]) as *mut _ as *mut [u8];
    // Safety: path is a valid pointer to a slice of the correct length
    let path = unsafe { &mut *path };
    path.clone_from_slice(bytes);
    let sockaddr = &mut sockaddr as *mut _ as *mut libc::sockaddr;
    let addr_len = mem::size_of::<sockaddr_un>() as libc::socklen_t;

    // Connect
    // Safety: sockaddr is a valid pointer to a sockaddr_un. addr_len is the correct size.
    let ret = unsafe { libc::connect(fd.as_raw_fd(), sockaddr, addr_len) };
    if ret == -1 {
        // Safety: close never fails
        return Err(io::Error::last_os_error());
    }

    Ok(fd)
}
