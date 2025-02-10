// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! Collect trace data - placeholder

use crate::data;
use anyhow::{Context, Error};
use feo_log::{debug, info, warn};
use feo_tracing::protocol;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::SystemTime;
use tokio::sync::mpsc;
use tokio::task;
use tokio_seqpacket::{UnixSeqpacket, UnixSeqpacketListener};

pub const UNIX_PACKET_PATH: &str = "/tmp/feo-tracer.sock";

pub async fn listen(path: &Path, sink: mpsc::Sender<data::TracePacket>) -> Result<(), Error> {
    // Bind
    info!("Binding to {path:?}");
    let mut listener = UnixSeqpacketListener::bind(path)?;

    // Listen
    info!("Listening on {path:?}");
    loop {
        let socket = listener
            .accept()
            .await
            .context("failed to accept packet connection")?;

        debug!("Accepted seqpacket connection");
        task::spawn(connection(socket, sink.clone()));
    }
}

async fn connection(socket: UnixSeqpacket, sink: mpsc::Sender<data::TracePacket>) {
    // Retrieve the PID of the peer
    let pid = socket.peer_cred().unwrap().pid().unwrap() as u32;

    // Capture the process name for the peer
    let process_name = fs::read_to_string(format!("/proc/{}/comm", pid))
        .map(|name| name.trim_end().to_string())
        .ok();

    // Create a cache for the thread names in order to avoid frequent reads of procfs entries
    let mut thread_cache = ThreadCache::new(pid);

    // Buffer for incoming packets
    let mut buffer = [0u8; protocol::MAX_PACKET_SIZE];

    info!(
        "Processing messages from {pid:x} ({})",
        process_name.as_deref().unwrap_or("")
    );

    // Send a process exec event
    sink.send(data::TracePacket {
        timestamp: SystemTime::now(),
        process: data::Process {
            id: pid,
            name: process_name.clone(),
        },
        thread: None,
        data: data::TraceData::Exec,
        metadata: data::Metadata::default(),
    })
    .await
    .expect("channel error");

    // Loop on messages received via the socket. Each message contains a full valid trace packet
    loop {
        let len = match socket.recv(&mut buffer).await {
            Ok(0) => {
                info!("Connection from {pid} closed");
                break;
            }
            Ok(len) => len,
            Err(e) => {
                warn!("Failed to receive data from {pid}: {e:?}. Closing connection",);
                break;
            }
        };

        let buffer = &buffer[..len];

        // Decode the packet
        let mut packet = match data::decode_packet(buffer) {
            Ok(packet) => packet,
            Err(e) => {
                warn!("Failed to decode packet from {pid}: {e:?}. Disconnecting",);
                break;
            }
        };

        // Extend the process and thread names
        packet.process.name = process_name.clone();
        if let Some(ref mut thread) = packet.thread {
            thread.name = thread_cache.get(thread.id).map(|s| s.to_string());
        }

        // Forward packet to consumers connected to the sink
        sink.send(packet).await.expect("channel error");
    }

    // Send a process exit event
    sink.send(data::TracePacket {
        timestamp: SystemTime::now(),
        process: data::Process {
            id: pid,
            name: None,
        },
        thread: None,
        data: data::TraceData::Exit,
        metadata: data::Metadata::default(),
    })
    .await
    .expect("channel error");
}

/// Cache for thread names in order to avoid frequent reads of procfs entries.
#[derive(Debug)]
struct ThreadCache {
    /// PID of the process
    pid: u32,
    /// Map of thread names indexed by their TID
    names: HashMap<u32, Option<String>>,
}

impl<'a> ThreadCache {
    /// Create a new thread cache for a given process
    fn new(pid: u32) -> Self {
        Self {
            pid,
            names: HashMap::new(),
        }
    }

    /// Get the name of a thread or query the kernel if not cached
    fn get(&'a mut self, tid: u32) -> Option<&'a str> {
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
