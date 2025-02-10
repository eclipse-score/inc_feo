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

/// Allow up to two recorder processes (that potentially need to subscribe to every topic)
pub const MAX_ADDITIONAL_SUBSCRIBERS: usize = 2;

pub fn pool_configuration() -> HashMap<AgentId, HashMap<WorkerId, Vec<ActivityIdAndBuilder>>> {
    // Assign activities to different workers
    let w40: WorkerAssignment = (
        40.into(),
        vec![(
            0.into(),
            Box::new(|id| Camera::build(id, TOPIC_CAMERA_FRONT)),
        )],
    );
    let w41: WorkerAssignment = (
        41.into(),
        vec![(1.into(), Box::new(|id| Radar::build(id, TOPIC_RADAR_FRONT)))],
    );

    let w42: WorkerAssignment = (
        42.into(),
        vec![
            (
                2.into(),
                Box::new(|id| {
                    NeuralNet::build(
                        id,
                        TOPIC_CAMERA_FRONT,
                        TOPIC_RADAR_FRONT,
                        TOPIC_INFERRED_SCENE,
                    )
                }),
            ),
            (
                3.into(),
                Box::new(|id| EnvironmentRenderer::build(id, TOPIC_INFERRED_SCENE)),
            ),
        ],
    );

    let w43: WorkerAssignment = (
        43.into(),
        vec![
            (
                4.into(),
                Box::new(|id| {
                    EmergencyBraking::build(id, TOPIC_INFERRED_SCENE, TOPIC_CONTROL_BRAKES)
                }),
            ),
            (
                6.into(),
                Box::new(|id| BrakeController::build(id, TOPIC_CONTROL_BRAKES)),
            ),
        ],
    );
    let w44: WorkerAssignment = (
        44.into(),
        vec![
            (
                5.into(),
                Box::new(|id| LaneAssist::build(id, TOPIC_INFERRED_SCENE, TOPIC_CONTROL_STEERING)),
            ),
            (
                7.into(),
                Box::new(|id| SteeringController::build(id, TOPIC_CONTROL_STEERING)),
            ),
        ],
    );

    // Assign workers to pools with exactly one pool belonging to one agent
    let a0: AgentAssignment = (100.into(), vec![w40, w41]);
    let a1: AgentAssignment = (101.into(), vec![w42]);
    let a2: AgentAssignment = (102.into(), vec![w43, w44]);

    let assignments = vec![a0, a1, a2];

    let mut agent_map = HashMap::new();
    for (agent, workers) in assignments {
        let mut worker_map = HashMap::new();
        for (worker_id, activities) in workers {
            let previous = worker_map.insert(worker_id, activities);
            assert!(
                previous.is_none(),
                "Duplicate worker {worker_id} in assignment list"
            );
        }
        let previous = agent_map.insert(agent, worker_map);
        assert!(
            previous.is_none(),
            "Duplicate agent {agent} in assignment list"
        );
    }
    agent_map
}

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
                .count()
                + MAX_ADDITIONAL_SUBSCRIBERS;

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
