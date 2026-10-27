//! What a run does with the remote cache: how much it uses, whether it runs actions there, and
//! what it does when the remote fails.
//!
//! Its own file because these are independent choices a run makes, and each is a closed set
//! the scheduler matches on. Enums, not flags, so a call site says what it means.
//!
//! Responsibilities: [`RemoteUse`], [`RemoteFailure`], [`RemoteExecution`] and [`RemotePolicy`],
//! which gathers them.
//!
//! Non-responsibilities: reading the switches that choose them, which is the composition
//! root's; and acting on them, which is the phase that executes.

use core::num::NonZeroUsize;

use crate::types::RemoteJobs;

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

/// Whether a run asks the remote to run the actions its cache cannot answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RemoteExecution {
    /// Actions that miss are run here.
    Off,
    /// Actions that miss are sent to the remote to run.
    On {
        /// How many actions may be with the remote at once.
        jobs: RemoteJobs,
        /// How many actions may run here at once.
        local: NonZeroUsize,
    },
}

/// The choices a run makes about the remote cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RemotePolicy {
    /// How much of the remote the run uses.
    pub use_: RemoteUse,
    /// What a failed call does.
    pub failure: RemoteFailure,
    /// Whether the remote runs the actions that miss.
    pub execution: RemoteExecution,
}

impl RemotePolicy {
    /// The policy of a run that never consults the remote.
    pub const OFF: Self = Self {
        use_: RemoteUse::Off,
        failure: RemoteFailure::Lenient,
        execution: RemoteExecution::Off,
    };
}

#[cfg(test)]
mod tests {
    use super::{RemoteExecution, RemoteFailure, RemotePolicy, RemoteUse};

    #[test]
    fn the_off_policy_reads_and_writes_nothing() {
        assert_eq!(RemotePolicy::OFF.use_, RemoteUse::Off);
        assert_eq!(RemotePolicy::OFF.failure, RemoteFailure::Lenient);
    }

    #[test]
    fn the_off_policy_executes_nothing() {
        assert_eq!(RemotePolicy::OFF.execution, RemoteExecution::Off);
    }
}
