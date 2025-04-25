// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use feo::agent::com_init::initialize_com_primary;
use feo::ids::AgentId;
use feo_log::{info, LevelFilter};
use feo_time::Duration;
use mini_adas::config::{
    agent_assignments_ids, topic_dependencies, COM_BACKEND, MAX_ADDITIONAL_SUBSCRIBERS,
};
use std::collections::HashSet;

const AGENT_ID: AgentId = AgentId::new(100);
const DEFAULT_FEO_CYCLE_TIME: Duration = Duration::from_secs(5);

fn main() {
    feo_logger::init(LevelFilter::Debug, true, true);
    feo_tracing::init(feo_tracing::LevelFilter::TRACE);

    let params = Params::from_args();

    info!("Starting primary agent {AGENT_ID}");

    let config = cfg::make_config(params);

    // Initialize topics. Do not drop.
    let _topic_guards = initialize_com_primary(
        COM_BACKEND,
        AGENT_ID,
        topic_dependencies(),
        &agent_assignments_ids(),
        MAX_ADDITIONAL_SUBSCRIBERS,
    );

    // Setup primary
    let mut primary = cfg::Primary::new(config);

    // Run primary
    primary.run().unwrap()
}

/// Parameters of the primary
struct Params {
    /// Cycle time in milli seconds
    feo_cycle_time: Duration,
    /// Recorder IDs
    #[allow(dead_code)]
    recorder_ids: Vec<AgentId>,
}

impl Params {
    fn from_args() -> Self {
        let args: Vec<String> = std::env::args().collect();

        // First argument is the cycle time in milli seconds, e.g. 30 or 2500
        let feo_cycle_time = args
            .get(1)
            .and_then(|x| x.parse::<u64>().ok())
            .map(Duration::from_millis)
            .unwrap_or(DEFAULT_FEO_CYCLE_TIME);

        // Second argument are the recorder IDs to wait for as dot-separated list, e.g. 900 or 900.901
        let recorder_ids = args
            .get(2)
            .and_then(|s| {
                s.split('.')
                    .map(|id| id.parse::<u64>().map(AgentId::from))
                    .collect::<Result<_, _>>()
                    .ok()
            })
            .unwrap_or_default();

        Self {
            feo_cycle_time,
            recorder_ids,
        }
    }
}

#[cfg(feature = "signalling_direct_mpsc")]
mod cfg {
    use super::{Duration, Params, AGENT_ID};
    use mini_adas::config::{activity_dependencies, agent_assignments};

    pub(super) use feo::agent::direct::primary_mpsc::{Primary, PrimaryConfig};

    pub(super) fn make_config(params: Params) -> PrimaryConfig {
        PrimaryConfig {
            cycle_time: params.feo_cycle_time,
            activity_dependencies: activity_dependencies(),
            // With only one agent, we cannot attach a recorder
            recorder_ids: vec![],
            worker_assignments: agent_assignments().remove(&AGENT_ID).unwrap(),
            timeout: Duration::from_secs(10),
        }
    }
}

#[cfg(feature = "signalling_direct_tcp")]
mod cfg {
    use super::{check_ids, Duration, Params, AGENT_ID};
    use feo::agent::NodeAddress;
    use feo::ids::AgentId;
    use mini_adas::config::{activity_dependencies, agent_assignments, BIND_ADDR};
    use std::collections::HashSet;

    pub(super) use feo::agent::direct::primary::{Primary, PrimaryConfig};

    pub(super) fn make_config(params: Params) -> PrimaryConfig {
        let agent_ids: HashSet<AgentId> = agent_assignments().keys().copied().collect();
        check_ids(&params.recorder_ids, &agent_ids);

        PrimaryConfig {
            cycle_time: params.feo_cycle_time,
            activity_dependencies: activity_dependencies(),
            recorder_ids: params.recorder_ids,
            worker_assignments: agent_assignments().remove(&AGENT_ID).unwrap(),
            timeout: Duration::from_secs(10),
            endpoint: NodeAddress::Tcp(BIND_ADDR),
        }
    }
}

#[cfg(feature = "signalling_direct_unix")]
mod cfg {
    use super::{check_ids, Duration, Params, AGENT_ID};
    use feo::agent::NodeAddress;
    use feo::ids::AgentId;
    use mini_adas::config::{activity_dependencies, agent_assignments, socket_paths};
    use std::collections::HashSet;

