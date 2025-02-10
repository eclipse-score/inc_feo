// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use crate::activity::{Activity, ActivityId, ActivityIdAndBuilder};
use crate::signalling::{Receiver, Sender, Signal};
use crate::timestamp::timestamp;
use feo_log::debug;
use feo_tracing::{span, Level};
use std::collections::HashMap;
use std::fmt::Display;
use std::thread;

/// Worker id type. This id is unique to each worker thread.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct WorkerId(usize);

impl From<usize> for WorkerId {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

impl From<&WorkerId> for usize {
    fn from(value: &WorkerId) -> Self {
        value.0
    }
}

impl From<WorkerId> for usize {
    fn from(value: WorkerId) -> Self {
        value.0
    }
}

impl Display for WorkerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "W{}", self.0)
    }
}

type ActivityBuilders = Vec<ActivityIdAndBuilder>;

/// A worker thread that steps activities.
#[allow(unused)]
pub struct Worker {
    id: WorkerId,
    thread: thread::JoinHandle<()>,
}

#[allow(unused)]
impl Worker {
    pub fn id(&self) -> WorkerId {
        self.id
    }

    /// Create a new worker thread that will build and execute activities.
    ///
    /// This function spawns a new thread.
    pub fn new<R, S>(
        id: WorkerId,
        stack_size: Option<usize>,
        builders: ActivityBuilders,
        mut trigger: R,
        mut ready: S,
    ) -> Worker
    where
        R: Receiver<Signal> + 'static,
        S: Sender<Signal> + 'static,
    {
        let thread_name = format!("feo-{id}").to_lowercase();
        let mut builder = thread::Builder::new().name(thread_name.clone());
        if let Some(stack_size) = stack_size {
            builder = builder.stack_size(stack_size);
        }
        let thread = builder
            .spawn(move || {
                run(id, thread_name, builders, trigger, ready);
            })
            .expect("could not spawn thread");

        Worker { thread, id }
    }
}

/// Worker thread main function
fn run<R, S>(
    wid: WorkerId,
    thread_name: String,
    builders: ActivityBuilders,
    mut trigger: R,
    mut ready: S,
) where
    R: Receiver<Signal> + 'static,
    S: Sender<Signal> + 'static,
{
    // instantiate all activities and keep them in a map
    let mut activities: HashMap<ActivityId, Box<dyn Activity>> = builders
        .into_iter()
        .map(|(id, builder)| (id, builder(id)))
        .collect();

    loop {
        // Receive next activity to step
        let signal = trigger.recv().expect("failed to receive trigger signal");
        let activity_id = signal.activity_id().expect("received unexpected signal");
        if let Some(activity) = activities.get_mut(&activity_id) {
            match signal {
                Signal::Startup(_) => {
                    debug!(
                        "Starting up activity {activity_id} in worker {wid} (thread {thread_name})"
                    );
                    let _span = span!(Level::INFO, "Startup", id = %activity_id, worker_id = %wid)
                        .entered();
                    activity.startup();
                }
                Signal::Step(_) => {
                    debug!(
                        "Stepping activity {activity_id} in worker {wid} (thread {thread_name})"
                    );
                    let _span =
                        span!(Level::INFO, "Step", id = %activity_id, worker_id = %wid).entered();
                    activity.step();
                }
                Signal::Shutdown(_) => {
                    debug!("Shutting down activity {activity_id} in worker {wid} (thread {thread_name})");
                    let _span = span!(Level::INFO, "Shutdown", id = %activity_id, worker_id = %wid)
                        .entered();
                    activity.shutdown();
                }
                _ => panic!("received unexpected trigger signal {signal:?}"),
            };
        } else {
            panic!("received trigger {signal} for unknown activity id {activity_id}");
        }

        // Operation finished => send ready signal with timestamp
        ready
            .send(Signal::Ready((activity_id, timestamp())))
            .unwrap();
    }
}
