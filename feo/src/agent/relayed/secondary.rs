// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! Implementation of a secondary agent for mixed signalling using sockets and mpsc channels

use crate::activity::ActivityIdAndBuilder;
use crate::agent::NodeAddress;
use crate::ids::{ActivityId, AgentId, WorkerId};
use crate::signalling::common::interface::ConnectWorker;
use crate::signalling::relayed::sockets_mpsc::{SecondaryConnectorTcp, SecondaryConnectorUnix};
use crate::signalling::relayed::ConnectSecondary;
use crate::worker::Worker;
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::time::Duration;
use feo_log::debug;
use std::collections::HashMap;
use std::thread::{self, JoinHandle};

/// Configuration of a secondary agent
pub struct SecondaryConfig {
    /// Id of the primary agent
    pub id: AgentId,
    /// Activity IDs with builders to run per [WorkerId]
    pub worker_assignments: Vec<(WorkerId, Vec<ActivityIdAndBuilder>)>,
    /// Maximum time for a worker to make no progress without panicking
    pub timeout: Duration,
    /// Address on which the scheduler connector is listening for sender channel connections
    pub bind_address_senders: NodeAddress,
    /// Address on which the scheduler connector is listening for receiver channel connections
    pub bind_address_receivers: NodeAddress,
}

/// Secondary agent
pub struct Secondary {
    /// ID
    id: AgentId,
    /// Connector from the secondary to to the primary process
    connector: Option<Box<dyn ConnectSecondary>>,
    /// Handles to the worker threads
    worker_threads: Vec<JoinHandle<()>>,
}

impl Secondary {
    /// Create a new instance
    pub fn new(config: SecondaryConfig) -> Self {
        let SecondaryConfig {
            id,
            worker_assignments,
            timeout,
            bind_address_senders,
            bind_address_receivers,
        } = config;

        let activity_worker_map: HashMap<ActivityId, WorkerId> = worker_assignments
            .iter()
            .flat_map(|(wid, acts)| acts.iter().map(|(aid, _)| (*aid, *wid)))
            .collect();

        // Create SecondaryConnector and builders of WorkerConnectors
        let (connector, mut connector_builders) = match (
            bind_address_receivers,
            bind_address_senders,
        ) {
            (NodeAddress::Tcp(bind_receivers), NodeAddress::Tcp(bind_senders)) => {
                let (connector, builders) = SecondaryConnectorTcp::create(
                    id,
                    activity_worker_map,
                    bind_senders,
                    bind_receivers,
                    timeout,
                );
                (Box::new(connector) as Box<dyn ConnectSecondary>, builders)
            }
            (NodeAddress::UnixSocket(bind_receivers), NodeAddress::UnixSocket(bind_senders)) => {
                let (connector, builders) = SecondaryConnectorUnix::create(
                    id,
                    activity_worker_map,
                    bind_senders,
                    bind_receivers,
                    timeout,
                );
                (Box::new(connector) as Box<dyn ConnectSecondary>, builders)
            }
            _ => {
                panic!("bind addresses must either be both TCP socket addresses or both Unix socket paths")
            }
        };

        let worker_threads = worker_assignments
            .into_iter()
            .map(|(id, activities)| {
                let connector_builder = connector_builders
                    .remove(&id)
                    .expect("missing connector builder");
                thread::spawn(move || {
                    let mut connector = connector_builder();
                    connector.connect_remote().expect("failed to connect");
                    let worker = Worker::new(id, activities, connector, timeout);

                    worker.run().expect("failed to run worker");
                })
            })
            .collect();

        Self {
            id,
            connector: Some(connector),
            worker_threads,
        }
    }

    /// Run the agent
    pub fn run(mut self) {
        debug!("Running secondary with ID {:?}", self.id);

        self.connector.take().unwrap().run_and_connect();

        for th in self.worker_threads {
            th.join().unwrap();
        }
    }
}
