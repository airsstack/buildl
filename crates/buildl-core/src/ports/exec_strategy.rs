//! The port through which a build runs one action.
//!
//! Its own file because running a command means making a directory, starting a process and
//! reading what it left behind, and this crate does none of those: it hands over an action whose
//! command and paths are already settled, and learns how the run ended.
//!
//! Responsibilities: [`ExecStrategy`].
//!
//! Non-responsibilities: implementing it, and deciding what an ending means. The first belongs
//! to an adapter crate, which also chooses how strongly an action is isolated; the second to the
//! phase that runs actions.
//!
//! This file holds only a trait definition, so it carries no logic to unit-test.

use crate::error::Result;
use crate::ports::ContentStore;
use crate::types::{Digest, RawOutcome, ReadyAction};

/// Runs one action in a directory of its own, and stores what the action produced.
///
/// The three methods are called in order for one action, by the same caller: [`prepare`], then
/// [`run`], then [`collect`]. `Send + Sync` because several actions run at once, each on its own
/// thread.
///
/// [`prepare`]: ExecStrategy::prepare
/// [`run`]: ExecStrategy::run
/// [`collect`]: ExecStrategy::collect
pub trait ExecStrategy: Send + Sync {
    /// The directory one action runs in, as this strategy knows it.
    type Dir;

    /// Makes a directory to run `action` in, where every path of its `inputs` and `deps` is
    /// readable at that relative path.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Adapter`](crate::Error::Adapter), naming
    /// [`PortName::ExecStrategy`](crate::PortName::ExecStrategy), when the directory cannot be
    /// made.
    fn prepare(&self, action: &ReadyAction) -> Result<Self::Dir>;

    /// Runs the action's `command` in `dir` as an argument vector, with no shell, with the
    /// variables of its `env` and none of the host's own; waits, and returns how the command
    /// ended with what it printed.
    ///
    /// A command that ran and failed is not an error: it is an outcome with a non-zero status.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Adapter`](crate::Error::Adapter), naming
    /// [`PortName::ExecStrategy`](crate::PortName::ExecStrategy), when the command cannot be
    /// started.
    fn run(&self, dir: &Self::Dir, action: &ReadyAction) -> Result<RawOutcome>;

    /// Stores into `cas` what the action left at each path of its `outputs`, and returns one
    /// entry per path, in order: the digest of what was there, or `None` when nothing was.
    ///
    /// # Errors
    ///
    /// Returns whatever `cas` returns, and [`Error::Adapter`](crate::Error::Adapter), naming
    /// [`PortName::ExecStrategy`](crate::PortName::ExecStrategy), when an output cannot be read.
    fn collect<C: ContentStore>(
        &self,
        dir: &Self::Dir,
        action: &ReadyAction,
        cas: &C,
    ) -> Result<Vec<Option<Digest>>>;
}
