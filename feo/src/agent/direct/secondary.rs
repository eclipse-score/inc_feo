// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! Implementation of a secondary agent for direct scheduler-to-worker signalling

use crate::activity::ActivityIdAndBuilder;
use crate::agent::NodeAddress;
use crate::ids::{AgentId, WorkerId};
use crate::signalling::common::interface::ConnectWorker;
use crate::signalling::direct::worker::{TcpWorkerConnector, UnixWorkerConnector};
use crate::worker::Worker;
use alloc::vec::Vec;
use core::time::Duration;
use feo_log::debug;
use std::thread::{self, JoinHandle};

/// Configuration of a secondary agent
pub struct SecondaryConfig {
    /// ID of this agent
    pub id: AgentId,
    /// Activity IDs with builders to run per [WorkerId]
    pub worker_assignments: Vec<(WorkerId, Vec<ActivityIdAndBuilder>)>,
    /// Maximum time for a worker to make no progress without panicking
    pub timeout: Duration,
    /// Endpoint on which the scheduler connector is listening
    pub endpoint: NodeAddress,
}

/// Secondary agent
pub struct Secondary {
    /// ID
    id: AgentId,
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
            endpoint,
        } = config;

        let worker_threads = worker_assignments
            .into_iter()
            .map(|(id, activities)| {
                let endpoint = endpoint.clone();
                thread::spawn(move || match endpoint {
                    NodeAddress::Tcp(addr) => {
                        let mut connector =
                            TcpWorkerConnector::new(addr, activities.iter().map(|(id, _)| *id));
                        connector.connect_remote().expect("failed to connect");
                        let worker = Worker::new(id, activities, connector, timeout);

                        worker.run().expect("failed to run worker");
                    }
                    NodeAddress::UnixSocket(path) => {
                        let mut connector =
                            UnixWorkerConnector::new(path, activities.iter().map(|(id, _)| *id));
                        connector.connect_remote().expect("failed to connect");
                        let worker = Worker::new(id, activities, connector, timeout);

                        worker.run().expect("failed to run worker");
                    }
                })
            })
            .collect();

        Self { id, worker_threads }
    }

    /// Run the agent
    pub fn run(self) {
        debug!("Running secondary with ID {:?}", self.id);

        for th in self.worker_threads {
            th.join().unwrap();
        }
    }
}