    pub(super) use feo::agent::direct::primary::{Primary, PrimaryConfig};

    pub(super) fn make_config(params: Params) -> PrimaryConfig {
        let agent_ids: HashSet<AgentId> = agent_assignments().keys().copied().collect();
        check_ids(&params.recorder_ids, &agent_ids);

        PrimaryConfig {
            cycle_time: params.feo_cycle_time,
            activity_dependencies: activity_dependencies(),
            recorder_ids: params.recorder_ids,
            worker_assignments: agent_assignments().remove(&AGENT_ID).unwrap(),
            timeout: Duration::from_secs(10),
            endpoint: NodeAddress::UnixSocket(socket_paths().0),
        }
    }
}

#[cfg(feature = "signalling_relayed_tcp")]
mod cfg {
    use super::{check_ids, Duration, Params, AGENT_ID};
    use feo::agent::NodeAddress;
    use feo::ids::{ActivityId, AgentId, WorkerId};
    use mini_adas::config::{
        activity_dependencies, agent_assignments, worker_agent_map, BIND_ADDR, BIND_ADDR2,
    };
    use std::collections::{HashMap, HashSet};

    pub(super) use feo::agent::relayed::primary::{Primary, PrimaryConfig};

    pub(super) fn make_config(params: Params) -> PrimaryConfig {
        let activity_worker_map: HashMap<ActivityId, WorkerId> = agent_assignments()
            .values()
            .flat_map(|vec| {
                vec.iter()
                    .flat_map(move |(wid, aid_b)| aid_b.iter().map(|v| (v.0, *wid)))
            })
            .collect();

        let agent_ids: HashSet<AgentId> = agent_assignments().keys().copied().collect();
        check_ids(&params.recorder_ids, &agent_ids);

        PrimaryConfig {
            cycle_time: params.feo_cycle_time,
            activity_dependencies: activity_dependencies(),
            recorder_ids: params.recorder_ids,
            worker_assignments: agent_assignments().remove(&AGENT_ID).unwrap(),
            timeout: Duration::from_secs(10),
            bind_address_senders: NodeAddress::Tcp(BIND_ADDR),
            bind_address_receivers: NodeAddress::Tcp(BIND_ADDR2),
            id: AGENT_ID,
            worker_agent_map: worker_agent_map(),
            activity_worker_map,
        }
    }
}

#[cfg(feature = "signalling_relayed_unix")]
mod cfg {
    use super::{check_ids, Duration, Params, AGENT_ID};
    use feo::agent::NodeAddress;
    use feo::ids::{ActivityId, AgentId, WorkerId};
    use mini_adas::config::{
        activity_dependencies, agent_assignments, socket_paths, worker_agent_map,
    };
    use std::collections::{HashMap, HashSet};

    pub(super) use feo::agent::relayed::primary::{Primary, PrimaryConfig};

    pub(super) fn make_config(params: Params) -> PrimaryConfig {
        let activity_worker_map: HashMap<ActivityId, WorkerId> = agent_assignments()
            .values()
            .flat_map(|vec| {
                vec.iter()
                    .flat_map(move |(wid, aid_b)| aid_b.iter().map(|v| (v.0, *wid)))
            })
            .collect();

        let agent_ids: HashSet<AgentId> = agent_assignments().keys().copied().collect();
        check_ids(&params.recorder_ids, &agent_ids);

        PrimaryConfig {
            cycle_time: params.feo_cycle_time,
            activity_dependencies: activity_dependencies(),
            recorder_ids: params.recorder_ids,
            worker_assignments: agent_assignments().remove(&AGENT_ID).unwrap(),
            timeout: Duration::from_secs(10),
            bind_address_senders: NodeAddress::UnixSocket(socket_paths().0),
            bind_address_receivers: NodeAddress::UnixSocket(socket_paths().1),
            id: AGENT_ID,
            worker_agent_map: worker_agent_map(),
            activity_worker_map,
        }
    }
}

#[allow(dead_code)]
fn check_ids<'t, T>(recorder_ids: &'t T, agent_ids: &HashSet<AgentId>)
where
    &'t T: IntoIterator<Item = &'t AgentId>,
{
    let mut ids = agent_ids.clone();
    for recorder_id in recorder_ids {
        let is_new = ids.insert(*recorder_id);
        assert!(
            is_new,
            "Agent id {recorder_id} of recorder is not unique within all agents"
        );
    }
}
