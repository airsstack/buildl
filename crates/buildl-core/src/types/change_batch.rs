//! The paths one burst of file events named.
//!
//! Its own file because a batch is a different concept from a single path: the watcher acts on
//! a burst as one unit, and a burst that names no path is not a burst at all.
//!
//! Responsibilities: [`ChangeBatch`], its [`ChangeBatch::new`] constructor, and reading it in
//! order.
//!
//! Non-responsibilities: collecting the events, and deciding what a path means to any target.

use std::collections::BTreeSet;

use crate::types::ChangedPath;

/// A non-empty set of changed paths, ordered by their bytes and holding each path once.
///
/// The least path is held apart from the rest so that a batch can name it without a fallible
/// lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeBatch {
    least: ChangedPath,
    rest: BTreeSet<ChangedPath>,
}

impl ChangeBatch {
    /// Wraps `paths`, or returns `None` when it holds none.
    #[must_use]
    pub fn new(mut paths: BTreeSet<ChangedPath>) -> Option<Self> {
        let least = paths.pop_first()?;
        Some(Self { least, rest: paths })
    }

    /// The least path of the batch.
    #[must_use]
    pub const fn first(&self) -> &ChangedPath {
        &self.least
    }

    /// How many paths the batch holds; always at least one.
    #[must_use]
    #[expect(
        clippy::len_without_is_empty,
        reason = "a batch is never empty, so an `is_empty` would always answer false"
    )]
    pub fn len(&self) -> usize {
        1 + self.rest.len()
    }

    /// The paths, least first.
    pub fn iter(&self) -> impl Iterator<Item = &ChangedPath> {
        core::iter::once(&self.least).chain(&self.rest)
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeSet;

    use super::ChangeBatch;
    use crate::types::ChangedPath;

    #[test]
    fn an_empty_set_makes_no_batch() {
        assert!(ChangeBatch::new(BTreeSet::new()).is_none());
    }

    #[test]
    fn a_batch_is_ordered_and_first_is_the_least_path() {
        let mut set = BTreeSet::new();
        set.insert(ChangedPath::parse("b").unwrap());
        set.insert(ChangedPath::parse("a").unwrap());
        let batch = ChangeBatch::new(set).unwrap();
        let seen: Vec<&str> = batch.iter().map(ChangedPath::as_str).collect();
        assert_eq!(seen, ["a", "b"]);
        assert_eq!(batch.first().as_str(), "a");
        assert_eq!(batch.len(), 2);
    }
}
