// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! Worker thread running FEO activities

use crate::activity::{Activity, ActivityBuilder};
use crate::error::Error;
use crate::ids::{ActivityId, WorkerId};
use crate::signalling::common::interface::ConnectWorker;
use crate::signalling::common::signals::Signal;
use crate::timestamp;
use alloc::boxed::Box;
use core::time::Duration;
use feo_log::debug;
use feo_time::Instant;
use std::collections::HashMap;

/// Worker
///
/// Activities are built in the worker thread with the passed builders
/// and never move to another thread after being built.
/// The connector passed to the worker is already connected to the scheduler.
pub(crate) struct Worker<T: ConnectWorker> {
    /// ID of this worker
    id: WorkerId,
    /// Map from [ActivityId] to the activity
    activities: HashMap<ActivityId, Box<dyn Activity>>,
    /// Connector to the scheduler
    connector: T,
    /// Timeout on `receive` calls
    timeout: Duration,
}

impl<T: ConnectWorker> Worker<T> {
    /// Create a new instance
    pub(crate) fn new(
        id: WorkerId,
        activity_builders: impl IntoIterator<Item = (ActivityId, Box<dyn ActivityBuilder>)>,
        connector: T,
        timeout: Duration,
    ) -> Self {
        // Build activities
        let activities: HashMap<ActivityId, _> = activity_builders
            .into_iter()
            .map(|(id, b)| (id, b(id)))
            .collect();

        Self {
            id,
            activities,
            connector,
            timeout,
        }
    }

    /// Run the worker
    pub(crate) fn run(mut self) -> Result<(), Error> {
        debug!("Running worker {}", self.id);

        loop {
            // Receive from connector
            let Some(signal) = self.connector.receive(self.timeout)? else {
                // TODO: Manage timeout
                continue;
            };

            match signal {
                Signal::Startup((id, _)) | Signal::Step((id, _)) | Signal::Shutdown((id, _)) => {
                    self.handle_activity_signal(&id, &signal)?;
                }
                Signal::StartupSync(sync_info) => {
                    timestamp::initialize_from(sync_info);
                }
                other => return Err(Error::UnexpectedSignal(other)),
            }
        }
    }

    fn handle_activity_signal(&mut self, id: &ActivityId, signal: &Signal) -> Result<(), Error> {
        let activity = self
            .activities
            .get_mut(id)
            .ok_or(Error::ActivityNotFound(*id))?;
        let start = Instant::now();

        match signal {
            Signal::Startup((activity_id, _)) => {
                activity.startup();
                let elapsed = start.elapsed();
                debug!("Ran startup of activity {id:?} in {elapsed:?}");
                self.connector
                    .send_to_scheduler(&Signal::Ready((*activity_id, timestamp::timestamp())))
            }
            Signal::Step((activity_id, _)) => {
                activity.step();
                let elapsed = start.elapsed();
                debug!("Stepped activity {id:?} in {elapsed:?}");
                self.connector
                    .send_to_scheduler(&Signal::Ready((*activity_id, timestamp::timestamp())))
            }
            Signal::Shutdown((activity_id, _)) => {
                activity.startup();
                let elapsed = start.elapsed();
                debug!("Ran shutdown of activity {id:?} in {elapsed:?}");
                self.connector
                    .send_to_scheduler(&Signal::Ready((*activity_id, timestamp::timestamp())))
            }
            other => Err(Error::UnexpectedSignal(*other)),
        }
    }
}
