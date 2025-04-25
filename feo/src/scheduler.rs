// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! Global activity scheduler

use crate::error::Error;
use crate::ids::{ActivityId, AgentId};
use crate::signalling::common::interface::ConnectScheduler;
use crate::signalling::common::signals::Signal;
use crate::timestamp::timestamp;
use alloc::boxed::Box;
use alloc::vec::Vec;
use feo_log::{debug, error, info, trace};
use feo_time::Instant;
use std::collections::HashMap;
use std::thread;

/// Global activity scheduler
///
/// The scheduler (aka 'FEO Executor') executes the FEO activities according to the defined order
pub(crate) struct Scheduler {
    /// Target duration of a task chain cycle
    cycle_time: feo_time::Duration,
    /// Timeout of receive function
    receive_timeout: core::time::Duration,

    /// For each activity: list of activities it depends on
    activity_depends: HashMap<ActivityId, Vec<ActivityId>>,
    /// Map keeping track of activity states
    activity_states: HashMap<ActivityId, ActivityState>,

    /// Helper object connecting to activities in all connected agents
    connector: Box<dyn ConnectScheduler>,

    /// Agent IDs of expected recorders
    recorder_ids: Vec<AgentId>,
    /// Map from recorder agent ID to ready state
    recorders_ready: HashMap<AgentId, bool>,
}

impl Scheduler {
    pub(crate) fn new(
        feo_cycle_time: feo_time::Duration,
        receive_timeout: core::time::Duration,
        activity_depends: HashMap<ActivityId, Vec<ActivityId>>,
        connector: Box<dyn ConnectScheduler>,
        recorder_ids: Vec<AgentId>,
    ) -> Self {
        // Pre-allocate state map
        let activity_states: HashMap<ActivityId, ActivityState> = activity_depends
            .keys()
            .map(|k| {
                (
                    *k,
                    ActivityState {
                        triggered: false,
                        ready: false,
                    },
                )
            })
            .collect();
        let recorders_ready = recorder_ids.iter().map(|id| (*id, false)).collect();

        Self {
            cycle_time: feo_cycle_time,
            receive_timeout,
            activity_depends,
            connector,
            activity_states,
            recorder_ids,
            recorders_ready,
        }
    }

    /// Synchronize all remote agents and recorders
    pub(crate) fn sync_remotes(&mut self) -> Result<(), Error> {
        self.connector.sync_time()?;
        info!("Time synchronization of remote agents done");
        Ok(())
    }

    /// Run the task lifecycle, i.e. startup, stepping, shutdown
    ///
    /// Shutdown is not implemented, as it is not yet defined in the architecture
    pub(crate) fn run(&mut self) {
        #[cfg(feature = "loop_duration_meter")]
        let mut meter = loop_duration_meter::LoopDurationMeter::<1000>::default();

        // Sort activity ids
        let mut activity_ids: Vec<_> = self.activity_states.keys().collect();
        activity_ids.sort();

        // Call startup on all activities sorted according to their ids
        // Note: Actual startup may occur in different order, depending on the assignment
        // of activities to worker threads. (A worker with greater id value may start up in
        // one thread before an activity with smaller id value in another thread.)
        for activity_id in activity_ids {
            Self::startup_activity(activity_id, &self.recorder_ids, &mut self.connector).unwrap();
        }

        // Wait until all activities have returned their ready signal
        while !self.all_ready() {
            self.wait_next_ready()
                .expect("failed while waiting for ready signal");
        }

        // Loop the FEO task chain
        loop {
            let task_chain_start = Instant::now();

            // Record start of task chain on registered recorders
            self.record_task_chain_start().unwrap();

            // Clear ready and triggered signals
            self.activity_states.values_mut().for_each(|v| {
                v.ready = false;
                v.triggered = false;
            });

            debug!("Starting task chain");

            while !self.all_ready() {
                // Step all activities that have their dependencies met
                self.step_ready_activities();
                // Wait until a new ready signal has been received
                self.wait_next_ready()
                    .expect("failed while waiting for ready signal");
            }

            // Record end of task chain on registered recorders => recorders will flush
            // => wait until all recorders have signalled to be ready
            trace!("Flushing recorders");
            let start_flush = Instant::now();
            self.record_task_chain_end().unwrap();
            self.wait_recorders_ready().unwrap();
            let flush_duration = start_flush.elapsed();
            trace!("Flushing recorders took {flush_duration:?}");

            let task_chain_duration = task_chain_start.elapsed();

            #[cfg(feature = "loop_duration_meter")]
            meter.track(&task_chain_duration);

            let time_left = self.cycle_time.saturating_sub(task_chain_duration);
            if time_left.is_zero() {
                error!(
                    "Finished task chain after {task_chain_duration:?}. Expected to be less than {:?}",
                    self.cycle_time
                );
            } else {
                debug!(
                    "Finished task chain after {task_chain_duration:?}. Sleeping for {time_left:?}"
                );
                thread::sleep(time_left);
            }
        }
    }

    /// Step all activities whose dependencies have signalled ready
    fn step_ready_activities(&mut self) {
        // Get data from activity_depends in self so that we can iterate over it
        // and at the same time modify another member of self
        for (act_id, dependencies) in self.activity_depends.iter() {
            // skip activity if already triggered
            if self.activity_states[act_id].triggered {
                continue;
            }

            // If dependencies are fulfilled
            let is_ready = self
                .activity_states
                .iter()
                .filter(|(id, _)| dependencies.contains(id))
                .all(|(_, state)| state.ready);
            if is_ready {
                Self::step_activity(act_id, &self.recorder_ids, &mut self.connector)
                    .expect("failed to step activity");
                self.activity_states.get_mut(act_id).unwrap().triggered = true;
            }
        }
    }

