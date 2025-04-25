// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use alloc::sync::Arc;
use core::any::TypeId;
use core::fmt::Debug;
use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};
use core::ptr;
use core::sync::atomic::{fence, AtomicBool, AtomicPtr, AtomicU16, Ordering};

// Mode of shared memory mapping
#[derive(Debug, Clone, Copy)]
pub enum TopicInitializationAgentRole {
    Primary {
        // Also map topic locally
        also_map: bool,
    },
    Secondary,
}

// Mode of shared memory mapping
#[derive(Debug, Clone, Copy)]
pub enum MappingMode {
    Read,
    Write,
}

// Managed pointer to T with run-time type check and access control checks
//
// Provides lock-free access control, panics on multiple write or mixed read/write access
// Implements interior mutablity and can be shared
// It's safe to map a (mutable) reference to memory of the `ptr` when it's not null:
// 1. Points to the memory of at least size_of::<T> bytes and size_of::<T> is not 0 and less or equals to isize::MAX (checked explicitly when mapped)
// 2. The alignment of the pointer is correct for T (provided by OS API)
// 3. The memory area is contained within a single allocated object (provided by OS API)
// 4. T can be initialized from a byte slice (using zerocopy trait)
// 5. The pointer is valid for 'static lifetime (mapped memory is never unmapped)
// 6. Runtime access check & type check prevent API misuse
// 7. Runtime check for write call coupled with send (panics on missed send)
//
// Implementation details:
// lock_state = u16::MAX - locked for writing
// lock_state in (0; u16::MAX) - locked for reading (<lock_state> active readers)
// lock_state = 0 - unlocked
// Dropping the [MappedPtrWriteGuard] without a [MappedPtrWriteGuard::send] call does *NOT* unlock the pointer and is guarantied to panic
#[derive(Debug)]
pub(crate) struct ReadWriteAccessControlPtr {
    pub(crate) type_id: TypeId,
    pub(crate) size: usize,
    pub(crate) lock_state: AtomicU16,
    pub(crate) ptr: AtomicPtr<()>,
    pub(crate) writable: AtomicBool,
}

impl ReadWriteAccessControlPtr {
    pub(crate) fn new_unmapped<T: 'static>(mapping_mode: MappingMode) -> Self {
        const {
            assert!(size_of::<T>() != 0, "zero-sized type is not allowed");
            assert!(
                size_of::<T>() <= isize::MAX as usize,
                "type size is too big"
            );
        }
        Self {
            type_id: TypeId::of::<T>(),
            size: size_of::<T>(),
            lock_state: AtomicU16::new(0),
            ptr: AtomicPtr::new(ptr::null_mut()),
            writable: AtomicBool::new(matches!(mapping_mode, MappingMode::Write)),
        }
    }

    fn ptr(&self) -> *mut () {
        self.ptr.load(Ordering::Relaxed)
    }

    fn lock_read(&self) {
        let mut state = self.lock_state.load(Ordering::Relaxed);
        loop {
            assert_ne!(state, u16::MAX - 1, "too many readers");
            assert_ne!(
                state,
                u16::MAX,
                "multiple exclusive access attempts detected"
            );
            let new_state = self.lock_state.compare_exchange(
                state,
                state + 1,
                Ordering::Acquire,
                Ordering::Relaxed,
            );
            match new_state {
                // Success
                Ok(_) => break,
                Err(actual_state) => state = actual_state,
            }
        }
    }

    fn unlock_read(&self) {
        let mut state = self.lock_state.load(Ordering::Relaxed);
        loop {
            assert_ne!(
                state,
                u16::MAX,
                "multiple exclusive access attempts detected"
            );
            assert_ne!(state, 0, "invalid state");
            let new_state = self.lock_state.compare_exchange(
                state,
                state - 1,
                Ordering::Release,
                Ordering::Relaxed,
            );
            match new_state {
                // Success
                Ok(_) => break,
                Err(actual_state) => state = actual_state,
            }
        }
    }

    fn lock_write(&self) {
        assert!(
            self.lock_state
                .compare_exchange(0, u16::MAX, Ordering::Acquire, Ordering::Relaxed)
                .is_ok(),
            "multiple exclusive access attempts detected"
        );
    }

    fn is_locked(&self) -> bool {
        self.lock_state.load(Ordering::Acquire) != 0
    }

    fn unlock_write(&self) {
        assert!(
            self.lock_state
                .compare_exchange(u16::MAX, 0, Ordering::Release, Ordering::Relaxed)
                .is_ok(),
            "invalid state"
        );
    }

    pub fn get<T: 'static>(self: &Arc<Self>) -> MappedPtrReadGuard<T> {
        assert!(!self.ptr.load(Ordering::Relaxed).is_null(), "unmapped");
        assert_eq!(TypeId::of::<T>(), self.type_id);
        MappedPtrReadGuard::new(self.clone())
    }

    pub fn get_mut<T: 'static>(self: &Arc<Self>) -> MappedPtrWriteGuard<T> {
        assert!(!self.ptr.load(Ordering::Relaxed).is_null(), "unmapped");
        assert_eq!(TypeId::of::<T>(), self.type_id, "invalid type");
        assert!(self.writable.load(Ordering::Relaxed), "not writable");
        MappedPtrWriteGuard::new(self.clone())
    }
}

#[derive(Debug)]
pub struct MappedPtrReadGuard<T> {
    mapped_ptr: Arc<ReadWriteAccessControlPtr>,
    _type: PhantomData<T>,
}

impl<T> MappedPtrReadGuard<T> {
    fn new(mapped_ptr: Arc<ReadWriteAccessControlPtr>) -> Self {
        mapped_ptr.lock_read();
        fence(Ordering::Acquire);
        Self {
            mapped_ptr,
            _type: PhantomData,
        }
    }
}

impl<T> Drop for MappedPtrReadGuard<T> {
    fn drop(&mut self) {
        self.mapped_ptr.unlock_read();
    }
}

impl<T> Deref for MappedPtrReadGuard<T> {
    type Target = T;
    fn deref(&self) -> &T {
        // Safety: MappedPtrReadGuard can only be created when all requirements met
        // See the safety requirements of [ReadWriteAccessControlPtr]
        unsafe { &*(self.mapped_ptr.ptr() as *const T) }
    }
}

#[derive(Debug)]
pub struct MappedPtrWriteGuard<T> {
    mapped_ptr: Arc<ReadWriteAccessControlPtr>,
    _type: PhantomData<T>,
}

impl<T> MappedPtrWriteGuard<T> {
    fn new(mapped_ptr: Arc<ReadWriteAccessControlPtr>) -> Self {
        mapped_ptr.lock_write();
        Self {
            mapped_ptr,
            _type: PhantomData,
        }
    }

    pub fn send(self) {
        fence(Ordering::Release);
        self.mapped_ptr.unlock_write();
    }
}

impl<T> Drop for MappedPtrWriteGuard<T> {
    fn drop(&mut self) {
        assert!(
            !self.mapped_ptr.is_locked(),
            "send call is mandatory for LinuxShm backend"
        );
    }
}

impl<T> Deref for MappedPtrWriteGuard<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        // Safety: MappedPtrWriteGuard can only be created when all requirements met
        // See the safety requirements of [ReadWriteAccessControlPtr]
        unsafe { &*(self.mapped_ptr.ptr() as *const T) }
    }
}

impl<T> DerefMut for MappedPtrWriteGuard<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // Safety: MappedPtrWriteGuard can only be created when all requirements met
        // See the safety requirements of [ReadWriteAccessControlPtr]
        unsafe { &mut *(self.mapped_ptr.ptr() as *mut T) }
    }
}
