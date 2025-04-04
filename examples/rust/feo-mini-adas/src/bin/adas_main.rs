// Copyright 2025 Qorix
//
// SPDX-License-Identifier: Apache-2.0
mod program;

use async_runtime::runtime::runtime::AsyncRuntimeBuilder;
use async_runtime::scheduler::execution_engine::ExecutionEngineBuilder;
use feo::com::TopicHandle;
use feo_mini_adas::config::*;
use feo_time::Duration;
use logging_tracing::prelude::*;
use logging_tracing::{TraceScope, TracingLibrary, TracingLibraryBuilder};
use orchestration::prelude::Event;
use program::main_program;
use std::future::Future;

// ****************************************************************** //
// **                  RUNTIME SETTINGS                            ** //
// ****************************************************************** //

const ENGINE_TASK_QUEUE_SIZE: usize = 256;
const ENGINE_NUM_OF_WORKERS: usize = 3;

const TRACE_SCOPE: TraceScope = TraceScope::SystemScope;
const LOG_LEVEL: Level = Level::TRACE;
const LOG_ENABLE: bool = true;

// ****************************************************************** //
// **                    APPLICATION                               ** //
// ****************************************************************** //

/// Init routine that should be called before the application execution
fn init() -> (TracingLibrary, Vec<TopicHandle>) {
    // Initialize logger and topic registration
    let mut logger = TracingLibraryBuilder::new()
        .global_log_level(LOG_LEVEL)
        .enable_tracing(TRACE_SCOPE)
        .enable_logging(LOG_ENABLE)
        .build();
    logger.init_log_trace();
    let topic_handles = initialize_topics();

    // Start the orchestrator's event polling thread
    Event::get_instance()
        .lock()
        .unwrap()
        .create_polling_thread();

    (logger, topic_handles)
}

/// Start the application execution
fn start<Func, Fut>(program: Func)
where
    Func: Fn() -> Fut,
    Fut: Future<Output = ()> + 'static + Send,
{
    AsyncRuntimeBuilder::new()
        .with_engine(
            ExecutionEngineBuilder::new()
                .task_queue_size(ENGINE_TASK_QUEUE_SIZE)
                .workers(ENGINE_NUM_OF_WORKERS),
        )
        .build()
        .unwrap()
        .enter_engine(program())
        .unwrap_or_default();
}

fn main() {
    // Init and keep logger and topic handles alive
    let (_logger_guard, _topics_guard) = init();

    // Run the main program
    start(main_program);

    // Wait for some time to let the program to finish
    std::thread::sleep(Duration::new(5, 0));
}
