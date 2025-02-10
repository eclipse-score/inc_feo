// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! Secondary agent builder

use crate::agent::secondary::SecondaryAgent;
use crate::signalling::{AgentId, IntraProcReceiver, Signal};
use crate::worker_pool::WorkerPool;
use std::net::SocketAddr;

/// Secondary agent builder
#[derive(Default)]
pub struct Builder {
    pub id: Option<AgentId>,
    pub primary: Option<SocketAddr>,
    pub worker_pool: Option<(WorkerPool, IntraProcReceiver<Signal>)>,
}

impl Builder {
    /// Set the id of the agent to build
    pub fn id(mut self, agent_id: AgentId) -> Self {
        self.id = Some(agent_id);
        self
    }

    /// Set the socket address of the primary agent
    pub fn primary(mut self, primary_addr: SocketAddr) -> Self {
        self.primary = Some(primary_addr);
        self
    }

    /// Set the worker pool and corresponding intra-process ready receiver
    pub fn worker_pool(
        mut self,
        worker_pool: WorkerPool,
        ready_receiver: IntraProcReceiver<Signal>,
    ) -> Self {
        self.worker_pool = Some((worker_pool, ready_receiver));
        self
    }

    /// Build the secondary agent
    pub fn build(self) -> SecondaryAgent {
        let id = self.id.expect("missing agent id");
        let primary_addr = self.primary.expect("missing remote socket address");
        let (worker_pool, ready_receiver) = self.worker_pool.expect("missing worker pool");

        SecondaryAgent::new(id, primary_addr, worker_pool, ready_receiver)
    }
}
