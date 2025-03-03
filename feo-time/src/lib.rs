// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! FEO Temporal quantification.
//!
//! This module is borrowed from the Rust standard library licensed unter the
//! Apache License, Version 2.0 and MIT license.
//!
//! # Examples
//!
//! There are multiple ways to create a new [`Duration`]:
//!
//! ```
//! # use std::time::Duration;
//! let five_seconds = Duration::from_secs(5);
//! assert_eq!(five_seconds, Duration::from_millis(5_000));
//! assert_eq!(five_seconds, Duration::from_micros(5_000_000));
//! assert_eq!(five_seconds, Duration::from_nanos(5_000_000_000));
//!
//! let ten_seconds = Duration::from_secs(10);
//! let seven_nanos = Duration::from_nanos(7);
//! let total = ten_seconds + seven_nanos;
//! assert_eq!(total, Duration::new(10, 7));
//! ```
//!
//! Using [`Instant`] to calculate how long a function took to run:
//!
//! ```ignore (incomplete)
//! let now = Instant::now();
//!
//! // Calling a slow function, it may take a while
//! slow_function();
//!
//! let elapsed_time = now.elapsed();
//! println!("Running slow_function() took {} seconds.", elapsed_time.as_secs());
//! ```

mod ffi;
#[cfg(test)]
mod tests;

use std::error::Error;
use std::ops::{Add, AddAssign, Sub, SubAssign};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{LazyLock, Once};
pub use std::time::Duration;
use std::{fmt, time};

/// An anchor in time which can be used to create new `SystemTime` instances or
/// learn about where in time a `SystemTime` lies.
//
// NOTE! this documentation is duplicated, here and in SystemTime::UNIX_EPOCH.
// The two copies are not quite identical, because of the difference in naming.
///
/// This constant is defined to be "1970-01-01 00:00:00 UTC" on all systems with
/// respect to the system clock. Using `duration_since` on an existing
/// [`SystemTime`] instance can tell how far away from this point in time a
/// measurement lies, and using `UNIX_EPOCH + duration` can be used to create a
/// [`SystemTime`] instance to represent another fixed point in time.
///
/// `duration_since(UNIX_EPOCH).unwrap().as_secs()` returns
/// the number of non-leap seconds since the start of 1970 UTC.
/// This is a POSIX `time_t` (as a `u64`),
/// and is the same time representation as used in many Internet protocols.
///
/// # Examples
///
/// ```no_run
/// use std::time::{SystemTime, UNIX_EPOCH};
///
/// match SystemTime::now().duration_since(UNIX_EPOCH) {
///     Ok(n) => println!("1970-01-01 00:00:00 UTC was {} seconds ago!", n.as_secs()),
///     Err(_) => panic!("SystemTime before UNIX EPOCH!"),
/// }
/// ```
pub const UNIX_EPOCH: SystemTime = SystemTime(time::UNIX_EPOCH);

#[derive(Clone, Debug)]
pub struct SystemTimeError(Duration);

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Instant(time::Instant);

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SystemTime(time::SystemTime);

/// Initialization synchronization. Ensures that `speed` can be set only once.
static INIT: Once = Once::new();
/// Time scaling start timestamps
static START: LazyLock<(SystemTime, Instant)> =
    LazyLock::new(|| (SystemTime::now(), Instant::now()));
/// Factor on systemtime and instant if set via `speed`
static FACTOR: AtomicI32 = AtomicI32::new(0);

/// A trait for scaling durations based on the factor set by `speed`.
pub trait Scaled {
    /// Scale the duration based on the factor set by `speed` for using in sleep functions.
    /// Background: std::thread::sleep and friends need a time base on the unscaled system time.
    /// If the factor is set to a positive value the duration must be shortened (shorter sleep).
    /// If the factor is set to a negative value the duration must be lengthened (longer sleep).
    fn scaled(&self) -> Self;
}

/// Set a speedup or down factor on the system time.
pub fn speed(factor: i32) {
    // Ensure that speed can be set only once
    assert!(!INIT.is_completed(), "speed can be set only once");
    INIT.call_once(|| ());

    // Initialize the start timestamps
    let _ = &*START;

    // Store the factor. This is guarded by the `INIT`
    FACTOR.store(factor, Ordering::Relaxed);
}

