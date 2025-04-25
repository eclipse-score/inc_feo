// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! Implementation of the primary agent for mixed signalling using sockets and mpsc channels

use crate::activity::ActivityIdAndBuilder;
use crate::agent::NodeAddress;
use crate::error::Error;
use crate::ids::{ActivityId, AgentId, WorkerId};
use crate::scheduler::Scheduler;
use crate::signalling::common::interface::{ConnectScheduler, ConnectWorker};
use crate::signalling::relayed::sockets_mpsc::{SchedulerConnectorTcp, SchedulerConnectorUnix};
use crate::timestamp;
use crate::worker::Worker;
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::time::Duration;
use std::collections::HashMap;
use std::thread::{self, JoinHandle};

/// Configuration of the primary agent
pub struct PrimaryConfig {
    /// Id of the primary agent
    pub id: AgentId,
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
    /// The socket address to which secondary agents' senders shall connect
    pub bind_address_senders: NodeAddress,
    /// The socket address to which secondary agents' receivers shall connect
    pub bind_address_receivers: NodeAddress,
    // Map of all workers to agent ids
    pub worker_agent_map: HashMap<WorkerId, AgentId>,
    /// Map of all activities to worker ids
    pub activity_worker_map: HashMap<ActivityId, WorkerId>,
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
            id,
            cycle_time,
            activity_dependencies,
            recorder_ids,
            bind_address_senders,
            bind_address_receivers,
            worker_assignments,
            timeout,
            worker_agent_map,
            activity_worker_map,
        } = config;

        // Create scheduler connector depending on given address types and
        // get worker connector builders to be moved into worker threads
        let (mut connector, mut builders) = match (bind_address_receivers, bind_address_senders) {
            (NodeAddress::Tcp(bind_receivers), NodeAddress::Tcp(bind_senders)) => {
                let mut connector = Box::new(SchedulerConnectorTcp::new(
                    id,
                    bind_senders,
                    bind_receivers,
                    timeout,
                    worker_agent_map,
                    activity_worker_map,
                    recorder_ids.clone(),
                ));
                let builders = connector.worker_connector_builders();
                (connector as Box<dyn ConnectScheduler>, builders)
            }
            (NodeAddress::UnixSocket(bind_receivers), NodeAddress::UnixSocket(bind_senders)) => {
                let mut connector = Box::new(SchedulerConnectorUnix::new(
                    id,
                    bind_senders,
                    bind_receivers,
                    timeout,
                    worker_agent_map,
                    activity_worker_map,
                    recorder_ids.clone(),
                ));
                let builders = connector.worker_connector_builders();
                (connector as Box<dyn ConnectScheduler>, builders)
            }
            _ => {
                panic!("bind addresses must either be both TCP socket addresses or both Unix socket paths")
            }
        };

        // Create worker threads first so that the connector of the scheduler can connect
        let _worker_threads = worker_assignments
            .into_iter()
            .map(|(id, activities)| {
                let connector_builder = builders.remove(&id).expect("missing connector builder");
                thread::spawn(move || {
                    let mut connector = connector_builder();
                    connector.connect_remote().expect("failed to connect");

                    let activity_builders = activities;
                    let worker = Worker::new(id, activity_builders, connector, timeout);
                    worker.run().expect("failed to run worker");
                })
            })
            .collect();

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
