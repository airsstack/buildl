//! The pipeline's driver: one bundle of ports, and one method per command.
//!
//! Its own file because the driver owns the sequence of phases: one method per command, each
//! running the phases up to the one the command stops at. It runs Load, Resolve, Plan and
//! Execute, and hands a verify the plan to re-run.
//!
//! Responsibilities: [`Pipeline`], its ports, [`Pipeline::check`], [`Pipeline::graph`],
//! [`Pipeline::query`], [`Pipeline::plan`], [`Pipeline::build`], [`Pipeline::test`] and
//! [`Pipeline::verify`], with the run of discovery targets that `build` and `test` begin with; and
//! [`check`], [`graph`] and [`query`], the three commands that reach only build files and the
//! state that holds discovery results, for a caller that holds only those ports.
//!
//! Non-responsibilities: the phases' own logic, which each phase's module holds.

use core::cmp::min_by;
use core::fmt;
use core::ops::ControlFlow;
use std::collections::{BTreeMap, BTreeSet};

use crate::discovery::{discovery_labels, load_twice, results, second_pass};
use crate::error::{Error, PortName, Result};
use crate::execute::execute;
use crate::grants::negotiate;
use crate::load::{declaration_order, load};
use crate::plan::plan;
use crate::ports::{
    ContentStore, DeclarationSource, Dispatcher, GraphRecord, PortSet, Ports, RunHistory,
};
use crate::resolve::{narrow, reach, resolve};
use crate::types::{
    ActionOutcome, Declaration, Diagnostic, Discovered, EntryName, Label, LiveOutput, Loaded,
    NodeId, OnFailure, Plan, Purpose, Query, Rerun, Sample, Scope, SettingName, SettingValue,
    TargetGraph, TargetRole, Verdict, Verification,
};
use crate::verify::verify;

