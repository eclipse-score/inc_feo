// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use crate::activities::DummyActivity;
use core::net::{IpAddr, Ipv4Addr, SocketAddr};
use feo::activity::ActivityIdAndBuilder;
use feo::ids::{ActivityId, AgentId, WorkerId};
use feo_log::info;
use serde::Deserialize;
use serde_json;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

pub type AgentAssignments = HashMap<AgentId, HashSet<WorkerId>>;
pub type WorkerAssignments = HashMap<AgentId, Vec<(WorkerId, Vec<ActivityIdAndBuilder>)>>;
pub type ActivityDependencies = HashMap<ActivityId, Vec<ActivityId>>;

/// Path of the config file relative to this file
static CONFIG_PATH: &str = "../config/cycle_bench.json";

pub const BIND_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8081);
pub const BIND_ADDR2: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8082);

pub fn socket_paths() -> (PathBuf, PathBuf) {
    (
        Path::new("/tmp/feo_listener1.socket").to_owned(),
        Path::new("/tmp/feo_listener2.socket").to_owned(),
    )
}

/// Configuration of the benchmark application
pub struct ApplicationConfig {
    /// Type of signalling
    signalling: SignallingType,
    /// ID of primary agent
    primary_agent: AgentId,
    /// Bind address(es) of primary agent
    bind_addrs: (SocketAddr, SocketAddr),
    /// Socket bind paths of primary agent
    socket_paths: (PathBuf, PathBuf),
    /// Agent IDs of recorders
    recorders: HashSet<AgentId>,
    /// Agent assignments
    ///
    /// For each agent id, a set of worker ids running on that agent.
    agent_assignments: AgentAssignments,
    /// Agent assignments
    ///
    /// For each worker id, a set of activities ids and activity builders running on that worker.
    worker_assignments: HashMap<WorkerId, HashSet<ActivityId>>,
    /// Dependencies of activities
    ///
    /// For each activity id, a list of ids of activities it depends on
    activity_deps: ActivityDependencies,
}

impl ApplicationConfig {
    pub fn load() -> Self {
        application_config()
    }

    pub fn signalling(&self) -> SignallingType {
        self.signalling
    }

    pub fn primary(&self) -> AgentId {
        self.primary_agent
    }

    pub fn secondaries(&self) -> Vec<AgentId> {
        self.agent_assignments
            .keys()
            .cloned()
            .filter(|id| *id != self.primary_agent)
            .collect()
    }

    pub fn activity_worker_map(&self) -> HashMap<ActivityId, WorkerId> {
        self.worker_assignments
            .iter()
            .flat_map(|(wid, aids)| aids.iter().map(move |aid| (*aid, *wid)))
            .collect()
    }

    pub fn bind_addrs(&self) -> (SocketAddr, SocketAddr) {
        self.bind_addrs
    }

    pub fn socket_paths(&self) -> (PathBuf, PathBuf) {
        self.socket_paths.clone()
    }

    pub fn agent_assignments(&self) -> AgentAssignments {
        self.agent_assignments.clone()
    }

    pub fn activity_dependencies(&self) -> ActivityDependencies {
        self.activity_deps.clone()
    }

    pub fn worker_agent_map(&self) -> HashMap<WorkerId, AgentId> {
        self.agent_assignments
            .iter()
            .flat_map(|(aid, wids)| wids.iter().map(move |wid| (*wid, *aid)))
            .collect()
    }

    pub fn recorders(&self) -> Vec<AgentId> {
        self.recorders.iter().copied().collect()
    }

    pub fn worker_assignments(&self) -> WorkerAssignments {
        let mut assignments: WorkerAssignments = Default::default();
        let mut workers_seen: HashSet<WorkerId> = Default::default();

        // Loop over configured agents
        for (agent, workers) in &self.agent_assignments {
            let mut assigned_workers: Vec<(WorkerId, Vec<ActivityIdAndBuilder>)> =
                Default::default();

            // Loop over workers configured for this agent
            for wid in workers {
                let is_new = workers_seen.insert(*wid);
                assert!(is_new, "Worker {wid} assigned to multiple agents");
                let mut acts_and_builders: Vec<ActivityIdAndBuilder> = Default::default();
                let aids = self
                    .worker_assignments
                    .get(wid)
                    .unwrap_or_else(|| panic!("Worker {wid} missing in worker assignments"));

                // Loop aver activity ids assigned to this worker and create required builders
                for aid in aids {
                    let builder = Box::new(DummyActivity::build);
                    acts_and_builders.push((*aid, builder));
                }
                assigned_workers.push((*wid, acts_and_builders));
            }
            assignments.insert(*agent, assigned_workers);
        }
        assignments
    }
}

#[derive(Deserialize, Debug, Clone, Copy)]
pub enum SignallingType {
    DirectMpsc,
    DirectTcp,
    DirectUnix,
    RelayedTcp,
    RelayedUnix,
}

