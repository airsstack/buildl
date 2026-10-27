//! What a remote cache answered when asked to run an action.
//!
//! Its own file because the answer comes from another machine and is trusted for nothing, as a
//! looked-up result is: the phase that executes checks a [`RemoteHit`] before it places a file,
//! and decides what each other answer means for the target.
//!
//! Responsibilities: [`Executed`].
//!
//! Non-responsibilities: sending the action, which the remote adapter does, and judging the
//! answer, which the phase that executes does.

use crate::types::{Blob, RemoteHit, RootPath};

/// How a remote ended a request to run an action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Executed {
    /// The server ran the action, and this is its result as it answered, unchecked. The exit
    /// code may be other than zero.
    Ran(RemoteHit),
    /// The server held a result for the action already and did not run it, unchecked.
    Cached(RemoteHit),
    /// The server gave up on the action after its time limit.
    ///
    /// Its logs are optional because a server need not send a result along with an error
    /// status.
    TimedOut {
        /// What the action printed on its standard output before it was ended, when the server
        /// kept it.
        stdout: Option<Blob>,
        /// What the action printed on its standard error before it was ended, when the server
        /// kept it.
        stderr: Option<Blob>,
    },
    /// An input file changed after its digest was taken, so the action was not sent.
    InputChanged(RootPath),
}
