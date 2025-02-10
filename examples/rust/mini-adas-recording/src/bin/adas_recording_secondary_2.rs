// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use configuration::secondary_agent::Builder;
use feo::configuration::worker_pool;
use feo::prelude::*;
use feo_log::{info, LevelFilter};
use mini_adas_recording::config;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

/// This agent's ID
const AGENT_ID: AgentId = AgentId::new(102);
/// Address of the primary agent
const PRIMARY_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8081);

fn main() {
    feo_logger::init(LevelFilter::Debug, true, true);
    feo_tracing::init(feo_tracing::LevelFilter::TRACE);

    info!("Starting agent {AGENT_ID}");

    // Create worker pool builder activity builder for local worker pool
    let mut worker_pool_builder = worker_pool::Builder::default();

    let mut worker_pool_configuration = config::pool_configuration();
    let assignments = worker_pool_configuration
        .remove(&AGENT_ID)
        .expect("missing agent id in pool configuration");

    // Assign activities to workers
    for (worker_id, activities) in assignments {
        for (activity_id, builder) in activities {
            worker_pool_builder.activity(worker_id, activity_id, builder);
        }
    }

    let (worker_pool, _, receiver) = worker_pool_builder.build().expect("Worker pool is empty");

    // Construct the agent
    let agent = Builder::default()
        .id(AGENT_ID)
        .primary(PRIMARY_ADDR)
        .worker_pool(worker_pool, receiver)
        .build();

    // Start the agent loop and never return.
    secondary::run(agent);
}