/// Get the current speed factor if set. Otherwise return None.
pub fn get_speed() -> Option<i32> {
    let factor = FACTOR.load(Ordering::Relaxed);
    (factor != 0).then_some(factor)
}

impl Instant {
    /// Returns an instant corresponding to "now".
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Instant;
    ///
    /// let now = Instant::now();
    /// ```
    #[must_use]
    pub fn now() -> Instant {
        // Get current system time unscaled from the os
        let now = Instant(time::Instant::now());

        // Load the factor set by `SystemTime::speed`
        let factor = FACTOR.load(Ordering::Relaxed);
        if factor != 0 {
            // Load start timestamp
            let start = START.1;

            // Calculate elapsed time since start timestamp
            let duration_since_start = now.duration_since(start);

            // Calculate new "feo" time
            if factor.is_positive() {
                // Factor is greater than 0, so we speed up time by multiplying
                // the elapsed time by factor add add to the start time
                let elapsed = duration_since_start * factor.unsigned_abs();
                start.checked_add(elapsed).expect("clock error")
            } else {
                // Factor is less than 0, so we slow down time by dividing
                // the elapsed time by factor add add to the start time
                let elapsed = duration_since_start / factor.unsigned_abs();
                start.checked_add(elapsed).expect("clock error")
            }
        } else {
            now
        }
    }

    /// Returns the amount of time elapsed from another instant to this one,
    /// or zero duration if that instant is later than this one.
    ///
    /// # Panics
    ///
    /// Previous Rust versions panicked when `earlier` was later than `self`. Currently this
    /// method saturates. Future versions may reintroduce the panic in some circumstances.
    /// See [Monotonicity].
    ///
    /// [Monotonicity]: Instant#monotonicity
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::time::{Duration, Instant};
    /// use std::thread::sleep;
    ///
    /// let now = Instant::now();
    /// sleep(Duration::new(1, 0));
    /// let new_now = Instant::now();
    /// println!("{:?}", new_now.duration_since(now));
    /// println!("{:?}", now.duration_since(new_now)); // 0ns
    /// ```
    #[must_use]
    pub fn duration_since(&self, earlier: Instant) -> Duration {
        self.checked_duration_since(earlier).unwrap_or_default()
    }

    /// Returns the amount of time elapsed from another instant to this one,
    /// or None if that instant is later than this one.
    ///
    /// Due to [monotonicity bugs], even under correct logical ordering of the passed `Instant`s,
    /// this method can return `None`.
    ///
    /// [monotonicity bugs]: Instant#monotonicity
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::time::{Duration, Instant};
    /// use std::thread::sleep;
    ///
    /// let now = Instant::now();
    /// sleep(Duration::new(1, 0));
    /// let new_now = Instant::now();
    /// println!("{:?}", new_now.checked_duration_since(now));
    /// println!("{:?}", now.checked_duration_since(new_now)); // None
    /// ```
    #[must_use]
    pub fn checked_duration_since(&self, earlier: Instant) -> Option<Duration> {
        self.0.checked_duration_since(earlier.0)
    }

    /// Returns the amount of time elapsed from another instant to this one,
    /// or zero duration if that instant is later than this one.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::time::{Duration, Instant};
    /// use std::thread::sleep;
    ///
    /// let now = Instant::now();
    /// sleep(Duration::new(1, 0));
    /// let new_now = Instant::now();
    /// println!("{:?}", new_now.saturating_duration_since(now));
    /// println!("{:?}", now.saturating_duration_since(new_now)); // 0ns
    /// ```
    #[must_use]
    pub fn saturating_duration_since(&self, earlier: Instant) -> Duration {
        self.checked_duration_since(earlier).unwrap_or_default()
    }

    /// Returns the amount of time elapsed since this instant.
    ///
    /// # Panics
    ///
    /// Previous Rust versions panicked when the current time was earlier than self. Currently this
    /// method returns a Duration of zero in that case. Future versions may reintroduce the panic.
    /// See [Monotonicity].
    ///
    /// [Monotonicity]: Instant#monotonicity
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::thread::sleep;
    /// use std::time::{Duration, Instant};
    ///
    /// let instant = Instant::now();
    /// let three_secs = Duration::from_secs(3);
    /// sleep(three_secs);
    /// assert!(instant.elapsed() >= three_secs);
    /// ```
    #[must_use]
    pub fn elapsed(&self) -> Duration {
        Instant::now() - *self
    }

