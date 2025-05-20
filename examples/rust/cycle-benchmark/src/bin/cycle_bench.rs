// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use cycle_benchmark::config::{ApplicationConfig, SignallingType};
use feo::ids::AgentId;
use feo::recording::recorder::RecordingRules;
use feo::recording::registry::TypeRegistry;
use feo_time::Duration;

const DEFAULT_FEO_CYCLE_TIME: Duration = Duration::from_millis(5);

fn main() {
    // Uncomment one or both of the following lines for benchmarking with logging/tracing
    // feo_logger::init(feo_log::LevelFilter::Debug, true, true);
    // feo_tracing::init(feo_tracing::LevelFilter::TRACE);

    let params = Params::from_args();
    let app_config = ApplicationConfig::load();

    if params.agent_id == app_config.primary() {
        run_as_primary(params, app_config);
    } else if app_config.secondaries().contains(&params.agent_id) {
        run_as_secondary(params, app_config);
    } else if app_config.recorders().contains(&params.agent_id) {
        run_as_recorder(params, app_config);
    } else {
        eprintln!(
            "ERROR: Agent or recorder id {} not defined in system configuration",
            params.agent_id
        );
    }
}

fn run_as_primary(params: Params, app_config: ApplicationConfig) {
    let signalling = app_config.signalling();
    println!(
        "Starting primary agent {} using signalling {:?}",
        params.agent_id, signalling
    );

    match signalling {
        SignallingType::DirectMpsc => {
            let config = direct_mpsc::make_primary_config(params, app_config);
            direct_mpsc::Primary::new(config).run().unwrap();
        }
        signalling @ SignallingType::DirectTcp | signalling @ SignallingType::DirectUnix => {
            let config = direct_sockets::make_primary_config(params, app_config, signalling);
            direct_sockets::Primary::new(config).run().unwrap();
        }
        signalling @ SignallingType::RelayedTcp | signalling @ SignallingType::RelayedUnix => {
            let config = relayed_sockets::make_primary_config(params, app_config, signalling);
            relayed_sockets::Primary::new(config).run().unwrap();
        }
    }
}

fn run_as_secondary(params: Params, app_config: ApplicationConfig) {
    let signalling = app_config.signalling();
    println!(
        "Starting secondary agent {} using signalling {:?}",
        params.agent_id, signalling
    );

    match signalling {
        SignallingType::DirectMpsc => {
            let config = direct_mpsc::make_secondary_config(params, app_config);
            direct_mpsc::Secondary::new(config).run();
        }
        signalling @ SignallingType::DirectTcp | signalling @ SignallingType::DirectUnix => {
            let config = direct_sockets::make_secondary_config(params, app_config, signalling);
            direct_sockets::Secondary::new(config).run();
        }
        signalling @ SignallingType::RelayedTcp | signalling @ SignallingType::RelayedUnix => {
            let config = relayed_sockets::make_secondary_config(params, app_config, signalling);
            relayed_sockets::Secondary::new(config).run();
        }
    }
}

fn run_as_recorder(params: Params, app_config: ApplicationConfig) {
    let signalling = app_config.signalling();
    println!(
        "Starting recorder {} using signalling {:?}",
        params.agent_id, signalling
    );

    // the benchmarking application does not exchange data,
    // so have an empty type registry and an empty set of recording rules
    let registry = TypeRegistry::default();
    let rules: RecordingRules = Default::default();

    match signalling {
        SignallingType::DirectMpsc => {
            let config = direct_mpsc::make_recorder_config(params, app_config, &registry, rules);
            direct_mpsc::Recorder::new(config).run();
        }
        signalling @ SignallingType::DirectTcp | signalling @ SignallingType::DirectUnix => {
            let config = direct_sockets::make_recorder_config(
                params, app_config, &registry, rules, signalling,
            );
            direct_sockets::Recorder::new(config).run();
        }
        signalling @ SignallingType::RelayedTcp | signalling @ SignallingType::RelayedUnix => {
            let config = relayed_sockets::make_recorder_config(
                params, app_config, &registry, rules, signalling,
            );
            relayed_sockets::Recorder::new(config).run();
        }
    }
}

