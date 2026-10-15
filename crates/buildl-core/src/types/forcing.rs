//! What makes one target run whatever its row says.
//!
//! Its own file because Plan and Execute both decide whether a target runs, and they must agree
//! for a target to run exactly once: both ask this one function, so a target Plan calls dirty
//! for a rerun is not served from its row at Execute.
//!
//! Responsibilities: [`Forcing`] and [`Forcing::of`].
//!
//! Non-responsibilities: what a target does when nothing forces it, which is the comparison of
//! its key with its row, made by the phase that decides.

use crate::types::{Freshness, Rerun, TargetRole};

/// Why a target runs even though a matching row may exist, if anything does.
///
/// Ranked: [`Always`](Self::Always) outranks [`Rerun`](Self::Rerun), which outranks the rules
/// that read the row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Forcing {
    /// Nothing forces it: its row decides.
    None,
    /// It is declared to run on every build.
    Always,
    /// It is a test, and the run asked for its tests to run again.
    Rerun,
}

impl Forcing {
    /// What forces a target declared with `freshness` and `role` in a run that asks for `rerun`.
    ///
    /// A target declared to always run stays [`Always`](Self::Always) under a rerun. A test is
    /// forced by a [`Rerun::Tests`] and nothing else is.
    pub(crate) const fn of(freshness: Freshness, role: TargetRole, rerun: Rerun) -> Self {
        match (freshness, role, rerun) {
            (Freshness::Always, _, _) => Self::Always,
            (Freshness::Cached, TargetRole::Test, Rerun::Tests) => Self::Rerun,
            (Freshness::Cached, _, _) => Self::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Forcing;
    use crate::types::{Freshness, Rerun, TargetRole};

    #[test]
    fn always_outranks_a_rerun() {
        assert_eq!(
            Forcing::of(Freshness::Always, TargetRole::Test, Rerun::Tests),
            Forcing::Always
        );
        assert_eq!(
            Forcing::of(Freshness::Always, TargetRole::Build, Rerun::Off),
            Forcing::Always
        );
    }

    #[test]
    fn a_rerun_forces_only_a_test() {
        assert_eq!(
            Forcing::of(Freshness::Cached, TargetRole::Test, Rerun::Tests),
            Forcing::Rerun
        );
        assert_eq!(
            Forcing::of(Freshness::Cached, TargetRole::Build, Rerun::Tests),
            Forcing::None
        );
    }

    #[test]
    fn without_a_rerun_nothing_cached_is_forced() {
        assert_eq!(
            Forcing::of(Freshness::Cached, TargetRole::Test, Rerun::Off),
            Forcing::None
        );
        assert_eq!(
            Forcing::of(Freshness::Cached, TargetRole::Build, Rerun::Off),
            Forcing::None
        );
    }
}
