//! The pipeline's driver: one bundle of ports, and one method per command.
//!
//! Its own file because the driver owns the sequence of phases: one method per command, each
//! running the phases up to the one the command stops at. It runs Load, Resolve, Plan and
//! Execute.
//!
//! Responsibilities: [`Pipeline`], its ports, [`Pipeline::check`], [`Pipeline::graph`],
//! [`Pipeline::query`], [`Pipeline::plan`] and [`Pipeline::build`]; and [`check`], [`graph`] and
//! [`query`], the three commands that reach only build files, for a caller that holds only a
//! source of them.
//!
//! Non-responsibilities: the phases' own logic, which each phase's module holds.

use core::cmp::min_by;
use core::fmt;
use std::collections::{BTreeMap, BTreeSet};

use crate::error::{Error, Result};
use crate::execute::execute;
use crate::load::{declaration_order, load};
use crate::plan::plan;
use crate::ports::{DeclarationSource, GraphRecord, PortSet, Ports};
use crate::resolve::{narrow, reach, resolve};
use crate::types::{
    ActionOutcome, Declaration, EntryName, Label, LiveOutput, OnFailure, Plan, Purpose, Query,
    Rerun, Scope, SettingName, SettingValue, TargetGraph,
};

/// The pipeline over one chosen implementation of each port.
pub struct Pipeline<P: Ports> {
    ports: PortSet<P>,
}

impl<P: Ports> fmt::Debug for Pipeline<P>
where
    PortSet<P>: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Pipeline")
            .field("ports", &self.ports)
            .finish()
    }
}

impl<P: Ports> Pipeline<P> {
    /// Assembles a pipeline from its ports.
    #[must_use]
    pub const fn new(ports: PortSet<P>) -> Self {
        Self { ports }
    }

    /// The ports this pipeline runs over.
    #[must_use]
    pub const fn ports(&self) -> &PortSet<P> {
        &self.ports
    }

    /// The ports this pipeline runs over, for a caller that changes one between two commands.
    #[must_use]
    pub const fn ports_mut(&mut self) -> &mut PortSet<P> {
        &mut self.ports
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
        check(&self.ports.source, entry)
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
        graph(&self.ports.source, entry)
    }

    /// Loads the workspace once, resolves it, and answers `asked`: the targets it reaches, in
    /// label order, without the target it names.
    ///
    /// # Errors
    ///
    /// Returns whatever [`Pipeline::graph`] returns, and [`Error::UnknownTarget`] when the target
    /// `asked` names is neither a target nor an alias.
    pub fn query(&self, entry: &EntryName, asked: &Query) -> Result<BTreeSet<Label>> {
        query(&self.ports.source, entry, asked)
    }

    /// Loads the workspace once, resolves it, and plans a build of the part of the graph `scope`
    /// asks for.
    ///
    /// `overrides` gives values to build settings for this run. A whole workspace means every
    /// target but tests, and a test is planned when it is named or a planned target depends on it.
    /// A target outside `scope` is not planned, so nothing it reads has to exist. Takes `&mut self`
    /// because a plan records the digest of every file it had to hash.
    ///
    /// # Errors
    ///
    /// Returns whatever the load returns, then whatever [`resolve`] returns, then
    /// [`Error::UnknownTarget`] for a label of `scope` that names neither a target nor an alias,
    /// and otherwise whatever [`plan`](fn@plan) returns: an override naming no setting, a want
    /// outside the workspace's ceiling, a program that cannot be found, an input that is missing
    /// or is not a regular file, or a port's own failure.
    pub fn plan(
        &mut self,
        entry: &EntryName,
        scope: &Scope,
        overrides: &BTreeMap<SettingName, SettingValue>,
    ) -> Result<Plan> {
        let whole = resolve(load(&self.ports.source, entry)?)?;
        let graph = narrow(&whole, scope, Purpose::Build)?;
        plan(&graph, overrides, Rerun::Off, &mut self.ports)
    }

    /// Loads the workspace once, resolves it, records the graph of the whole workspace, plans a
    /// build of the part of the graph `scope` asks for, and builds that part.
    ///
    /// `overrides` gives values to build settings for this run, `on_failure` says what a failed
    /// action does to the rest of the build, and `live` whether one running action at a time is
    /// shown as it prints. A whole workspace means every target but tests, and a test is built
    /// when it is named or a built target depends on it. The outcomes are one per target of that
    /// part, in id order; a target that failed or was skipped is an outcome, not an error. A
    /// target outside `scope` is left alone: it has no outcome, and its row and outputs stay.
    ///
    /// The graph is recorded through the graph-record port before anything is planned, and it is
    /// the whole workspace's whatever `scope` asks for, so the record never depends on which
    /// targets one build was asked for.
    ///
    /// # Errors
    ///
    /// Returns the first of: whatever the load returns; whatever [`resolve`] returns; the
    /// graph-record port's failure; [`Error::UnknownTarget`] for a label of `scope` that names
    /// neither a target nor an alias; whatever [`plan`](fn@plan) returns; and whatever
    /// [`execute`](fn@execute) returns: two outputs of the workspace that would be written to one
    /// place, whether or not both targets were asked for, a command that names no program or one
    /// outside the workspace's ceiling once it is expanded, or a port's own failure.
    pub fn build(
        &mut self,
        entry: &EntryName,
        scope: &Scope,
        overrides: &BTreeMap<SettingName, SettingValue>,
        on_failure: OnFailure,
        live: LiveOutput,
    ) -> Result<Vec<ActionOutcome>> {
        let whole = resolve(load(&self.ports.source, entry)?)?;
        self.ports.graph.record(&whole)?;
        let graph = narrow(&whole, scope, Purpose::Build)?;
        let plan = plan(&graph, overrides, Rerun::Off, &mut self.ports)?;
        execute(
            &whole,
            &graph,
            &plan,
            on_failure,
            live,
            Rerun::Off,
            &mut self.ports,
        )
    }
}

/// Loads the workspace twice through `source` and returns its declarations when both loads
/// agree: what [`Pipeline::check`] does, for a caller that holds a source and no other port.
///
/// A command that only evaluates build files reads no state a build keeps, so it needs none of
/// the ports that hold it.
///
/// # Errors
///
/// Returns whatever either load returns, and [`Error::Nondeterministic`] when the two loads
/// declare different things.
pub fn check<S: DeclarationSource>(source: &S, entry: &EntryName) -> Result<Vec<Declaration>> {
    let first = load(source, entry)?;
    let second = load(source, entry)?;
    first_difference(&first, &second).map_or(Ok(first), Err)
}

/// Loads the workspace once through `source` and resolves its declarations into the target
/// graph: what [`Pipeline::graph`] does, for a caller that holds a source and no other port.
///
/// # Errors
///
/// Returns whatever the load returns, and otherwise whatever [`resolve`] returns.
pub fn graph<S: DeclarationSource>(source: &S, entry: &EntryName) -> Result<TargetGraph> {
    resolve(load(source, entry)?)
}

/// Loads the workspace once through `source`, resolves it, and answers `asked`: what
/// [`Pipeline::query`] does, for a caller that holds a source and no other port.
///
/// The answer is in label order and does not hold the target `asked` names.
///
/// # Errors
///
/// Returns whatever [`graph`] returns, and [`Error::UnknownTarget`] when the target `asked` names
/// is neither a target nor an alias.
pub fn query<S: DeclarationSource>(
    source: &S,
    entry: &EntryName,
    asked: &Query,
) -> Result<BTreeSet<Label>> {
    reach(&graph(source, entry)?, asked)
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
