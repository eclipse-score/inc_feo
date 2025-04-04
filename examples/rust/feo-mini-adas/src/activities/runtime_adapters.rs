// Copyright (c) 2025 Qorix GmbH
//
// This program and the accompanying materials are made available under the
// terms of the Apache License, Version 2.0 which is available at
// https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: Apache-2.0
//

//
// Well known issues:
// - currently activity must be hidden behind Mutex - subject to be lifted
// - !Send issues due to iceoryx
// - ...
//
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use orchestration::{
    prelude::*,
    program::{Program, ProgramBuilder},
};

use super::components::ActivityAdapterTrait;
use logging_tracing::prelude::*;

pub struct ActivityDetails {
    binded_hooks: (
        Option<Box<dyn ActionTrait>>,
        Option<Box<dyn ActionTrait>>,
        Option<Box<dyn ActionTrait>>,
    ),

    name: &'static str,
}

///
/// Returns startup, step, shutdown for activity as invoke actions
///
pub fn activity_into_invokes<T>(obj: &Arc<Mutex<T>>) -> ActivityDetails
where
    T: 'static + Send + ActivityAdapterTrait<T = T>,
{
    let start = Invoke::from_arc(obj.clone(), T::start);
    let step = Invoke::from_arc_mtx(obj.clone(), T::step_runtime);
    let stop = Invoke::from_arc(obj.clone(), T::stop);
    ActivityDetails {
        binded_hooks: (Some(start), Some(step), Some(stop)),
        name: obj.lock().unwrap().get_named_id(),
    }
}

///
/// Responsible to react on request coming from primary process
///
pub struct LocalFeoAgent {
    activities: Vec<ActivityDetails>,
    agent_name: &'static str,
}

impl LocalFeoAgent {
    pub fn new(activities: Vec<ActivityDetails>, agent_name: &'static str) -> Self {
        Self {
            activities,
            agent_name,
        }
    }

    pub fn create_program(&mut self) -> Program {
        let mut program = ProgramBuilder::new("local");

        program = program.with_startup_hook(self.create_startup());
        program = program.with_body(self.create_body());
        program = program.with_shutdown_notification(self.create_shutdown_notification());
        program = program.with_shutdown_hook(self.create_shutdown());

        program.build()
    }

    fn create_startup(&mut self) -> Box<dyn ActionTrait> {
        let mut seq = Sequence::new()
            .with_step(Trigger::new(format!("{}_alive", self.agent_name).as_str()))
            .with_step(Sync::new(
                format!("{}_waiting_startup", self.agent_name).as_str(),
            ));

        let mut concurrent = Concurrency::new();

        // startups from al activities
        for e in &mut self.activities {
            concurrent = concurrent.with_branch(e.binded_hooks.0.take().unwrap());
        }

        seq = seq.with_step(concurrent);
        seq.with_step(Trigger::new(
            format!("{}_startup_done", self.agent_name).as_str(),
        ))
    }

    fn create_body(&mut self) -> Box<dyn ActionTrait> {
        let mut concurrent = Concurrency::new();

        for e in &mut self.activities {
            concurrent = concurrent.with_branch(
                Sequence::new()
                    .with_step(Sync::new(format!("{}_start", e.name).as_str()))
                    .with_step(e.binded_hooks.1.take().unwrap())
                    .with_step(Trigger::new(format!("{}_done", e.name).as_str())),
            );
        }

        concurrent
    }

    fn create_shutdown_notification(&mut self) -> Box<dyn ActionTrait> {
        let seq = Sequence::new().with_step(Sync::new(
            format!("{}_waiting_shutdown", self.agent_name).as_str(),
        ));

        seq
    }

    fn create_shutdown(&mut self) -> Box<dyn ActionTrait> {
        let mut seq = Sequence::new();

        let mut concurrent = Concurrency::new();

        // shutdown from all activities
        for e in &mut self.activities {
            concurrent = concurrent.with_branch(e.binded_hooks.2.take().unwrap());
        }

        seq = seq.with_step(concurrent);
        seq.with_step(Trigger::new(
            format!("{}_shutdown_done", self.agent_name).as_str(),
        ))
    }
}

///
/// Responsible for controlling Task Chain execution across processes according to provided configuration
///
pub struct GlobalOrchestrator {
    agents: Vec<String>,
    cycle: Duration,
}

impl GlobalOrchestrator {
    pub fn new(agents: Vec<String>, cycle: Duration) -> Self {
        Self { agents, cycle }
    }

