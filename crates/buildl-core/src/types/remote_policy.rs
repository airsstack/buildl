//! What a run does with the remote cache, and what it does when the remote fails.
//!
//! Its own file because these are two independent choices a run makes, and each is a closed set
//! the scheduler matches on. Enums, not flags, so a call site says what it means.
//!
//! Responsibilities: [`RemoteUse`], [`RemoteFailure`] and [`RemotePolicy`], which pairs them.
//!
//! Non-responsibilities: reading the switches that choose them, which is the composition
//! root's; and acting on them, which is the phase that executes.

/// How much of the remote cache a run uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RemoteUse {
    /// The remote is never consulted.
    Off,
    /// Results are looked up and downloaded, and nothing is uploaded.
    Read,
    /// Results are looked up and downloaded, and what a run produced is uploaded.
    ReadWrite,
}

/// What a run does when a call to the remote fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RemoteFailure {
    /// Warn once, run without the remote from then on.
    Lenient,
    /// Fail the build.
    Strict,
}

/// The two choices a run makes about the remote cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RemotePolicy {
    /// How much of the remote the run uses.
    pub use_: RemoteUse,
    /// What a failed call does.
    pub failure: RemoteFailure,
}

impl RemotePolicy {
    /// The policy of a run that never consults the remote.
    pub const OFF: Self = Self {
        use_: RemoteUse::Off,
        failure: RemoteFailure::Lenient,
    };
}

#[cfg(test)]
mod tests {
    use super::{RemoteFailure, RemotePolicy, RemoteUse};

    #[test]
    fn the_off_policy_reads_and_writes_nothing() {
        assert_eq!(RemotePolicy::OFF.use_, RemoteUse::Off);
        assert_eq!(RemotePolicy::OFF.failure, RemoteFailure::Lenient);
    }
}
