// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! Communication interface
//!
//! The following points where kept in mind when designing the communication API:
//! - During the startup phase, we can do heap allocations,
//!   so using dynamically-sized trait objects allocated on the heap is fine.
//! - During the main step loop, we cannot do heap allocations any longer,
//!   meaning dynamically-sized types which require heap allocations cannot be used.
//! - References to backing buffers need lifetimes.
//!
//! To attach a lifetime to a buffer which only exists ephemerally, we wrap it in a guard.
//! We have [ActivityInput], [ActivityOutput] and [ActivityOutputDefault] defined as traits,
//! but their trait methods return types of a known size,
//! the enums [InputGuard], [OutputGuard] and [OutputUninitGuard].

#[cfg(feature = "ipc_iceoryx2")]
use crate::iox2;
#[cfg(feature = "ipc_iceoryx2")]
use crate::iox2::{Iox2InputGuard, Iox2OutputGuard, Iox2OutputUninitGuard};
#[cfg(feature = "ipc_linux_shm")]
use crate::linux_shm;
use crate::linux_shm::shared_memory::{MappingMode, TopicInitializationAgentRole};
#[cfg(feature = "ipc_linux_shm")]
use crate::linux_shm::{LinuxShmInputGuard, LinuxShmOutputGuard, LinuxShmOutputUninitGuard};
use alloc::boxed::Box;
use core::any::Any;
use core::fmt;
use core::mem::MaybeUninit;
use core::ops::{Deref, DerefMut};

pub type Topic<'a> = &'a str;

// COM backend runtime switch.
#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq)]
pub enum ComBackend {
    #[cfg(feature = "ipc_iceoryx2")]
    Iox2,
    #[cfg(feature = "ipc_linux_shm")]
    LinuxShm,
}

/// Error type of communication module
#[derive(Debug, Clone, Copy)]
pub enum Error {
    NoEmptyBuffer,
    SendFailed,
}

/// A trait for structs which can provide handles to input buffers
pub trait ActivityInput<T>: fmt::Debug
where
    T: fmt::Debug,
{
    /// Get a handle to an input buffer
    fn read(&self) -> Result<InputGuard<T>, Error>;
}

/// Handle to an input buffer
///
/// This handle wraps buffers of specific com implementations
/// and thereby provides references to the buffer with a lifetime.
/// It is an enum so that it has a size known at compile-time.
pub enum InputGuard<T>
where
    T: fmt::Debug,
{
    #[cfg(feature = "ipc_iceoryx2")]
    Iox2(Iox2InputGuard<T>),
    #[cfg(feature = "ipc_linux_shm")]
    LinuxShm(LinuxShmInputGuard<T>),
}

impl<T> Deref for InputGuard<T>
where
    T: fmt::Debug,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match self {
            #[cfg(feature = "ipc_iceoryx2")]
            Self::Iox2(guard) => guard,
            #[cfg(feature = "ipc_linux_shm")]
            Self::LinuxShm(guard) => guard,
        }
    }
}

/// A trait for structs which can provide handles to uninitialized output buffers
pub trait ActivityOutput<T>: fmt::Debug
where
    T: fmt::Debug,
{
    /// Get a handle to an uninitialized output buffer
    fn write_uninit(&mut self) -> Result<OutputUninitGuard<T>, Error>;
}

/// A trait for structs which can provide handles to default-initialized output buffers
pub trait ActivityOutputDefault<T>: fmt::Debug
where
    T: fmt::Debug + Default,
{
    /// Get a handle to a default initialized output buffer
    fn write_init(&mut self) -> Result<OutputGuard<T>, Error>;
}

/// Handle to an initialized output buffer
///
/// This handle wraps buffers of specific com implementations
/// and thereby provides references to the buffer with a lifetime.
/// It is an enum so that it has a size known at compile-time.
///
/// This handle can be obtained in several ways.
/// If the inner type has a [Default] implementation:
/// - Directly from a struct implementing [ActivityOutputDefault],
///   providing a buffer initialized with its [Default] implementation.
/// - Indirectly by calling `init` on an uninitialized buffer
///   obtained from a struct implementing [ActivityOutput].
///
/// If the inner type does not have a [Default] implementation:
/// - Indirectly, by writing a complete value to an uninitialized handle.
/// - Indirectly, by writing directly to the buffer in an uninitialized handle
///   and calling `assume_init` (`unsafe`) on the handle.
///
/// For the buffer to be receivable as input, it has to be [Self::send],
/// consuming the handle.
#[must_use = "buffer has to be sent to be observable"]
pub enum OutputGuard<T: fmt::Debug> {
    #[cfg(feature = "ipc_iceoryx2")]
    Iox2(Iox2OutputGuard<T>),
    #[cfg(feature = "ipc_linux_shm")]
    LinuxShm(LinuxShmOutputGuard<T>),
}

