// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! Basic shared memory com backend
//!
//! This is an experimental shared memory com backend that can be used
//! in the absence of other communication backends or as a reference
//! for benchmarking.
//!
//! Note the following specific behaviours:
//! - There is no explicit synchronization between write access and read accesses
//!   of a given topic, but it is assumed that there is an implicit synchronization
//!   resulting from the fact that publishers and subscribers are always run
//!   within a deterministic FEO task chain and there will be only one writing
//!   task in the chain.
//! - The `send` operation is is a no-op from the perspective of data update, i.e.
//!   data is updated while written to the memory buffer. In order to prevent
//!   unintentional "publication" of data without an explicit call of `send`,
//!   the application will panic, if a [MappedPtrWriteGuard] is dropped without a
//!   preceding call of [MappedPtrWriteGuard::send].
//!

pub(crate) mod shared_memory;

use crate::interface::{
    ActivityInput, ActivityOutput, ActivityOutputDefault, Error, InputGuard, OutputGuard,
    OutputUninitGuard, Topic, TopicHandle,
};
use crate::linux_shm::shared_memory::{
    MappedPtrReadGuard, MappedPtrWriteGuard, MappingMode, ReadWriteAccessControlPtr,
    TopicInitializationAgentRole,
};
use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt::Debug;
use core::marker::PhantomData;
use core::mem::{size_of, MaybeUninit};
use core::ops::{Deref, DerefMut};
use core::slice::from_raw_parts;
use core::sync::atomic::Ordering;
use feo_log::{debug, error, info};
use nix::fcntl::OFlag;
use nix::sys::mman::{mmap, shm_open, MapFlags, ProtFlags};
use nix::sys::stat::Mode;
use nix::unistd;
use std::collections::HashMap;
use std::io::{read_to_string, Write};
use std::net::Shutdown;
use std::os::fd::OwnedFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::{LazyLock, Mutex, MutexGuard};
use std::thread;
use std::thread::JoinHandle;

// Global runtime
static RUNTIME: LazyLock<Mutex<ComRuntime>> = LazyLock::new(|| Mutex::new(ComRuntime::new()));

impl ReadWriteAccessControlPtr {
    fn map(&self, native_mapping: &OwnedFd) {
        assert!(self.ptr.load(Ordering::Relaxed).is_null(), "already mapped");
        let flags = if self.writable.load(Ordering::Relaxed) {
            ProtFlags::PROT_READ | ProtFlags::PROT_WRITE
        } else {
            ProtFlags::PROT_READ
        };
        // Safety: FFI call
        let ptr = unsafe {
            mmap(
                None,
                self.size
                    .try_into()
                    .expect("zero-sized type is not allowed"),
                flags,
                MapFlags::MAP_SHARED,
                native_mapping,
                0,
            )
        }
        .expect("mmap failed")
        .as_ptr();
        let ptr = ptr as *mut ();
        assert!(!ptr.is_null());
        self.ptr.store(ptr, Ordering::Relaxed);
    }
}

struct TopicMapping {
    ptr: Arc<ReadWriteAccessControlPtr>,
    // Unique mapping id
    mapping_id: String,
}

// COM runtime state
pub struct ComRuntime {
    topics: HashMap<String, TopicMapping>,
    _thread: Option<JoinHandle<()>>,
}

const SOCKET: &str = "/tmp/score_feo.socket";

impl ComRuntime {
    pub fn run_service(requests_to_serve: usize) {
        let thread = thread::spawn(move || ComRuntime::service_main(requests_to_serve));
        let mut com = ComRuntime::global_runtime();
        com._thread = Some(thread);
    }

    /// Run COM runtime services
    fn service_main(mut requests_to_serve: usize) {
        let _ = std::fs::remove_file(SOCKET);
        let listener = UnixListener::bind(SOCKET)
            .unwrap_or_else(|e| panic!("can't bind socket at {SOCKET}: {e}"));
        debug!("Listening for {requests_to_serve} topic mapping requests...");
        loop {
            if requests_to_serve < 1 {
                break;
            }
            match listener.accept() {
                Ok((socket, _)) => {
                    if Self::serve_connection(socket) {
                        requests_to_serve -= 1;
                    }
                }
                Err(e) => {
                    error!("socket connection failed: {e}");
                }
            }
        }
        debug!("COM primary service shutdown");
    }

    // Serves one connection from secondary
    // Returns true if served successfully
    fn serve_connection(mut stream: UnixStream) -> bool {
        debug!("Connection accepted, handling...");
        let s = read_to_string(&mut stream).expect("socket read failed");
        let com = Self::global_runtime();
        let mut result = false;
        match com.topics.get(s.as_str()) {
            Some(mapping) => {
                match stream.write_all(
                    format!("ok\n{}\n{}", mapping.ptr.size, &mapping.mapping_id).as_bytes(),
                ) {
                    Ok(_) => result = true,
                    Err(e) => error!("socket write failed: {e}"),
                }
            }
            None => {
                if let Err(e) = stream.write_all(b"error\ntopic not found") {
                    error!("socket write failed: {e}");
                }
            }
        }
        stream
            .shutdown(Shutdown::Both)
            .expect("socket shutdown failed");
        result
    }