/// What a command that runs targets holds once its discovery targets have run.
struct Prepared {
    /// The graph of the workspace as declared from the discovery results.
    whole: TargetGraph,
    /// The outcomes of the discovery run, in id order; empty when no discovery target is declared.
    outcomes: Vec<ActionOutcome>,
}

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

    /// Loads the workspace and returns its declarations when two evaluations agree.
    ///
    /// Declaration is deterministic by construction only if every build file is; evaluating twice
    /// and comparing is how a nondeterministic one is caught. A `pairs` loop is not
    /// nondeterminism here: the declarations are sorted before they are compared. A workspace
    /// that declares a discovery target is evaluated once without discovery results and twice
    /// with the last ones; see [`check`](fn@check).
    ///
    /// # Errors
    ///
    /// Returns whatever either load returns, [`Error::Nondeterministic`] when the two loads
    /// declare different things, and [`Error::DiscoveryChanged`] when reading discovery results
    /// changes which discovery targets are declared.
    pub fn check(&mut self, entry: &EntryName) -> Result<Vec<Declaration>> {
        check(
            &self.ports.source,
            &mut self.ports.dispatcher,
            entry,
            &self.ports.log,
            &self.ports.cas,
        )
        .map(|loaded| loaded.declarations)
    }

    /// Loads the workspace, with the last discovery results, and resolves its declarations into
    /// the target graph.
    ///
    /// The double evaluation that catches a nondeterministic build file is [`Pipeline::check`]'s;
    /// this command evaluates a workspace without a discovery target once.
    ///
    /// # Errors
    ///
    /// Returns whatever the load returns, [`Error::DiscoveryChanged`] when reading discovery
    /// results changes which discovery targets are declared, and otherwise whatever [`resolve`]
    /// returns: a name declared more than once, a reference that names nothing or the wrong kind
    /// of thing, or a dependency cycle.
    pub fn graph(&mut self, entry: &EntryName) -> Result<TargetGraph> {
        graph(
            &self.ports.source,
            &mut self.ports.dispatcher,
            entry,
            &self.ports.log,
            &self.ports.cas,
        )
        .map(|(graph, _)| graph)
    }

    /// Loads the workspace as [`Pipeline::graph`] does, resolves it, and answers `asked`: the
    /// targets it reaches, in label order, without the target it names.
    ///
    /// # Errors
    ///
    /// Returns whatever [`Pipeline::graph`] returns, and [`Error::UnknownTarget`] when the target
    /// `asked` names is neither a target nor an alias.
    pub fn query(&mut self, entry: &EntryName, asked: &Query) -> Result<BTreeSet<Label>> {
        query(
            &self.ports.source,
            &mut self.ports.dispatcher,
            entry,
            &self.ports.log,
            &self.ports.cas,
            asked,
        )
        .map(|(answer, _)| answer)
    }

    /// Loads the workspace, with the last discovery results, resolves it, and plans a build of
    /// the part of the graph `scope` asks for.
    ///
    /// `overrides` gives values to build settings for this run. A whole workspace means every
    /// target but tests, and a test is planned when it is named or a planned target depends on it.
    /// A target outside `scope` is not planned, so nothing it reads has to exist. Takes `&mut self`
    /// because a plan records the digest of every file it had to hash. The approver is not asked:
    /// a plan runs nothing.
    ///
    /// The discovery targets are planned as well, whatever `scope` is, and each one that is not
    /// clean is named by [`Plan::stale_discovery`]: the targets declared from it may change once
    /// it has run.
    ///
    /// # Errors
    ///
    /// Returns whatever the load returns, [`Error::DiscoveryChanged`] when reading discovery
    /// results changes which discovery targets are declared, then whatever [`resolve`] returns,
    /// then [`Error::UnknownTarget`] for a label of `scope` that names neither a target nor an
    /// alias, and otherwise whatever [`plan`](fn@plan) returns: an override naming no setting,
    /// two outputs of the planned targets that would be written to one place, a want outside the
    /// workspace's ceiling, a program that cannot be found, an input that is missing or is not a
    /// regular file, or a port's own failure.
    pub fn plan(
        &mut self,
        entry: &EntryName,
        scope: &Scope,
        overrides: &BTreeMap<SettingName, SettingValue>,
    ) -> Result<Plan> {
        let whole = resolve(self.declared(entry)?.declarations)?;
        let graph = narrow(&whole, scope, Purpose::Build)?;
        let planned = plan(&graph, overrides, Rerun::Off, &mut self.ports)?;
        let stale = self.stale_discovery(&whole, overrides)?;
        Ok(planned.with_stale_discovery(stale))
    }

    /// The workspace's declarations, from two loads when it declares a discovery target, with the
    /// last result of each discovery target read through this pipeline's log and store.
    fn declared(&mut self, entry: &EntryName) -> Result<Loaded> {
        let PortSet {
            source,
            dispatcher,
            log,
            cas,
            ..
        } = &mut self.ports;
        load_twice(source, dispatcher, entry, |labels| {
            results(log, cas, labels)
        })
    }

    /// The discovery targets of `whole` that a plan finds not clean, in label order.
    ///
    /// What a discovery target depends on is planned with it and is not listed: only a discovery
    /// target's own result declares targets.
    fn stale_discovery(
        &mut self,
        whole: &TargetGraph,
        overrides: &BTreeMap<SettingName, SettingValue>,
    ) -> Result<Vec<Label>> {
        let labels: BTreeSet<Label> = whole
            .nodes()
            .iter()
            .filter(|node| node.role == TargetRole::Discovery)
            .map(|node| node.label.clone())
            .collect();
        if labels.is_empty() {
            return Ok(Vec::new());
        }
        let graph = narrow(whole, &Scope::Targets(labels.clone()), Purpose::Build)?;
        let discovery = plan(&graph, overrides, Rerun::Off, &mut self.ports)?;
        Ok((0_u32..)
            .map(NodeId::new)
            .zip(discovery.verdicts())
            .filter(|(_, verdict)| !matches!(verdict, Verdict::Clean { .. }))
            .filter_map(|(id, _)| discovery.label(id).cloned())
            .filter(|label| labels.contains(label))
            .collect())
    }

    /// Loads the workspace for a command that runs targets, and runs its discovery targets first.
    ///
    /// A workspace that declares no discovery target is loaded once. One that does has the
    /// discovery targets built through the same path as any build, narrowed to them, and is then
    /// loaded again with the result each left; the build files are evaluated twice, never three
    /// times. The graph is recorded for each load that goes on to be run against.
    ///
    /// Breaks with the outcomes of the discovery run when any target of it did not succeed:
    /// nothing is declared from a failed discovery. Otherwise continues with the graph of the
    /// final load and the outcomes of the discovery run, empty when there was none.
    fn prepare(
        &mut self,
        entry: &EntryName,
        overrides: &BTreeMap<SettingName, SettingValue>,
        on_failure: OnFailure,
        live: LiveOutput,
    ) -> Result<ControlFlow<Vec<ActionOutcome>, Prepared>> {
        let pass_one = load(
            &self.ports.source,
            &mut self.ports.dispatcher,
            entry,
            &Discovered::empty(),
        )?;
        let labels = discovery_labels(&pass_one);
        if labels.is_empty() {
            let whole = resolve(pass_one)?;
            self.ports.graph.record(&whole)?;
            return Ok(ControlFlow::Continue(Prepared {
                whole,
                outcomes: Vec::new(),
            }));
        }
        let first = resolve(pass_one.clone())?;
        self.ports.graph.record(&first)?;
        let discovery = narrow(
            &first,
            &Scope::Targets(labels.iter().cloned().collect()),
            Purpose::Build,
        )?;
        let outcomes = self.run(&first, &discovery, overrides, Rerun::Off, on_failure, live)?;
        if outcomes.iter().any(|outcome| !outcome.class.succeeded()) {
            return Ok(ControlFlow::Break(outcomes));
        }
        let (discovered, unread) = results(&self.ports.log, &self.ports.cas, &labels);
        if !unread.is_empty() {
            return Err(self.unreadable_results(&unread));
        }
        let declarations = second_pass(
            &self.ports.source,
            &mut self.ports.dispatcher,
            entry,
            &pass_one,
            &discovered,
        )?;
        let whole = resolve(declarations)?;
        self.ports.graph.record(&whole)?;
        Ok(ControlFlow::Continue(Prepared { whole, outcomes }))
    }

    /// The failure of a discovery target that succeeded and whose result cannot be read back.
    ///
    /// The log is named when it cannot be read at all, otherwise the store, which then does not
    /// hold or cannot read what the target produced.
    fn unreadable_results(&self, unread: &[Label]) -> Error {
        let port = if self.ports.log.lines().is_err() {
            PortName::RunHistory
        } else {
            PortName::ContentStore
        };
        let labels: Vec<String> = unread.iter().map(ToString::to_string).collect();
        Error::Adapter {
            port,
            diagnostic: Diagnostic::new(format!(
                "the result of {} cannot be read back after it ran",
                labels.join(", ")
            )),
        }
    }

    /// Plans `graph`, puts what the plan wants to the approver, and executes it, as the part of
    /// `whole` that one run of a command covers.
    fn run(
        &mut self,
        whole: &TargetGraph,
        graph: &TargetGraph,
        overrides: &BTreeMap<SettingName, SettingValue>,
        rerun: Rerun,
        on_failure: OnFailure,
        live: LiveOutput,
    ) -> Result<Vec<ActionOutcome>> {
        let plan = plan(graph, overrides, rerun, &mut self.ports)?;
        negotiate(plan.wanted(), plan.attribution(), &mut self.ports.approver)?;
        execute(
            whole,
            graph,
            &plan,
            on_failure,
            live,
            rerun,
            &mut self.ports,
        )
    }

    /// Loads the workspace, builds its discovery targets first when it declares any, resolves it,
    /// records the graph of the whole workspace, plans a build of the part of the graph `scope`
    /// asks for, and builds that part.
    ///
    /// `overrides` gives values to build settings for this run, `on_failure` says what a failed
    /// action does to the rest of the build, and `live` whether one running action at a time is
    /// shown as it prints. A whole workspace means every target but tests, and a test is built
    /// when it is named or a built target depends on it. The outcomes are one per target of that
    /// part, in id order; a target that failed or was skipped is an outcome, not an error. A
    /// target outside `scope` is left alone: it has no outcome, and its row and outputs stay.
    ///
    /// The graph is recorded through the graph-record port before each run is planned, and it is
    /// the whole workspace's whatever `scope` asks for, so the record never depends on which
    /// targets one build was asked for.
    ///
    /// Once planned, the plan's wanted set is put to the [`Approver`](crate::Approver), and
    /// nothing runs if it does not approve, even when every target is cached.
    ///
    /// A workspace that declares a discovery target is loaded without discovery results, its
    /// discovery targets are built as a build of just them is, then it is loaded again with their
    /// results and built as above: two plans, two approvals, and two reports, the discovery run's
    /// first. The outcomes are those of the discovery run followed by those of the build; a
    /// discovery target `scope` names too is in both, cached the second time. When a discovery
    /// target did not succeed the outcomes are those of the discovery run alone, and nothing
    /// else is loaded, planned or built. The build files are evaluated twice, never three times.
    ///
    /// # Errors
    ///
    /// Returns the first of: whatever the load returns; [`Error::DiscoveryChanged`] when the
    /// results of the discovery targets change which of them are declared; [`Error::Adapter`] of
    /// the store or the log when the result of a discovery target that succeeded cannot be read
    /// back; whatever [`resolve`] returns; the graph-record port's failure; [`Error::UnknownTarget`] for a label of `scope`
    /// that names neither a target nor an alias; whatever [`plan`](fn@plan) returns, two outputs
    /// of the planned targets that would be written to one place included; [`Error::NotApproved`]
    /// when the approver does not approve what a plan wants, or the approver's own failure; and
    /// whatever [`execute`](fn@execute) returns: two outputs of the workspace that would be
    /// written to one place, whether or not both targets were asked for, a command that names no
    /// program or one outside the workspace's ceiling once it is expanded, or a port's own
    /// failure.
    pub fn build(
        &mut self,
        entry: &EntryName,
        scope: &Scope,
        overrides: &BTreeMap<SettingName, SettingValue>,
        on_failure: OnFailure,
        live: LiveOutput,
    ) -> Result<Vec<ActionOutcome>> {
        let Prepared {
            whole,
            mut outcomes,
        } = match self.prepare(entry, overrides, on_failure, live)? {
            ControlFlow::Break(stopped) => return Ok(stopped),
            ControlFlow::Continue(prepared) => prepared,
        };
        let graph = narrow(&whole, scope, Purpose::Build)?;
        outcomes.extend(self.run(&whole, &graph, overrides, Rerun::Off, on_failure, live)?);
        Ok(outcomes)
    }

    /// Loads the workspace, builds its discovery targets first when it declares any, resolves it,
    /// records the graph of the whole workspace, plans a run of the tests `scope` asks for, and
    /// runs them.
    ///
    /// `overrides` gives values to build settings for this run, `on_failure` says what a failed
    /// action does to the rest of the run, and `live` whether one running action at a time is
    /// shown as it prints. A whole workspace means every test, with what each test needs; a
    /// target that is not a test and that no selected test needs is left alone: it has no
    /// outcome, and its row and outputs stay. The outcomes are one per target of the run, in id
    /// order; a target that failed or was skipped is an outcome, not an error.
    ///
    /// With [`Rerun::Tests`] the selected tests run whatever their keys say and their new results
    /// are recorded; what they depend on does not run again unless its own key says so.
    ///
    /// The graph is recorded through the graph-record port before each run is planned, and it is
    /// the whole workspace's whatever `scope` asks for, so the record never depends on which
    /// tests one run was asked for.
    ///
    /// Once planned, the plan's wanted set is put to the [`Approver`](crate::Approver), and
    /// nothing runs if it does not approve, even when every target is cached.
    ///
    /// A workspace that declares a discovery target is run as [`Pipeline::build`] runs it: its
    /// discovery targets are built first, as a build of just them is, then the workspace is
    /// loaded again with their results and the tests run. The outcomes are those of the discovery
    /// run followed by those of the test run, and a discovery target that did not succeed ends
    /// the command with the outcomes of the discovery run alone. The discovery run's summary
    /// counts the discovery targets in [`Summary::all`](crate::Summary::all) and no test in
    /// [`Summary::tests`](crate::Summary::tests).
    ///
    /// # Errors
    ///
    /// Returns the first of: whatever the load returns; [`Error::DiscoveryChanged`] when the
    /// results of the discovery targets change which of them are declared; [`Error::Adapter`] of
    /// the store or the log when the result of a discovery target that succeeded cannot be read
    /// back; whatever [`resolve`] returns; the graph-record port's failure; [`Error::UnknownTarget`] for a label of `scope`
    /// that names neither a target nor an alias; [`Error::NotATest`] for a label of `scope` that
    /// names a target that is not a test; whatever [`plan`](fn@plan) returns, two outputs of the
    /// planned targets that would be written to one place included; [`Error::NotApproved`] when
    /// the approver does not approve what the plan wants, or the approver's own failure; and
    /// whatever [`execute`](fn@execute) returns: two outputs of the workspace that would be
    /// written to one place, whether or not both targets were asked for, a command that names no
    /// program or one outside the workspace's ceiling once it is expanded, or a port's own
    /// failure.
    pub fn test(
        &mut self,
        entry: &EntryName,
        scope: &Scope,
        overrides: &BTreeMap<SettingName, SettingValue>,
        on_failure: OnFailure,
        live: LiveOutput,
        rerun: Rerun,
    ) -> Result<Vec<ActionOutcome>> {
        let Prepared {
            whole,
            mut outcomes,
        } = match self.prepare(entry, overrides, on_failure, live)? {
            ControlFlow::Break(stopped) => return Ok(stopped),
            ControlFlow::Continue(prepared) => prepared,
        };
        let graph = narrow(&whole, scope, Purpose::Test)?;
        outcomes.extend(self.run(&whole, &graph, overrides, rerun, on_failure, live)?);
        Ok(outcomes)
    }

    /// Loads the workspace, with the last discovery results, resolves it, plans a build of the
    /// part of the graph `scope` asks for, and re-runs the cached targets of that part to see
    /// whether they reproduce what was cached.
    ///
    /// Planned as [`Pipeline::build`] plans: `overrides` gives values to build settings for this
    /// run, and a whole workspace means every target but tests, with a test checked when it is
    /// named or a checked target depends on it. `sample` says which of the cached targets
    /// re-run. Each target is one [`Check`](crate::Check) in the [`Verification`]; a target whose
    /// re-run differs from the cache, or fails, is a result and not an error.
    ///
    /// Nothing a build keeps is changed: the graph is not recorded, no row is recorded or
    /// forgotten, no event is logged, and nothing is reported. Once planned, the plan's wanted set
    /// is put to the [`Approver`](crate::Approver), and nothing is re-run if it does not approve.
    ///
    /// # Errors
    ///
    /// Returns the first of: whatever the load returns; [`Error::DiscoveryChanged`] when reading
    /// discovery results changes which discovery targets are declared; whatever [`resolve`]
    /// returns; [`Error::UnknownTarget`] for a label of `scope` that names neither a target nor
    /// an alias; whatever [`plan`](fn@plan) returns; [`Error::NotApproved`] when the approver does
    /// not approve what the plan wants, or the approver's own failure; and whatever
    /// [`verify`](fn@verify) returns.
    pub fn verify(
        &mut self,
        entry: &EntryName,
        scope: &Scope,
        overrides: &BTreeMap<SettingName, SettingValue>,
        sample: Sample,
    ) -> Result<Verification> {
        let whole = resolve(self.declared(entry)?.declarations)?;
        let graph = narrow(&whole, scope, Purpose::Build)?;
        let plan = plan(&graph, overrides, Rerun::Off, &mut self.ports)?;
        negotiate(plan.wanted(), plan.attribution(), &mut self.ports.approver)?;
        verify(&whole, &graph, &plan, sample, &mut self.ports)
    }
}

