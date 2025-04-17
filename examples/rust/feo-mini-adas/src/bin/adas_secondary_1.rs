// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use async_runtime::{
    runtime::{runtime::AsyncRuntimeBuilder, *},
    scheduler::execution_engine::ExecutionEngineBuilder,
};
use feo_mini_adas::activities::runtime_adapters::{
    activity_into_invokes, ActivityDetailsBuilder, LocalFeoAgent,
};
use foundation::threading::thread_wait_barrier::*;

use configuration::secondary_agent::Builder;
use feo::configuration::worker_pool;
use feo::prelude::*;
use feo_log::{info, LevelFilter};
use feo_mini_adas::{
    activities::components::{EnvironmentRenderer, NeuralNet},
    config::{self, *},
};

use logging_tracing::{prelude::*, TracingLibrary};
use orchestration::actions::event::Event;
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{Arc, Mutex},
    time::Duration,
};

/// This agent's ID
const AGENT_ID: AgentId = AgentId::new(101);
/// Address of the primary agent
const PRIMARY_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8081);

fn main() {
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
                .workers(1),
        )
        .build()
        .unwrap();

    Event::get_instance()
        .lock()
        .unwrap()
        .create_polling_thread();

    let activities = ActivityDetailsBuilder::new()
        .add_activity(|| EnvironmentRenderer::build(3.into(), TOPIC_INFERRED_SCENE))
        .add_activity(|| {
            NeuralNet::build_val(
                2.into(),
                TOPIC_CAMERA_FRONT,
                TOPIC_RADAR_FRONT,
                TOPIC_INFERRED_SCENE,
            )
        })
        .build();

    LocalFeoAgent::run_agent(APPLICATION_NAME, activities, SECONDARY1_NAME, &mut runtime)
}