fn application_config() -> ApplicationConfig {
    let config_file = Path::new(file!())
        .parent()
        .unwrap()
        .join(CONFIG_PATH)
        .canonicalize()
        .unwrap();
    info!("Reading configuration from {}", config_file.display());

    let file =
        File::open(config_file).unwrap_or_else(|e| panic!("failed to open config file: {e}"));
    let reader = BufReader::new(file);

    // Read the JSON file to an instance of `RawConfig`.
    let config: RawConfig = serde_json::from_reader(reader)
        .unwrap_or_else(|e| panic!("failed to parse config file: {e}"));

    // Convert raw config to application config
    let agent_assignments: AgentAssignments = config
        .agent_assignments
        .iter()
        .map(|(agent, workers)| {
            let wids: HashSet<WorkerId> = workers.iter().map(|w| WorkerId::from(*w)).collect();
            (AgentId::from(*agent), wids)
        })
        .collect();

    let worker_assignments: HashMap<WorkerId, HashSet<ActivityId>> = config
        .worker_assignments
        .iter()
        .map(|(worker, activities)| {
            let acts: HashSet<ActivityId> =
                activities.iter().map(|id| ActivityId::new(*id)).collect();
            (WorkerId::new(*worker), acts)
        })
        .collect();

    let activity_deps: ActivityDependencies = config
        .activity_deps
        .iter()
        .map(|(act, deps)| {
            let dependencies = deps.iter().map(|id| ActivityId::new(*id)).collect();
            (ActivityId::new(*act), dependencies)
        })
        .collect();

    let recorders: HashSet<AgentId> = config.recorders.iter().map(|r| AgentId::new(*r)).collect();

    check_consistency(&config, &recorders, &agent_assignments, &activity_deps);

    ApplicationConfig {
        signalling: config.signalling,
        primary_agent: AgentId::new(config.primary_agent),
        bind_addrs: (BIND_ADDR, BIND_ADDR2),
        socket_paths: socket_paths(),
        recorders,
        agent_assignments,
        worker_assignments,
        activity_deps,
    }
}

fn check_consistency(
    config: &RawConfig,
    recorders: &HashSet<AgentId>,
    agent_assignments: &AgentAssignments,
    activity_deps: &ActivityDependencies,
) {
    // do consistency check wrt primary agent id
    assert!(
        config.agent_assignments.contains_key(&config.primary_agent),
        "Primary agent ID not listed in agent assignments"
    );

    // check consistency of recorders
    for rid in recorders.iter() {
        assert!(
            !agent_assignments.contains_key(rid),
            "Recorder ID {} must not appear in agent assignments",
            rid
        );
    }

    // do basic consistency check wrt worker ids
    let all_workers_agents: HashSet<WorkerId> = config
        .agent_assignments
        .values()
        .flatten()
        .map(|id| WorkerId::from(*id))
        .collect();
    let all_workers_workers: HashSet<WorkerId> = config
        .worker_assignments
        .keys()
        .map(|id| WorkerId::new(*id))
        .collect();

    assert_eq!(
        all_workers_agents, all_workers_workers,
        "Set of workers listed in agent assignments differs from set of workers listed in worker assignments"
    );

    // do basic consistency checks wrt activity ids
    let all_activities_workers: HashSet<ActivityId> = config
        .worker_assignments
        .values()
        .flat_map(|activity_id| activity_id.iter().map(|id| ActivityId::new(*id)))
        .collect();
    let depending_activities: HashSet<ActivityId> = activity_deps.keys().copied().collect();
    let dependency_activities: HashSet<ActivityId> = activity_deps
        .values()
        .flat_map(|deps| deps.iter().copied())
        .collect();
    let all_activities_dependencies: HashSet<ActivityId> = depending_activities
        .iter()
        .chain(dependency_activities.iter())
        .copied()
        .collect();

    assert_eq!(all_activities_workers, depending_activities,
               "Set of activities assigned to workers does not match set of activities listed as dependants in activity dependencies");
    assert_eq!(all_activities_workers, all_activities_dependencies,
               "Set of activities assigned to workers does not include all activities mentioned in activity dependencies");
}

/// Configuration of the benchmark application
#[derive(Deserialize, Debug, Clone)]
struct RawConfig {
    /// Type of signalling
    signalling: SignallingType,
    /// ID of primary agent
    primary_agent: u64,
    /// IDs of recorders
    recorders: HashSet<u64>,
    /// Agent assignments
    ///
    /// For each agent id, a set of worker ids running on that agent.
    agent_assignments: HashMap<u64, HashSet<u64>>,
    /// Agent assignments
    ///
    /// For each worker id, a set of activities ids running on that worker.
    worker_assignments: HashMap<u64, HashSet<u64>>,
    /// Dependencies of activities
    ///
    /// For each activity id, a list of ids of activities it depends on
    activity_deps: HashMap<u64, Vec<u64>>,
}
