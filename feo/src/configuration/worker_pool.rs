// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! Worker pool builder

use crate::activity::{ActivityBuilder, ActivityId, ActivityIdAndBuilder};
use crate::signalling::{channel, IntraProcReceiver, IntraProcSender, Signal};
use crate::worker_pool::{WorkerId, WorkerPool};
use std::collections::HashMap;

/// Map describing assignments of activities to workers in a worker pool
pub type WorkerPoolAssignments = HashMap<WorkerId, Vec<ActivityIdAndBuilder>>;

/// Configuration of a worker pool to be executed by a FEO agent (primary or secondary)
#[derive(Default)]
pub struct Builder {
    /// Map of activities per worker
    pub assignments: WorkerPoolAssignments,
    /// Workers' stack size
    stack_size: Option<usize>,
}

/// Worker pool builder
impl Builder {
    /// Create a builder from given configuration components
    pub fn new(assignments: WorkerPoolAssignments) -> Self {
        Self {
            assignments,
            stack_size: None,
        }
    }

    /// Provide the pool assignment map
    pub fn assignments<K>(&mut self, assignments: WorkerPoolAssignments) -> &mut Self {
        self.assignments = assignments;
        self
    }

    /// Set worker threads' stack size
    pub fn stack_size(&mut self, stack_size: usize) -> &mut Self {
        self.stack_size = Some(stack_size);
        self
    }

    /// Insert the given activity builder into the pool assignment map
    pub fn activity(
        &mut self,
        worker_id: WorkerId,
        activity_id: ActivityId,
        activity_builder: Box<dyn ActivityBuilder>,
    ) -> &mut Self {
        // make sure, there is no activity with the same id
        assert!(
            !self.contains_activity(activity_id),
            "Activity id {activity_id} already exists in the configuration"
        );

        // Get current set of activity builders
        let builders = self.assignments.get_mut(&worker_id);

        // push new activity into existing list or create new map with a single entry
        match builders {
            Some(builders) => {
                builders.push((activity_id, activity_builder));
            }
            None => {
                let builders: Vec<ActivityIdAndBuilder> = vec![(activity_id, activity_builder)];
                self.assignments.insert(worker_id, builders);
            }
        }
        self
    }

    /// Check if the configuration contains an activity with the given id
    pub fn contains_activity(&self, activity_id: ActivityId) -> bool {
        // Check for all workers
        self.assignments.values().any(|activities| {
            // Check if any of the activities reports to have the given id
            activities.iter().any(|(id, _)| *id == activity_id)
        })
    }

    /// Check if the worker pool assignment map is empty
    pub fn is_assignments_empty(&self) -> bool {
        self.assignments.is_empty()
    }

    /// Build a worker pool using the given parameters.
    /// Returns Some((worker pool, receiver, sender)) if the assignments are non-empty,
    ///         None otherwise;
    /// The receiver will receive signals from the activities in the pool,
    /// The sender is connected to the receiver and allows to send signals from an external source
    pub fn build(
        self,
    ) -> Option<(
        WorkerPool,
        IntraProcSender<Signal>,
        IntraProcReceiver<Signal>,
    )> {
        // Return None, if no assignments have been set
        if self.assignments.is_empty() {
            return None;
        }

        // Otherwise create channel for intra-process forwarding of ready-signals from the worker pool
        let (intra_ready_sender, intra_ready_receiver) = channel::<Signal>();

        // Create and return the worker pool together with receiver and sender
        Some((
            WorkerPool::new(self.assignments, &intra_ready_sender, self.stack_size),
            intra_ready_sender,
            intra_ready_receiver,
        ))
    }
}