/// Loads the workspace through `source` and returns its declarations when two evaluations agree.
///
/// What [`Pipeline::check`] does, for a caller that holds a source, a dispatcher, and the log and
/// store that hold discovery results, and no other port.
///
/// A workspace that declares no discovery target is evaluated twice without discovery results,
/// and the two must agree. One that does is evaluated once without them, then twice with the last
/// result of each discovery target as `history` and `cas` hold it; the last two must agree, and
/// declare the discovery targets of the first. A result that cannot be read counts as none, so a
/// damaged log never stops a check. The labels of the discovery targets with no result come back
/// beside the declarations.
///
/// A command that only evaluates build files reads no state a build keeps but the log and the
/// store, and takes no lock and writes nothing to them, but for a store's own removal of a blob
/// whose bytes fail their digest.
///
/// # Errors
///
/// Returns whatever any load returns, [`Error::Nondeterministic`] when two loads declare
/// different things, and [`Error::DiscoveryChanged`] when the loads with results declare
/// different discovery targets than the load without.
pub fn check<S, D, H, C>(
    source: &S,
    dispatcher: &mut D,
    entry: &EntryName,
    history: &H,
    cas: &C,
) -> Result<Loaded>
where
    S: DeclarationSource,
    D: Dispatcher,
    H: RunHistory,
    C: ContentStore,
{
    let first = load(source, dispatcher, entry, &Discovered::empty())?;
    let labels = discovery_labels(&first);
    if labels.is_empty() {
        let second = load(source, dispatcher, entry, &Discovered::empty())?;
        return first_difference(&first, &second).map_or(
            Ok(Loaded {
                declarations: first,
                missing: Vec::new(),
            }),
            Err,
        );
    }
    let (discovered, missing) = results(history, cas, &labels);
    let second = second_pass(source, dispatcher, entry, &first, &discovered)?;
    let third = load(source, dispatcher, entry, &discovered)?;
    first_difference(&second, &third).map_or(
        Ok(Loaded {
            declarations: second,
            missing,
        }),
        Err,
    )
}

