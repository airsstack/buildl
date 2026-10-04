//! The port through which this crate reads the host wall clock.
//!
//! Its own file because the rule it implements is an absence: `now()` is read only for event
//! timestamps and durations, and never by this crate directly. `Instant`, `SystemTime` and
//! `SystemTimeError` are banned here by `crates/buildl-core/clippy.toml`, so the only way a real
//! instant enters is a caller passing an implementation of this trait.
//!
//! Responsibilities: [`Clock`].
//!
//! Non-responsibilities: implementing it. The real adapter lives in the composition root; a
//! fake one lives in this crate's tests.

use crate::types::Timestamp;

/// Reads the host wall clock.
///
/// The only implementation in this crate is a test fake. A real one reads the system clock,
/// which is why it belongs to an adapter rather than here.
pub trait Clock {
    /// The current instant.
    fn now(&self) -> Timestamp;
}

#[cfg(test)]
mod tests {
    use super::Clock;
    use crate::types::Timestamp;

    /// A clock that returns a fixed instant — the shape every test in this crate will use in
    /// place of the host clock.
    struct FixedClock(Timestamp);

    impl Clock for FixedClock {
        fn now(&self) -> Timestamp {
            self.0
        }
    }

    #[test]
    fn a_port_implementation_needs_no_host_clock() {
        let clock = FixedClock(Timestamp::from_unix_nanos(1_758_412_800_123_456_789));
        assert_eq!(clock.now().as_unix_nanos(), 1_758_412_800_123_456_789);
    }

    #[test]
    fn two_reads_of_a_fixed_clock_span_no_time() {
        let clock = FixedClock(Timestamp::from_unix_nanos(42));
        let first = clock.now();
        let second = clock.now();
        assert_eq!(second.duration_since(first).map(|d| d.as_nanos()), Some(0));
    }
}
