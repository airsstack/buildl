//! The port through which a build asks whether a workspace path is ignored.
//!
//! Its own file because an ignore file is read from the workspace and its patterns matched
//! against a path, work this crate cannot do. What an answer means for a declaration stays with
//! the phase that asks.
//!
//! Responsibilities: [`IgnoreRules`].
//!
//! Non-responsibilities: implementing it. The real implementation belongs in an adapter crate.
//!
//! This file holds only a trait definition, so it carries no logic to unit-test.

use crate::error::Result;
use crate::types::{EntryKind, Ignoring};

/// The ignore files of a workspace, asked about one path at a time.
///
/// Called from several threads at once, because the build files are evaluated in parallel.
pub trait IgnoreRules: Sync {
    /// The pattern that ignores `path`, or `None` when nothing does.
    ///
    /// `path` is workspace-relative and `/`-separated, and never empty: the workspace root itself
    /// is never asked. `kind` says what the asker knows the path to be, because a pattern ending
    /// in `/` matches folders only. The decision is made from the root down, as git makes it:
    /// each folder on the way to `path`, then `path` itself, is matched in turn. For one step, a
    /// deeper ignore file outranks a shallower one, and within a file the last matching line
    /// wins; a matching `!` line keeps the step. The first step that ends ignored decides, so
    /// nothing under an ignored folder can be kept again.
    ///
    /// # Errors
    ///
    /// Returns [`Error::IgnoreFile`](crate::Error::IgnoreFile) when an ignore file that bears on
    /// the path cannot be used.
    fn ignored(&self, path: &str, kind: EntryKind) -> Result<Option<Ignoring>>;
}
