// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use crate::error::Error;
use crate::ids::{ActivityId, AgentId};
use crate::signalling::common::interface::ConnectScheduler;
use crate::signalling::common::signals::Signal;
use crate::signalling::common::socket::server::{Listen, SocketServer, TcpServer, UnixServer};
use crate::signalling::common::socket::ProtocolSignal;
use crate::timestamp::sync_info;
use alloc::vec::Vec;
use core::net::SocketAddr;
use core::time::Duration;
use feo_log::warn;
use mio::net::{TcpListener, UnixListener};
use mio::{Events, Token};
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// TCP based connector for the scheduler
pub(crate) type TcpSchedulerConnector = SchedulerConnector<TcpListener>;

/// Unix socket based connector for the scheduler
pub(crate) type UnixSchedulerConnector = SchedulerConnector<UnixListener>;

/// Connector for the scheduler
pub(crate) struct SchedulerConnector<L>
where
    L: Listen<ProtocolSignal>,
{
    events: Events,
    server: SocketServer<L>,

    activity_id_token_map: HashMap<ActivityId, Token>,
    recorder_id_token_map: HashMap<AgentId, Token>,

    all_activities: Vec<ActivityId>,
    all_recorders: Vec<AgentId>,
}

impl<L> SchedulerConnector<L>
where
    L: Listen<ProtocolSignal>,
{
    /// Create a new instance
    fn new_with_server(
        server: SocketServer<L>,
        activity_ids: impl IntoIterator<Item = ActivityId>,
        recorder_ids: impl IntoIterator<Item = AgentId>,
    ) -> Self {
        let events = Events::with_capacity(32);

        let activity_id_token_map = HashMap::new();
        let recorder_id_token_map = HashMap::new();

        let all_activities = activity_ids.into_iter().collect();
        let all_recorders = recorder_ids.into_iter().collect();

        Self {
            events,
            server,
            activity_id_token_map,
            recorder_id_token_map,
            all_activities,
            all_recorders,
        }
    }
}

impl TcpSchedulerConnector {
    /// Create a new instance
    pub(crate) fn new(
        bind_address: SocketAddr,
        activity_ids: impl IntoIterator<Item = ActivityId>,
        recorder_ids: impl IntoIterator<Item = AgentId>,
    ) -> Self {
        let tcp_server = TcpServer::new(bind_address);
        Self::new_with_server(tcp_server, activity_ids, recorder_ids)
    }
}

impl UnixSchedulerConnector {
    /// Create a new instance
    pub(crate) fn new(
        path: &Path,
        activity_ids: impl IntoIterator<Item = ActivityId>,
        recorder_ids: impl IntoIterator<Item = AgentId>,
    ) -> Self {
        let unix_server = UnixServer::new(path);
        Self::new_with_server(unix_server, activity_ids, recorder_ids)
    }
}

impl<L> ConnectScheduler for SchedulerConnector<L>
where
    L: Listen<ProtocolSignal>,
{
    fn connect_remotes(&mut self) -> Result<(), Error> {
        let mut missing_activities: HashSet<ActivityId> =
            self.all_activities.iter().cloned().collect();
        let mut missing_recorders: HashSet<AgentId> = self.all_recorders.iter().cloned().collect();

        while !missing_activities.is_empty() || !missing_recorders.is_empty() {
            if let Some((token, signal)) = self
                .server
                .receive(&mut self.events, Duration::from_secs(1))
            {
                match signal {
                    ProtocolSignal::ActivityHello(activity_id) => {
                        self.activity_id_token_map.insert(activity_id, token);
                        missing_activities.remove(&activity_id);
                    }
                    ProtocolSignal::RecorderHello(agent_id) => {
                        self.recorder_id_token_map.insert(agent_id, token);
                        missing_recorders.remove(&agent_id);
                    }
                    other => {
                        warn!("received unexpected signal {other:?} from connection with token {token:?}");
                    }
                }
            }
        }

        Ok(())
    }

    fn sync_time(&mut self) -> Result<(), Error> {
        let signal = Signal::StartupSync(sync_info());

        // Send startup time to all workers and recorders
        for token in self
            .activity_id_token_map
            .values()
            .chain(self.recorder_id_token_map.values())
        {
            self.server
                .send(token, &ProtocolSignal::Core(signal))
                .map_err(|e| Error::Io((e, "failed to send")))?;
        }

        Ok(())
    }

    fn receive(&mut self, timeout: Duration) -> Result<Option<Signal>, Error> {
        if let Some((_, signal)) = self.server.receive(&mut self.events, timeout) {
            match signal {
                ProtocolSignal::Core(signal) => return Ok(Some(signal)),
                other => panic!("received unexpected signal {other:?}"),
            }
        }

        Ok(None)
    }

    fn send_to_activity(&mut self, activity_id: ActivityId, signal: &Signal) -> Result<(), Error> {
        let token = self
            .activity_id_token_map
            .get(&activity_id)
            .unwrap_or_else(|| panic!("failed to find token for activity ID {activity_id}"));
        self.server
            .send(token, &ProtocolSignal::Core(*signal))
            .map_err(|e| Error::Io((e, "failed to send")))
    }

    fn send_to_recorder(&mut self, recorder_id: AgentId, signal: &Signal) -> Result<(), Error> {
        let token = self
            .recorder_id_token_map
            .get(&recorder_id)
            .unwrap_or_else(|| panic!("failed to find token for recorder ID {recorder_id}"));
        self.server
            .send(token, &ProtocolSignal::Core(*signal))
            .map_err(|e| Error::Io((e, "failed to send")))
    }
}
