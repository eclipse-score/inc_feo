// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use crate::activities::components::{
    BrakeController, Camera, EmergencyBraking, EnvironmentRenderer, LaneAssist, NeuralNet, Radar,
    SteeringController,
};
use crate::activities::messages::{BrakeInstruction, CameraImage, RadarScan, Scene, Steering};
use configuration::topics::Direction;
use feo::activity::ActivityIdAndBuilder;
use feo::com::{init_topic, TopicHandle};
use feo::configuration::topics::TopicSpecification;
use feo::prelude::*;
use std::collections::HashMap;

pub type WorkerAssignment = (WorkerId, Vec<(ActivityId, Box<dyn ActivityBuilder>)>);
pub type AgentAssignment = (AgentId, Vec<WorkerAssignment>);

// For each activity list the activities it needs to wait for
pub type ActivityDependencies = HashMap<ActivityId, Vec<ActivityId>>;

pub const TOPIC_INFERRED_SCENE: &str = "feo/com/vehicle/inferred/scene";
pub const TOPIC_CONTROL_BRAKES: &str = "feo/com/vehicle/control/brakes";
pub const TOPIC_CONTROL_STEERING: &str = "feo/com/vehicle/control/steering";
pub const TOPIC_CAMERA_FRONT: &str = "feo/com/vehicle/camera/front";
pub const TOPIC_RADAR_FRONT: &str = "feo/com/vehicle/radar/front";

pub const CAM_ACTIVITY_NAME: &'static str = "cam_activity";
pub const RADAR_ACTIVITY_NAME: &'static str = "radar_activity";
pub const NEURAL_NET_ACTIVITY_NAME: &'static str = "neuralnet_activity";
pub const ENV_READER_ACTIVITY_NAME: &'static str = "env_reader_activity";
pub const EMG_BREAK_ACTIVITY_NAME: &'static str = "emg_break_activity";
pub const BREAK_CTL_ACTIVITY_NAME: &'static str = "break_ctl_activity";
pub const LANE_ASST_ACTIVITY_NAME: &'static str = "lane_asst_activity";
pub const STR_CTL_ACTIVITY_NAME: &'static str = "str_ctl_activity";

pub const APPLICATION_NAME: &'static str = "adas_feo";
pub const PRIMARY_NAME: &'static str = "primary_agent";
pub const SECONDARY1_NAME: &'static str = "secondary1_agent";
pub const SECONDARY2_NAME: &'static str = "secondary2_agent";

pub fn activity_dependencies() -> ActivityDependencies {
    //      Primary              |       Secondary1         |                  Secondary2
    // ---------------------------------------------------------------------------------------------------
    //
    //   Camera(40)   Radar(41)
    //        \           \
    //                                 NeuralNet(42)
    //                                      |                           \                     \
    //                             EnvironmentRenderer(42)       EmergencyBraking(43)    LaneAssist(44)
    //                                                                   |                     |
    //                                                            BrakeController(43)   SteeringController(44)

    let dependencies = [
        // Camera
        (0.into(), vec![]),
        // Radar
        (1.into(), vec![]),
        // NeuralNet
        (2.into(), vec![0.into(), 1.into()]),
        // EnvironmentRenderer
        (3.into(), vec![2.into()]),
        // EmergencyBraking
        (4.into(), vec![2.into()]),
        // LaneAssist
        (5.into(), vec![2.into()]),
        // BrakeController
        (6.into(), vec![4.into()]),
        // SteeringController
        (7.into(), vec![5.into()]),
    ];

    dependencies.into()
}

pub fn initialize_topics() -> Vec<TopicHandle> {
    topic_dependencies()
        .into_iter()
        .map(|spec| {
            let writers = spec
                .peers
                .iter()
                .filter(|(_, dir)| matches!(dir, Direction::Outgoing))
                .count();
            let readers = spec
                .peers
                .iter()
                .filter(|(_, dir)| matches!(dir, Direction::Incoming))
                .count();

            (spec.init_fn)(writers, readers)
        })
        .collect()
}

fn topic_dependencies() -> Vec<TopicSpecification> {
    use Direction::*;
    vec![
        TopicSpecification {
            peers: vec![(0.into(), Outgoing), (2.into(), Incoming)],
            init_fn: Box::new(|w, r| init_topic::<CameraImage>(TOPIC_CAMERA_FRONT, w, r)),
        },
        TopicSpecification {
            peers: vec![(1.into(), Outgoing), (2.into(), Incoming)],
            init_fn: Box::new(|w, r| init_topic::<RadarScan>(TOPIC_RADAR_FRONT, w, r)),
        },
        TopicSpecification {
            peers: vec![
                (2.into(), Outgoing),
                (3.into(), Incoming),
                (4.into(), Incoming),
                (5.into(), Incoming),
            ],
            init_fn: Box::new(|w, r| init_topic::<Scene>(TOPIC_INFERRED_SCENE, w, r)),
        },
        TopicSpecification {
            peers: vec![(4.into(), Outgoing), (6.into(), Incoming)],
            init_fn: Box::new(|w, r| init_topic::<BrakeInstruction>(TOPIC_CONTROL_BRAKES, w, r)),
        },
        TopicSpecification {
            peers: vec![(5.into(), Outgoing), (7.into(), Incoming)],
            init_fn: Box::new(|w, r| init_topic::<Steering>(TOPIC_CONTROL_STEERING, w, r)),
        },
    ]
}
