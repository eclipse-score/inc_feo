// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! Primary agent builder

use crate::activity::ActivityId;
use crate::agent::primary::{PrimaryAgent, PrimaryAgentConfig};
use crate::signalling::{AgentId, IntraProcReceiver, IntraProcSender, Signal};
use crate::worker_pool::{WorkerId, WorkerPool};
use feo_time::Duration;
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;

/// Map of activity dependencies for the FEO scheduler
pub type ActivityDependencies = HashMap<ActivityId, Vec<ActivityId>>;

pub trait ActivityDependenciesBuilder {
    /// Insert an activity as a dependency of another activity into the map
    fn add_dependency(&mut self, activity_id: ActivityId, dependency: ActivityId);

    /// Insert a list of activities as dependencies of another activity into the map
    fn add_dependencies<'s, K>(&mut self, activity_id: ActivityId, dependencies: K)
    where
        K: IntoIterator<Item = &'s ActivityId>;
}

impl ActivityDependenciesBuilder for ActivityDependencies {
    fn add_dependency(&mut self, activity_id: ActivityId, dependency: ActivityId) {
        // Make sure that the dependency is not equal to the dependent activity
        assert_ne!(
            activity_id, dependency,
            "Activity {activity_id} must not  depend on itself"
        );

        // Push new activity into existing entry or create new entry
        self.entry(activity_id)
            .and_modify(|s| {
                if !s.contains(&dependency) {
                    s.push(activity_id)
                }
            })
            .or_insert_with(|| vec![activity_id]);
    }

    fn add_dependencies<'s, K>(&mut self, activity_id: ActivityId, dependencies: K)
    where
        K: IntoIterator<Item = &'s ActivityId>,
    {
        // Insert all dependencies
        dependencies.into_iter().for_each(|dependency| {
            self.add_dependency(activity_id, *dependency);
        });
    }
}

/// Information needed by the primary agent about each agent's worker pool configuration
pub type WorkerPoolConfigInfo = HashMap<WorkerId, Vec<ActivityId>>;

pub trait WorkerPoolConfigBuilder {
    /// Insert the given activity for the specified worker
    fn add_activity(&mut self, worker_id: WorkerId, activity_id: ActivityId);

    /// Insert multiple activities for the specified worker
    fn add_activities<'s, K>(&mut self, worker_id: WorkerId, activities: K)
    where
        K: IntoIterator<Item = &'s ActivityId>;

    /// Check if the object contains an activity with the given id
    fn contains_activity(&self, activity_id: ActivityId) -> bool;
}

impl WorkerPoolConfigBuilder for WorkerPoolConfigInfo {
    fn add_activity(&mut self, worker_id: WorkerId, activity_id: ActivityId) {
        // make sure, there is no activity with the same id
        assert!(
            !self.contains_activity(activity_id),
            "Activity id {activity_id} already exists"
        );

        // Push new activity into existing entry or create new entry
        self.entry(worker_id)
            .and_modify(|s| s.push(activity_id))
            .or_insert_with(|| vec![activity_id]);
    }

    fn add_activities<'s, K>(&mut self, worker_id: WorkerId, activities: K)
    where
        K: IntoIterator<Item = &'s ActivityId>,
    {
        // Insert all dependencies
        for activity in activities {
            self.add_activity(worker_id, *activity);
        }
    }

    /// Check if the object contains an activity with the given id
    fn contains_activity(&self, activity_id: ActivityId) -> bool {
        // Check for all workers
        self.values().any(|activities| {
            // Check if any of the activities reports to have the given id
            activities.contains(&activity_id)
        })
    }
}

/// Information needed by the primary agent about each agent's configuration
pub type AgentConfig = (AgentId, WorkerPoolConfigInfo);

/// Primary agent configuration
#[derive(Default)]
pub struct Builder {
    pub id: Option<AgentId>,
    pub bind: Option<SocketAddr>,
    pub agent_map: Option<HashMap<AgentId, HashMap<WorkerId, Vec<ActivityId>>>>,
    pub recorders: Option<HashSet<AgentId>>,
    pub activity_deps: Option<ActivityDependencies>,
    pub feo_cycle_time: Option<Duration>,
    pub worker_pool: Option<WorkerPool>,
    pub intra_proc_ready_channel: Option<(IntraProcSender<Signal>, IntraProcReceiver<Signal>)>,
}

impl Builder {
    /// Set the agent id
    pub fn id(mut self, agent_id: AgentId) -> Self {
        self.id = Some(agent_id);
        self
    }

    /// Set the feo cycle time
    pub fn cycle_time(mut self, feo_cycle_time: Duration) -> Self {
        self.feo_cycle_time = Some(feo_cycle_time);
        self
    }

    /// Set the optional local worker pool with intra-process receiver (can be None)
    pub fn worker_pool(mut self, worker_pool: Option<WorkerPool>) -> Self {
        self.worker_pool = worker_pool;
        self
    }

    /// Set sender and receiver to be used for intra-process transmission of agent signals
    pub fn intra_proc_ready_channel(
        mut self,
        intra_proc_ready_sender: IntraProcSender<Signal>,
        intra_proc_ready_receiver: IntraProcReceiver<Signal>,
    ) -> Self {
        self.intra_proc_ready_channel = Some((intra_proc_ready_sender, intra_proc_ready_receiver));
        self
    }

    /// Set the local bind address
    pub fn bind(mut self, bind: SocketAddr) -> Self {
        self.bind = Some(bind);
        self
    }

    /// Set the agent configuration map
    pub fn agent_map<K>(mut self, agent_map: K) -> Self
    where
        K: IntoIterator<Item = AgentConfig>,
    {
        let map = agent_map.into_iter().collect();
        self.agent_map = Some(map);
        self
    }

    /// Set the recorder agents to expect
    pub fn recorders<K>(mut self, recorders: K) -> Self
    where
        K: IntoIterator<Item = AgentId>,
    {
        let recorders = recorders.into_iter().collect();
        self.recorders = Some(recorders);
        self
    }

    /// Set the activity dependencies
    pub fn activity_dependencies(mut self, activity_deps: ActivityDependencies) -> Self {
        self.activity_deps = Some(activity_deps);
        self
    }

    pub fn build(self) -> PrimaryAgent {
        let agent_id = self.id.expect("missing agent id");
        let bind_addr = self.bind.expect("missing local socket address");
        let feo_cycle_time = self.feo_cycle_time.expect("missing feo cycle time");
        let agent_map = self.agent_map.expect("missing agent map");
        let recorders = self.recorders;
        let local_worker_pool = self.worker_pool;
        let activity_depends = self.activity_deps.expect("missing activity dependency map");
        let (intra_ready_sender, intra_ready_receiver) = self
            .intra_proc_ready_channel
            .expect("missing intra process channel");

        let configuration = PrimaryAgentConfig {
            agent_id,
            bind_addr,
            cycle_time: feo_cycle_time,
            agent_map,
            recorders,
            activity_depends,
            local_worker_pool,
            intra_ready_sender,
            intra_ready_receiver,
        };

        PrimaryAgent::new(configuration)
    }
}
