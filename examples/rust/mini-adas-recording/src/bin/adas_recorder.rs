// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use feo::prelude::AgentId;
use feo::recording::recorder;
use feo::recording::recorder::RecordingRules;
use feo_log::{debug, LevelFilter};
use mini_adas_recording::activities::messages::{
    self, BrakeInstruction, CameraImage, RadarScan, Scene, Steering,
};
use mini_adas_recording::config::{
    TOPIC_CAMERA_FRONT, TOPIC_CONTROL_BRAKES, TOPIC_CONTROL_STEERING, TOPIC_INFERRED_SCENE,
    TOPIC_RADAR_FRONT,
};
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

const AGENT_ID: AgentId = AgentId::new(900);
const REMOTE_SOCKET: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8081);

fn main() {
    feo_logger::init(LevelFilter::Trace, true, true);

    let type_registry = messages::type_registry();
    let recording_rules: RecordingRules = HashMap::from([
        (TOPIC_CAMERA_FRONT, core::any::type_name::<CameraImage>()),
        (
            TOPIC_CONTROL_BRAKES,
            core::any::type_name::<BrakeInstruction>(),
        ),
        (TOPIC_CONTROL_STEERING, core::any::type_name::<Steering>()),
        (TOPIC_INFERRED_SCENE, core::any::type_name::<Scene>()),
        (TOPIC_RADAR_FRONT, core::any::type_name::<RadarScan>()),
    ]);

    debug!("Creating recorder");
    let mut recorder = recorder::Recorder::new(
        AGENT_ID,
        REMOTE_SOCKET,
        "rec.bin",
        recording_rules,
        &type_registry,
    )
    .expect("failed to setup recorder");

    debug!("Starting to record");
    recorder.run()
}
