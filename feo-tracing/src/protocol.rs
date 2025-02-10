// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use std::process;
use std::time::{self, UNIX_EPOCH};
use tracing_serde_structured::{SerializeAttributes, SerializeEvent, SerializeRecord};

pub type Id = u64;

pub const MAX_PACKET_SIZE: usize = 16 * 1024;

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
pub enum TraceData<'a> {
    NewSpan {
        id: Id,
        #[serde(borrow)]
        attributes: SerializeAttributes<'a>,
    },
    Record {
        span: Id,
        #[serde(borrow)]
        values: SerializeRecord<'a>,
    },
    Event {
        parent_span: Option<Id>,
        #[serde(borrow)]
        event: SerializeEvent<'a>,
    },
    Enter {
        span: Id,
    },
    Exit {
        span: Id,
    },
}

// Safety: For now the whole application runs single threadded so this is safe to
// manually implement Send here.
unsafe impl Send for TraceData<'_> {}

/// A trace packet
#[derive(Debug, Serialize, Deserialize)]
pub struct TracePacket<'a> {
    pub timestamp: u64, // nanoseconds
    pub process: Process,
    #[serde(borrow)]
    pub data: TraceData<'a>,
}

impl<'a> TracePacket<'a> {
    pub fn new(timestamp: u64, process: Process, data: TraceData<'a>) -> TracePacket<'a> {
        TracePacket {
            timestamp,
            process,
            data,
        }
    }

    pub fn now_with_data(data: TraceData<'a>) -> TracePacket<'a> {
        TracePacket {
            timestamp: timestamp(),
            process: Process::this(),
            data,
        }
    }
}

/// Now epoch in nanoseconds
fn timestamp() -> u64 {
    time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
}

mod thread {
    /// The type of a thread id
    pub type ThreadId = u32;

    /// Get the current thread id
    pub fn id() -> ThreadId {
        // Safety: gettid(2) says this never fails
        unsafe { libc::gettid() as u32 }
    }
}
