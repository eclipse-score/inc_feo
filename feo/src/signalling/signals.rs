// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use crate::prelude::ActivityId;
use crate::timestamp::{SyncInfo, Timestamp};
#[cfg(feature = "recording")]
use postcard::experimental::max_size::MaxSize;
#[cfg(feature = "recording")]
use serde::{Deserialize, Serialize};
use std::fmt::Display;

/// Identifies an Agent / Process
#[cfg_attr(feature = "recording", derive(Serialize, Deserialize, MaxSize))]
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct AgentId(pub usize);

impl AgentId {
    pub const fn new(i: usize) -> Self {
        Self(i)
    }
}

impl Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "A{}", self.0)
    }
}

impl From<usize> for AgentId {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

impl From<&AgentId> for usize {
    fn from(value: &AgentId) -> Self {
        value.0
    }
}

impl From<AgentId> for usize {
    fn from(value: AgentId) -> Self {
        value.0
    }
}

/// Signal types sent between threads or processes
#[cfg_attr(feature = "recording", derive(Serialize, Deserialize, MaxSize))]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Signal {
    // Signal sent from a secondary agent to the primary agent during initialization phase
    // to open the channel on which it will send its ready signals later on.
    HelloReady(AgentId),

    // Signal sent from a secondary agent to the primary agent during initialization phase
    // to open the channel on which it will receive trigger signals later on.
    HelloTrigger(AgentId),

    // Signal sent from the primary agent to each secondary agent containing synchronization info
    StartupSync(SyncInfo),

    // Signal sent by the scheduler to the recorders whenever the taskchain starts
    TaskChainStart(Timestamp),

    // Signal sent by the scheduler to the recorders whenever the taskchain ends
    TaskChainEnd(Timestamp),

    // Signal sent by the scheduler on the primary agent to trigger an activity's startup method
    Startup((ActivityId, Timestamp)),

    // Signal sent by the scheduler on the primary agent to trigger an activity's shutdown method
    Shutdown((ActivityId, Timestamp)),

    // Signal sent by the scheduler on the primary agent to trigger an activity's step method
    Step((ActivityId, Timestamp)),

    // Signal sent to indicate that a previously triggered activity method has finished
    Ready((ActivityId, Timestamp)),

    // Signal sent to indicate that a recorder operation has finished
    RecorderReady((AgentId, Timestamp)),
}

/// The id type wrapped in a Signal
enum SignalWrappedId {
    AgentId(AgentId),
    ActivityId(ActivityId),
}

impl Signal {
    /// Return the wrapped agent id or None
    pub fn agent_id(&self) -> Option<AgentId> {
        let wrapped_id = self.wrapped_id();
        wrapped_id.as_ref()?;
        match wrapped_id.unwrap() {
            SignalWrappedId::AgentId(id) => Some(id),
            _ => None,
        }
    }

    /// Return the wrapped activity id or None
    pub fn activity_id(&self) -> Option<ActivityId> {
        let wrapped_id = self.wrapped_id();
        let wrapped_id = wrapped_id.as_ref()?;
        match wrapped_id {
            SignalWrappedId::ActivityId(id) => Some(*id),
            _ => None,
        }
    }

    /// Return the wrapped timestamp
    pub fn timestamp(&self) -> Option<Timestamp> {
        match self {
            Signal::TaskChainStart(tstamp) => Some(*tstamp),
            Signal::TaskChainEnd(tstamp) => Some(*tstamp),
            Signal::Shutdown((_, tstamp)) => Some(*tstamp),
            Signal::Startup((_, tstamp)) => Some(*tstamp),
            #[allow(unreachable_patterns)]
            Signal::Shutdown((_, tstamp)) => Some(*tstamp),
            Signal::Step((_, tstamp)) => Some(*tstamp),
            Signal::Ready((_, tstamp)) => Some(*tstamp),
            Signal::RecorderReady((_, tstamp)) => Some(*tstamp),
            _ => None,
        }
    }

    /// Return the synchronization info
    pub fn sync_info(&self) -> Option<SyncInfo> {
        match self {
            Signal::StartupSync(info) => Some(*info),
            _ => None,
        }
    }

    /// Determine the id type wrapped in the signal
    fn wrapped_id(&self) -> Option<SignalWrappedId> {
        match self {
            Signal::HelloReady(id) => Some(SignalWrappedId::AgentId(*id)),
            Signal::HelloTrigger(id) => Some(SignalWrappedId::AgentId(*id)),
            Signal::StartupSync(_) => None,
            Signal::TaskChainStart(_) => None,
            Signal::TaskChainEnd(_) => None,
            Signal::Startup((id, _)) => Some(SignalWrappedId::ActivityId(*id)),
            Signal::Shutdown((id, _)) => Some(SignalWrappedId::ActivityId(*id)),
            Signal::Step((id, _)) => Some(SignalWrappedId::ActivityId(*id)),
            Signal::Ready((id, _)) => Some(SignalWrappedId::ActivityId(*id)),
            Signal::RecorderReady((id, _)) => Some(SignalWrappedId::AgentId(*id)),
        }
    }
}

impl Display for Signal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Signal::HelloReady(id) => write!(f, "HelloReady({id})"),
            Signal::HelloTrigger(id) => write!(f, "HelloTrigger({id})"),
            Signal::StartupSync(t) => write!(f, "StartupSync({t:?})"),
            Signal::TaskChainStart(t) => write!(f, "TaskChainStart({t:?})"),
            Signal::TaskChainEnd(t) => write!(f, "TaskChainEnd({t:?})"),
            Signal::Startup((id, t)) => write!(f, "Startup({id}, {t:?})"),
            Signal::Shutdown((id, t)) => write!(f, "Shutdown({id}, {t:?})"),
            Signal::Step((id, t)) => write!(f, "Step({id}, {t:?})"),
            Signal::Ready((id, t)) => write!(f, "Ready({id}, {t:?})"),
            Signal::RecorderReady((id, t)) => write!(f, "RecorderReady({id}, {t:?})"),
        }
    }
}