    fn new() -> Self {
        Self {
            topics: HashMap::new(),
            _thread: None,
        }
    }

    fn unique_mapping_id() -> String {
        format!("score_feo_{:X}", rand::random::<u64>())
    }

    /// Initialize the topic and register it in the COM runtime
    fn init_topic<T: Debug + Default + 'static>(
        &mut self,
        topic: Topic,
        mapping_mode: MappingMode,
        initialization: TopicInitializationAgentRole,
    ) {
        match initialization {
            TopicInitializationAgentRole::Primary { also_map } => {
                self.init_topic_primary::<T>(topic, mapping_mode, also_map);
            }
            TopicInitializationAgentRole::Secondary => {
                self.init_topic_secondary::<T>(topic, mapping_mode);
            }
        }
    }

    fn init_topic_primary<T: Debug + Default + 'static>(
        &mut self,
        topic: Topic,
        mapping_mode: MappingMode,
        also_map: bool,
    ) {
        let size = size_of::<T>();
        info!("Initializing topic {topic} (LinuxShm, {size} bytes)...");
        let mapping_id = Self::unique_mapping_id();
        let native_mapping = shm_open(
            mapping_id.as_str(),
            OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_RDWR,
            Mode::S_IRUSR | Mode::S_IWUSR,
        )
        .unwrap_or_else(|e| panic!("can't create memory mapping for {topic}: {e}"));
        assert_eq!(
            size,
            unistd::write(&native_mapping, unsafe {
                from_raw_parts((&T::default() as *const T) as *const u8, size_of::<T>())
            })
            .expect("can't write shared memory init value")
        );
        let ptr = Arc::new(ReadWriteAccessControlPtr::new_unmapped::<T>(mapping_mode));
        if also_map {
            ptr.map(&native_mapping);
        }
        let mapping = TopicMapping { ptr, mapping_id };
        assert!(
            self.topics.insert(topic.to_owned(), mapping).is_none(),
            "COM topic already initialized"
        );
    }

    fn init_topic_secondary<T: Debug + 'static>(
        &mut self,
        topic: Topic,
        mapping_mode: MappingMode,
    ) {
        let (size, mapping_id) = Self::request_primary(topic);
        assert_eq!(size_of::<T>(), size);
        let native_mapping = shm_open(
            &*mapping_id,
            if matches!(mapping_mode, MappingMode::Write) {
                OFlag::O_RDWR
            } else {
                OFlag::O_RDONLY
            },
            Mode::S_IRUSR,
        )
        .unwrap_or_else(|e| panic!("can't open mapping {mapping_id}: {e}"));
        let ptr = Arc::new(ReadWriteAccessControlPtr::new_unmapped::<T>(mapping_mode));
        ptr.map(&native_mapping);
        let mapping = TopicMapping {
            ptr,
            mapping_id: mapping_id.to_string(),
        };
        assert!(
            self.topics.insert(topic.to_owned(), mapping).is_none(),
            "COM topic already initialized"
        );
    }

    // Make a request to primary
    fn request_primary(topic: Topic) -> (usize, String) {
        let mut stream = UnixStream::connect(SOCKET)
            .unwrap_or_else(|e| panic!("can't connect to socket {SOCKET}: {e}"));
        stream
            .write_all(topic.as_bytes())
            .expect("socket write failed");
        stream
            .shutdown(Shutdown::Write)
            .expect("socket shutdown failed");
        let response = read_to_string(&mut stream).expect("socket read failed");
        stream
            .shutdown(Shutdown::Both)
            .expect("socket shutdown failed");
        let response = response.split('\n').collect::<Vec<&str>>();
        let [status, size, mapping_id] = response.as_slice() else {
            panic!("invalid response")
        };
        assert_eq!(*status, "ok");
        let size = size
            .parse()
            .unwrap_or_else(|e| panic!("can't parse size '{size}': {e}"));
        (size, mapping_id.to_string())
    }

    pub(crate) fn global_runtime() -> MutexGuard<'static, ComRuntime> {
        RUNTIME.lock().expect("can't aquire lock to COM runtime")
    }

    // Create and register mapping for topic
    pub(crate) fn topic_mapping<T>(
        &mut self,
        topic: Topic,
        mode: MappingMode,
    ) -> Arc<ReadWriteAccessControlPtr> {
        const {
            assert!(size_of::<T>() != 0, "zero-sized type is not allowed");
            assert!(
                size_of::<T>() <= isize::MAX as usize,
                "type size is too big"
            );
        }
        let mapping = self
            .topics
            .get_mut(topic)
            .unwrap_or_else(|| panic!("COM topic {topic} is not configured"));
        if matches!(mode, MappingMode::Write) {
            assert!(
                mapping.ptr.writable.load(Ordering::Relaxed),
                "topic {topic} is not writable"
            );
        }
        mapping.ptr.clone()
    }
}

