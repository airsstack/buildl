//! What a verify found for one target, and how a re-run differed from the cache when it did.
//!
//! Its own file because a check is the answer a verify gives per target: the re-run reproduced
//! the cached outputs, or it differed, failed, or could not be made. A target whose re-run
//! differs is a result and not an error, so one mismatch does not hide what the rest of the
//! verify found.
//!
//! Responsibilities: [`Check`], [`OutputDifference`], [`FileChange`] and [`EntryChange`].
//!
//! Non-responsibilities: making a check, which the `verify` module does; and rendering one
//! for a person, which belongs to whoever shows it.
//!
//! These are type definitions with no logic of their own; the one test below checks that they, and
//! a verification holding them, can be handed to another thread.

use crate::types::{Diagnostic, Label, OutcomeClass, OutputName, TreePath};

/// How a file output differed between the cache and the re-run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileChange {
    /// The two files hold different bytes.
    Bytes,
    /// The two files hold the same bytes and differ in their mode.
    Mode,
}

/// How one entry of a tree output differed between the cache and the re-run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryChange {
    /// The entry holds different bytes in the two.
    Bytes,
    /// The entry holds the same bytes in the two and differs in its mode.
    Mode,
    /// Only the cache holds the entry.
    OnlyCached,
    /// Only the re-run holds the entry.
    OnlyRerun,
}

/// The first way an output differed between the cache and the re-run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputDifference {
    /// One holds a file where the other holds a tree.
    Kind,
    /// The output is a file in both, and the files differ.
    File(FileChange),
    /// The output is a tree in both, and this entry differs.
    Entry {
        /// The entry's path inside the tree.
        path: TreePath,
        /// How the entry differs.
        change: EntryChange,
    },
}

/// What a verify found for one target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Check {
    /// The re-run reproduced every cached output.
    Verified,
    /// The re-run ran, and one of its outputs differs from the cache's.
    Mismatched {
        /// The first output, in declaration order, that differs.
        output: OutputName,
        /// How that output differs.
        difference: OutputDifference,
    },
    /// The re-run did not end in outputs to compare.
    Failed {
        /// How the re-run ended.
        class: OutcomeClass,
        /// The builder's message, when it gave one.
        diagnostic: Option<Diagnostic>,
        /// What the action wrote to its standard output.
        stdout: Vec<u8>,
        /// What the action wrote to its standard error.
        stderr: Vec<u8>,
    },
    /// The re-run did not begin, because a target it needs was not put back in place: nothing was
    /// cached for it, or putting it back failed.
    Skipped {
        /// The target that was not put back in place.
        because: Label,
    },
    /// The cached outputs could not be put back, so there was nothing to compare against.
    Unplaced {
        /// The builder's message for the failure.
        diagnostic: Diagnostic,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Verification;

    /// Stands for handing a value to another thread: it must be `Send` and borrow nothing.
    fn handed_over<T: Send + 'static>(value: T) -> T {
        value
    }

    #[test]
    fn a_check_and_a_verification_can_be_handed_to_another_thread() {
        let check = handed_over(Check::Failed {
            class: OutcomeClass::ActionFailure { status: 1 },
            diagnostic: Some(Diagnostic::new("boom")),
            stdout: b"out".to_vec(),
            stderr: b"err".to_vec(),
        });
        assert!(matches!(check, Check::Failed { .. }));
        let difference = handed_over(OutputDifference::File(FileChange::Mode));
        assert_eq!(difference, OutputDifference::File(FileChange::Mode));
        assert_eq!(handed_over(EntryChange::OnlyRerun), EntryChange::OnlyRerun);
        let verification = handed_over(Verification::new(Vec::new(), 1, 2));
        assert_eq!(verification.not_cached(), 1);
    }
}
