// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! iceoryx2 com backend

use crate::interface::{
    ActivityInput, ActivityOutput, ActivityOutputDefault, Error, InputGuard, OutputGuard,
    OutputUninitGuard, Topic, TopicHandle,
};
use alloc::boxed::Box;
use alloc::format;
use core::fmt;
use core::mem::MaybeUninit;
use core::ops::{Deref, DerefMut};
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
use std::process;

/// Initialize topic with the given number of writers (publishers) and readers (subscribers).
pub fn init_topic<T: core::fmt::Debug + 'static>(
    topic: Topic,
    writers: usize,
    readers: usize,
) -> TopicHandle {
    info!("Initializing topic {topic} (Iceoryx2, {writers} writers and {readers} readers)");
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

/// Wrapper around a [Subscriber] implementing [ActivityInput]
#[derive(Debug)]
pub struct Iox2Input<T>
where
    T: fmt::Debug + 'static,
{
    subscriber: Subscriber<ipc::Service, T, ()>,
}

impl<T> Iox2Input<T>
where
    T: fmt::Debug + 'static,
{
    // Create a new instance for the given `topic`
    pub fn new(topic: &str) -> Self {
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
        Self { subscriber }
    }
}

impl<T> ActivityInput<T> for Iox2Input<T>
where
    T: fmt::Debug + 'static,
{
    fn read(&self) -> Result<InputGuard<T>, Error> {
        match self.subscriber.receive() {
            Ok(Some(sample)) => Ok(InputGuard::Iox2(Iox2InputGuard { sample })),
            Ok(None) | Err(_) => Err(Error::NoEmptyBuffer),
        }
    }
}

/// Wrapper around a [Publisher] implementing both [ActivityOutput] and [ActivityOutputDefault]
#[derive(Debug)]
pub struct Iox2Output<T>
where
    T: fmt::Debug + 'static,
{
    publisher: Publisher<ipc::Service, T, ()>,
}

impl<T> Iox2Output<T>
where
    T: fmt::Debug + 'static,
{
    // Create a new instance for the given `topic`
    pub fn new(topic: &str) -> Self {
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
        Self { publisher }
    }
}

impl<T> ActivityOutput<T> for Iox2Output<T>
where
    T: fmt::Debug + 'static,
{
    /// Get a handle to an uninitialized buffer
    fn write_uninit(&mut self) -> Result<OutputUninitGuard<T>, Error> {
        self.publisher
            .loan_uninit()
            .map(|sample| OutputUninitGuard::Iox2(Iox2OutputUninitGuard { sample }))
            .map_err(|_| Error::NoEmptyBuffer)
    }
}

impl<T> ActivityOutputDefault<T> for Iox2Output<T>
where
    T: fmt::Debug + Default + 'static,
{
    /// Get a handle to a buffer initialized with the [Default] trait
    fn write_init(&mut self) -> Result<OutputGuard<T>, Error> {
        self.publisher
            .loan()
            .map(|sample| OutputGuard::Iox2(Iox2OutputGuard { sample }))
            .map_err(|_| Error::NoEmptyBuffer)
    }
}

/// Handle to an input buffer
pub struct Iox2InputGuard<T: fmt::Debug> {
    sample: Sample<ipc::Service, T, ()>,
}

impl<T: fmt::Debug> Deref for Iox2InputGuard<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.sample.payload()
    }
}

/// Handle to an initialized output buffer
pub struct Iox2OutputGuard<T: fmt::Debug> {
    sample: SampleMut<ipc::Service, T, ()>,
}

impl<T> Iox2OutputGuard<T>
where
    T: fmt::Debug,
{
    /// Send this buffer, making it receivable as input and consuming the buffer
    pub(crate) fn send(self) -> Result<(), Error> {
        self.sample
            .send()
            .map(|_| {})
            .map_err(|_| Error::SendFailed)
    }
}

impl<T: fmt::Debug> Deref for Iox2OutputGuard<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.sample.payload()
    }
}

impl<T: fmt::Debug> DerefMut for Iox2OutputGuard<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.sample.payload_mut()
    }
}

/// Handle to an uninitialized output buffer
pub struct Iox2OutputUninitGuard<T: fmt::Debug> {
    sample: SampleMutUninit<ipc::Service, MaybeUninit<T>, ()>,
}

impl<T> Iox2OutputUninitGuard<T>
where
    T: fmt::Debug,
{
    /// Assume the backing buffer is initialized
    ///
    /// # Safety
    ///
    /// This is safe as long as the backing buffer has been validly initialized beforehand.
    pub(crate) unsafe fn assume_init(self) -> Iox2OutputGuard<T> {
        let sample = unsafe { self.sample.assume_init() };
        Iox2OutputGuard { sample }
    }

    /// Write a complete valid type into the uninitialized buffer, initializing it in the process
    pub(crate) fn write_payload(self, value: T) -> Iox2OutputGuard<T> {
        let sample = self.sample.write_payload(value);
        Iox2OutputGuard { sample }
    }
}

impl<T> Iox2OutputUninitGuard<T>
where
    T: fmt::Debug + Default,
{
    /// Initialize this buffer with its [Default] implementation
    pub(crate) fn init(self) -> Iox2OutputGuard<T> {
        let sample = self.sample.write_payload(T::default());
        Iox2OutputGuard { sample }
    }
}

impl<T: fmt::Debug> Deref for Iox2OutputUninitGuard<T> {
    type Target = MaybeUninit<T>;

    fn deref(&self) -> &Self::Target {
        self.sample.payload()
    }
}

impl<T: fmt::Debug> DerefMut for Iox2OutputUninitGuard<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.sample.payload_mut()
    }
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
