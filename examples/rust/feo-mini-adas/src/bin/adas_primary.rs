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
use feo_mini_adas::activities::components::{
    Camera, Radar, BREAK_CTL_ACTIVITY_NAME, CAM_ACTIVITY_NAME, EMG_BREAK_ACTIVITY_NAME,
    ENV_READER_ACTIVITY_NAME, LANE_ASST_ACTIVITY_NAME, NEURAL_NET_ACTIVITY_NAME, PRIMARY_NAME,
    RADAR_ACTIVITY_NAME, SECONDARY1_NAME, SECONDARY2_NAME, STR_CTL_ACTIVITY_NAME,
};
use feo_mini_adas::activities::runtime_adapters::{
    activity_into_invokes, GlobalOrchestrator, LocalFeoAgent,
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
const DEFAULT_FEO_CYCLE_TIME: Duration = Duration::from_secs(1);

fn main() {
    let params = Params::from_args();

    //Initialize in LogMode with AppScope
    let mut logger = TracingLibraryBuilder::new()
        .global_log_level(Level::DEBUG)
        .enable_tracing(TraceScope::SystemScope)
        .enable_logging(true)
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
                .workers(3),
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
            // VEC of activities(s) which has to be executed in sequence, TRUE: if the activities(s) can be executed concurrently.
            let execution_structure = vec![
                (vec![CAM_ACTIVITY_NAME], true),
                (vec![RADAR_ACTIVITY_NAME], true),
                (vec![NEURAL_NET_ACTIVITY_NAME], false),
                (vec![ENV_READER_ACTIVITY_NAME], true),
                (vec![EMG_BREAK_ACTIVITY_NAME, BREAK_CTL_ACTIVITY_NAME], true),
                (vec![LANE_ASST_ACTIVITY_NAME, STR_CTL_ACTIVITY_NAME], true),
            ];

            let local_agent_program = async_runtime::spawn(async {
                let cam_act = Arc::new(Mutex::new(Camera::build(1.into(), TOPIC_CAMERA_FRONT)));
                let radar_act = Arc::new(Mutex::new(Radar::build(2.into(), TOPIC_RADAR_FRONT)));

                let mut acts = Vec::new();
                acts.push(activity_into_invokes(&cam_act));
                acts.push(activity_into_invokes(&radar_act));

                let mut agent = LocalFeoAgent::new(acts, PRIMARY_NAME);
                let mut program = agent.create_program();
                println!("{:?}", program);

                program.run().await;
            });

            let global_orch = GlobalOrchestrator::new(agents, params.feo_cycle_time);

            global_orch.run(&execution_structure).await;
            local_agent_program.await;

            notifier.ready();
        })
        .unwrap_or_default();

    waiter
        .wait_for_all(Duration::new(2000, 0))
        .unwrap_or_default();
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
