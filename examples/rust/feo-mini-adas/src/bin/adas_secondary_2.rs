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
        components::{
            BrakeController, EmergencyBraking, LaneAssist, SteeringController, SECONDARY2_NAME,
        },
        runtime_adapters::{activity_into_invokes, LocalFeoAgent},
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

    info!("Starting agent {AGENT_ID}");

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

    // Since runtime `enter_engine` is now not blocking, we do it manually here.
    let waiter = Arc::new(ThreadWaitBarrier::new(1));
    let notifier = waiter.get_notifier().unwrap();

    runtime
        .enter_engine(async {
            let emg_brk_act = Arc::new(Mutex::new(EmergencyBraking::build(
                5.into(),
                TOPIC_INFERRED_SCENE,
                TOPIC_CONTROL_BRAKES,
            )));
            let brk_ctr_act = Arc::new(Mutex::new(BrakeController::build(
                6.into(),
                TOPIC_CONTROL_BRAKES,
            )));
            let lane_asst_act = Arc::new(Mutex::new(LaneAssist::build(
                7.into(),
                TOPIC_INFERRED_SCENE,
                TOPIC_CONTROL_STEERING,
            )));

            let str_ctr_act = Arc::new(Mutex::new(SteeringController::build(
                8.into(),
                TOPIC_CONTROL_STEERING,
            )));

            let mut acts = Vec::new();
            acts.push(activity_into_invokes(&emg_brk_act));
            acts.push(activity_into_invokes(&brk_ctr_act));
            acts.push(activity_into_invokes(&lane_asst_act));
            acts.push(activity_into_invokes(&str_ctr_act));

            let mut agent = LocalFeoAgent::new(acts, SECONDARY2_NAME);
            let mut program = agent.create_program();
            info!("{:?}", program);

            program.run().await;
            info!("Finished");
            notifier.ready();
        })
        .unwrap_or_default();

    waiter
        .wait_for_all(Duration::new(2000, 0))
        .unwrap_or_default();
}
