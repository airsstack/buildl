//! The port through which a build has several things done at once.
//!
//! Its own file because doing work in parallel means starting threads, and this crate starts
//! none: it hands over what to do with one task and what to do with one result, and an adapter
//! decides how many tasks are worked on at a time.
//!
//! Responsibilities: [`Dispatcher`].
//!
//! Non-responsibilities: implementing it, and knowing what a task is. The first belongs to an
//! adapter crate; the port is generic over the task and the result, so it knows nothing of
//! targets or actions.
//!
//! This file holds only a trait definition, so it carries no logic to unit-test.

use crate::error::Result;

/// Runs tasks, possibly several at once, and hands each result back one at a time.
pub trait Dispatcher {
    /// Runs every task `step` yields through `work`, until none is left.
    ///
    /// `step(None)` is called once, first, and each task it returns is run through `work`. Each
    /// result is handed to `step(Some(result))`, one at a time, and the tasks that returns are run
    /// in turn. The call returns when no task is running and none is waiting.
    ///
    /// `step` is always called on the calling thread. `work` may be called on any thread, several
    /// calls at once. Every task `step` returns is run, and every result reaches `step`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Adapter`](crate::Error::Adapter), naming
    /// [`PortName::Dispatcher`](crate::PortName::Dispatcher), for the adapter's own failure.
    /// Neither `work` nor `step` can fail here: a task that fails says so in its result.
    fn dispatch<T, R, W, S>(&mut self, work: W, step: S) -> Result<()>
    where
        T: Send,
        R: Send,
        W: Fn(T) -> R + Sync,
        S: FnMut(Option<R>) -> Vec<T>;
}
