// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use crate::runtime_adapters::{get_activity_name, ActivityAdapterTrait};
use feo::activity::{Activity, ActivityId};
use feo_tracing::{instrument, tracing, Level};
use orchestration::prelude::ActionResult;
use std::future::Future;
use std::sync::{Arc, Mutex};
use tracing::event;

/// This is a dummy activity that does nothing.
#[derive(Debug)]
pub struct DummyActivity {
    /// ID of the activity
    activity_id: ActivityId,
    activity_name: &'static str,
}

impl DummyActivity {
    pub fn build(activity_id: &ActivityId) -> Self {
        Self {
            activity_id: *activity_id,
            activity_name: get_activity_name(activity_id.into()),
        }
    }
}

impl Activity for DummyActivity {
    fn id(&self) -> ActivityId {
        self.activity_id
    }

    #[instrument(name = "Activity startup")]
    fn startup(&mut self) {}

    #[instrument(name = "Activity step")]
    fn step(&mut self) {
        event!(Level::INFO, id = self.activity_name);
    }

    #[instrument(name = "Activity shutdown")]
    fn shutdown(&mut self) {}
}

impl ActivityAdapterTrait for DummyActivity {
    type T = DummyActivity;

    fn step_runtime(instance: Arc<Mutex<Self::T>>) -> impl Future<Output = ActionResult> + Send {
        async move {
            instance.lock().unwrap().step();
            Ok(())
        }
    }

    fn start(&mut self) -> ActionResult {
        self.startup();
        Ok(())
    }

    fn stop(&mut self) -> ActionResult {
        self.shutdown();
        Ok(())
    }

    fn get_named_id(&self) -> &'static str {
        self.activity_name
    }
}
