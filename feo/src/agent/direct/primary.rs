// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! Implementation of the primary agent for direct scheduler-to-worker signalling

use crate::activity::ActivityIdAndBuilder;
use crate::agent::NodeAddress;
use crate::error::Error;
use crate::ids::{ActivityId, AgentId, WorkerId};
use crate::scheduler::Scheduler;
use crate::signalling::common::interface::{ConnectScheduler, ConnectWorker};
use crate::signalling::direct::scheduler::{TcpSchedulerConnector, UnixSchedulerConnector};
use crate::signalling::direct::worker::{TcpWorkerConnector, UnixWorkerConnector};
use crate::timestamp;
use crate::worker::Worker;
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::time::Duration;
use std::collections::HashMap;
use std::thread::{self, JoinHandle};

/// Configuration of the primary agent
pub struct PrimaryConfig {
    /// Cycle time of the step loop
    pub cycle_time: Duration,
    /// Dependencies per activity
    pub activity_dependencies: HashMap<ActivityId, Vec<ActivityId>>,
    /// IDs of all recorders for which the scheduler waits
    pub recorder_ids: Vec<AgentId>,
    /// Worker assignments to be run in this agent
    pub worker_assignments: Vec<(WorkerId, Vec<ActivityIdAndBuilder>)>,
    /// Receive timeout of the scheduler's connector
    pub timeout: Duration,
    /// Endpoint on which the connector of the scheduler waits for connections
    pub endpoint: NodeAddress,
}

/// Primary agent
pub struct Primary {
    /// Scheduler
    scheduler: Scheduler,
    /// Handles to the worker threads
    _worker_threads: Vec<JoinHandle<()>>,
}

impl Primary {
    /// Create a new instance
    pub fn new(config: PrimaryConfig) -> Self {
        let PrimaryConfig {
            cycle_time,
            activity_dependencies,
            recorder_ids,
            endpoint,
            worker_assignments,
            timeout,
        } = config;

        // Create worker threads first so that the connector of the scheduler can connect
        let _worker_threads = worker_assignments
            .into_iter()
            .map(|(id, activities)| {
                let endpoint = endpoint.clone();
                thread::spawn(move || match endpoint {
                    NodeAddress::Tcp(addr) => {
                        let mut connector =
                            TcpWorkerConnector::new(addr, activities.iter().map(|(id, _)| *id));
                        connector.connect_remote().expect("failed to connect");

                        let activity_builders = activities;
                        let worker = Worker::new(id, activity_builders, connector, timeout);

                        worker.run().expect("failed to run worker");
                    }
                    NodeAddress::UnixSocket(path) => {
                        let mut connector =
                            UnixWorkerConnector::new(path, activities.iter().map(|(id, _)| *id));
                        connector.connect_remote().expect("failed to connect");

                        let activity_builders = activities;
                        let worker = Worker::new(id, activity_builders, connector, timeout);

                        worker.run().expect("failed to run worker");
                    }
                })
            })
            .collect();

        let mut connector = match endpoint {
            NodeAddress::Tcp(addr) => Box::new(TcpSchedulerConnector::new(
                addr,
                activity_dependencies.keys().cloned(),
                recorder_ids.iter().cloned(),
            )) as Box<dyn ConnectScheduler>,
            NodeAddress::UnixSocket(path) => Box::new(UnixSchedulerConnector::new(
                &path,
                activity_dependencies.keys().cloned(),
                recorder_ids.iter().cloned(),
            )) as Box<dyn ConnectScheduler>,
        };
        connector.connect_remotes().expect("failed to connect");

        let scheduler = Scheduler::new(
            cycle_time,
            timeout,
            activity_dependencies,
            connector,
            recorder_ids,
        );

        Self {
            scheduler,
            _worker_threads,
        }
    }

    /// Run the agent
    pub fn run(&mut self) -> Result<(), Error> {
        // Initialize local time
        timestamp::initialize();

        // Sync time on remotes
        self.scheduler.sync_remotes()?;

        // TODO: Bubble up errors
        self.scheduler.run();

        Ok(())
    }
}
