// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Error};
use feo_tracing::protocol;
use std::time;
use std::time::SystemTime;

pub type ProcessId = u32;
pub type ThreadId = u32;
pub type Id = u64;
pub type Value = serde_json::Value; // TODO

/// A process identifier
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Process {
    /// Process ID
    pub id: ProcessId,
    /// Process name
    pub name: Option<String>,
}

/// A thread identifier
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Thread {
    /// Thread ID
    pub id: ThreadId,
    /// Thread name
    pub name: Option<String>,
}

/// Trace data.
#[derive(Debug)]
pub enum TraceData {
    /// Process spawned (connected)
    Exec,
    /// Process exited (disconnected)
    Exit,
    /// New span created
    NewSpan { id: Id, attributes: Value },
    /// Record added to span
    Record { id: Id, event: Value },
    /// Event emitted
    Event {
        /// Parent span of the event
        parent_span: Option<Id>,
        /// Event data. For simplicity, we use a JSON value here. This might be suboptimal for
        /// performance.
        event: Value,
    },
    /// Span entered
    EnterSpan { id: Id },
    /// Span exited
    ExitSpan { id: Id },
}

#[derive(Debug, Default)]
pub struct Metadata {
    /// The size of the wire message
    pub wire_size: Option<u64>,
}

/// A trace packet
#[derive(Debug)]
pub struct TracePacket {
    /// Timestamp of the trace packet based on UNIX epoch
    pub timestamp: SystemTime,
    /// Process information
    pub process: Process,
    /// Process information
    pub thread: Option<Thread>,
    /// Trace data
    pub data: TraceData,
    /// Metadata
    pub metadata: Metadata,
}

impl TracePacket {
    /// Create a new trace packet
    pub fn new(
        timestamp: SystemTime,
        process: Process,
        thread: Option<Thread>,
        data: TraceData,
        metadata: Metadata,
    ) -> Self {
        Self {
            timestamp,
            process,
            thread,
            data,
            metadata,
        }
    }
}

/// Decode a trace packet from a byte slice.
pub fn decode_packet(packet: &[u8]) -> Result<TracePacket, Error> {
    let trace_packet: protocol::TracePacket =
        postcard::from_bytes(packet).context("Failed to deserialize packet")?;

    // Process packet
    let timestamp = time::UNIX_EPOCH + time::Duration::from_nanos(trace_packet.timestamp);
    let process = Process {
        id: trace_packet.process.pid,
        name: None,
    };
    let thread = Some(Thread {
        id: trace_packet.process.tid,
        name: None,
    });
    let data = match trace_packet.data {
        protocol::TraceData::NewSpan { id, attributes } => TraceData::NewSpan {
            id,
            attributes: serde_json::to_value(attributes).expect("invalid attributes"),
        },
        protocol::TraceData::Record { span, values } => TraceData::Event {
            parent_span: Some(span),
            event: serde_json::to_value(values).expect("invalid values"),
        },
        protocol::TraceData::Event { parent_span, event } => TraceData::Event {
            parent_span,
            event: serde_json::to_value(event).expect("invalid event"),
        },
        protocol::TraceData::Enter { span } => TraceData::EnterSpan { id: span },
        protocol::TraceData::Exit { span } => TraceData::ExitSpan { id: span },
    };
    let metadata = Metadata {
        wire_size: Some(packet.len() as u64),
    };
    let packet = TracePacket::new(timestamp, process, thread, data, metadata);

    Ok(packet)
}
