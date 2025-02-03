// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use crate::protocol::{
    truncate, EventInfo, TraceData, TracePacket, MAX_INFO_SIZE, MAX_PACKET_SIZE,
};
use core::sync::atomic;
use core::sync::atomic::AtomicBool;
use core::time::Duration;
use feo_log::error;
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;
use std::{io, thread};
use tracing::level_filters::LevelFilter;
use tracing::span;
use tracing::subscriber::set_global_default;

/// The unix socket path used by the tracing daemon to receive trace packets
pub const UNIX_PACKET_PATH: &str = "/tmp/feo-tracer.sock";

/// Size of the channel (number of packets) for transmitting trace packets to the serializing thread
const MPSC_CHANNEL_BOUND: usize = 512;

/// Size of the buffer (bytes) for transmitting serialized packets to the trace daemon
const BUFWRITER_SIZE: usize = 512 * MAX_PACKET_SIZE;

/// Size of the maximal time interval after which to flush packets to the daemon
const FLUSH_INTERVAL: Duration = Duration::from_millis(500);

/// Initialize the tracing subscriber with the given level
pub fn init(level: LevelFilter) {
    let (sender, receiver) = mpsc::sync_channel::<TracePacket>(MPSC_CHANNEL_BOUND);
    let enabled = Arc::new(AtomicBool::new(true));

    // Spawn thread for serializing trace packets and sending to the trace daemon
    let _thread = {
        let enabled = Arc::clone(&enabled);
        thread::spawn(|| Subscriber::thread_main(receiver, enabled))
    };

    let subscriber = Subscriber {
        max_level: level,
        enabled,
        _thread,
        sender,
    };
    set_global_default(subscriber).expect("setting tracing default failed");
}

/// A subscriber sending trace data to the feo-tracer via unix socket and postcard serialized data.
///
/// See the `TraceData` and `TracePacket` types for the data format.
struct Subscriber {
    max_level: LevelFilter,
    enabled: Arc<AtomicBool>,
    _thread: JoinHandle<()>,
    sender: mpsc::SyncSender<TracePacket>,
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

    fn thread_main(receiver: mpsc::Receiver<TracePacket>, enabled: Arc<AtomicBool>) {
        let connection = match UnixStream::connect(UNIX_PACKET_PATH) {
            Ok(connection) => connection,
            Err(e) => {
                error!("Failed to connect to feo-tracer: {:?}, aborting", e);
                // disable further tracing (TODO: add a time period of retrying)
                enabled.store(false, atomic::Ordering::Relaxed);
                return;
            }
        };

        // Create buffer for serialization
        let mut buffer = [0u8; MAX_PACKET_SIZE];

        // Create BufferedWriter for socket
        let mut socket_writer = io::BufWriter::with_capacity(BUFWRITER_SIZE, connection);
        let mut last_flush = std::time::Instant::now();

        loop {
            let packet = receiver
                .recv()
                .expect("trace subscriber failed to receive, aborting");

            let serialized = match postcard::to_slice_cobs(&packet, &mut buffer[..]) {
                Ok(serialized) => serialized,
                Err(e) => {
                    error!("Failed to serialize trace packet: {e:?}");
                    continue;
                }
            };

            let ret = socket_writer.write_all(serialized);
            if let Err(error) = ret {
                error!("Failed to send to feo-tracer: {error:?}, aborting");
                enabled.store(false, atomic::Ordering::Relaxed);
                return;
            }

            // Flush, if pre-defined time interval elapsed or insufficient spare capacity
            if last_flush.elapsed() > FLUSH_INTERVAL {
                socket_writer.flush().expect("failed to flush");
                last_flush = std::time::Instant::now();
            }
        }
    }

    // Send a value to the tracer
    fn send(&self, packet: TracePacket) {
        if !self.enabled.load(atomic::Ordering::Relaxed) {
            return;
        }
        if let Err(e) = self.sender.send(packet) {
            error!("Failed to connect to feo-tracer: {:?}, aborting", e);
            self.enabled.store(false, atomic::Ordering::Relaxed);
        }
    }
}

impl tracing::Subscriber for Subscriber {
    fn enabled(&self, metadata: &tracing::Metadata<'_>) -> bool {
        // A span or event is enabled if it is at or below the configured
        // maximum level
        metadata.level() <= &self.max_level
    }

    fn max_level_hint(&self) -> Option<LevelFilter> {
        Some(self.max_level)
    }

    fn new_span(&self, span: &span::Attributes) -> span::Id {
        let id = self.new_span_id();
        let mut name = [0u8; MAX_INFO_SIZE];
        let name_len = truncate(span.metadata().name(), &mut name);
        let mut info = EventInfo::default();
        span.record(&mut info);
        let trace_data = TraceData::NewSpan {
            id: id.into_u64(),
            name,
            name_len,
            info,
        };
        let trace_packet = TracePacket::now_with_data(trace_data);
        self.send(trace_packet);
        id
    }

    fn record(&self, span: &span::Id, _: &span::Record) {
        let trace_data = TraceData::Record {
            span: span.into_u64(),
        };
        let trace_packet = TracePacket::now_with_data(trace_data);
        self.send(trace_packet);
    }

    fn record_follows_from(&self, _span: &span::Id, _follows: &span::Id) {}

    fn event(&self, event: &tracing::Event) {
        let mut name = [0u8; MAX_INFO_SIZE];
        let name_len = truncate(event.metadata().name(), &mut name);
        let mut info = EventInfo::default();
        event.record(&mut info);
        let trace_data = TraceData::Event {
            parent_span: self.current_span().id().map(|id| id.into_u64()),
            name,
            name_len,
            info,
        };
        let trace_packet = TracePacket::now_with_data(trace_data);
        self.send(trace_packet);
    }

    fn enter(&self, span: &span::Id) {
        let trace_data = TraceData::Enter {
            span: span.into_u64(),
        };
        let trace_packet = TracePacket::now_without_process(trace_data);
        self.send(trace_packet);
    }

    fn exit(&self, span: &span::Id) {
        let trace_data = TraceData::Exit {
            span: span.into_u64(),
        };
        let trace_packet = TracePacket::now_without_process(trace_data);
        self.send(trace_packet);
    }
}