impl<T> OutputGuard<T>
where
    T: fmt::Debug,
{
    /// Send this buffer
    pub fn send(self) -> Result<(), Error> {
        match self {
            #[cfg(feature = "ipc_iceoryx2")]
            Self::Iox2(guard) => guard.send(),
            #[cfg(feature = "ipc_linux_shm")]
            Self::LinuxShm(guard) => guard.send(),
        }
    }
}

impl<T> Deref for OutputGuard<T>
where
    T: fmt::Debug,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match self {
            #[cfg(feature = "ipc_iceoryx2")]
            Self::Iox2(guard) => guard,
            #[cfg(feature = "ipc_linux_shm")]
            Self::LinuxShm(guard) => guard,
        }
    }
}

impl<T> DerefMut for OutputGuard<T>
where
    T: fmt::Debug + Default,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            #[cfg(feature = "ipc_iceoryx2")]
            Self::Iox2(guard) => guard,
            #[cfg(feature = "ipc_linux_shm")]
            Self::LinuxShm(guard) => guard,
        }
    }
}

/// Handle to an uninitialized output buffer
///
/// This handle wraps buffers of specific com implementations
/// and thereby provides references to the buffer with a lifetime.
/// It is an enum so that it has a size known at compile-time.
///
/// For the buffer to be sendable, it has to be written/initialized
/// to be turned into the initialized variant of the handle.
/// There are three ways to achieve this:
/// - Writing a complete valid payload into the uninitialized buffer,
///   using [Self::write_payload]. This might be unefficient
///   for large `T` because the type has to exist somewhere in memory before.
/// - Calling [Self::init] for types which have a [Default] implementation.
/// - Writing directly to the uninitialized memory and call [Self::assume_init].
///   This is `unsafe` and the caller has to ensure that the buffer is initialized
///   to a valid value before calling [Self::assume_init].
pub enum OutputUninitGuard<T: fmt::Debug> {
    #[cfg(feature = "ipc_iceoryx2")]
    Iox2(Iox2OutputUninitGuard<T>),
    #[cfg(feature = "ipc_linux_shm")]
    LinuxShm(LinuxShmOutputUninitGuard<T>),
}

impl<T> OutputUninitGuard<T>
where
    T: fmt::Debug,
{
    /// Assume the backing buffer is initialized
    ///
    /// # Safety
    ///
    /// The caller has to ensure that the uninitialized memory
    /// was completely initialized with a valid value.
    pub unsafe fn assume_init(self) -> OutputGuard<T> {
        match self {
            #[cfg(feature = "ipc_iceoryx2")]
            Self::Iox2(guard) => unsafe { OutputGuard::Iox2(guard.assume_init()) },
            #[cfg(feature = "ipc_linux_shm")]
            Self::LinuxShm(guard) => OutputGuard::LinuxShm(guard.assume_init()),
        }
    }

    /// Write a complete valid type into the uninitialized buffer, initializing it in the process
    pub fn write_payload(self, value: T) -> OutputGuard<T> {
        match self {
            #[cfg(feature = "ipc_iceoryx2")]
            Self::Iox2(guard) => OutputGuard::Iox2(guard.write_payload(value)),
            #[cfg(feature = "ipc_linux_shm")]
            Self::LinuxShm(guard) => OutputGuard::LinuxShm(guard.write_payload(value)),
        }
    }
}

impl<T> OutputUninitGuard<T>
where
    T: fmt::Debug + Default,
{
    /// Initialize the uninitialized buffer with its [Default] trait
    pub fn init(self) -> OutputGuard<T> {
        match self {
            #[cfg(feature = "ipc_iceoryx2")]
            Self::Iox2(guard) => OutputGuard::Iox2(guard.init()),
            #[cfg(feature = "ipc_linux_shm")]
            Self::LinuxShm(guard) => OutputGuard::LinuxShm(guard.init()),
        }
    }
}

