// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use super::worker::{Worker, WorkerId};
use crate::activity::{ActivityId, ActivityIdAndBuilder};
use crate::signalling::{self, Sender, Signal};
use std::collections::HashMap;

/// Trigger that can trigger an activity in a worker pool
pub struct WorkerPoolTrigger {
    trigger_senders: HashMap<ActivityId, Box<dyn Sender<Signal>>>,
}

impl WorkerPoolTrigger {
    /// Trigger an activity in the pool using the given signal
    pub fn trigger(&mut self, signal: Signal) {
        let activity_id = signal.activity_id().unwrap_or_else(|| {
            panic!("received unexpected trigger signal {signal:?} for worker pool")
        });

        // Determine the sender to the target activity
        let sender = self
            .trigger_senders
            .get_mut(&activity_id)
            .unwrap_or_else(|| panic!("failed to trigger unknown activity id {activity_id}"));

        // send the signal
        sender
            .send(signal)
            .expect("failed to transmit signal to activity in worker pool");
    }
}

/// Listener that can wait for events or test the state of a worker pool
pub struct WorkerPoolListener {
    activities_ready: HashMap<ActivityId, bool>,
    ready_receiver: Box<dyn signalling::Receiver<Signal>>,
}

impl WorkerPoolListener {
    /// Create a new worker pool listener
    pub fn new(
        activity_ids: &[ActivityId],
        ready_receiver: impl signalling::Receiver<Signal> + 'static,
    ) -> WorkerPoolListener {
        let mut activities_ready: HashMap<ActivityId, bool> = Default::default();
        for act_id in activity_ids {
            // Initialize activity-ready flag for the current activity id and check for duplicates
            let previous = activities_ready.insert(*act_id, false);
            assert!(
                previous.is_none(),
                "duplicate activity id {act_id} given to WorkerPoolListener"
            );
        }

        WorkerPoolListener {
            activities_ready,
            ready_receiver: Box::new(ready_receiver),
        }
    }

    /// Wait until next ready flag has been received
    pub fn wait_next_ready(&mut self) {
        // Wait for next ready signal from one of the workers
        loop {
            let signal = self
                .ready_receiver
                .recv()
                .expect("failed to get signal from worker");
            if let Signal::Ready((activity_id, _)) = signal {
                // Set corresponding ready flag and return
                self.activities_ready.insert(activity_id, true);
                break;
            }
        }
    }

    /// Clear all ready flags
    pub fn clear_ready(&mut self) {
        self.activities_ready.values_mut().for_each(|v| *v = false);
    }

    /// Check if all ready flags are set
    pub fn is_all_ready(&self, activity_ids: &[ActivityId]) -> bool {
        self.activities_ready
            .iter()
            .filter(|(k, _)| activity_ids.contains(k))
            .all(|(_, v)| *v)
    }

    /// Return an iterator to the map of ready flags
    pub fn ready_iter(&self) -> std::collections::hash_map::Iter<ActivityId, bool> {
        self.activities_ready.iter()
    }
}

/// A pool of worker threads
pub struct WorkerPool {
    workers: Vec<Worker>,
    activity_ids: Vec<ActivityId>,
    workpool_trigger: WorkerPoolTrigger,
}

impl WorkerPool {
    /// Create a new worker pool
    pub fn new(
        builder_map: HashMap<WorkerId, Vec<ActivityIdAndBuilder>>,
        ready_sender: &(impl Sender<Signal> + Clone + 'static),
        stack_size: Option<usize>,
    ) -> WorkerPool {
        assert!(
            !builder_map.is_empty(),
            "cannot create worker pool from empty configuration"
        );

        let mut trigger_senders: HashMap<ActivityId, Box<dyn Sender<Signal>>> = Default::default();
        let mut workers: Vec<Worker> = vec![];
        let mut activity_ids: Vec<ActivityId> = vec![];

        // Loop over all required worker ids, create worker with trigger channel and ready channel
        for (worker_id, builders) in builder_map {
            // Create channel for triggering activities in the given worker
            let (trigger_sender, trigger_receiver) = signalling::channel();

            // Loop over all activities to be executed by the current worker
            for (act_id, _) in &builders {
                // Store sender to use for triggering this activity
                let previous = trigger_senders.insert(*act_id, Box::new(trigger_sender.clone()));

                // Make sure the activity has not been assigned before (i.e. to another worker)
                assert!(previous.is_none(), "duplicate activity id");

                // Initialize activity-ready flag for the current activity
                activity_ids.push(*act_id);
            }

            workers.push(Worker::new(
                worker_id,
                stack_size,
                builders,
                trigger_receiver,
                ready_sender.clone(),
            ));
        }

        WorkerPool {
            workers,
            activity_ids,
            workpool_trigger: WorkerPoolTrigger { trigger_senders },
        }
    }

    /// Create a listener to this worker pool
    pub fn listener(
        &self,
        ready_receiver: impl signalling::Receiver<Signal> + 'static,
    ) -> WorkerPoolListener {
        WorkerPoolListener::new(&self.activity_ids, ready_receiver)
    }

    /// Split the worker pool into a set of workers and a WorkerPoolTrigger object
    pub fn split(self) -> (Vec<Worker>, WorkerPoolTrigger) {
        (self.workers, self.workpool_trigger)
    }

    /// Trigger an activity in the pool using the given signal
    pub fn trigger(&mut self, signal: Signal) {
        self.workpool_trigger.trigger(signal)
    }
}
