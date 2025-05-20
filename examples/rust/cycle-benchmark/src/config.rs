// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use crate::activities::DummyActivity;
use crate::composites::{composite_builder, find_composites};
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
#[derive(Clone)]
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
    /// Chains of activities to be put into CompositeActivities
    ///
    /// IDs of composite activities mapped to a sequence of contained activities.
    /// The IDs of the contained activities do not have to be unique.
    composite_activities: HashMap<ActivityId, Vec<ActivityId>>,
}

#[derive(Deserialize, Debug, Clone, Copy)]
pub enum SignallingType {
    DirectMpsc,
    DirectTcp,
    DirectUnix,
    RelayedTcp,
    RelayedUnix,
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
        let activity_chains = find_composites(&self.activity_deps, &self.activity_worker_map());
        for chain in activity_chains {
            let numeric: Vec<u64> = chain.iter().map(u64::from).collect();
            println!("Found potential composite activity: {:?}", numeric);
        }

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
                    // If the id corresponds to a composite activity, create a composite builder;
                    // otherwise, create standard builder
                    let builder = if self.composite_activities.contains_key(aid) {
                        let components = &self.composite_activities[aid];
                        composite_builder(components)
                    } else {
                        Box::new(DummyActivity::build)
                    };
                    acts_and_builders.push((*aid, builder));
                }
                assigned_workers.push((*wid, acts_and_builders));
            }
            assignments.insert(*agent, assigned_workers);
        }
        assignments
    }

    /// Return an optimized configuration replacing activity chains with composite activities
    pub fn optimize_composite(&self) -> Self {
        let chains = find_composites(&self.activity_deps, &self.activity_worker_map());
        let mut optimized = (*self).clone();

        println!("Preparing composite activities:");
        for chain in &chains {
            let numeric: Vec<u64> = chain.iter().map(u64::from).collect();
            println!("ID {}: {:?}", u64::from(chain[0]), numeric);
        }

        // Each chain will be represented by a composite activity
        // whose ID will be the ID of the first activity in the chain
        // =>
        // Here, we remove all chain activities from the dependency tree,
        // except for the first one as a dependant and the last one as a dependency.
        // In the dependencies, we then replace the last one with the first one.

        let dependencies = &mut optimized.activity_deps;
        let assignments = &mut optimized.worker_assignments;
        let composites = &mut optimized.composite_activities;

        // Loop over all chains
        for chain in chains {
            assert!(chain.len() >= 2, "Unexpected length of activity chain");
            let (first_id, chain_wo_first) = chain.split_first().unwrap();
            let (last_id, chain_wo_last) = chain.split_last().unwrap();

            // Filter and replace IDs in dependency keys (i.e. dependants)
            // and values (i.e. dependencies) as described above
            *dependencies = dependencies
                .iter()
                .filter(|(id, _)| !chain_wo_first.contains(id))
                .map(|(id, deps)| {
                    let adapted_deps: Vec<ActivityId> = deps
                        .iter()
                        .filter(|id| !chain_wo_last.contains(id))
                        .map(|id| if id == last_id { *first_id } else { *id })
                        .collect();
                    (*id, adapted_deps)
                })
                .map(|(id, deps)| {
                    if &id == last_id {
                        (*first_id, deps)
                    } else {
                        (id, deps)
                    }
                })
                .collect();

            // Filter and replace IDs in the worker assignments
            *assignments = assignments
                .iter()
                .map(|(w_id, a_ids)| {
                    let adapted_a_ids: HashSet<ActivityId> = a_ids
                        .iter()
                        .filter(|a_id| !chain_wo_first.contains(a_id))
                        .copied()
                        .collect();
                    (*w_id, adapted_a_ids)
                })
                .collect();

            // Append the chain to the list of composite activities
            composites.insert(*first_id, chain);
        }

        optimized
    }
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

    let app_config = ApplicationConfig {
        signalling: config.signalling,
        primary_agent: AgentId::new(config.primary_agent),
        bind_addrs: (BIND_ADDR, BIND_ADDR2),
        socket_paths: socket_paths(),
        recorders,
        agent_assignments,
        worker_assignments,
        activity_deps,
        composite_activities: Default::default(),
    };

    if config.optimize_composite_activities {
        app_config.optimize_composite()
    } else {
        app_config
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
    /// Whether to put straight chains of activities into composite activities
    ///
    /// If true, the dependency tree will be checked for chains of activities running in
    /// the same thread and having no dependencies to other activities. Each identified
    /// activity chain will be factored into a composite activity in order to save unnecessary
    /// trigger signals between activities and the scheduler.
    optimize_composite_activities: bool,
}