// Initialize the topic and register it in the global COM runtime
pub fn init_topic<T: Debug + Default + 'static>(
    topic: Topic,
    mapping_mode: MappingMode,
    agent_role: TopicInitializationAgentRole,
) -> TopicHandle {
    ComRuntime::global_runtime().init_topic::<T>(topic, mapping_mode, agent_role);
    TopicHandle::from(Box::new(()))
}

pub struct LinuxShmInputGuard<T: Debug>(MappedPtrReadGuard<T>);

impl<T: Debug> Deref for LinuxShmInputGuard<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        Deref::deref(&self.0)
    }
}

pub struct LinuxShmOutputGuard<T: Debug> {
    ptr: MappedPtrWriteGuard<T>,
}

impl<T> LinuxShmOutputGuard<T>
where
    T: Debug,
{
    pub(crate) fn send(self) -> Result<(), Error> {
        self.ptr.send();
        Ok(())
    }
}

impl<T: Debug> Deref for LinuxShmOutputGuard<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        Deref::deref(&self.ptr)
    }
}

impl<T: Debug> DerefMut for LinuxShmOutputGuard<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        DerefMut::deref_mut(&mut self.ptr)
    }
}

pub struct LinuxShmOutputUninitGuard<T: Debug>(MappedPtrWriteGuard<T>);

impl<T> LinuxShmOutputUninitGuard<T>
where
    T: Debug,
{
    // Value is initialized when allocated
    pub(crate) fn assume_init(self) -> LinuxShmOutputGuard<T> {
        LinuxShmOutputGuard { ptr: self.0 }
    }

    // Overwrites with given value
    pub(crate) fn write_payload(mut self, value: T) -> LinuxShmOutputGuard<T> {
        *DerefMut::deref_mut(&mut self.0) = value;
        LinuxShmOutputGuard { ptr: self.0 }
    }
}

impl<T> LinuxShmOutputUninitGuard<T>
where
    T: Debug + Default,
{
    // Overwrites with [Default::default]
    pub(crate) fn init(mut self) -> LinuxShmOutputGuard<T> {
        *DerefMut::deref_mut(&mut self.0) = T::default();
        LinuxShmOutputGuard { ptr: self.0 }
    }
}

impl<T: Debug> Deref for LinuxShmOutputUninitGuard<T> {
    type Target = MaybeUninit<T>;

    fn deref(&self) -> &Self::Target {
        // Safety: MaybeUninit<T> is guaranteed to have the same size, alignment, and ABI as T (according to Rust docs)
        // T is guarantied to be initialized
        unsafe { &*(Deref::deref(&self.0) as *const T as *const MaybeUninit<T>) }
    }
}

impl<T: Debug> DerefMut for LinuxShmOutputUninitGuard<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // Safety: MaybeUninit<T> is guaranteed to have the same size, alignment, and ABI as T (according to Rust docs)
        // T is guarantied to be initialized
        unsafe { &mut *(DerefMut::deref_mut(&mut self.0) as *mut T as *mut MaybeUninit<T>) }
    }
}

#[derive(Debug)]
pub struct LinuxShmInput<T> {
    ptr: Arc<ReadWriteAccessControlPtr>,
    _type: PhantomData<T>,
}

impl<T: Debug + 'static> LinuxShmInput<T> {
    pub fn new(topic: Topic) -> Self {
        Self {
            ptr: ComRuntime::global_runtime().topic_mapping::<T>(topic, MappingMode::Read),
            _type: PhantomData,
        }
    }
}

impl<T> ActivityInput<T> for LinuxShmInput<T>
where
    T: Debug + 'static,
{
    fn read(&self) -> Result<InputGuard<T>, Error> {
        Ok(InputGuard::LinuxShm(LinuxShmInputGuard(self.ptr.get())))
    }
}

#[derive(Debug)]
pub struct LinuxShmOutput<T> {
    ptr: Arc<ReadWriteAccessControlPtr>,
    _type: PhantomData<T>,
}

impl<T: Debug + 'static> LinuxShmOutput<T> {
    pub fn new(topic: Topic) -> Self {
        Self {
            ptr: ComRuntime::global_runtime().topic_mapping::<T>(topic, MappingMode::Write),
            _type: PhantomData,
        }
    }
}

impl<T> ActivityOutput<T> for LinuxShmOutput<T>
where
    T: Debug + 'static,
{
    // Initialized when allocated
    fn write_uninit(&mut self) -> Result<OutputUninitGuard<T>, Error> {
        Ok(OutputUninitGuard::LinuxShm(LinuxShmOutputUninitGuard(
            self.ptr.get_mut(),
        )))
    }
}

impl<T> ActivityOutputDefault<T> for LinuxShmOutput<T>
where
    T: Debug + Default + 'static,
{
    // Overwrites with [Default::default]
    fn write_init(&mut self) -> Result<OutputGuard<T>, Error> {
        let mut ptr = self.ptr.get_mut();
        *ptr = T::default();
        Ok(OutputGuard::LinuxShm(LinuxShmOutputGuard { ptr }))
    }
}
