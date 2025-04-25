// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! Collect trace data - placeholder

use crate::data;
use anyhow::{Context, Error};
use feo_log::{debug, info, warn};
use feo_tracing::protocol;
use postcard::accumulator::{CobsAccumulator, FeedResult};
use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use std::time::SystemTime;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::mpsc;
use tokio::task;

/// Size of the buffer (bytes) used for deserializing incoming trace packets
const READ_BUFFER_SIZE: usize = 32 * protocol::MAX_PACKET_SIZE;

pub async fn listen(path: &Path, sink: mpsc::Sender<data::TraceRecord>) -> Result<(), Error> {
    // Bind
    info!("Binding to {path:?}");
    let listener = UnixListener::bind(path)?;

    // Listen
    info!("Listening on {path:?}");
    loop {
        let (socket, _) = listener
            .accept()
            .await
            .context("failed to accept connection")?;

        debug!("Accepted connection");
        task::spawn(connection(socket, sink.clone()));
    }
}

async fn connection(socket: UnixStream, sink: mpsc::Sender<data::TraceRecord>) {
    // Retrieve the PID of the peer
    let pid = socket.peer_cred().unwrap().pid().unwrap() as u32;

    // Capture the process name for the peer
    let process_name = fs::read_to_string(format!("/proc/{}/comm", pid))
        .map(|name| name.trim_end().to_string())
        .ok();

    info!(
        "Processing messages from {pid:x} ({})",
        process_name.as_deref().unwrap_or("")
    );

    // Send a process exec event
    sink.send(data::TraceRecord {
        timestamp: SystemTime::now(),
        process: data::Process {
            id: pid,
            name: process_name.clone(),
        },
        thread: None,
        data: data::RecordData::Exec,
    })
    .await
    .expect("channel error");

    // Create a cache for the thread names in order to avoid frequent reads of procfs entries
    let mut thread_name_cache = ThreadNameCache::new(pid);

    // Buffers for incoming packets and postcard deserialization
    let mut read_buffer = [0u8; READ_BUFFER_SIZE];
    let mut cobs_buffer: CobsAccumulator<READ_BUFFER_SIZE> = CobsAccumulator::new();

    'deser: loop {
        socket.readable().await.expect("socket error");

        let len = match socket.try_read(&mut read_buffer) {
            Ok(0) => {
                info!("Connection from {pid} closed");
                break;
            }
            Ok(len) => len,
            Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                continue;
            }
            Err(e) => {
                warn!("Failed to receive data from {pid}: {e:?}. Closing connection");
                break;
            }
        };

        let buffer = &read_buffer[..len];
        let mut remaining = buffer;

        while !remaining.is_empty() {
            remaining = match cobs_buffer.feed_ref::<protocol::TracePacket>(remaining) {
                FeedResult::Consumed => break,
                FeedResult::OverFull(_) => {
                    warn!("Deserialization buffer overflow in {pid}. Closing connection");
                    break 'deser;
                }
                FeedResult::DeserError(remaining) => remaining,
                FeedResult::Success { data, remaining } => {
                    // Data successfully decoded, add thread and process info
                    // and transmit to sink
                    let packet = match data::decode_packet(
                        pid,
                        data,
                        &mut thread_name_cache,
                        process_name.clone(),
                    ) {
                        Ok(packet) => packet,
                        Err(e) => {
                            warn!("Failed to decode packet from {pid}: {e:?}. Closing connection",);
                            break 'deser;
                        }
                    };
                    sink.send(packet).await.expect("channel error");
                    remaining
                }
            };
        }
    }

    // Send a process exit event
    sink.send(data::TraceRecord {
        timestamp: SystemTime::now(),
        process: data::Process {
            id: pid,
            name: None,
        },
        thread: None,
        data: data::RecordData::Exit,
    })
    .await
    .expect("channel error");
}

/// Cache for thread names in order to avoid frequent reads of procfs entries.
#[derive(Debug)]
pub struct ThreadNameCache {
    /// PID of the process
    pid: u32,
    /// Map of thread names indexed by their TID
    names: HashMap<u32, Option<String>>,
}

impl<'a> ThreadNameCache {
    /// Create a new thread cache for a given process
    pub fn new(pid: u32) -> Self {
        Self {
            pid,
            names: HashMap::new(),
        }
    }

    /// Get the name of a thread or query the kernel if not cached
    pub fn get(&'a mut self, tid: u32) -> Option<&'a str> {
        self.names
            .entry(tid)
            .or_insert_with(|| {
                let pid = self.pid;
                fs::read_to_string(format!("/proc/{pid}/task/{tid}/comm"))
                    .map(|s| s.trim_end().to_string())
                    .ok()
            })
            .as_deref()
    }
}
