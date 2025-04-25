// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! Signals

use crate::ids::{ActivityId, AgentId};
use crate::timestamp::{SyncInfo, Timestamp};
use core::fmt::Display;
#[cfg(feature = "recording")]
use postcard::experimental::max_size::MaxSize;
#[cfg(feature = "recording")]
use serde::{Deserialize, Serialize};

/// Signal types sent between threads or processes
#[cfg_attr(feature = "recording", derive(Serialize, Deserialize, MaxSize))]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Signal {
    // Signal sent from the primary agent to each secondary agent containing synchronization info
    StartupSync(SyncInfo),

    // Signal sent by the scheduler on the primary agent to trigger an activity's startup method
    Startup((ActivityId, Timestamp)),

    // Signal sent by the scheduler on the primary agent to trigger an activity's shutdown method
    Shutdown((ActivityId, Timestamp)),

    // Signal sent by the scheduler on the primary agent to trigger an activity's step method
    Step((ActivityId, Timestamp)),

    // Signal sent to indicate that a previously triggered activity method has finished
    Ready((ActivityId, Timestamp)),

    // Signal sent by the scheduler to the recorders whenever the taskchain starts
    TaskChainStart(Timestamp),

    // Signal sent by the scheduler to the recorders whenever the taskchain ends
    TaskChainEnd(Timestamp),

    // Signal sent to indicate that a recorder operation has finished
    RecorderReady((AgentId, Timestamp)),
}

impl Display for Signal {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Signal::StartupSync(t) => write!(f, "StartupSync({t:?})"),
            Signal::Startup((id, t)) => write!(f, "Startup({id}, {t:?})"),
            Signal::Shutdown((id, t)) => write!(f, "Shutdown({id}, {t:?})"),
            Signal::Step((id, t)) => write!(f, "Step({id}, {t:?})"),
            Signal::Ready((id, t)) => write!(f, "Ready({id}, {t:?})"),
            Signal::TaskChainStart(t) => write!(f, "TaskChainStart({t:?})"),
            Signal::TaskChainEnd(t) => write!(f, "TaskChainEnd({t:?})"),
            Signal::RecorderReady((id, t)) => write!(f, "RecorderReady({id}, {t:?})"),
        }
    }
}
