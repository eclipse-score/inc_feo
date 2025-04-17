// Copyright 2025 Qorix
//
// This is the single process version of the feo-mini-adas example
//
// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0
use async_runtime::runtime::runtime::AsyncRuntimeBuilder;
use async_runtime::scheduler::execution_engine::ExecutionEngineBuilder;
use feo::com::TopicHandle;
use feo_mini_adas::activities::components::{
    BrakeController, Camera, EmergencyBraking, EnvironmentRenderer, LaneAssist, NeuralNet, Radar,
    SteeringController,
};
use feo_mini_adas::activities::runtime_adapters::{
    activity_into_invokes, ActivityDetailsBuilder, GlobalOrchestrator, LocalFeoAgent,
};
use feo_mini_adas::config::*;
use feo_time::Duration;
use foundation::threading::thread_wait_barrier::*;
use logging_tracing::prelude::*;
use logging_tracing::{TraceScope, TracingLibrary, TracingLibraryBuilder};
use orchestration::prelude::Event;
use orchestration::program::ProgramBuilder;
use std::sync::{Arc, Mutex};

// ****************************************************************** //
// **                  RUNTIME SETTINGS                            ** //
// ****************************************************************** //

const ENGINE_TASK_QUEUE_SIZE: usize = 256;
const ENGINE_NUM_OF_WORKERS: usize = 3;

const TRACE_SCOPE: TraceScope = TraceScope::SystemScope;
const LOG_LEVEL: Level = Level::INFO;
const LOG_ENABLE: bool = true;

const DEFAULT_FEO_CYCLE_TIME: Duration = Duration::from_secs(1);

// ****************************************************************** //
// **                    PROGRAMS                                  ** //
// ****************************************************************** //

/// The entry point of the application
async fn main_program() {
    let agents: Vec<String> = vec![
        PRIMARY_NAME.to_string(),
        SECONDARY1_NAME.to_string(),
        SECONDARY2_NAME.to_string(),
    ];

    let primary_agent_program = async_runtime::spawn(primary_agent_program());
    let secondary_1_agent_program = async_runtime::spawn(secondary_1_agent_program());
    let secondary_2_agent_program = async_runtime::spawn(secondary_2_agent_program());

    let global_orch = GlobalOrchestrator::new(APPLICATION_NAME, agents, DEFAULT_FEO_CYCLE_TIME);
    global_orch.run(&activity_dependencies()).await;
    primary_agent_program.await;
    secondary_1_agent_program.await;
    secondary_2_agent_program.await;
}

async fn primary_agent_program() {
    info!("Starting primary agent {PRIMARY_NAME}. Waiting for connections",);

    let activities = ActivityDetailsBuilder::new()
        .add_activity(|| Radar::build(1.into(), TOPIC_RADAR_FRONT))
        .add_activity(|| Camera::build(0.into(), TOPIC_CAMERA_FRONT))
        .build();

    let mut agent = LocalFeoAgent::new(APPLICATION_NAME, activities, PRIMARY_NAME);
    let mut program = agent.create_program();

    program.run().await;
}

async fn secondary_1_agent_program() {
    info!("Starting secondary_1 agent {SECONDARY1_NAME}",);

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

    let mut agent = LocalFeoAgent::new(APPLICATION_NAME, activities, SECONDARY1_NAME);
    let mut program = agent.create_program();

    program.run().await;
}

async fn secondary_2_agent_program() {
    info!("Starting secondary_2 agent {SECONDARY2_NAME}",);
    let activities = ActivityDetailsBuilder::new()
        .add_activity(|| {
            EmergencyBraking::build(4.into(), TOPIC_INFERRED_SCENE, TOPIC_CONTROL_BRAKES)
        })
        .add_activity(|| BrakeController::build(6.into(), TOPIC_CONTROL_BRAKES))
        .add_activity(|| LaneAssist::build(5.into(), TOPIC_INFERRED_SCENE, TOPIC_CONTROL_STEERING))
        .add_activity(|| SteeringController::build(7.into(), TOPIC_CONTROL_STEERING))
        .build();

    let mut agent = LocalFeoAgent::new(APPLICATION_NAME, activities, SECONDARY2_NAME);
    let mut program = agent.create_program();

    program.run().await;
}

/// Init routine that should be called before the application execution
fn init() -> (TracingLibrary, Vec<TopicHandle>) {
    // Initialize logger and topic registration
    let mut logger = TracingLibraryBuilder::new()
        .global_log_level(LOG_LEVEL)
        .enable_tracing(TRACE_SCOPE)
        .enable_logging(LOG_ENABLE)
        .build();
    logger.init_log_trace();
    let topic_handles = initialize_topics();

    // Start the orchestrator's event polling thread
    Event::get_instance()
        .lock()
        .unwrap()
        .create_polling_thread();

    (logger, topic_handles)
}

fn main() {
    // Init and keep logger and topic handles alive
    let (_logger_guard, _topics_guard) = init();

    // Since runtime `enter_engine` is now not blocking, we do it manually here.
    let waiter = Arc::new(ThreadWaitBarrier::new(1));
    let notifier = waiter.get_notifier().unwrap();

    // Run the main program
    AsyncRuntimeBuilder::new()
        .with_engine(
            ExecutionEngineBuilder::new()
                .task_queue_size(ENGINE_TASK_QUEUE_SIZE)
                .workers(ENGINE_NUM_OF_WORKERS),
        )
        .build()
        .unwrap()
        .enter_engine(async {
            main_program().await;
            notifier.ready();
        })
        .unwrap_or_default();

    waiter
        .wait_for_all(Duration::new(2000, 0))
        .unwrap_or_default();
}