/// Parameters of the primary
struct Params {
    /// Agent ID
    agent_id: AgentId,

    /// Cycle time in milli seconds
    feo_cycle_time: Duration,
}

impl Params {
    fn from_args() -> Self {
        let args: Vec<String> = std::env::args().collect();

        // First argument is the ID of this agent
        let agent_id = args
            .get(1)
            .and_then(|x| x.parse::<u64>().ok())
            .map(AgentId::new)
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

mod direct_mpsc {
    use super::{Duration, Params};
    use cycle_benchmark::config::ApplicationConfig;
    use feo::recording::recorder::RecordingRules;
    use feo::recording::registry::TypeRegistry;

    pub(super) use feo::agent::direct::primary_mpsc::{Primary, PrimaryConfig};
    pub(super) use feo::agent::direct::recorder::{Recorder, RecorderConfig};
    pub(super) use feo::agent::direct::secondary::{Secondary, SecondaryConfig};

    pub(super) fn make_primary_config(
        params: Params,
        app_config: ApplicationConfig,
    ) -> PrimaryConfig {
        assert!(
            app_config.secondaries().is_empty(),
            "mpsc-only signalling does not support multi-agent configurations",
        );
        assert!(
            app_config.recorders().is_empty(),
            "ERROR: mpsc-only signalling does not support configurations with recorders"
        );

        let agent_id = params.agent_id;
        PrimaryConfig {
            cycle_time: params.feo_cycle_time,
            activity_dependencies: app_config.activity_dependencies(),
            // With only one agent, we cannot attach a recorder
            recorder_ids: vec![],
            worker_assignments: app_config.worker_assignments().remove(&agent_id).unwrap(),
            timeout: Duration::from_secs(10),
        }
    }

    pub(super) fn make_secondary_config(_: Params, _: ApplicationConfig) -> SecondaryConfig {
        panic!("direct mpsc signalling does not support secondary agents");
    }

    pub(super) fn make_recorder_config(
        _: Params,
        _: ApplicationConfig,
        _: &TypeRegistry,
        _: RecordingRules,
    ) -> RecorderConfig {
        panic!("direct mpsc signalling does not support recorders");
    }
}

mod direct_sockets {
    use super::{Duration, Params};
    use cycle_benchmark::config::{ApplicationConfig, SignallingType};
    use feo::agent::NodeAddress;
    use feo::recording::recorder::RecordingRules;
    use feo::recording::registry::TypeRegistry;

    pub(super) use feo::agent::direct::primary::{Primary, PrimaryConfig};
    pub(super) use feo::agent::direct::recorder::{Recorder, RecorderConfig};
    pub(super) use feo::agent::direct::secondary::{Secondary, SecondaryConfig};

    fn endpoint(app_config: &ApplicationConfig, signalling: SignallingType) -> NodeAddress {
        match signalling {
            SignallingType::DirectTcp => NodeAddress::Tcp(app_config.bind_addrs().0),
            SignallingType::DirectUnix => NodeAddress::UnixSocket(app_config.socket_paths().0),
            other => panic!("no endpoint defined for signalling type {other:?}"),
        }
    }

    pub(super) fn make_primary_config(
        params: Params,
        app_config: ApplicationConfig,
        signalling: SignallingType,
    ) -> PrimaryConfig {
        let agent_id = params.agent_id;
        PrimaryConfig {
            cycle_time: params.feo_cycle_time,
            activity_dependencies: app_config.activity_dependencies(),
            recorder_ids: app_config.recorders(),
            worker_assignments: app_config.worker_assignments().remove(&agent_id).unwrap(),
            timeout: Duration::from_secs(10),
            endpoint: endpoint(&app_config, signalling),
        }
    }

    pub(super) fn make_secondary_config(
        params: Params,
        app_config: ApplicationConfig,
        signalling: SignallingType,
    ) -> SecondaryConfig {
        SecondaryConfig {
            id: params.agent_id,
            worker_assignments: app_config
                .worker_assignments()
                .remove(&params.agent_id)
                .unwrap(),
            timeout: Duration::from_secs(1),
            endpoint: endpoint(&app_config, signalling),
        }
    }

    pub(super) fn make_recorder_config(
        params: Params,
        app_config: ApplicationConfig,
        type_registry: &TypeRegistry,
        recording_rules: RecordingRules,
        signalling: SignallingType,
    ) -> RecorderConfig {
        let agent_id = params.agent_id;
        RecorderConfig {
            id: agent_id,
            record_file: "./rec.bin",
            rules: recording_rules,
            registry: type_registry,
            receive_timeout: Duration::from_secs(10),
            endpoint: endpoint(&app_config, signalling),
        }
    }
}

mod relayed_sockets {
    use super::{Duration, Params};
    use cycle_benchmark::config::{ApplicationConfig, SignallingType};
    use feo::agent::NodeAddress;
    use feo::recording::recorder::RecordingRules;
    use feo::recording::registry::TypeRegistry;

    pub(super) use feo::agent::relayed::primary::{Primary, PrimaryConfig};
    pub(super) use feo::agent::relayed::recorder::{Recorder, RecorderConfig};
    pub(super) use feo::agent::relayed::secondary::{Secondary, SecondaryConfig};

    fn endpoints(
        app_config: &ApplicationConfig,
        signalling: SignallingType,
    ) -> (NodeAddress, NodeAddress) {
        match signalling {
            SignallingType::RelayedTcp => (
                NodeAddress::Tcp(app_config.bind_addrs().0),
                NodeAddress::Tcp(app_config.bind_addrs().1),
            ),
            SignallingType::RelayedUnix => (
                NodeAddress::UnixSocket(app_config.socket_paths().0),
                NodeAddress::UnixSocket(app_config.socket_paths().1),
            ),
            other => panic!("no endpoint defined for signalling type {other:?}"),
        }
    }

    pub(super) fn make_primary_config(
        params: Params,
        app_config: ApplicationConfig,
        signalling: SignallingType,
    ) -> PrimaryConfig {
        let agent_id = params.agent_id;
        let endpoints = endpoints(&app_config, signalling);

        PrimaryConfig {
            cycle_time: params.feo_cycle_time,
            activity_dependencies: app_config.activity_dependencies(),
            recorder_ids: app_config.recorders(),
            worker_assignments: app_config.worker_assignments().remove(&agent_id).unwrap(),
            timeout: Duration::from_secs(10),
            bind_address_senders: endpoints.0,
            bind_address_receivers: endpoints.1,
            id: agent_id,
            worker_agent_map: app_config.worker_agent_map(),
            activity_worker_map: app_config.activity_worker_map(),
        }
    }

    pub(super) fn make_secondary_config(
        params: Params,
        app_config: ApplicationConfig,
        signalling: SignallingType,
    ) -> SecondaryConfig {
        let agent_id = params.agent_id;
        let endpoints = endpoints(&app_config, signalling);

        SecondaryConfig {
            id: agent_id,
            worker_assignments: app_config.worker_assignments().remove(&agent_id).unwrap(),
            timeout: Duration::from_secs(10),
            bind_address_senders: endpoints.0,
            bind_address_receivers: endpoints.1,
        }
    }

    pub(super) fn make_recorder_config(
        params: Params,
        app_config: ApplicationConfig,
        type_registry: &TypeRegistry,
        recording_rules: RecordingRules,
        signalling: SignallingType,
    ) -> RecorderConfig {
        let agent_id = params.agent_id;
        let endpoints = endpoints(&app_config, signalling);
        RecorderConfig {
            id: agent_id,
            record_file: "./rec.bin",
            rules: recording_rules,
            registry: type_registry,
            receive_timeout: Duration::from_secs(10),
            bind_address_senders: endpoints.0,
            bind_address_receivers: endpoints.1,
        }
    }
}
