// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! Implementation of a recorder for direct scheduler-to-worker signalling

use crate::agent::NodeAddress;
use crate::ids::AgentId;
use crate::recording::recorder::{FileRecorder, RecordingRules};
use crate::recording::registry::TypeRegistry;
use crate::signalling::common::interface::ConnectRecorder;
use crate::signalling::direct::recorder::{TcpRecorderConnector, UnixRecorderConnector};
use alloc::boxed::Box;
use core::time::Duration;

/// Configuration of a recorder
pub struct RecorderConfig<'r> {
    /// ID of the recorder
    pub id: AgentId,
    /// File to which to write recorded data
    pub record_file: &'static str,
    /// Rules about which data to record
    pub rules: RecordingRules,
    /// Registry with types sent on topics
    pub registry: &'r TypeRegistry,
    /// Timeout for receiving calls on the connector
    pub receive_timeout: Duration,
    /// Endpoint on which the scheduler connector is listening
    pub endpoint: NodeAddress,
}

/// Recorder agent
pub struct Recorder<'s> {
    /// Wrapped file recorder
    recorder: FileRecorder<'s>,
}

impl<'s> Recorder<'s> {
    /// Create a new instance
    pub fn new<'c: 's>(config: RecorderConfig<'c>) -> Self {
        let RecorderConfig {
            id,
            endpoint,
            receive_timeout,
            record_file,
            rules,
            registry,
        } = config;

        let mut connector = match endpoint {
            NodeAddress::Tcp(addr) => {
                Box::new(TcpRecorderConnector::new(id, addr)) as Box<dyn ConnectRecorder>
            }
            NodeAddress::UnixSocket(path) => {
                Box::new(UnixRecorderConnector::new(id, path)) as Box<dyn ConnectRecorder>
            }
        };
        connector
            .connect_remote()
            .expect("failed to connect to scheduler");

        let recorder =
            FileRecorder::new(id, connector, receive_timeout, record_file, rules, registry)
                .unwrap();

        Self { recorder }
    }

    /// Run the agent
    pub fn run(&mut self) {
        self.recorder.run();
    }
}
