// Copyright 2025 Qorix
//
// SPDX-License-Identifier: Apache-2.0
use feo::prelude::*;
use feo_mini_adas::activities::components::{
    BrakeController, Camera, EmergencyBraking, EnvironmentRenderer, LaneAssist, NeuralNet, Radar,
    SteeringController, BREAK_CTL_ACTIVITY_NAME, CAM_ACTIVITY_NAME, EMG_BREAK_ACTIVITY_NAME,
    ENV_READER_ACTIVITY_NAME, LANE_ASST_ACTIVITY_NAME, NEURAL_NET_ACTIVITY_NAME, PRIMARY_NAME,
    RADAR_ACTIVITY_NAME, SECONDARY1_NAME, SECONDARY2_NAME, STR_CTL_ACTIVITY_NAME,
};
use feo_mini_adas::activities::runtime_adapters::{
    activity_into_invokes, GlobalOrchestrator, LocalFeoAgent,
};
use feo_mini_adas::config::*;
use logging_tracing::prelude::*;
use std::sync::{Arc, Mutex};

// ****************************************************************** //
// **                  SETTINGS                                    ** //
// ****************************************************************** //

const PRIMARY_AGENT_ID: AgentId = AgentId::new(100);
const SECONDARY_1_AGENT_ID: AgentId = AgentId::new(101);
const SECONDARY_2_AGENT_ID: AgentId = AgentId::new(102);

// ****************************************************************** //
// **                  PROGRAMS                                    ** //
// ****************************************************************** //

/// The entry point of the application
pub(crate) async fn main_program() {
    let agents: Vec<String> = vec![
        PRIMARY_NAME.to_string(),
        SECONDARY1_NAME.to_string(),
        SECONDARY2_NAME.to_string(),
    ];

    // VEC of activitie(s) which has to be executed in sequence, TRUE: if the activitie(s) can be executed concurrently.
    let execution_structure = vec![
        (vec![CAM_ACTIVITY_NAME], true),
        (vec![RADAR_ACTIVITY_NAME], true),
        (vec![NEURAL_NET_ACTIVITY_NAME], false),
        (vec![ENV_READER_ACTIVITY_NAME], true),
        (vec![EMG_BREAK_ACTIVITY_NAME, BREAK_CTL_ACTIVITY_NAME], true),
        (vec![LANE_ASST_ACTIVITY_NAME, STR_CTL_ACTIVITY_NAME], true),
    ];

    let primary_agent_program = async_runtime::spawn(primary_agent_program());
    let secondary_1_agent_program = async_runtime::spawn(secondary_1_agent_program());
    let secondary_2_agent_program = async_runtime::spawn(secondary_2_agent_program());

    let global_orch = GlobalOrchestrator::new(agents);
    global_orch.run(&execution_structure).await;
    primary_agent_program.await;
    secondary_1_agent_program.await;
    secondary_2_agent_program.await;
}

async fn primary_agent_program() {
    info!("Starting primary agent {PRIMARY_AGENT_ID}. Waiting for connections",);

    let cam_act = Arc::new(Mutex::new(Camera::build(1.into(), TOPIC_CAMERA_FRONT)));
    let radar_act = Arc::new(Mutex::new(Radar::build(2.into(), TOPIC_RADAR_FRONT)));

    let mut acts = Vec::new();
    acts.push(activity_into_invokes(&cam_act));
    acts.push(activity_into_invokes(&radar_act));

    let mut agent = LocalFeoAgent::new(acts, PRIMARY_NAME);
    let mut program = agent.create_program();
    println!("{:?}", program);

    program.run_n(5).await;
}

async fn secondary_1_agent_program() {
    info!("Starting secondary_1 agent {SECONDARY_1_AGENT_ID}",);

    let neural_net_act = Arc::new(Mutex::new(NeuralNet::build_val(
        3.into(),
        TOPIC_CAMERA_FRONT,
        TOPIC_RADAR_FRONT,
        TOPIC_INFERRED_SCENE,
    )));

    let environ_renderer_act = Arc::new(Mutex::new(EnvironmentRenderer::build(
        4.into(),
        TOPIC_INFERRED_SCENE,
    )));

    let mut acts = Vec::new();
    acts.push(activity_into_invokes(&neural_net_act));
    acts.push(activity_into_invokes(&environ_renderer_act));

    let mut agent = LocalFeoAgent::new(acts, SECONDARY1_NAME);
    let mut program = agent.create_program();

    program.run_n(5).await;
}

async fn secondary_2_agent_program() {
    info!("Starting secondary_2 agent {SECONDARY_2_AGENT_ID}",);

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

    program.run_n(5).await;
}
