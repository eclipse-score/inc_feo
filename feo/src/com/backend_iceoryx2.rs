// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use crate::com::interface::{Input, InputGuard, Output, OutputGuard, TopicHandle};
use crate::configuration::topics::Topic;
use feo_log::{error, info};
use iceoryx2::config::Config;
use iceoryx2::node::{Node, NodeBuilder, NodeState};
use iceoryx2::port::publisher::Publisher;
use iceoryx2::port::subscriber::Subscriber;
use iceoryx2::prelude::{CallbackProgression, NodeName};
use iceoryx2::sample::Sample;
use iceoryx2::sample_mut::SampleMut;
use iceoryx2::sample_mut_uninit::SampleMutUninit;
use iceoryx2::service::ipc;
use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::process;

pub type IpcPayload<T> = Sample<ipc::Service, T, ()>;
pub type IpcPayloadMut<T> = SampleMut<ipc::Service, T, ()>;
pub type IpcPayloadMutUninit<T> = SampleMutUninit<ipc::Service, MaybeUninit<T>, ()>;

impl<T: std::fmt::Debug> Input<T, Subscriber<ipc::Service, T, ()>> {
    /// Get an input handle by topic.
    pub fn get(topic: &str) -> Self {
        let subscriber = ipc_node()
            .service_builder(
                &topic
                    .try_into()
                    .unwrap_or_else(|_| panic!("invalid topic {topic}")),
            )
            .publish_subscribe::<T>()
            .open()
            .unwrap_or_else(|e| panic!("failed to open subscriber for topic {topic}: {e}"))
            .subscriber_builder()
            .create()
            .unwrap_or_else(|_| panic!("failed to create subscriber for topic {topic}"));

        Self {
            inner: subscriber,
            _type: PhantomData,
        }
    }

    /// Get a guard with a payload to read.
    pub fn read(&self) -> Option<InputGuard<T, IpcPayload<T>>> {
        if let Ok(sample_opt) = self.inner.receive() {
            return sample_opt.map(|s| InputGuard {
                inner: s,
                _type: PhantomData,
            });
        }

        None
    }
}

impl<T: std::fmt::Debug> InputGuard<T, IpcPayload<T>> {
    /// Get a reference to the payload.
    pub fn get(&self) -> &T {
        &self.inner
    }
}

impl<T: std::fmt::Debug> Output<T, Publisher<ipc::Service, T, ()>> {
    /// Get an output handle by topic.
    pub fn get(topic: &str) -> Self {
        let publisher = ipc_node()
            .service_builder(
                &topic
                    .try_into()
                    .unwrap_or_else(|_| panic!("invalid topic {topic}")),
            )
            .publish_subscribe::<T>()
            .open()
            .unwrap_or_else(|e| panic!("failed to open subscriber for topic {topic}: {e}"))
            .publisher_builder()
            .create()
            .unwrap_or_else(|_| panic!("failed to create subscriber for topic {topic}"));

        Self {
            inner: publisher,
            _type: PhantomData,
        }
    }
}

impl<T: std::fmt::Debug + Default> Output<T, Publisher<ipc::Service, T, ()>> {
    /// Get a guard with an initialized payload to write to.
    ///
    /// In most cases, you should prefer `write_uninit` to avoid the initialization cost.
    pub fn write_init(&self) -> Option<OutputGuard<T, IpcPayloadMut<T>>> {
        self.inner.loan().ok().map(|s| OutputGuard {
            inner: s,
            _type: PhantomData,
        })
    }
}

impl<T: std::fmt::Debug> Output<T, Publisher<ipc::Service, T, ()>> {
    /// Get a guard with an uninitialized payload to write to.
    pub fn write_uninit(&self) -> Option<OutputGuard<T, IpcPayloadMutUninit<T>>> {
        self.inner.loan_uninit().ok().map(|s| OutputGuard {
            inner: s,
            _type: PhantomData,
        })
    }
}

impl<T: std::fmt::Debug> OutputGuard<T, IpcPayloadMutUninit<T>> {
    /// Write payload.
    ///
    /// To send the written payload, use `send`.
    pub fn write_payload(self, payload: T) -> OutputGuard<T, IpcPayloadMut<T>> {
        let inner = self.inner.write_payload(payload);
        OutputGuard {
            inner,
            _type: PhantomData,
        }
    }

    /// Mutably access the payload.
    pub fn payload_mut(&mut self) -> &mut MaybeUninit<T> {
        self.inner.payload_mut()
    }

    /// Assume that the payload is initialized.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the underlying `MaybeUninit` really is initialized.
    /// Calling this when the content is not fully initialized causes immediate undefined behavior.
    pub unsafe fn assume_init(self) -> OutputGuard<T, IpcPayloadMut<T>> {
        let inner = self.inner.assume_init();
        OutputGuard {
            inner,
            _type: PhantomData,
        }
    }
}

impl<T: std::fmt::Debug> OutputGuard<T, IpcPayloadMut<T>> {
    /// Get a mutable reference to the payload.
    ///
    /// After writing the payload through the mutable reference, all `send` to send it out.
    pub fn get_mut(&mut self) -> &mut T {
        self.inner.payload_mut()
    }

    /// Send payload.
    pub fn send(self) {
        self.inner.send().unwrap();
    }
}

/// Initialize topic with the given number of writers (publishers) and readers (subscribers).
pub fn init_topic<T: std::fmt::Debug + 'static>(
    topic: Topic,
    writers: usize,
    readers: usize,
) -> TopicHandle {
    info!("Initializing topic {topic} for {writers} writers and {readers} readers");
    let port_factory = ipc_node()
        .service_builder(
            &(*topic)
                .try_into()
                .unwrap_or_else(|_| panic!("invalid topic {topic}")),
        )
        .publish_subscribe::<T>()
        .max_publishers(writers)
        .max_subscribers(readers)
        .enable_safe_overflow(true)
        .subscriber_max_buffer_size(1)
        .create()
        .unwrap_or_else(|e| panic!("failed to create subscriber for topic {topic}: {e}"));
    Box::new(port_factory).into()
}

fn ipc_node() -> &'static Node<ipc::Service> {
    static ICEORYX_NODE: std::sync::OnceLock<Node<ipc::Service>> = std::sync::OnceLock::new();

    ICEORYX_NODE.get_or_init(|| {
        let config = {
            let mut config = Config::default();
            config.global.prefix = "feo_ipc".try_into().unwrap();
            config
        };

        // Ensure there is no left-over state from dead nodes.
        Node::<ipc::Service>::cleanup_dead_nodes(&config);
        Node::<ipc::Service>::list(&config, |node_state| {
            if let NodeState::<ipc::Service>::Dead(view) = node_state {
                if let Err(e) = view.remove_stale_resources() {
                    error!("Failed to clean iceoryx2 resources: {:?}", e);
                }
            }
            CallbackProgression::Continue
        })
        .expect("failed to clean iceoryx2 state");

        let name =
            NodeName::new(&format!("feo_node_{}", process::id())).expect("invalid node name");

        NodeBuilder::new()
            .name(&name)
            .config(&config)
            .create::<ipc::Service>()
            .expect("failed to create ipc node")
    })
}
