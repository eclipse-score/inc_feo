// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! Implementation of a recorder for mixed signalling using sockets and mpsc channels

use crate::agent::NodeAddress;
use crate::ids::AgentId;
use crate::recording::recorder::{FileRecorder, RecordingRules};
use crate::recording::registry::TypeRegistry;
use crate::signalling::common::interface::ConnectRecorder;
use crate::signalling::relayed::sockets_mpsc::{RecorderConnectorTcp, RecorderConnectorUnix};
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
    /// Address on which the scheduler connector is listening for sender channel connections
    pub bind_address_senders: NodeAddress,
    /// Address on which the scheduler connector is listening for receiver channel connections
    pub bind_address_receivers: NodeAddress,
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
            bind_address_senders,
            bind_address_receivers,
            receive_timeout,
            record_file,
            rules,
            registry,
        } = config;

        let mut connector = match (bind_address_receivers, bind_address_senders) {
            (NodeAddress::Tcp(bind_receivers), NodeAddress::Tcp(bind_senders)) => Box::new(
                RecorderConnectorTcp::new(id, bind_senders, bind_receivers, receive_timeout),
            )
                as Box<dyn ConnectRecorder>,
            (NodeAddress::UnixSocket(bind_receivers), NodeAddress::UnixSocket(bind_senders)) => {
                Box::new(RecorderConnectorUnix::new(
                    id,
                    bind_senders,
                    bind_receivers,
                    receive_timeout,
                )) as Box<dyn ConnectRecorder>
            }

            _ => {
                panic!("bind addresses must either be both TCP socket addresses or both Unix socket paths")
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
