// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use feo_time::Scaled;
#[cfg(feature = "recording")]
use postcard::experimental::max_size::MaxSize;
#[cfg(feature = "recording")]
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use std::{self};

/// Maximal acceptable tolerance between when determining startup time info
const MAX_DELAY: std::time::Duration = std::time::Duration::from_nanos(100);

/// Maximal number of tries when determining startup time info
const MAX_TRIES: i32 = 10;

/// Startup time info (initialized from std::time i.e. without any scaling)
#[derive(Debug)]
struct TimeInfo {
    systime: std::time::SystemTime,
    instant: std::time::Instant,
}

static STARTUP_TIME: OnceLock<TimeInfo> = OnceLock::new();

/// Initialize the instant of system startup
///
/// # Panics:
///
/// Panics if the method has been called before
pub fn initialize() {
    let startup_time_info = time_info_now();
    STARTUP_TIME
        .set(startup_time_info)
        .expect("failed to initialize startup time");
}

/// Initialize the instant of system startup from a given
///
/// # Panics:
///
/// Panics if the method has been called before
pub fn initialize_from(sync_info: SyncInfo) {
    // Get current system time and corresponding instant
    let time_info_now = time_info_now();

    // Calculate the startup time of the primary agent
    let startup_time = std::time::SystemTime::UNIX_EPOCH + sync_info.since_epoch;

    // Calculate the time elapsed since the startup of the primary agent;
    // assumption is that system clocks are synchronized (but monotonic clocks can be unsynchronized).
    // This works, even if the secondary agent (calling this method) starts before primary agent,
    // because this method will only be called after the startup of the primary agent.
    let elapsed_since_startup = time_info_now
        .systime
        .duration_since(startup_time)
        .expect("failed to synchronize startup time");

    // Calculate the instant of the local monotonic clock at which the primary agent started up.
    // This works as long as the monotonic clock of the secondary agent is not started *after*
    // the startup of the primary agent.
    let startup_instant = time_info_now
        .instant
        .checked_sub(elapsed_since_startup)
        .expect("failed to synchronize startup time");

    // Set the startup time info
    let startup_time_info = TimeInfo {
        instant: startup_instant,
        systime: startup_time,
    };
    STARTUP_TIME
        .set(startup_time_info)
        .expect("failed to initialize startup time");
}

/// Return the startup instant
///
/// # Panics
///
/// Panics, if neither [`initialize()`] nor [`initialize_from()`] has been called
pub fn startup_instant() -> std::time::Instant {
    STARTUP_TIME
        .get()
        .expect("failed to get startup instant: not initialized")
        .instant
}

/// Return a SyncInfo object that can be used by another host to initialize using [`initialize_from()`]
///
/// # Panics
///
/// Panics, if neither [`initialize()`] nor [`initialize_from()`] has been called
pub fn sync_info() -> SyncInfo {
    let since_epoch = STARTUP_TIME
        .get()
        .expect("failed to get sync info: not initialized")
        .systime
        .duration_since(std::time::UNIX_EPOCH)
        .expect("failed to obtain system time for synchronization");
    SyncInfo { since_epoch }
}

/// A timestamp: Duration since system startup
#[cfg_attr(feature = "recording", derive(Serialize, Deserialize))]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct Timestamp(pub feo_time::Duration);

pub fn timestamp() -> Timestamp {
    // get real time duration since startup and scale it with feo-time speed factor
    let real_duration = std::time::Instant::now().duration_since(startup_instant());
    let feo_duration: feo_time::Duration = real_duration.scaled();
    Timestamp(feo_duration)
}

#[cfg(feature = "recording")]
impl MaxSize for Timestamp {
    const POSTCARD_MAX_SIZE: usize = u64::POSTCARD_MAX_SIZE + u32::POSTCARD_MAX_SIZE;
}

/// Synchronization information
///
/// For now, synchronization information is the startup time (UTC) on the primary agent as
/// the duration since the EPOCH. That means, secondary agents synchronizing later based on
/// that value might get affected by leap seconds occurring in between.
#[cfg_attr(feature = "recording", derive(Serialize, Deserialize))]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct SyncInfo {
    since_epoch: std::time::Duration,
}

/// Return current system time and instant as a TimeInfo object  
fn time_info_now() -> TimeInfo {
    let mut tries_remaining: i32 = MAX_TRIES;
    loop {
        // Get system time and corresponding instant
        let instant = std::time::Instant::now();
        let systime = std::time::SystemTime::now();
        let instant2 = std::time::Instant::now();

        // If duration between both instances is less than the maximum allowed delay,
        // return info
        if instant2.saturating_duration_since(instant) < MAX_DELAY {
            return TimeInfo { instant, systime };
        }

        tries_remaining -= 1;
        assert!(
            tries_remaining > 0,
            "failed to get synchronized time information"
        );
    }
}

#[cfg(feature = "recording")]
impl MaxSize for SyncInfo {
    const POSTCARD_MAX_SIZE: usize = u64::POSTCARD_MAX_SIZE + u32::POSTCARD_MAX_SIZE;
}

impl From<SyncInfo> for u128 {
    fn from(info: SyncInfo) -> u128 {
        info.since_epoch.as_nanos()
    }
}

impl From<SyncInfo> for u64 {
    fn from(info: SyncInfo) -> u64 {
        let nanos = info.since_epoch.as_nanos();
        assert!(nanos <= u64::MAX.into(), "input value too large");
        nanos as u64
    }
}

impl From<u128> for SyncInfo {
    fn from(nanos: u128) -> SyncInfo {
        assert!(nanos <= u64::MAX.into(), "input value too large");
        SyncInfo {
            since_epoch: std::time::Duration::from_nanos(nanos as u64),
        }
    }
}

impl From<u64> for SyncInfo {
    fn from(nanos: u64) -> SyncInfo {
        SyncInfo {
            since_epoch: std::time::Duration::from_nanos(nanos),
        }
    }
}

impl From<Timestamp> for u128 {
    fn from(tstamp: Timestamp) -> u128 {
        tstamp.0.as_nanos()
    }
}

impl From<Timestamp> for u64 {
    fn from(tstamp: Timestamp) -> u64 {
        let nanos = tstamp.0.as_nanos();
        assert!(nanos <= u64::MAX.into(), "input value too large");
        nanos as u64
    }
}

impl From<u128> for Timestamp {
    fn from(nanos: u128) -> Timestamp {
        assert!(nanos <= u64::MAX.into(), "input value too large");
        Timestamp(feo_time::Duration::from_nanos(nanos as u64))
    }
}

impl From<u64> for Timestamp {
    fn from(nanos: u64) -> Timestamp {
        Timestamp(feo_time::Duration::from_nanos(nanos))
    }
}

#[cfg(test)]
mod test {
    #[cfg(feature = "recording")]
    use super::{MaxSize, Timestamp};

    #[cfg(feature = "recording")]
    #[test]
    fn test_max_size_for_timestamp() {
        let time_stamp = Timestamp(feo_time::Duration::MAX);
        let mut buf = [0u8; Timestamp::POSTCARD_MAX_SIZE];
        postcard::to_slice(&time_stamp, &mut buf).expect("should fit");
    }
}
