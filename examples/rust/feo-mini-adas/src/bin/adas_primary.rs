// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use async_runtime::runtime::runtime::AsyncRuntimeBuilder;
use async_runtime::scheduler::execution_engine::ExecutionEngineBuilder;
use configuration::primary_agent::Builder;
use feo::configuration::worker_pool;
use feo::prelude::*;
use feo::signalling::{channel, Signal};
use feo_log::{info, LevelFilter};
use feo_mini_adas::activities::components::{Camera, Radar};
use feo_mini_adas::activities::runtime_adapters::{
    activity_into_invokes, ActivityDetailsBuilder, GlobalOrchestrator, LocalFeoAgent,
};
use feo_mini_adas::config::{self, *};
use feo_time::Duration;
use foundation::threading::thread_wait_barrier::*;
use logging_tracing::prelude::*;
use logging_tracing::{TraceScope, TracingLibraryBuilder};
use orchestration::prelude::Event;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};

const AGENT_ID: AgentId = AgentId::new(100);
const BIND_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8081);
const DEFAULT_FEO_CYCLE_TIME: Duration = Duration::from_millis(500);

fn main() {
    let params = Params::from_args();

    //Initialize in LogMode with AppScope
    let mut logger = TracingLibraryBuilder::new()
        .global_log_level(Level::TRACE)
        .enable_tracing(TraceScope::SystemScope)
        .enable_logging(false)
        .build();

    logger.init_log_trace();

    let _topic_guards = initialize_topics();

    let agents: Vec<String> = vec![
        PRIMARY_NAME.to_string(),
        SECONDARY1_NAME.to_string(),
        SECONDARY2_NAME.to_string(),
    ];

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
        .add_activity(|| Radar::build(1.into(), TOPIC_RADAR_FRONT))
        .add_activity(|| Camera::build(0.into(), TOPIC_CAMERA_FRONT))
        .build();

    GlobalOrchestrator::run_primary(
        APPLICATION_NAME,
        agents,
        params.feo_cycle_time,
        activity_dependencies(),
        activities,
        PRIMARY_NAME,
        &mut runtime,
    )
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
