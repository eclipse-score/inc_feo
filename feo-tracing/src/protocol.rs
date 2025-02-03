// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use core::fmt::Debug;
use serde::{Deserialize, Serialize};
use std::process;
use std::time::{self, UNIX_EPOCH};
use tracing::field::Field;
use tracing_subscriber::field::Visit;

pub const MAX_INFO_SIZE: usize = 30;

/// The maximal allowed size of serialized packet data
///
/// Packets exceeding this size will be dropped with an error message
pub const MAX_PACKET_SIZE: usize = 148;

type Id = u64;

#[derive(Debug, Serialize, Deserialize)]
pub struct Process {
    pub pid: u32,
    pub tid: u32,
}

impl Process {
    fn this() -> Self {
        Self {
            pid: process::id(),
            tid: thread::id(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum TraceData {
    NewSpan {
        id: Id,
        name: [u8; MAX_INFO_SIZE],
        name_len: usize,
        info: EventInfo,
    },
    Record {
        span: Id,
    },
    Event {
        parent_span: Option<Id>,
        name: [u8; MAX_INFO_SIZE],
        name_len: usize,
        info: EventInfo,
    },
    Enter {
        span: Id,
    },
    Exit {
        span: Id,
    },
}

/// Additional info that can be attached to an event
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct EventInfo {
    pub name: [u8; MAX_INFO_SIZE],
    pub name_len: Option<usize>,
    pub value: [u8; MAX_INFO_SIZE],
    pub value_len: usize,
}

impl Visit for EventInfo {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.name_len = Some(truncate(field.name(), &mut self.name));
        self.value_len = truncate(value, &mut self.value);
    }

    fn record_debug(&mut self, _: &Field, _: &dyn Debug) {}
}

/// A trace packet
#[derive(Debug, Serialize, Deserialize)]
pub struct TracePacket {
    pub timestamp: u64, // nanoseconds
    pub process: Option<Process>,
    pub data: TraceData,
}

impl TracePacket {
    pub fn new(timestamp: u64, process: Option<Process>, data: TraceData) -> TracePacket {
        TracePacket {
            timestamp,
            process,
            data,
        }
    }

    pub fn now_with_data(data: TraceData) -> TracePacket {
        TracePacket {
            timestamp: timestamp(),
            process: Some(Process::this()),
            data,
        }
    }

    /// Create trace packet with data except process information, thus saving time of syscall
    pub fn now_without_process(data: TraceData) -> TracePacket {
        TracePacket {
            timestamp: timestamp(),
            process: None,
            data,
        }
    }
}

/// Truncate the given string slice into the given output byte buffer
///
/// Returns the number of valid bytes in the buffer
pub fn truncate(slice: &str, buffer: &mut [u8]) -> usize {
    let len = trunc_len(slice, buffer.len());
    buffer[0..len].clone_from_slice(&slice.as_bytes()[0..len]);
    len
}

/// Now epoch in nanoseconds
fn timestamp() -> u64 {
    time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
}

/// Return the byte length of the given utf-8 encoded string slice
/// truncated to fit into the specified maximal length in bytes
fn trunc_len(slice: &str, max_byte_len: usize) -> usize {
    let mut len: usize = 0;
    for (pos, _) in slice.char_indices() {
        if pos >= max_byte_len {
            break;
        }
        if pos + 1 > len && pos < max_byte_len {
            len = pos + 1;
        }
    }
    len
}

mod thread {
    /// The type of a thread id
    pub(crate) type ThreadId = u32;

    /// Get the current thread id
    pub(crate) fn id() -> ThreadId {
        // Safety: gettid(2) says this never fails
        unsafe { libc::gettid() as u32 }
    }
}
