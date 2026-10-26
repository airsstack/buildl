//! What a build tells the person running it about the remote cache.
//!
//! Its own file because a remote failure does not fail a lenient build: it is a warning, and the
//! warning is a value the phase that executes hands to the reporter, which decides the words.
//!
//! Responsibilities: [`RemoteCall`] — the calls a build makes to a remote — and [`RemoteNotice`].
//!
//! Non-responsibilities: rendering a notice, which the reporter does.

use core::fmt;

use crate::types::{Diagnostic, Label, RootPath};

/// One of the calls a build makes to a remote cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RemoteCall {
    /// Asking the server what it supports.
    Capabilities,
    /// Asking for the result stored for an action.
    Lookup,
    /// Fetching what a result names.
    Download,
    /// Storing a result and what it names.
    Upload,
    /// Asking the server to run an action.
    Execute,
}

impl fmt::Display for RemoteCall {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Capabilities => "capabilities",
            Self::Lookup => "lookup",
            Self::Download => "download",
            Self::Upload => "upload",
            Self::Execute => "execute",
        })
    }
}

/// Something about the remote cache a build tells the person running it.
///
/// A failure is said once per command, because it turns the remote off; a changed input is said
/// once per target it affects, because the remote stays on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteNotice {
    /// A call failed.
    Failed {
        /// The call that failed.
        call: RemoteCall,
        /// What went wrong, for display.
        diagnostic: Diagnostic,
    },
    /// A target's result was not uploaded because one of its input files changed after its
    /// digest was taken.
    InputChanged {
        /// The target that ran.
        label: Label,
        /// The input file that changed.
        path: RootPath,
    },
}

#[cfg(test)]
mod tests {
    use super::RemoteCall;

    #[test]
    fn each_call_renders_as_its_lowercase_name() {
        let names: Vec<String> = [
            RemoteCall::Capabilities,
            RemoteCall::Lookup,
            RemoteCall::Download,
            RemoteCall::Upload,
        ]
        .iter()
        .map(ToString::to_string)
        .collect();
        assert_eq!(names, ["capabilities", "lookup", "download", "upload"]);
    }

    #[test]
    fn execute_is_shown_as_execute() {
        assert_eq!(RemoteCall::Execute.to_string(), "execute");
    }
}
