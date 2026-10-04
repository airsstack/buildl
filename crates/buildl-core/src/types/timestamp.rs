//! An instant on the host wall clock, as a nanosecond count since the Unix epoch.
//!
//! Its own file because it is the only value in this crate that comes from outside the build
//! graph, and the only one a port must hand in: `std::time::Instant` and `SystemTime` are banned
//! here, so a duration can only be the difference of two of these.
//!
//! Responsibilities: [`Timestamp`], its epoch constant, and the span between two instants.
//!
//! Non-responsibilities: reading the clock, and formatting. Reading is the clock port's; a
//! human-readable rendering belongs to whichever adapter shows it, where a date library may be
//! taken.

use core::time::Duration;

use serde::{Deserialize, Serialize};

/// An instant on the host wall clock, in nanoseconds since 1970-01-01T00:00:00Z.
///
/// Nanoseconds rather than seconds because the same clock feeds action durations, and second
/// resolution would measure every action shorter than a second as zero.
///
/// Unrelated to `SOURCE_DATE_EPOCH`, which is a whole-second value the sandbox hands to an
/// action so *its* output is reproducible. A `Timestamp` is never an action-key component — it
/// is precisely the value that must not affect a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Timestamp(u64);

impl Timestamp {
    /// 1970-01-01T00:00:00Z.
    pub const UNIX_EPOCH: Self = Self(0);

    /// Wraps a nanosecond count since the Unix epoch.
    #[must_use]
    pub const fn from_unix_nanos(nanos: u64) -> Self {
        Self(nanos)
    }

    /// The nanosecond count since the Unix epoch.
    #[must_use]
    pub const fn as_unix_nanos(self) -> u64 {
        self.0
    }

    /// The span from `earlier` to this instant, or `None` when `earlier` is the later of the two.
    ///
    /// `None` rather than a panic or a zero: the wall clock can go backwards between two reads,
    /// and a caller that cares needs to be able to tell that apart from no elapsed time.
    #[must_use]
    pub fn duration_since(self, earlier: Self) -> Option<Duration> {
        self.0.checked_sub(earlier.0).map(Duration::from_nanos)
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use super::Timestamp;

    #[test]
    fn carries_the_nanosecond_count_it_was_given() {
        assert_eq!(
            Timestamp::from_unix_nanos(1_758_412_800_123_456_789).as_unix_nanos(),
            1_758_412_800_123_456_789
        );
        assert_eq!(Timestamp::UNIX_EPOCH.as_unix_nanos(), 0);
    }

    #[test]
    fn measures_the_span_between_two_instants() {
        let early = Timestamp::from_unix_nanos(1_000);
        let late = Timestamp::from_unix_nanos(3_500);
        assert_eq!(late.duration_since(early).unwrap().as_nanos(), 2_500);
        assert_eq!(early.duration_since(early).unwrap().as_nanos(), 0);
    }

    #[test]
    fn reports_no_span_when_the_clock_went_backwards() {
        let early = Timestamp::from_unix_nanos(1_000);
        let late = Timestamp::from_unix_nanos(3_500);
        assert!(early.duration_since(late).is_none());
    }

    #[test]
    fn serializes_as_a_bare_number() {
        let json = serde_json::to_string(&Timestamp::from_unix_nanos(42)).unwrap();
        assert_eq!(json, "42");
        assert_eq!(
            serde_json::from_str::<Timestamp>(&json).unwrap(),
            Timestamp::from_unix_nanos(42)
        );
    }

    #[test]
    fn orders_chronologically() {
        let mut stamps = [
            Timestamp::from_unix_nanos(30),
            Timestamp::UNIX_EPOCH,
            Timestamp::from_unix_nanos(10),
        ];
        stamps.sort();
        assert_eq!(
            stamps,
            [
                Timestamp::UNIX_EPOCH,
                Timestamp::from_unix_nanos(10),
                Timestamp::from_unix_nanos(30)
            ]
        );
    }
}
