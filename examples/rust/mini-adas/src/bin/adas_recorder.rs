// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use feo::ids::AgentId;
use feo::recording::recorder::RecordingRules;
use feo_log::{debug, LevelFilter};
use mini_adas::activities::messages::{
    self, BrakeInstruction, CameraImage, RadarScan, Scene, Steering,
};

use feo::agent::com_init::initialize_com_recorder;
use feo::topicspec::TopicSpecification;
use mini_adas::config::{
    topic_dependencies, COM_BACKEND, TOPIC_CAMERA_FRONT, TOPIC_CONTROL_BRAKES,
    TOPIC_CONTROL_STEERING, TOPIC_INFERRED_SCENE, TOPIC_RADAR_FRONT,
};
use std::collections::HashMap;

fn main() {
    feo_logger::init(LevelFilter::Trace, true, true);

    let params = Params::from_args();

    let registry = &messages::type_registry();
    let rules: RecordingRules = HashMap::from([
        (TOPIC_CAMERA_FRONT, core::any::type_name::<CameraImage>()),
        (
            TOPIC_CONTROL_BRAKES,
            core::any::type_name::<BrakeInstruction>(),
        ),
        (TOPIC_CONTROL_STEERING, core::any::type_name::<Steering>()),
        (TOPIC_INFERRED_SCENE, core::any::type_name::<Scene>()),
        (TOPIC_RADAR_FRONT, core::any::type_name::<RadarScan>()),
    ]);

    let config = cfg::make_config(params.agent_id, rules.clone(), registry);

    // initialize reading based on topic specs corresponding to recording rules
    let topic_specs: Vec<TopicSpecification> = topic_dependencies()
        .into_iter()
        .filter(|s| rules.contains_key(s.topic))
        .collect();

    // Initialize topics. Do not drop.
    let _topic_guards = initialize_com_recorder(COM_BACKEND, topic_specs);

    debug!("Creating recorder with agent id {}", params.agent_id);
    let mut recorder = cfg::Recorder::new(config);

    debug!("Starting to record");
    recorder.run()
}

/// Parameters of the primary
struct Params {
    /// Agent id of the recorder
    agent_id: AgentId,
}

impl Params {
    fn from_args() -> Self {
        let args: Vec<String> = std::env::args().collect();

        // First argument is the agent ID of the recorder
        let agent_id = args
            .get(1)
            .and_then(|x| x.parse::<u64>().ok())
            .map(AgentId::new)
            .expect("missing or invalid agent id");

        Self { agent_id }
    }
}

#[cfg(feature = "signalling_direct_tcp")]
mod cfg {
    use feo::agent::NodeAddress;
    use feo::ids::AgentId;
    use feo::recording::recorder::RecordingRules;
    use feo::recording::registry::TypeRegistry;
    use feo_time::Duration;
    use mini_adas::config::BIND_ADDR;

    pub(super) use feo::agent::direct::recorder::{Recorder, RecorderConfig};

    pub(super) fn make_config(
        agent_id: AgentId,
        rules: RecordingRules,
        registry: &TypeRegistry,
    ) -> RecorderConfig {
        RecorderConfig {
            id: agent_id,
            record_file: "./rec.bin",
            rules,
            registry,
            receive_timeout: Duration::from_secs(10),
            endpoint: NodeAddress::Tcp(BIND_ADDR),
        }
    }
}

#[cfg(feature = "signalling_direct_unix")]
mod cfg {
    use feo::agent::NodeAddress;
    use feo::ids::AgentId;
    use feo::recording::recorder::RecordingRules;
    use feo::recording::registry::TypeRegistry;
    use feo_time::Duration;
    use mini_adas::config::socket_paths;

    pub(super) use feo::agent::direct::recorder::{Recorder, RecorderConfig};

    pub(super) fn make_config(
        agent_id: AgentId,
        rules: RecordingRules,
        registry: &TypeRegistry,
    ) -> RecorderConfig {
        RecorderConfig {
            id: agent_id,
            record_file: "./rec.bin",
            rules,
            registry,
            receive_timeout: Duration::from_secs(10),
            endpoint: NodeAddress::UnixSocket(socket_paths().0),
        }
    }
}

#[cfg(feature = "signalling_relayed_tcp")]
mod cfg {
    use feo::agent::NodeAddress;
    use feo::ids::AgentId;
    use feo::recording::recorder::RecordingRules;
    use feo::recording::registry::TypeRegistry;
    use feo_time::Duration;
    use mini_adas::config::{BIND_ADDR, BIND_ADDR2};

    pub(super) use feo::agent::relayed::recorder::{Recorder, RecorderConfig};

    pub(super) fn make_config(
        agent_id: AgentId,
        rules: RecordingRules,
        registry: &TypeRegistry,
    ) -> RecorderConfig {
        RecorderConfig {
            id: agent_id,
            record_file: "./rec.bin",
            rules,
            registry,
            receive_timeout: Duration::from_secs(10),
            bind_address_senders: NodeAddress::Tcp(BIND_ADDR),
            bind_address_receivers: NodeAddress::Tcp(BIND_ADDR2),
        }
    }
}

#[cfg(feature = "signalling_relayed_unix")]
mod cfg {
    use feo::agent::NodeAddress;
    use feo::ids::AgentId;
    use feo::recording::recorder::RecordingRules;
    use feo::recording::registry::TypeRegistry;
    use feo_time::Duration;
    use mini_adas::config::socket_paths;

    pub(super) use feo::agent::relayed::recorder::{Recorder, RecorderConfig};

    pub(super) fn make_config(
        agent_id: AgentId,
        rules: RecordingRules,
        registry: &TypeRegistry,
    ) -> RecorderConfig {
        RecorderConfig {
            id: agent_id,
            record_file: "./rec.bin",
            rules,
            registry,
            receive_timeout: Duration::from_secs(10),
            bind_address_senders: NodeAddress::UnixSocket(socket_paths().0),
            bind_address_receivers: NodeAddress::UnixSocket(socket_paths().1),
        }
    }
}
