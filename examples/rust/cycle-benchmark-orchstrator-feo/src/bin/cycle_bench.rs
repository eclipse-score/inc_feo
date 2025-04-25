// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use async_runtime::runtime::runtime::AsyncRuntimeBuilder;
use async_runtime::scheduler::execution_engine::ExecutionEngineBuilder;
use cycle_benchmark::activities::DummyActivity;
use cycle_benchmark::config::ApplicationConfig;
use cycle_benchmark::runtime_adapters::{
    activity_into_invokes, get_activity_name, get_agent_name, init_activity_ids, init_agent_ids,
    ActivityDetails, GlobalOrchestrator, LocalFeoAgent,
};
use feo::prelude::ActivityId;
use feo_log::info;
use feo_time::Duration;
use foundation::threading::thread_wait_barrier::ThreadWaitBarrier;
use logging_tracing::prelude::*;
use orchestration::prelude::Event;
use std::collections::HashMap;
use std::string::ToString;
use std::sync::{Arc, Mutex};
// TODO: number of threads

const DEFAULT_FEO_CYCLE_TIME: Duration = Duration::from_millis(5);

// Finish the program after this number of cycles
const NUM_FEO_CYCLES: usize = 1000000;

// log level
const LOG_LEVEL: Level = Level::ERROR; // same as for tracing

fn main() {
    // Uncomment one or both of the following lines for benchmarking with logging/tracing
    // feo_logger::init(feo_log::LevelFilter::Debug, true, true);  // TODO: not working in orchestrator?
    // feo_tracing::init(feo_tracing::LevelFilter::INFO);

    // // Initialize LogMode with AppScope
    // let mut logger = TracingLibraryBuilder::new()
    //     .global_log_level(LOG_LEVEL)
    //     .enable_tracing(TraceScope::SystemScope)
    //     .enable_logging(true)
    //     .build();
    //
    // logger.init_log_trace();

    let params = Params::from_args();
    let app_config = ApplicationConfig::load();

    // initialize static maps of agent names and activity names for orchestrator
    let activity_names: HashMap<usize, String> = app_config
        .all_activities()
        .iter()
        .map(|id| (*id, id.to_string()))
        .collect();
    let agent_names: HashMap<usize, String> = app_config
        .all_agents()
        .iter()
        .map(|id| (*id, id.to_string()))
        .collect();
    init_activity_ids(activity_names);
    init_agent_ids(agent_names);

    if params.agent_id == app_config.primary() {
        run_as_primary(params, app_config);
    } else if app_config.secondaries().contains(&params.agent_id) {
        run_as_secondary(params, app_config);
    } else {
        eprintln!(
            "ERROR: Agent id {} not defined in system configuration",
            params.agent_id
        );
    }
}

fn run_as_primary(params: Params, app_config: ApplicationConfig) {
    println!("Starting primary agent {}", params.agent_id);

    let agent_id = params.agent_id;
    let (local_activity_sequence, concurrency) = app_config.sequence_and_concurrency(agent_id);

    println!("Agent activity sequence: {:?}", local_activity_sequence);
    println!("Agent concurrency: {:?}", concurrency);

    let mut runtime = AsyncRuntimeBuilder::new()
        .with_engine(
            ExecutionEngineBuilder::new()
                .task_queue_size(256)
                .workers(app_config.num_threads(agent_id)),
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

    let activity_graph_names: Vec<(Vec<&'static str>, bool)> = app_config
        .activity_graph()
        .into_iter()
        .map(|(ids, parallel)| {
            let ids: Vec<&'static str> = ids.iter().map(|id| get_activity_name(*id)).collect();
            (ids, parallel)
        })
        .collect();

    println!("activity graph: {:?}", activity_graph_names);

    let agent_names: Vec<String> = app_config
        .all_agents()
        .iter()
        .map(|id| get_agent_name(*id).to_string())
        .collect();

    runtime
        .enter_engine(async move {
            // VEC of activities(s) which has to be executed in sequence, TRUE: if the activities(s) can be executed concurrently.

            let local_agent_program = async_runtime::spawn(async move {
                let acts = activities(&app_config, &agent_id);
                let mut agent = LocalFeoAgent::new(acts, get_agent_name(agent_id));
                let mut program = agent.create_program();
                println!("{:?}", program);

                program.run().await;
            });

            let global_orch =
                GlobalOrchestrator::new(agent_names, params.feo_cycle_time, NUM_FEO_CYCLES);

            let execution_structure = activity_graph_names;
            global_orch.run(&execution_structure).await;
            local_agent_program.await;

            notifier.ready();
        })
        .unwrap_or_default();

    waiter
        .wait_for_all(Duration::new(2000, 0))
        .unwrap_or_default();
}

fn run_as_secondary(params: Params, app_config: ApplicationConfig) {
    println!("Starting secondary agent {}", params.agent_id);

    let agent_id = params.agent_id;
    let (activity_sequence, concurrency) = app_config.sequence_and_concurrency(agent_id);

    println!("Agent activity sequence: {:?}", activity_sequence);
    println!("Agent concurrency: {:?}", concurrency);

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
        .enter_engine(async move {
            let acts = activities(&app_config, &agent_id);
            let mut agent = LocalFeoAgent::new(acts, get_agent_name(agent_id));
            let mut program = agent.create_program();
            println!("{:?}", program);

            program.run().await;
            info!("Finished");
            notifier.ready();
        })
        .unwrap_or_default();

    waiter
        .wait_for_all(Duration::new(2000, 0))
        .unwrap_or_default();
}

fn activities(cfg: &ApplicationConfig, agent_id: &usize) -> Vec<ActivityDetails> {
    cfg.activities(*agent_id)
        .iter()
        .map(|id| {
            activity_into_invokes(&Arc::new(Mutex::new(DummyActivity::build(
                &ActivityId::from(*id),
            ))))
        })
        .collect()
}

/// Parameters of the primary
struct Params {
    /// Agent ID
    agent_id: usize,

    /// Cycle time in milli seconds
    feo_cycle_time: Duration,
}

impl Params {
    fn from_args() -> Self {
        let args: Vec<String> = std::env::args().collect();

        // First argument is the ID of this agent
        let agent_id = args
            .get(1)
            .and_then(|x| x.parse::<usize>().ok())
            .expect("Missing or invalid agent id");

        // Second argument is the cycle time in milli seconds, e.g. 30 or 2500,
        // only needed for primary agent, ignored for secondaries
        let feo_cycle_time = args
            .get(2)
            .and_then(|x| x.parse::<u64>().ok())
            .map(Duration::from_millis)
            .unwrap_or(DEFAULT_FEO_CYCLE_TIME);

        Self {
            agent_id,
            feo_cycle_time,
        }
    }
}
