// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! Activity and related structs and traits

#[cfg(feature = "recording")]
use postcard::experimental::max_size::MaxSize;
#[cfg(feature = "recording")]
use serde::{Deserialize, Serialize};
use std::fmt::Display;

/// Identifies an Activity / Task
#[cfg_attr(feature = "recording", derive(Serialize, Deserialize, MaxSize))]
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct ActivityId(usize);

impl From<usize> for ActivityId {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

impl From<&ActivityId> for usize {
    fn from(value: &ActivityId) -> Self {
        value.0
    }
}

impl From<ActivityId> for usize {
    fn from(value: ActivityId) -> Self {
        value.0
    }
}

impl Display for ActivityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "A{}", self.0)
    }
}

/// Activity trait, to be implemented by any activity intended to run in a WorkerPool
pub trait Activity {
    /// Get the ID of the activity
    fn id(&self) -> ActivityId;

    /// Called upon startup
    fn startup(&mut self);

    /// Called upon each step
    fn step(&mut self);

    /// Called upon shutdown
    fn shutdown(&mut self);
}

/// Activity Builder trait.
///
/// To instantiate a worker pool with activities, an ActivityBuilder
/// shall be passed for each activity. At startup of the worker threads are started the
/// activities will be built within their respective thread.
/// In this way, activities can avoid implementing the Send trait, which may not
/// always be possible.
pub trait ActivityBuilder: FnOnce(ActivityId) -> Box<dyn Activity> + Send {}

impl<T: FnOnce(ActivityId) -> Box<dyn Activity> + Send> ActivityBuilder for T {}

/// [ActivityId] coupled with an [ActivityBuilder].
pub type ActivityIdAndBuilder = (ActivityId, Box<dyn ActivityBuilder>);
