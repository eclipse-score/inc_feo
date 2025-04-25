// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use crate::io::ThreadNameCache;
use anyhow::Error;
use feo_tracing::protocol;
use feo_tracing::protocol::EventInfo;
use std::time;
use std::time::SystemTime;

pub type ProcessId = u32;
pub type ThreadId = u32;
pub type Id = u64;

/// A process identifier
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Process {
    /// Process ID
    pub id: ProcessId,
    /// Process name
    pub name: Option<String>,
}

/// A thread identifier
#[derive(Debug, Clone, Hash, PartialEq, Eq, Default)]
pub struct Thread {
    /// Thread ID
    pub id: ThreadId,
    /// Thread name
    pub name: Option<String>,
}

/// A trace record to be stored in the trace output
#[derive(Debug)]
pub struct TraceRecord {
    /// Timestamp of the trace packet based on UNIX epoch
    pub timestamp: SystemTime,
    /// Process information
    pub process: Process,
    /// Process information
    pub thread: Option<Thread>,
    /// Trace data
    pub data: RecordData,
}

impl TraceRecord {
    /// Create a new trace packet
    pub fn new(
        timestamp: SystemTime,
        process: Process,
        thread: Option<Thread>,
        data: RecordData,
    ) -> Self {
        Self {
            timestamp,
            process,
            thread,
            data,
        }
    }
}

/// Trace data.
#[derive(Debug)]
pub enum RecordData {
    /// Process spawned (connected)
    Exec,
    /// Process exited (disconnected)
    Exit,
    /// New span created
    NewSpan {
        id: Id,
        name: String,
        info: RecordEventInfo,
    },
    /// Record added to span
    Record { span: Id },
    /// Event emitted
    Event {
        /// Parent span of the event
        parent_span: Option<Id>,
        name: String,
        info: RecordEventInfo,
    },
    /// Span entered
    EnterSpan { id: Id },
    /// Span exited
    ExitSpan { id: Id },
}

impl From<protocol::TraceData> for RecordData {
    fn from(trace_data: protocol::TraceData) -> Self {
        match trace_data {
            protocol::TraceData::NewSpan {
                id,
                name,
                name_len,
                info,
            } => {
                let record_info: RecordEventInfo = info.into();
                RecordData::NewSpan {
                    id,
                    name: String::from_utf8_lossy(&name[0..name_len]).to_string(),
                    info: record_info,
                }
            }
            protocol::TraceData::Record { span } => RecordData::Record { span },
            protocol::TraceData::Event {
                parent_span,
                name,
                name_len,
                info,
            } => {
                let record_info: RecordEventInfo = info.into();
                RecordData::Event {
                    parent_span,
                    name: String::from_utf8_lossy(&name[0..name_len]).to_string(),
                    info: record_info,
                }
            }
            protocol::TraceData::Enter { span } => RecordData::EnterSpan { id: span },
            protocol::TraceData::Exit { span } => RecordData::ExitSpan { id: span },
        }
    }
}

#[derive(Debug, Default)]
pub struct RecordEventInfo {
    pub name: Option<String>,
    pub value: String,
}

impl From<EventInfo> for RecordEventInfo {
    fn from(info: EventInfo) -> Self {
        let name_len = info.name_len;
        let name = name_len.map(|len| String::from_utf8_lossy(&info.name[0..len]).to_string());
        let value = String::from_utf8_lossy(&info.value[0..info.value_len]).to_string();
        RecordEventInfo { name, value }
    }
}

/// Decode a trace packet from a byte slice.
pub(crate) fn decode_packet(
    pid: u32,
    trace_packet: protocol::TracePacket,
    thread_cache: &mut ThreadNameCache,
    process_name: Option<String>,
) -> Result<TraceRecord, Error> {
    // Process packet
    let timestamp = time::UNIX_EPOCH + time::Duration::from_nanos(trace_packet.timestamp);
    let data = trace_packet.data.into();
    let process = Process {
        id: pid,
        name: process_name,
    };

    let thread = trace_packet.process.map(|p| Thread {
        id: p.tid,
        name: thread_cache.get(p.tid).map(|s| s.to_string()),
    });

    let packet = TraceRecord::new(timestamp, process, thread, data);

    Ok(packet)
}