    /// Returns `Some(t)` where `t` is the time `self + duration` if `t` can be represented as
    /// `Instant` (which means it's inside the bounds of the underlying data structure), `None`
    /// otherwise.
    pub fn checked_add(&self, duration: Duration) -> Option<Instant> {
        self.0.checked_add(duration).map(Instant)
    }

    /// Returns `Some(t)` where `t` is the time `self - duration` if `t` can be represented as
    /// `Instant` (which means it's inside the bounds of the underlying data structure), `None`
    /// otherwise.
    pub fn checked_sub(&self, duration: Duration) -> Option<Instant> {
        self.0.checked_sub(duration).map(Instant)
    }
}

impl Add<Duration> for Instant {
    type Output = Instant;

    /// # Panics
    ///
    /// This function may panic if the resulting point in time cannot be represented by the
    /// underlying data structure. See [`Instant::checked_add`] for a version without panic.
    fn add(self, other: Duration) -> Instant {
        self.checked_add(other)
            .expect("overflow when adding duration to instant")
    }
}

impl AddAssign<Duration> for Instant {
    fn add_assign(&mut self, other: Duration) {
        *self = *self + other;
    }
}

impl Sub<Duration> for Instant {
    type Output = Instant;

    fn sub(self, other: Duration) -> Instant {
        self.checked_sub(other)
            .expect("overflow when subtracting duration from instant")
    }
}

impl SubAssign<Duration> for Instant {
    fn sub_assign(&mut self, other: Duration) {
        *self = *self - other;
    }
}

impl Sub<Instant> for Instant {
    type Output = Duration;

    /// Returns the amount of time elapsed from another instant to this one,
    /// or zero duration if that instant is later than this one.
    ///
    /// # Panics
    ///
    /// Previous Rust versions panicked when `other` was later than `self`. Currently this
    /// method saturates. Future versions may reintroduce the panic in some circumstances.
    /// See [Monotonicity].
    ///
    /// [Monotonicity]: Instant#monotonicity
    fn sub(self, other: Instant) -> Duration {
        self.duration_since(other)
    }
}

impl fmt::Debug for Instant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl SystemTime {
    /// An anchor in time which can be used to create new `SystemTime` instances or
    /// learn about where in time a `SystemTime` lies.
    pub const UNIX_EPOCH: SystemTime = UNIX_EPOCH;

    pub fn now() -> SystemTime {
        // Get current system time unscaled from the os
        let now = SystemTime(time::SystemTime::now());

        // Load the factor set by `SystemTime::speed`
        let factor = FACTOR.load(Ordering::Relaxed);

        if factor != 0 {
            // Load start timestamp
            let start = START.0;

            // Calculate elapsed "real" time since start timestamp
            let duration_since_start = now.duration_since(start).unwrap();

            // Calculate new "feo" time
            if factor.is_positive() {
                // Factor is greater than 0, so we speed up time by multiplying
                // the elapsed time by factor add add to the start time
                let elapsed = duration_since_start * factor.unsigned_abs();
                start.checked_add(elapsed).expect("clock error")
            } else {
                // Factor is less than 0, so we slow down time by dividing
                // the elapsed time by factor add add to the start time
                let elapsed = duration_since_start / factor.unsigned_abs();
                start.checked_add(elapsed).expect("clock error")
            }
        } else {
            now
        }
    }

    /// Returns the amount of time elapsed from an earlier point in time.
    ///
    /// This function may fail because measurements taken earlier are not
    /// guaranteed to always be before later measurements (due to anomalies such
    /// as the system clock being adjusted either forwards or backwards).
    /// [`Instant`] can be used to measure elapsed time without this risk of failure.
    ///
    /// If successful, <code>[Ok]\([Duration])</code> is returned where the duration represents
    /// the amount of time elapsed from the specified measurement to this one.
    ///
    /// Returns an [`Err`] if `earlier` is later than `self`, and the error
    /// contains how far from `self` the time is.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::time::SystemTime;
    ///
    /// let sys_time = SystemTime::now();
    /// let new_sys_time = SystemTime::now();
    /// let difference = new_sys_time.duration_since(sys_time)
    ///     .expect("Clock may have gone backwards");
    /// println!("{difference:?}");
    /// ```
    pub fn duration_since(&self, earlier: Self) -> Result<Duration, SystemTimeError> {
        self.0.duration_since(earlier.0).map_err(Into::into)
    }

