//! The ignore rules of a build that has none.
//!
//! Its own file because every bundle and test that is not about ignoring names it, and a pure
//! port that holds nothing belongs in the crate that declares the port.
//!
//! Responsibilities: [`NothingIgnored`].
//!
//! Non-responsibilities: reading real ignore files, which an adapter does.

use crate::error::Result;
use crate::ports::IgnoreRules;
use crate::types::{EntryKind, Ignoring};

/// Ignore rules under which no path is ignored.
///
/// Every question is answered with `None`, so a build that has no ignore files behaves as one
/// that was never offered any.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NothingIgnored;

impl IgnoreRules for NothingIgnored {
    fn ignored(&self, _path: &str, _kind: EntryKind) -> Result<Option<Ignoring>> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use super::NothingIgnored;
    use crate::ports::IgnoreRules;
    use crate::types::EntryKind;

    #[test]
    fn nothing_ignored_ignores_nothing() {
        for kind in [EntryKind::File, EntryKind::Folder, EntryKind::Unknown] {
            assert_eq!(NothingIgnored.ignored("notes/a.md", kind).unwrap(), None);
        }
    }
}
