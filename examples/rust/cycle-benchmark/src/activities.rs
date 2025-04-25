// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use feo::activity::Activity;
use feo::ids::ActivityId;
use feo_tracing::{instrument, tracing};

/// This is a dummy activity that does nothing.
#[derive(Debug)]
pub struct DummyActivity {
    /// ID of the activity
    activity_id: ActivityId,
}

impl DummyActivity {
    pub fn build(activity_id: ActivityId) -> Box<dyn Activity> {
        Box::new(Self { activity_id })
    }
}

impl Activity for DummyActivity {
    fn id(&self) -> ActivityId {
        self.activity_id
    }

    #[instrument(name = "Activity startup")]
    fn startup(&mut self) {}

    #[instrument(name = "Activity step")]
    fn step(&mut self) {}

    #[instrument(name = "Activity shutdown")]
    fn shutdown(&mut self) {}
}
