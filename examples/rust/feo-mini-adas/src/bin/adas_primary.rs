// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use configuration::primary_agent::Builder;
use feo::configuration::worker_pool;
use feo::prelude::*;
use feo::signalling::{channel, Signal};
use feo_log::{info, LevelFilter};
use feo_mini_adas::config;
use feo_time::Duration;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

const AGENT_ID: AgentId = AgentId::new(100);
const BIND_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8081);
const DEFAULT_FEO_CYCLE_TIME: Duration = Duration::from_secs(5);

fn main() {
    feo_logger::init(LevelFilter::Debug, true, true);
    feo_tracing::init(feo_tracing::LevelFilter::TRACE);

    let params = Params::from_args();

    info!("Starting primary agent {AGENT_ID}. Waiting for connections",);

    // Initialize topics. Do not drop.
    let _topic_guards = config::initialize_topics();

    // Create local worker pool
    let (worker_pool, agent_map, ready_channel) = {
        let pool_configuration = config::pool_configuration();
        let mut worker_pool_builder = worker_pool::Builder::default();
        let mut agent_map: HashMap<AgentId, HashMap<WorkerId, Vec<ActivityId>>> = HashMap::new();

        // Recreate the HashMap without the builder on the lowest level.
        for (agent_id, assignments) in pool_configuration.into_iter() {
            for (worker_id, activities) in assignments.into_iter() {
                for (activity_id, builder) in activities {
                    if agent_id == AGENT_ID {
                        worker_pool_builder.activity(worker_id, activity_id, builder);
                    }

                    // Reinsert with same structure but without the builder on the lowest level.
                    agent_map
                        .entry(agent_id)
                        .or_default()
                        .entry(worker_id)
                        .and_modify(|act_ids| act_ids.push(activity_id))
                        .or_insert_with(|| vec![activity_id]);
                }
            }
        }

        let (worker_pool, ready_channel) = match worker_pool_builder.build() {
            Some((pool, sender, receiver)) => (Some(pool), (sender, receiver)),
            None => {
                let ready_channel = channel::<Signal>();
                (None, ready_channel)
            }
        };

        (worker_pool, agent_map, ready_channel)
    };

    let activity_dependencies = config::activity_dependencies();

    // Construct the agent
    let agent = Builder::default()
        .id(AGENT_ID)
        .cycle_time(params.feo_cycle_time)
        .bind(BIND_ADDR)
        .agent_map(agent_map)
        .worker_pool(worker_pool)
        .activity_dependencies(activity_dependencies)
        .intra_proc_ready_channel(ready_channel.0, ready_channel.1)
        .build();

    // Start the agent loop and never return.
    primary::run(agent);
}

/// Parameters of the primary
struct Params {
    /// Cycle time in milli seconds
    feo_cycle_time: Duration,
}

impl Params {
    fn from_args() -> Self {
        let args: Vec<String> = std::env::args().collect();

        let feo_cycle_time = args
            .get(1)
            .and_then(|x| x.parse::<u64>().ok())
            .map(Duration::from_millis)
            .unwrap_or(DEFAULT_FEO_CYCLE_TIME);

        Self { feo_cycle_time }
    }
}
