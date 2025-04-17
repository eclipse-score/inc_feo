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

use async_runtime::runtime::runtime::AsyncRuntime;
use feo::{configuration::primary_agent::ActivityDependencies, prelude::ActivityId};
use foundation::threading::thread_wait_barrier::{ThreadReadyNotifier, ThreadWaitBarrier};
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

    id: ActivityId,
}

pub struct ActivityDetailsBuilder {
    data: Vec<ActivityDetails>,
}

impl ActivityDetailsBuilder {
    pub fn new() -> Self {
        Self { data: vec![] }
    }

    pub fn add_activity<T: 'static + Send + ActivityAdapterTrait<T = T>, U: FnMut() -> T>(
        mut self,
        mut builder: U,
    ) -> Self {
        let wrapped = Arc::new(Mutex::new(builder()));
        self.data.push(activity_into_invokes(&wrapped));
        self
    }

    pub fn build(self) -> Vec<ActivityDetails> {
        self.data
    }
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
        id: obj.lock().unwrap().get_act_id(),
    }
}

///
/// Responsible to react on request coming from primary process
///
pub struct LocalFeoAgent {
    activities: Vec<ActivityDetails>,
    agent_name: &'static str,
    app_name: &'static str,
}

impl LocalFeoAgent {
    pub fn run_agent(
        app_name: &'static str,
        activities: Vec<ActivityDetails>,
        agent_name: &'static str,
        runtime: &mut AsyncRuntime,
    ) {
        // Since runtime `enter_engine` is now not blocking, we do it manually here.
        let waiter = Arc::new(ThreadWaitBarrier::new(1));
        let notifier = waiter.get_notifier().unwrap();

        runtime
            .enter_engine(
                //
                async move {
                    let mut agent = LocalFeoAgent::new(app_name, activities, agent_name);
                    let mut program = agent.create_program();

                    info!("{:?}", program);

                    program.run().await;
                    info!("Finished");
                    notifier.ready();
                },
            )
            .unwrap_or_default();

        waiter
            .wait_for_all(Duration::new(2000, 0))
            .unwrap_or_default();
    }

    pub fn new(
        app_name: &'static str,
        activities: Vec<ActivityDetails>,
        agent_name: &'static str,
    ) -> Self {
        Self {
            activities,
            agent_name,
            app_name,
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
            .with_step(Trigger::new(
                format!("{}/{}/alive", self.app_name, self.agent_name).as_str(),
            ))
            .with_step(Sync::new(format!("{}/startup", self.app_name).as_str()));

        let mut concurrent = Concurrency::new();

        // startups from al activities
        for e in &mut self.activities {
            concurrent = concurrent.with_branch(e.binded_hooks.0.take().unwrap());
        }

        seq = seq.with_step(concurrent);
        seq.with_step(Trigger::new(
            format!("{}/{}/startup_completed", self.app_name, self.agent_name).as_str(),
        ))
    }

    fn create_body(&mut self) -> Box<dyn ActionTrait> {
        let mut concurrent = Concurrency::new();

        for e in &mut self.activities {
            concurrent = concurrent.with_branch(
                Sequence::new()
                    .with_step(Sync::new(
                        format!("{}/{}/step", self.app_name, e.id).as_str(),
                    ))
                    .with_step(e.binded_hooks.1.take().unwrap())
                    .with_step(Trigger::new(
                        format!("{}/{}/step_completed", self.app_name, e.id).as_str(),
                    )),
            );
        }

        concurrent
    }

    fn create_shutdown_notification(&mut self) -> Box<dyn ActionTrait> {
        let seq =
            Sequence::new().with_step(Sync::new(format!("{}/shutdown", self.app_name).as_str()));

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
            format!("{}/{}/shutdown_completed", self.app_name, self.agent_name).as_str(),
        ))
    }
}

///
/// Responsible for controlling Task Chain execution across processes according to provided configuration
///
pub struct GlobalOrchestrator {
    agents: Vec<String>,
    cycle: Duration,
    app_name: &'static str,
}

impl GlobalOrchestrator {
    pub fn run_primary(
        app_name: &'static str,
        agents: Vec<String>,
        cycle: Duration,
        graph: ActivityDependencies,
        local_activities: Vec<ActivityDetails>,
        local_agent_name: &'static str,
        runtime: &mut AsyncRuntime,
    ) {
        // Since runtime `enter_engine` is now not blocking, we do it manually here.
        let waiter = Arc::new(ThreadWaitBarrier::new(1));
        let notifier = waiter.get_notifier().unwrap();

        runtime
            .enter_engine(
                //
                async move {
                    let local_agent_program = async_runtime::spawn(async move {
                        let mut agent =
                            LocalFeoAgent::new(app_name, local_activities, local_agent_name);
                        let mut program = agent.create_program();

                        program.run().await;
                        info!("Finished local program");
                    });

                    let global_orch = GlobalOrchestrator::new(app_name, agents, cycle);
                    global_orch.run(&graph).await;

                    local_agent_program.await;
                    notifier.ready();
                },
            )
            .unwrap_or_default();

        waiter
            .wait_for_all(Duration::new(2000, 0))
            .unwrap_or_default();
    }

    pub fn new(app_name: &'static str, agents: Vec<String>, cycle: Duration) -> Self {
        Self {
            agents,
            cycle,
            app_name,
        }
    }

    pub async fn run(&self, graph: &ActivityDependencies) {
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
            let sub_sequence = Sync::new(format!("{}/{}/alive", self.app_name, name).as_str());

            top = top.with_branch(sub_sequence);
        }

        top
    }

    fn release_agents(&self) -> Box<dyn ActionTrait> {
        Sequence::new_with_id(NamedId::new_static("release_agents"))
            .with_step(Trigger::new(format!("{}/startup", self.app_name).as_str()))
    }

    fn wait_startup_completed(&self) -> Box<dyn ActionTrait> {
        let mut top = Sequence::new_with_id(NamedId::new_static("wait_startup_completed"));

        for name in &self.agents {
            let sub_sequence =
                Sync::new(format!("{}/{}/startup_completed", self.app_name, name).as_str());

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
        Sequence::new_with_id(NamedId::new_static("shutdown_agents"))
            .with_step(Trigger::new(format!("{}/shutdown", self.app_name).as_str()))
    }

    fn wait_shutdown_completed(&self) -> Box<dyn ActionTrait> {
        let mut top = Sequence::new_with_id(NamedId::new_static("wait_shutdown_completed"));

        for name in &self.agents {
            let sub_sequence =
                Sync::new(format!("{}/{}/shutdown_completed", self.app_name, name).as_str());

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
    fn generate_body(&self, graph: &ActivityDependencies) -> Box<dyn ActionTrait> {
        let mut body = Concurrency::new();

        // For now simply mapping, without optimization
        for node in graph {
            if node.1.is_empty() {
                body = body.with_branch(self.generate_step(node.0));
                continue;
            }

            let mut s = Sequence::new();
            for dep in node.1 {
                s = s.with_step(Sync::new(
                    format!("{}/{}/step_completed", self.app_name, dep).as_str(),
                ));
            }

            s = s.with_step(self.generate_step(node.0));

            body = body.with_branch(s);
        }
        body
    }

    fn generate_step(&self, id: &ActivityId) -> Box<dyn ActionTrait> {
        Sequence::new()
            .with_step(Trigger::new(
                format!("{}/{}/step", self.app_name, id).as_str(),
            ))
            .with_step(Sync::new(
                format!("{}/{}/step_completed", self.app_name, id).as_str(),
            ))
    }
}