    /// Returns the difference from this system time to the
    /// current clock time.
    ///
    /// This function may fail as the underlying system clock is susceptible to
    /// drift and updates (e.g., the system clock could go backwards), so this
    /// function might not always succeed. If successful, <code>[Ok]\([Duration])</code> is
    /// returned where the duration represents the amount of time elapsed from
    /// this time measurement to the current time.
    ///
    /// To measure elapsed time reliably, use [`Instant`] instead.
    ///
    /// Returns an [`Err`] if `self` is later than the current system time, and
    /// the error contains how far from the current system time `self` is.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::thread::sleep;
    /// use std::time::{Duration, SystemTime};
    ///
    /// let sys_time = SystemTime::now();
    /// let one_sec = Duration::from_secs(1);
    /// sleep(one_sec);
    /// assert!(sys_time.elapsed().unwrap() >= one_sec);
    /// ```
    pub fn elapsed(&self) -> Result<Duration, SystemTimeError> {
        SystemTime::now().duration_since(*self)
    }

    /// Returns `Some(t)` where `t` is the time `self + duration` if `t` can be represented as
    /// `SystemTime` (which means it's inside the bounds of the underlying data structure), `None`
    /// otherwise.
    pub fn checked_add(&self, duration: Duration) -> Option<SystemTime> {
        self.0.checked_add(duration).map(SystemTime)
    }

    /// Returns `Some(t)` where `t` is the time `self - duration` if `t` can be represented as
    /// `SystemTime` (which means it's inside the bounds of the underlying data structure), `None`
    /// otherwise.
    pub fn checked_sub(&self, duration: Duration) -> Option<SystemTime> {
        self.0.checked_sub(duration).map(SystemTime)
    }
}

impl Add<Duration> for SystemTime {
    type Output = SystemTime;

    /// # Panics
    ///
    /// This function may panic if the resulting point in time cannot be represented by the
    /// underlying data structure. See [`SystemTime::checked_add`] for a version without panic.
    fn add(self, dur: Duration) -> SystemTime {
        SystemTime(self.0.add(dur))
    }
}

impl AddAssign<Duration> for SystemTime {
    fn add_assign(&mut self, other: Duration) {
        *self = *self + other;
    }
}

impl Sub<Duration> for SystemTime {
    type Output = SystemTime;

    fn sub(self, dur: Duration) -> SystemTime {
        SystemTime(self.0.sub(dur))
    }
}

impl SubAssign<Duration> for SystemTime {
    fn sub_assign(&mut self, other: Duration) {
        *self = *self - other;
    }
}

impl fmt::Debug for SystemTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl SystemTimeError {
    /// Returns the positive duration which represents how far forward the
    /// second system time was from the first.
    #[must_use]
    pub fn duration(&self) -> Duration {
        self.0
    }
}

impl fmt::Display for SystemTimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "second time provided was later than self")
    }
}

impl Error for SystemTimeError {
    #[allow(deprecated)]
    fn description(&self) -> &str {
        "other time was not earlier than self"
    }
}

impl From<time::SystemTimeError> for SystemTimeError {
    fn from(e: time::SystemTimeError) -> Self {
        SystemTimeError(e.duration())
    }
}

impl Scaled for Duration {
    fn scaled(&self) -> Self {
        let factor = FACTOR.load(Ordering::Relaxed);
        if factor != 0 {
            if factor.is_positive() {
                // Factor is greater than 0, so we speed up time by dividing
                // the duration by factor
                *self / factor.unsigned_abs()
            } else {
                // Factor is less than 0, so we slow down time by multiplying
                // the duration by factor
                *self * factor.unsigned_abs()
            }
        } else {
            *self
        }
    }
}
