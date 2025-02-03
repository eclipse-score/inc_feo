// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! Implementation of the primary agent for mpsc-only signalling

use crate::activity::ActivityIdAndBuilder;
use crate::error::Error;
use crate::ids::{ActivityId, AgentId, WorkerId};
use crate::scheduler::Scheduler;
use crate::signalling::common::interface::{ConnectScheduler, ConnectWorker};
use crate::signalling::direct::mpsc::scheduler::SchedulerConnector;
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
            worker_assignments,
            timeout,
        } = config;

        let activity_worker_map: HashMap<ActivityId, WorkerId> = worker_assignments
            .iter()
            .flat_map(|(wid, aid_bld)| aid_bld.iter().map(move |id_b| (id_b.0, *wid)))
            .collect();

        // Create scheduler connector
        let mut connector = Box::new(SchedulerConnector::new(activity_worker_map));

        // Get worker connector builders to be moved into worker threads
        let mut connector_builders = connector.worker_connector_builders();

        // Create worker threads first so that the connector of the scheduler can connect
        let _worker_threads = worker_assignments
            .into_iter()
            .map(|(id, activities)| {
                let connector_builder = connector_builders
                    .remove(&id)
                    .expect("missing connector builder");
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
