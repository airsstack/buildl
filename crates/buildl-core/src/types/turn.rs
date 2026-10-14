//! One turn of the thread that schedules a build: what a dispatcher hands it each time.
//!
//! Its own file because it is the value that crosses the dispatching port towards the caller:
//! a build is begun once, is told things by the tasks that are running, and is handed the result
//! of each task when it is done.
//!
//! Responsibilities: [`Turn`].
//!
//! Non-responsibilities: what a message or a result is. The port is generic over both.
//!
//! This file holds only a type definition, so it carries no logic to unit-test.

/// What a dispatcher hands the calling thread: the start, a message from a running task, or the
/// result of a finished one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Turn<M, R> {
    /// Nothing has run yet. Handed over once, first.
    Begin,
    /// A task that is running said this.
    Told(M),
    /// A task is done, with this result.
    Done(R),
}
