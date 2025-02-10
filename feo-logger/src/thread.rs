// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

/// The type of a thread id
pub type ThreadId = u32;

/// Get the current thread id
pub fn id() -> ThreadId {
    // Safety: gettid(2) says this never fails
    unsafe { libc::gettid() as u32 }
}
