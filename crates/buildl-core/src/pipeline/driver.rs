//! The pipeline's driver: one bundle of ports, and one method per command.
//!
//! Its own file because the driver owns the sequence of phases, and the sequence grows by one
//! method per command as later phases arrive. Today it runs Load and Resolve.
//!
//! Responsibilities: [`Pipeline`], [`Pipeline::check`] and [`Pipeline::graph`].
//!
//! Non-responsibilities: the phases' own logic, which each phase's module holds.

use core::cmp::min_by;
use core::fmt;

use crate::error::{Error, Result};
use crate::load::{declaration_order, load};
use crate::ports::Ports;
use crate::resolve::resolve;
use crate::types::{Declaration, EntryName, TargetGraph};

/// The pipeline over one chosen implementation of each port.
pub struct Pipeline<P: Ports> {
    source: P::Source,
}

impl<P: Ports> fmt::Debug for Pipeline<P>
where
    P::Source: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Pipeline")
            .field("source", &self.source)
            .finish()
    }
}

impl<P: Ports> Pipeline<P> {
    /// Assembles a pipeline from its ports.
    #[must_use]
    pub const fn new(source: P::Source) -> Self {
        Self { source }
    }

    /// Loads the workspace twice and returns its declarations when both loads agree.
    ///
    /// Declaration is deterministic by construction only if every build file is; evaluating twice
    /// and comparing is how a nondeterministic one is caught. A `pairs` loop is not
    /// nondeterminism here: the declarations are sorted before they are compared.
    ///
    /// # Errors
    ///
    /// Returns whatever either load returns, and [`Error::Nondeterministic`] when the two loads
    /// declare different things.
    pub fn check(&self, entry: &EntryName) -> Result<Vec<Declaration>> {
        let first = load(&self.source, entry)?;
        let second = load(&self.source, entry)?;
        first_difference(&first, &second).map_or(Ok(first), Err)
    }

    /// Loads the workspace once and resolves its declarations into the target graph.
    ///
    /// The double evaluation that catches a nondeterministic build file is [`Pipeline::check`]'s;
    /// this command loads once.
    ///
    /// # Errors
    ///
    /// Returns whatever the load returns, and otherwise whatever [`resolve`] returns: a name
    /// declared more than once, a reference that names nothing or the wrong kind of thing, or a
    /// dependency cycle.
    pub fn graph(&self, entry: &EntryName) -> Result<TargetGraph> {
        resolve(load(&self.source, entry)?)
    }
}

/// The error naming the first place two sorted declaration lists differ, if they do.
///
/// Both lists agree before the first differing index, so the lesser of the two declarations there,
/// in the order Load sorts by, is the one the other list lacks, and its build file is the one to
/// fix.
fn first_difference(first: &[Declaration], second: &[Declaration]) -> Option<Error> {
    let len = first.len().max(second.len());
    (0..len).find_map(|index| {
        let a = first.get(index);
        let b = second.get(index);
        if a == b {
            return None;
        }
        let lacking = match (a, b) {
            (Some(a), Some(b)) => min_by(a, b, |x, y| declaration_order(x, y)),
            (Some(only), None) | (None, Some(only)) => only,
            (None, None) => return None,
        };
        Some(Error::Nondeterministic {
            provenance: lacking.provenance().clone(),
            first_run: a.cloned().map(Box::new),
            second_run: b.cloned().map(Box::new),
        })
    })
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::path::PathBuf;

    use super::first_difference;
    use crate::error::Error;
    use crate::types::{
        Declaration, Declared, Directory, Provenance, Setting, SettingName, SettingValue,
    };

    fn setting_in(directory: &str, name: &str) -> Declaration {
        let directory = Directory::parse(directory).unwrap();
        let file = if directory.is_root() {
            PathBuf::from("build.lua")
        } else {
            PathBuf::from(format!("{directory}/build.lua"))
        };
        Declaration::new(
            Provenance::new(file, directory),
            Declared::Setting(Setting {
                name: SettingName::parse(name).unwrap(),
                default: SettingValue::parse("").unwrap(),
            }),
        )
    }

    fn named_file(error: &Error) -> String {
        match error {
            Error::Nondeterministic { provenance, .. } => provenance.to_string(),
            other => unreachable!("expected Nondeterministic, got {other:?}"),
        }
    }

    #[test]
    fn equal_lists_have_no_difference() {
        let list = [setting_in("", "a"), setting_in("lib", "b")];
        assert!(first_difference(&list, &list).is_none());
        assert!(first_difference(&[], &[]).is_none());
    }

    #[test]
    fn an_extra_declaration_names_its_own_file() {
        let first = [setting_in("y", "y1")];
        let second = [setting_in("x", "x1"), setting_in("y", "y1")];
        let error = first_difference(&first, &second).unwrap();
        assert_eq!(named_file(&error), "x/build.lua");
        match error {
            Error::Nondeterministic {
                first_run,
                second_run,
                ..
            } => {
                assert_eq!(first_run.as_deref(), Some(&setting_in("y", "y1")));
                assert_eq!(second_run.as_deref(), Some(&setting_in("x", "x1")));
            }
            other => unreachable!("expected Nondeterministic, got {other:?}"),
        }
    }

    #[test]
    fn a_root_declaration_is_named_over_a_subdirectory_one() {
        let first = [setting_in("app", "x")];
        let second = [setting_in("", "r"), setting_in("app", "x")];
        let error = first_difference(&first, &second).unwrap();
        assert_eq!(named_file(&error), "build.lua");
    }

    #[test]
    fn a_dash_directory_is_named_over_a_nested_one_it_sorts_before() {
        let first = [setting_in("a/b", "x")];
        let second = [setting_in("a-b", "x")];
        let error = first_difference(&first, &second).unwrap();
        assert_eq!(named_file(&error), "a-b/build.lua");
    }

    #[test]
    fn a_declaration_past_the_end_of_the_other_list_is_named() {
        let first = [setting_in("", "a"), setting_in("lib", "b")];
        let second = [setting_in("", "a")];
        let error = first_difference(&first, &second).unwrap();
        assert_eq!(named_file(&error), "lib/build.lua");
        match error {
            Error::Nondeterministic { second_run, .. } => assert!(second_run.is_none()),
            other => unreachable!("expected Nondeterministic, got {other:?}"),
        }
    }
}