/// Loads the workspace through `source` and resolves its declarations into the target graph.
///
/// What [`Pipeline::graph`] does, for a caller that holds a source, a dispatcher, and the log and
/// store that hold discovery results, and no other port.
///
/// A workspace that declares a discovery target is loaded again with the last result of each,
/// read without a lock; the labels of the ones with no result come back beside the graph.
///
/// # Errors
///
/// Returns whatever the load returns, [`Error::DiscoveryChanged`] when the second load declares
/// different discovery targets than the first, and otherwise whatever [`resolve`] returns.
pub fn graph<S, D, H, C>(
    source: &S,
    dispatcher: &mut D,
    entry: &EntryName,
    history: &H,
    cas: &C,
) -> Result<(TargetGraph, Vec<Label>)>
where
    S: DeclarationSource,
    D: Dispatcher,
    H: RunHistory,
    C: ContentStore,
{
    let loaded = load_twice(source, dispatcher, entry, |labels| {
        results(history, cas, labels)
    })?;
    Ok((resolve(loaded.declarations)?, loaded.missing))
}

/// Loads the workspace as [`graph`] does, resolves it, and answers `asked`: what
/// [`Pipeline::query`] does, for a caller that holds the ports [`graph`] takes.
///
/// The answer is in label order and does not hold the target `asked` names. The labels of the
/// discovery targets with no result come back beside it.
///
/// # Errors
///
/// Returns whatever [`graph`] returns, and [`Error::UnknownTarget`] when the target `asked` names
/// is neither a target nor an alias.
pub fn query<S, D, H, C>(
    source: &S,
    dispatcher: &mut D,
    entry: &EntryName,
    history: &H,
    cas: &C,
    asked: &Query,
) -> Result<(BTreeSet<Label>, Vec<Label>)>
where
    S: DeclarationSource,
    D: Dispatcher,
    H: RunHistory,
    C: ContentStore,
{
    let (graph, missing) = graph(source, dispatcher, entry, history, cas)?;
    Ok((reach(&graph, asked)?, missing))
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
        Declaration, Declared, Directory, Origin, Provenance, Setting, SettingName, SettingValue,
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
            Origin::BuildFile,
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
