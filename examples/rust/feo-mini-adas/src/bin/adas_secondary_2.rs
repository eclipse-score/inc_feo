// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use async_runtime::{
    runtime::runtime::AsyncRuntimeBuilder, scheduler::execution_engine::ExecutionEngineBuilder,
};
use configuration::secondary_agent::Builder;
use feo::configuration::worker_pool;
use feo::prelude::*;
use feo_log::{info, LevelFilter};
use feo_mini_adas::{
    activities::{
        components::{BrakeController, EmergencyBraking, LaneAssist, SteeringController},
        runtime_adapters::{activity_into_invokes, ActivityDetailsBuilder, LocalFeoAgent},
    },
    config::{self, *},
};
use foundation::threading::thread_wait_barrier::*;
use logging_tracing::{prelude::*, TraceScope, TracingLibraryBuilder};
use orchestration::prelude::Event;
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{Arc, Mutex},
    time::Duration,
};

/// This agent's ID
const AGENT_ID: AgentId = AgentId::new(102);
/// Address of the primary agent
const PRIMARY_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8081);

fn main() {
    // feo_logger::init(LevelFilter::Debug, true, true);
    // feo_tracing::init(feo_tracing::LevelFilter::TRACE);

    let mut logger = TracingLibraryBuilder::new()
        .global_log_level(Level::DEBUG)
        .enable_tracing(TraceScope::SystemScope)
        .enable_logging(false)
        .build();

    logger.init_log_trace();

    let mut runtime = AsyncRuntimeBuilder::new()
        .with_engine(
            ExecutionEngineBuilder::new()
                .task_queue_size(256)
                .workers(2),
        )
        .build()
        .unwrap();

    Event::get_instance()
        .lock()
        .unwrap()
        .create_polling_thread();

    let activities = ActivityDetailsBuilder::new()
        .add_activity(|| {
            EmergencyBraking::build(4.into(), TOPIC_INFERRED_SCENE, TOPIC_CONTROL_BRAKES)
        })
        .add_activity(|| BrakeController::build(6.into(), TOPIC_CONTROL_BRAKES))
        .add_activity(|| LaneAssist::build(5.into(), TOPIC_INFERRED_SCENE, TOPIC_CONTROL_STEERING))
        .add_activity(|| SteeringController::build(7.into(), TOPIC_CONTROL_STEERING))
        .build();

    LocalFeoAgent::run_agent(APPLICATION_NAME, activities, SECONDARY2_NAME, &mut runtime)
}