    /// Send startup signal to the given activity
    fn startup_activity(
        id: &ActivityId,
        recorder_ids: &[AgentId],
        connector: &mut Box<dyn ConnectScheduler>,
    ) -> Result<(), Error> {
        debug!("Triggering startup for activity {}", id);
        let signal = Signal::Startup((*id, timestamp()));
        Self::trigger_activity(id, &signal, recorder_ids, connector)
    }

    /// Send step signal to the given activity
    fn step_activity(
        id: &ActivityId,
        recorder_ids: &[AgentId],
        connector: &mut Box<dyn ConnectScheduler>,
    ) -> Result<(), Error> {
        debug!("Triggering step for activity {}", id);
        let signal = Signal::Step((*id, timestamp()));
        Self::trigger_activity(id, &signal, recorder_ids, connector)
    }

    /// Send shutdown signal to the given activity
    #[allow(dead_code)]
    fn shutdown_activity(
        id: &ActivityId,
        recorder_ids: &[AgentId],
        connector: &mut Box<dyn ConnectScheduler>,
    ) -> Result<(), Error> {
        debug!("Triggering shutdown for activity {}", id);
        let signal = Signal::Shutdown((*id, timestamp()));
        Self::trigger_activity(id, &signal, recorder_ids, connector)
    }

    /// Trigger activity by forwarding the signal to the activity and all recorders
    fn trigger_activity(
        id: &ActivityId,
        signal: &Signal,
        recorder_ids: &[AgentId],
        connector: &mut Box<dyn ConnectScheduler>,
    ) -> Result<(), Error> {
        connector.send_to_activity(*id, signal)?;
        for id in recorder_ids {
            connector.send_to_recorder(*id, signal)?;
        }
        Ok(())
    }

    /// Wait for the next incoming ready signal
    fn wait_next_ready(&mut self) -> Result<(), Error> {
        // Wait for next intra-process ready signal from one of the workers
        let activity_id = loop {
            let received = self.connector.receive(self.receive_timeout)?;
            match received {
                None => continue,
                Some(signal @ Signal::Ready((id, _))) => {
                    for recorder_id in self.recorder_ids.iter() {
                        self.connector.send_to_recorder(*recorder_id, &signal)?;
                    }
                    break id;
                }
                Some(other) => {
                    error!("Received unexpected signal {other:?} while waiting for ready signal");
                }
            }
        };

        // Set corresponding ready flag
        self.activity_states.get_mut(&activity_id).unwrap().ready = true;
        Ok(())
    }

    /// Check if all activities have signalled 'ready'
    fn all_ready(&self) -> bool {
        self.activity_states.values().all(|v| v.ready)
    }

    fn record_task_chain_start(&mut self) -> Result<(), Error> {
        trace!("Recording task chain start");
        let signal = &Signal::TaskChainStart(timestamp());
        for id in self.recorder_ids.iter() {
            self.connector.send_to_recorder(*id, signal)?;
        }
        Ok(())
    }

    fn record_task_chain_end(&mut self) -> Result<(), Error> {
        trace!("Recording task chain end");
        let signal = &Signal::TaskChainEnd(timestamp());
        for id in self.recorder_ids.iter() {
            self.connector.send_to_recorder(*id, signal)?;
        }
        Ok(())
    }

    fn wait_recorders_ready(&mut self) -> Result<(), Error> {
        // If there are no recorders registered, return immediately
        if self.recorder_ids.is_empty() {
            return Ok(());
        }

        // Clear all ready flags
        for value in self.recorders_ready.values_mut() {
            *value = false;
        }

        // Loop until all recorders have signalled ready
        while !self.recorders_ready.values().all(|v| *v) {
            let received = self.connector.receive(self.receive_timeout)?;
            match received {
                None => continue,
                Some(Signal::RecorderReady((id, _))) => {
                    if let Some(value) = self.recorders_ready.get_mut(&id) {
                        *value = true;
                    } else {
                        error!("Received unexpected id {id} in recorder ready signal");
                    }
                }
                Some(other) => {
                    error!("Received unexpected signal {other} while waiting for recorder ready signal");
                }
            }
        }

        Ok(())
    }
}

/// Current state of an activity
struct ActivityState {
    /// Whether the activity has been triggered for an action
    triggered: bool,

    /// Whether the activity has finished its previously triggered operation
    ready: bool,
}

#[cfg(feature = "loop_duration_meter")]
mod loop_duration_meter {
    use std::println;

    /// Meter to track and periodically print average durations
    #[derive(Default)]
    pub(super) struct LoopDurationMeter<const NUM_STEPS: usize> {
        duration_micros: usize,
        num_steps: usize,
    }

    impl<const NUM_STEPS: usize> LoopDurationMeter<NUM_STEPS> {
        pub fn track(&mut self, duration: &feo_time::Duration) {
            self.duration_micros +=
                duration.subsec_micros() as usize + 1000000 * duration.as_secs() as usize;
            self.num_steps += 1;

            if self.num_steps == NUM_STEPS {
                let avg_micros = self.duration_micros / NUM_STEPS;
                println!("Loop duration {avg_micros}µs (average of {NUM_STEPS} steps)");
                self.duration_micros = 0;
                self.num_steps = 0;
            }
        }
    }
}