impl<T> Deref for OutputUninitGuard<T>
where
    T: fmt::Debug,
{
    type Target = MaybeUninit<T>;

    fn deref(&self) -> &Self::Target {
        match self {
            #[cfg(feature = "ipc_iceoryx2")]
            Self::Iox2(guard) => guard,
            #[cfg(feature = "ipc_linux_shm")]
            Self::LinuxShm(guard) => guard,
        }
    }
}

impl<T> DerefMut for OutputUninitGuard<T>
where
    T: fmt::Debug,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            #[cfg(feature = "ipc_iceoryx2")]
            Self::Iox2(guard) => guard,
            #[cfg(feature = "ipc_linux_shm")]
            Self::LinuxShm(guard) => guard,
        }
    }
}

/// COM backend topic initialization arguments for the primary agent
#[derive(Clone, Copy)]
#[allow(unused)]
pub struct ComBackendTopicPrimaryInitialization<'a> {
    topic: Topic<'a>,
    backend: ComBackend,
    readers: usize,
    writers: usize,
    map_locally: bool,
    is_local_write: bool,
}

impl<'a> ComBackendTopicPrimaryInitialization<'a> {
    pub fn new(
        topic: Topic<'a>,
        backend: ComBackend,
        readers: usize,
        writers: usize,
        map_locally: bool,
        is_local_write: bool,
    ) -> Self {
        Self {
            topic,
            backend,
            readers,
            writers,
            map_locally,
            is_local_write,
        }
    }
}

/// COM backend topic initialization arguments for secondary agents (and recorders)
#[derive(Clone, Copy)]
pub struct ComBackendTopicSecondaryInitialization<'a> {
    topic: Topic<'a>,
    backend: ComBackend,
    is_local_write: bool,
}

impl<'a> ComBackendTopicSecondaryInitialization<'a> {
    pub fn new(topic: Topic<'a>, backend: ComBackend, is_local_write: bool) -> Self {
        Self {
            topic,
            backend,
            is_local_write,
        }
    }
}

pub fn init_topic_primary<T: fmt::Debug + Default + 'static>(
    params: &ComBackendTopicPrimaryInitialization,
) -> TopicHandle {
    match params.backend {
        #[cfg(feature = "ipc_iceoryx2")]
        ComBackend::Iox2 => iox2::init_topic::<T>(params.topic, params.writers, params.readers),

        #[cfg(feature = "ipc_linux_shm")]
        ComBackend::LinuxShm => {
            let agent_role = TopicInitializationAgentRole::Primary {
                also_map: params.map_locally,
            };

            let mapping_mode = {
                if params.is_local_write {
                    MappingMode::Write
                } else {
                    MappingMode::Read
                }
            };
            linux_shm::init_topic::<T>(params.topic, mapping_mode, agent_role)
        }
    }
}

pub fn init_topic_secondary<T: fmt::Debug + Default + 'static>(
    params: &ComBackendTopicSecondaryInitialization,
) -> TopicHandle {
    match params.backend {
        // For iox2: do nothing and return dummy handle
        #[cfg(feature = "ipc_iceoryx2")]
        ComBackend::Iox2 => TopicHandle::from(Box::new(0u8)),

        #[cfg(feature = "ipc_linux_shm")]
        ComBackend::LinuxShm => {
            let agent_role = TopicInitializationAgentRole::Secondary;
            let mapping_mode = {
                if params.is_local_write {
                    MappingMode::Write
                } else {
                    MappingMode::Read
                }
            };
            linux_shm::init_topic::<T>(params.topic, mapping_mode, agent_role)
        }
    }
}

#[must_use = "keep me alive until activities are created"]
/// Opaque handle of a topic.
///
/// This must be kept alive after topic initialization until the activities are started.
pub struct TopicHandle {
    _inner: Box<dyn Any>,
}

impl<T: 'static> From<Box<T>> for TopicHandle {
    fn from(value: Box<T>) -> Self {
        TopicHandle { _inner: value }
    }
}

/// Start the given backend, if necessary
pub fn run_backend(backend: ComBackend, _local_requests: usize, _remote_requests: usize) {
    match backend {
        #[cfg(feature = "ipc_iceoryx2")]
        ComBackend::Iox2 => {}
        #[cfg(feature = "ipc_linux_shm")]
        ComBackend::LinuxShm => {
            linux_shm::ComRuntime::run_service(_remote_requests);
        }
    }
}
