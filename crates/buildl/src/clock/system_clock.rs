//! The system clock as a build reads it.
//!
//! Its own file because it is the only place in the crate where the wall clock is read.
//!
//! Responsibilities: [`SystemClock`].

use std::time::{SystemTime, UNIX_EPOCH};

use buildl_core::{Clock, Timestamp};

/// Reads the host wall clock.
///
/// A clock set before 1970 reads as the epoch, and one past the year 2554, where a `u64` of
/// nanoseconds ends, reads as that last instant.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        let since = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        Timestamp::from_unix_nanos(u64::try_from(since.as_nanos()).unwrap_or(u64::MAX))
    }
}

#[cfg(test)]
mod tests {
    use buildl_core::{Clock, Timestamp};

    use super::SystemClock;

    #[test]
    fn the_clock_reads_after_the_epoch_and_does_not_run_backwards_between_two_reads() {
        let clock = SystemClock;
        let first = clock.now();
        let second = clock.now();
        assert!(first > Timestamp::from_unix_nanos(0));
        assert!(second.duration_since(first).is_some());
    }
}
