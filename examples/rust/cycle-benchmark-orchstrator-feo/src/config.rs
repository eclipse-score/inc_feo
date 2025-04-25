// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use serde::Deserialize;
use serde_json;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

/// Agent assignments
///
/// For each agent id, a number of threads and set of activity ids running on that agent.
pub type AgentAssignments = HashMap<usize, (usize, HashSet<usize>)>;

/// Graph of activity dependencies
///
/// A sequence of activity chains with parallel-execution flags
pub type ActivityGraph = Vec<(Vec<usize>, bool)>;

/// Path of the config file relative to this file
static CONFIG_PATH: &str = "../config/cycle_bench.json";

/// Configuration of the benchmark application
#[derive(Deserialize)]
pub struct ApplicationConfig {
    /// FEO cycle time in ms
    cycle_time_ms: u64,
    /// ID of primary agent
    primary_agent: usize,
    /// Agent assignments
    ///
    /// For each agent id, a number of threads and set of activity ids running on that agent.
    agent_assignments: AgentAssignments,
    /// Graph of activity dependencies
    ///
    /// A sequence of activity chains with parallel-execution flags
    activity_graph: ActivityGraph,
}

impl ApplicationConfig {
    pub fn load() -> Self {
        application_config()
    }

    pub fn cycle_time(&self) -> u64 {
        self.cycle_time_ms
    }

    pub fn primary(&self) -> usize {
        self.primary_agent
    }

    pub fn secondaries(&self) -> Vec<usize> {
        self.agent_assignments
            .keys()
            .cloned()
            .filter(|id| *id != self.primary_agent)
            .collect()
    }

    pub fn all_agents(&self) -> Vec<usize> {
        self.agent_assignments.keys().cloned().collect()
    }

    pub fn num_threads(&self, agent_id: usize) -> usize {
        self.agent_assignments
            .get(&agent_id)
            .expect("agent id missing in config")
            .0
    }

    pub fn agent_assignments(&self) -> AgentAssignments {
        self.agent_assignments.clone()
    }

    pub fn activity_graph(&self) -> ActivityGraph {
        self.activity_graph.clone()
    }

    pub fn activities(&self, agent_id: usize) -> HashSet<usize> {
        self.agent_assignments()
            .get(&agent_id)
            .expect("agent id missing in config")
            .1
            .clone()
    }

    pub fn all_activities(&self) -> HashSet<usize> {
        self.agent_assignments
            .values()
            .flat_map(|(_, activity_id)| activity_id.iter())
            .copied()
            .collect()
    }

    pub fn sequence_and_concurrency(&self, agent_id: usize) -> (Vec<usize>, Vec<bool>) {
        // determine activities on this agent
        let activities = self.activities(agent_id);

        let mut sequence: Vec<usize> = vec![];
        let mut concurrency: Vec<bool> = vec![];
        for (program, _) in self.activity_graph() {
            let mut is_first: bool = true;
            for id in program {
                if activities.contains(&id) {
                    sequence.push(id);
                    concurrency.push(is_first); // first program entry true, then false
                    if is_first {
                        is_first = false;
                    }
                }
            }
        }
        (sequence, concurrency)
    }
}

fn application_config() -> ApplicationConfig {
    let config_file = Path::new(file!())
        .parent()
        .unwrap()
        .join(CONFIG_PATH)
        .canonicalize()
        .unwrap();
    println!("Reading configuration from {}", config_file.display());

    let file =
        File::open(config_file).unwrap_or_else(|e| panic!("failed to open config file: {e}"));
    let reader = BufReader::new(file);

    // Read the JSON file to an instance of `RawConfig`.
    let config: ApplicationConfig = serde_json::from_reader(reader)
        .unwrap_or_else(|e| panic!("failed to parse config file: {e}"));

    check_consistency(&config);
    config
}

fn check_consistency(config: &ApplicationConfig) {
    // do consistency check wrt primary agent id
    assert!(
        config.agent_assignments.contains_key(&config.primary_agent),
        "Primary agent ID not listed in agent assignments"
    );

    // do basic consistency checks wrt activity ids
    let vec_activities_agents: Vec<usize> = config
        .agent_assignments
        .values()
        .flat_map(|(_, activity_id)| activity_id.iter())
        .copied()
        .collect();

    let mut all_activities_agents: HashSet<usize> = Default::default();
    for aid in vec_activities_agents {
        let is_new = all_activities_agents.insert(aid);
        assert!(is_new, "duplicate activity {aid} in agent assignments");
    }

    let vec_activities_graph: Vec<usize> = config
        .activity_graph()
        .iter()
        .flat_map(|(aids, _)| aids.iter().copied())
        .collect();

    let mut all_activities_graph: HashSet<usize> = Default::default();
    for aid in vec_activities_graph {
        let is_new = all_activities_graph.insert(aid);
        assert!(is_new, "duplicate activity {aid} in activity graph");
    }

    assert_eq!(
        all_activities_agents, all_activities_graph,
        "Set of activities assigned to agents does not match set of activities in activity graph"
    );
}
