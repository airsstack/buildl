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
use crate::types::{Found, LiveSink, RawOutcome, ReadyAction};

/// Runs one action in a directory of its own, says what the action left at its output paths,
/// and stores the regular files among it.
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
    /// readable at that relative path. A dependency's output that is a directory is there whole,
    /// with every file below it.
    ///
    /// Each path of its `outputs` is left ready to be written: the directory that will contain
    /// it exists, and nothing exists at the path itself — no file, no directory, and nothing an
    /// earlier run of the same target left there. The command creates what it produces, a file
    /// or a directory; one that writes below an output path makes the directory at that path
    /// first.
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
    /// The command starts with its output paths as [`prepare`](ExecStrategy::prepare) left
    /// them, and nothing but the command writes there before
    /// [`collect`](ExecStrategy::collect) looks.
    ///
    /// A command that ran and failed is not an error: it is an outcome with a non-zero status,
    /// or with the signal that ended it.
    ///
    /// With `live`, every run of bytes the command writes is also handed to it as it is read,
    /// with the output it was written to, before the command has ended. `live` may be called
    /// from any thread, until this call returns. What is returned is the same with or without
    /// it: for each of the two outputs, the bytes handed to `live`, joined in the order they
    /// were handed over, are exactly the bytes of that output in the result.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Adapter`](crate::Error::Adapter), naming
    /// [`PortName::ExecStrategy`](crate::PortName::ExecStrategy), when the command cannot be
    /// started.
    fn run(
        &self,
        dir: &Self::Dir,
        action: &ReadyAction,
        live: Option<LiveSink<'_>>,
    ) -> Result<RawOutcome>;

    /// Says what the action left at each path of its `outputs`, one entry per path, in order,
    /// and stores into `cas` the bytes of every regular file it reports.
    ///
    /// Nothing was at an output path when the command started, so what is found there is what
    /// this run of the command made, and a path the command did not write is reported as
    /// [`Found::Nothing`].
    ///
    /// A regular file is reported with its digest and whether it may be run. A directory is
    /// reported with everything below it, in any order: each regular file, and each thing that
    /// is not one. No symbolic link is followed, at an output path or below it, and nothing is
    /// left out because it cannot be kept: what a report means for the build is decided from
    /// it, not by the strategy.
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
    ) -> Result<Vec<Found>>;
}