    pub async fn run(&self, graph: &Vec<(Vec<&str>, bool)>) {
        let mut program = ProgramBuilder::new("main")
            .with_startup_hook(self.startup())
            .with_body(self.generate_body(&graph))
            .with_shutdown_notification(self.orch_shutdown_notification())
            .with_shutdown_hook(self.shutdown())
            .with_cycle_time(self.cycle)
            .build();

        info!("Executor starts syncing with agents and execution of activity chain 20 times for demo...");
        info!("{:?}", program);

        program.run_n(20).await;

        info!("Done");
    }

    fn sync_to_agents(&self) -> Box<dyn ActionTrait> {
        let mut top = Concurrency::new_with_id(NamedId::new_static("sync_to_agents"));

        for name in &self.agents {
            let sub_sequence = Sync::new(format!("{}_alive", name).as_str());

            top = top.with_branch(sub_sequence);
        }

        top
    }

    fn release_agents(&self) -> Box<dyn ActionTrait> {
        let mut top = Sequence::new_with_id(NamedId::new_static("release_agents"));

        for name in &self.agents {
            let sub_sequence = Trigger::new(format!("{}_waiting_startup", name).as_str());

            top = top.with_step(sub_sequence);
        }

        top
    }

    fn wait_startup_completed(&self) -> Box<dyn ActionTrait> {
        let mut top = Sequence::new_with_id(NamedId::new_static("wait_startup_completed"));

        for name in &self.agents {
            let sub_sequence = Sync::new(format!("{}_startup_done", name).as_str());

            top = top.with_step(sub_sequence);
        }

        top
    }

    fn startup(&self) -> Box<dyn ActionTrait> {
        let seq = Sequence::new_with_id(NamedId::new_static("startup"))
            .with_step(self.sync_to_agents())
            .with_step(self.release_agents())
            .with_step(self.wait_startup_completed());

        seq
    }

    fn shutdown_agents(&self) -> Box<dyn ActionTrait> {
        let mut top = Sequence::new_with_id(NamedId::new_static("shutdown_agents"));

        for name in &self.agents {
            let sub_sequence = Trigger::new(format!("{}_waiting_shutdown", name).as_str());

            top = top.with_step(sub_sequence);
        }

        top
    }

    fn wait_shutdown_completed(&self) -> Box<dyn ActionTrait> {
        let mut top = Sequence::new_with_id(NamedId::new_static("wait_shutdown_completed"));

        for name in &self.agents {
            let sub_sequence = Sync::new(format!("{}_shutdown_done", name).as_str());

            top = top.with_step(sub_sequence);
        }

        top
    }

    fn shutdown(&self) -> Box<dyn ActionTrait> {
        let seq = Sequence::new_with_id(NamedId::new_static("shutdown"))
            .with_step(self.shutdown_agents())
            .with_step(self.wait_shutdown_completed());

        seq
    }

    // This can be used to stop orchestration from another application for demo.
    fn orch_shutdown_notification(&self) -> Box<dyn ActionTrait> {
        let seq = Sequence::new_with_id(NamedId::new_static("shutdown"))
            .with_step(Sync::new("qorix_orch_shutdown_event"));

        seq
    }

    // Converts a dependency graph into an execution sequence.
    fn generate_body(&self, execution_structure: &Vec<(Vec<&str>, bool)>) -> Box<dyn ActionTrait> {
        let mut sequence = Sequence::new(); // The overall execution sequence
        let mut concurrency_action = Concurrency::new();

        let mut concurrent_block_added = false;

        for task_group in execution_structure {
            if task_group.1 == false {
                // Add the concurrency block into sequence
                if concurrent_block_added {
                    sequence = sequence.with_step(concurrency_action);
                    concurrency_action = Concurrency::new();
                    concurrent_block_added = false;
                }
                // sequence
                let action = self.generate_step(task_group.0.clone());
                sequence = sequence.with_step(action);
            } else {
                // concurrency block
                let action = self.generate_step(task_group.0.clone());
                concurrency_action = concurrency_action.with_branch(action);
                concurrent_block_added = true;
            }
        }
        if concurrent_block_added {
            sequence = sequence.with_step(concurrency_action);
        }
        sequence
    }

    fn generate_step(&self, names: Vec<&str>) -> Box<dyn ActionTrait> {
        let mut sequence = Sequence::new();
        for name in names {
            sequence = sequence
                .with_step(Trigger::new(format!("{}_start", name).as_str()))
                .with_step(Sync::new(format!("{}_done", name).as_str()));
        }
        return sequence;
    }
}
