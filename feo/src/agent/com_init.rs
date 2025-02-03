// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use crate::ids::{ActivityId, AgentId, WorkerId};
use crate::topicspec::{Direction, TopicSpecification};
use alloc::vec::Vec;
use feo_com::interface::{
    run_backend, ComBackend, ComBackendTopicPrimaryInitialization,
    ComBackendTopicSecondaryInitialization, TopicHandle,
};
use std::collections::{HashMap, HashSet};

/// Initialize feo-com for the primary agent using the given configuration parameters
///
/// # Arguments
///
/// * backend: the com backend to use
/// * agent_id: the agent id of the primary agent
/// * topics_specs: Specifications of all topics used in the application
///   (i.e., primary and secondary agents)
/// * agent_assignments: Map from agent ids to lists of workers with their activities
/// * max_additional_readers: The maximum number of optional additional readers on a topic
///   (usually recorder processes)
pub fn initialize_com_primary(
    backend: ComBackend,
    agent_id: AgentId,
    topic_specs: Vec<TopicSpecification>,
    agent_assignments: &HashMap<AgentId, Vec<(WorkerId, Vec<ActivityId>)>>,
    max_additional_readers: usize,
) -> Vec<TopicHandle> {
    let num_local_requests = local_requests(agent_assignments, agent_id, &topic_specs);
    let num_remote_requests = remote_requests(
        agent_assignments,
        agent_id,
        &topic_specs,
        max_additional_readers,
    );

    let mut handles = Vec::with_capacity(topic_specs.len());
    let local_activities = local_activities(agent_assignments, agent_id);

    for spec in topic_specs {
        let writers = spec
            .peers
            .iter()
            .filter(|(_, dir)| matches!(dir, Direction::Outgoing))
            .count();
        let readers = spec
            .peers
            .iter()
            .filter(|(_, dir)| matches!(dir, Direction::Incoming))
            .count()
            + max_additional_readers;
        let is_local_write = is_local_write(agent_assignments, agent_id, &spec);
        let map_locally = spec.peers.iter().any(|(p, _)| local_activities.contains(p));
        let init_params = ComBackendTopicPrimaryInitialization::new(
            spec.topic,
            backend,
            readers,
            writers,
            map_locally,
            is_local_write,
        );

        let handle = (spec.init_primary_fn)(&init_params);
        handles.push(handle);
    }

    run_backend(backend, num_local_requests, num_remote_requests);

    handles
}

/// Initialize feo-com for a secondary agent using the given configuration parameters
///
/// # Arguments
///
/// * backend: the com backend to use
/// * topics_specs: Specifications of all topics used by this agent
/// * local_activities: Set of ids of activities executed by this agent
pub fn initialize_com_secondary(
    backend: ComBackend,
    topic_specs: Vec<TopicSpecification>,
    local_activities: &HashSet<ActivityId>,
) -> Vec<TopicHandle> {
    let mut handles = Vec::with_capacity(topic_specs.len());
    for spec in topic_specs {
        let is_local_write = is_write(local_activities, &spec);
        let init_params =
            ComBackendTopicSecondaryInitialization::new(spec.topic, backend, is_local_write);
        let handle = (spec.init_secondary_fn)(&init_params);
        handles.push(handle);
    }
    handles
}

/// Initialize feo-com for a secondary agent using the given configuration parameters
///
/// # Arguments
///
/// * backend: the com backend to use
/// * topics_specs: Specifications of all topics possibly recorded
pub fn initialize_com_recorder(
    backend: ComBackend,
    topic_specs: Vec<TopicSpecification>,
) -> Vec<TopicHandle> {
    let mut handles = Vec::with_capacity(topic_specs.len());
    for spec in topic_specs {
        let init_params = ComBackendTopicSecondaryInitialization::new(spec.topic, backend, false);
        let handle = (spec.init_secondary_fn)(&init_params);
        handles.push(handle);
    }
    handles
}

/// Find number of local connection requests to be expected from the given configuration
fn local_requests(
    agent_assignments: &HashMap<AgentId, Vec<(WorkerId, Vec<ActivityId>)>>,
    agent_id: AgentId,
    topic_specs: &Vec<TopicSpecification<'_>>,
) -> usize {
    let local_activities = local_activities(agent_assignments, agent_id);
    topic_specs
        .iter()
        .flat_map(|ts| {
            ts.peers
                .iter()
                .map(|a| a.0)
                .filter(|a| local_activities.contains(a))
        })
        .count()
}

/// Find number of local connection requests to be expected from the given configuration
fn remote_requests(
    agent_assignments: &HashMap<AgentId, Vec<(WorkerId, Vec<ActivityId>)>>,
    agent_id: AgentId,
    topic_specs: &Vec<TopicSpecification<'_>>,
    max_additional_subscribers: usize,
) -> usize {
    let remote_activities = remote_activities(agent_assignments, agent_id);

    // Calculate number of actual subscribers
    let num_requests = topic_specs
        .iter()
        .flat_map(|ts| {
            ts.peers
                .iter()
                .map(|a| a.0)
                .filter(|a| remote_activities.contains(a))
        })
        .count();

    // Add `max_additional_subscribers` for each topic
    let max_additional_requests = topic_specs.len() * max_additional_subscribers;
    num_requests + max_additional_requests
}

/// Determine if the given agent needs write access to the specified topic
fn is_local_write(
    agent_assignments: &HashMap<AgentId, Vec<(WorkerId, Vec<ActivityId>)>>,
    agent_id: AgentId,
    topic_spec: &TopicSpecification<'_>,
) -> bool {
    let local_activities = local_activities(agent_assignments, agent_id);
    is_write(&local_activities, topic_spec)
}

/// Determine if one of the given activities needs write access to the specified topic
fn is_write(activities: &HashSet<ActivityId>, topic_spec: &TopicSpecification<'_>) -> bool {
    topic_spec
        .peers
        .iter()
        .filter(|(a, _)| activities.contains(a))
        .any(|(_, d)| *d == Direction::Outgoing)
}

/// Find local activities of the given agent
fn local_activities(
    agent_assignments: &HashMap<AgentId, Vec<(WorkerId, Vec<ActivityId>)>>,
    agent_id: AgentId,
) -> HashSet<ActivityId> {
    agent_assignments
        .get(&agent_id)
        .unwrap_or_else(|| panic!("agent id {agent_id} not found"))
        .iter()
        .flat_map(|(_, acts)| acts.iter().copied())
        .collect()
}

/// Find remote activities of the given agent
fn remote_activities(
    agent_assignments: &HashMap<AgentId, Vec<(WorkerId, Vec<ActivityId>)>>,
    agent_id: AgentId,
) -> HashSet<ActivityId> {
    agent_assignments
        .iter()
        .filter(|(a, _)| *a != &agent_id)
        .flat_map(|(_, acts)| acts.iter().flat_map(|(_, a)| a.iter().copied()))
        .collect()
}
