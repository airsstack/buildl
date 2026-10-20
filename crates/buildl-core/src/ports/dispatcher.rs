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
use crate::types::Turn;

/// Runs tasks, possibly several at once, and hands each result back one at a time.
pub trait Dispatcher {
    /// Runs every task `step` yields through `work`, until none is left.
    ///
    /// `step(Turn::Begin)` is called once, first, and each task it returns is run through
    /// `work`. Each result is handed to `step(Turn::Done(result))`, one at a time, and the
    /// tasks that returns are run in turn. The call returns when no task is running and none is
    /// waiting.
    ///
    /// `work` is handed, with its task, a way to tell `step` something while the task is still
    /// running. Each message it is given reaches `step` as `Turn::Told(message)`: the messages
    /// of one task in the order that task sent them, and all of them before that task's result.
    /// Messages of different tasks reach `step` in any order. The tasks `step` returns for a
    /// message are run like any other.
    ///
    /// `step` is always called on the calling thread. `work` may be called on any thread, several
    /// calls at once, and what it is handed to tell with may be called from any thread until
    /// `work` returns. Every task `step` returns is run, and every message and every result
    /// reaches `step`. None can happen twice, and none can be made up: a task, a message and a
    /// result are each handed over by value, and the port asks for no way to copy or to build
    /// one. A dispatcher that loses a task or its result breaks the contract: a build over it
    /// fails with [`Error::DispatcherFault`](crate::Error::DispatcherFault), and a load of build
    /// files over it with [`Error::LostBuildFiles`](crate::Error::LostBuildFiles), instead of
    /// reporting success.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Adapter`](crate::Error::Adapter), naming
    /// [`PortName::Dispatcher`](crate::PortName::Dispatcher), for the adapter's own failure.
    /// Neither `work` nor `step` can fail here: a task that fails says so in its result.
    fn dispatch<T, M, R, W, S>(&mut self, work: W, step: S) -> Result<()>
    where
        T: Send,
        M: Send,
        R: Send,
        W: Fn(T, &(dyn Fn(M) + Sync)) -> R + Sync,
        S: FnMut(Turn<M, R>) -> Vec<T>;
}
