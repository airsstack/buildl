//! The verify assembly: from a plan, through the ports, to one check per cached target.
//!
//! Its own file because it is where the steps of a verify meet and where their order is fixed.
//! Everything that can refuse a verify is checked before anything is touched, as a build does.
//! Then the outputs of every cached target are put back in place, a sample of those targets is
//! chosen, and only then do the chosen actions run, independent of one another, each compared
//! with what was cached.
//!
//! Responsibilities: [`verify`]; which targets are cached and have their outputs put back; the
//! direct dependency that keeps a target from re-running; running the chosen actions; and the
//! comparison of a re-run's outputs with the cached ones.
//!
//! Non-responsibilities: which targets a sample takes and where two manifests differ, which each
//! have a sibling file; and running one action, which is [`perform`](crate::execute::perform)'s.
//! Nothing here records a row, forgets one, logs an event or reports to a person.

use core::sync::atomic::AtomicBool;
use std::collections::BTreeSet;

use super::{Chosen, chosen, first_difference};
use crate::error::{Error, Result};
use crate::execute::assembly::{Prepared, made_from, prepared};
use crate::execute::layout::Layout;
use crate::execute::perform::{Ended, Kept, Performed, Shared, Task, Told, Worked, perform};
use crate::execute::record::{restore, shown};
use crate::execute::settle::restorable;
use crate::ports::{
    ActionCache, Clock, ContentStore, Dispatcher, ExecStrategy, Manifest, PortSet, Ports,
};
use crate::types::{
    ActionKey, CacheRow, Check, Diagnostic, Digest, Label, NodeId, OutputManifest, OutputName,
    Plan, Sample, TargetGraph, Turn, Verdict, Verification,
};

/// Re-runs the cached targets of `graph` and returns one check for each target that was looked at.
///
/// `plan` must be the plan made from `graph`, and `whole` the graph of the whole workspace, the
/// one `graph` was narrowed from, or `graph` itself. A target is cached when its verdict is clean
/// and everything its row names is still held, in a form that hashes to the digest the row
/// records. The outputs of every cached target are put back in place first, in graph order, as a
/// build does for a target that need not run; a target they cannot be put back for is
/// [`Check::Unplaced`].
///
/// A cached target with a direct dependency that was not put back, because that dependency is
/// not cached or is unplaced, is [`Check::Skipped`], naming the lowest such dependency: its
/// action would not find what it reads. A skipped target's own outputs are in place, so a target
/// above it is not skipped for it.
///
/// `sample` then chooses which of the remaining targets re-run, by their key digests; the others
/// are counted in [`Verification::not_sampled`]. The chosen actions run through the dispatcher
/// port, with no live output. Each is [`Check::Verified`] when every output it produced equals
/// the cached one, [`Check::Mismatched`] naming the first output that does not, and
/// [`Check::Failed`] when it did not end in outputs. A failed re-run does not stop the others.
///
/// Every other target of `graph` is counted in [`Verification::not_cached`].
///
/// No row is recorded or forgotten, no event is logged, nothing is reported, and what a re-run
/// produced is never put in place: only the restores place content. Content a re-run stores is
/// named by no row and no log line.
///
/// # Errors
///
/// Before anything is touched, returns the first failure met, in this order:
///
/// 1. [`Error::ForeignPlan`] when `plan` was not made from `graph`;
/// 2. whatever the manifest port returns;
/// 3. [`Error::OutputConflict`] when two outputs of `whole` would be written to one place;
/// 4. for the first target, in id order, whose expanded command cannot run: [`Error::NoProgram`]
///    when it names no program, and [`Error::Refused`] when the program is outside the
///    workspace's ceiling.
///
/// After that, returns whatever the content store returns when the manifest of a recorded output
/// cannot be read or what a re-run printed cannot be read back, [`Error::CanonicalJson`] for a
/// manifest that cannot be serialized, and whatever the dispatcher itself returns. When the
/// dispatcher returns success and a chosen action has no result, returns
/// [`Error::DispatcherFault`] naming those targets.
pub fn verify<P: Ports>(
    whole: &TargetGraph,
    graph: &TargetGraph,
    plan: &Plan,
    sample: Sample,
    ports: &mut PortSet<P>,
) -> Result<Verification> {
    if !made_from(plan, graph) {
        return Err(Error::ForeignPlan);
    }
    let workspace = ports.manifest.workspace()?;
    Layout::of(whole, &workspace.out)?;
    let layout = Layout::of(graph, &workspace.out)?;
    let targets = prepared(graph, plan, &layout, &workspace.ceiling)?;

    let PortSet {
        cache,
        cas,
        strategy,
        dispatcher,
        clock,
        ..
    } = ports;
    let (cache, cas, strategy, clock) = (&*cache, &*cas, &*strategy, &*clock);

    let PutBack {
        restored,
        mut unplaced,
        not_cached,
    } = restore_cached(&targets, plan, &layout, cache, cas)?;

    let in_place: BTreeSet<&Label> = targets
        .iter()
        .zip(&restored)
        .filter(|(_, restored)| restored.is_some())
        .map(|(target, _)| &target.node.label)
        .collect();
    let mut checked = Vec::new();
    let mut candidates = Vec::new();
    for (target, restored) in targets.iter().zip(&restored) {
        let Some(restored) = restored else {
            continue;
        };
        match unmet(&target.deps, &in_place) {
            Some(because) => checked.push((
                target.node.label.clone(),
                Check::Skipped {
                    because: because.clone(),
                },
            )),
            None => candidates.push((target.node.label.clone(), restored.key.clone())),
        }
    }

    let Chosen { rerun, left_out } = chosen(candidates, sample);
    let mut tasks = Vec::new();
    for ((node, target), restored) in (0_u32..).map(NodeId::new).zip(&targets).zip(&restored) {
        if let Some(restored) = restored
            && rerun.binary_search(&target.node.label).is_ok()
        {
            tasks.push(Task {
                node,
                key: restored.key.clone(),
                action: target.action.clone(),
            });
        }
    }
    let asked: Vec<NodeId> = tasks.iter().map(|task| task.node).collect();
    let performed = run(tasks, strategy, cas, clock, dispatcher)?;
    let lost: Vec<Label> = asked
        .iter()
        .filter(|node| !performed.iter().any(|done| done.node == **node))
        .filter_map(|node| graph.node(*node))
        .map(|node| node.label.clone())
        .collect();
    if !lost.is_empty() {
        return Err(Error::DispatcherFault { targets: lost });
    }

    for done in performed {
        let (Some(target), Some(Some(cached))) = (
            targets.get(done.node.index()),
            restored.get(done.node.index()),
        ) else {
            continue;
        };
        let label = &target.node.label;
        let check = match done.ended {
            Ended::Produced { outputs, .. } => {
                compared(&target.node.outputs, &cached.manifests, &outputs)
            }
            Ended::Failed {
                class,
                diagnostic,
                printed,
            } => Check::Failed {
                class,
                diagnostic,
                stdout: shown(cas, label, printed.map(|printed| printed.stdout))?,
                stderr: shown(cas, label, printed.map(|printed| printed.stderr))?,
            },
        };
        checked.push((label.clone(), check));
    }
    checked.append(&mut unplaced);
    Ok(Verification::new(checked, not_cached, left_out))
}

/// A cached target whose outputs are in place: what it needs to be re-run and compared.
struct Restored {
    /// The key its action re-runs under: the one its row was recorded under.
    key: ActionKey,
    /// What each of its outputs was when it was cached, in declared order.
    manifests: Vec<OutputManifest>,
}

/// What putting the cached targets back in place came to.
struct PutBack {
    /// One slot per target, in id order: filled for a target whose outputs are in place.
    restored: Vec<Option<Restored>>,
    /// The targets whose outputs could not be put back, as checks.
    unplaced: Vec<(Label, Check)>,
    /// How many targets have nothing cached to check against.
    not_cached: usize,
}

/// Puts the outputs of every cached target of `targets` back in place, in id order.
///
/// A target is cached when its verdict is clean and its row is [`cached_manifests`]. Every other
/// target is not cached. Nothing is written to `cache`.
///
/// # Errors
///
/// Returns whatever [`cached_manifests`] returns.
fn restore_cached<A: ActionCache, C: ContentStore>(
    targets: &[Prepared<'_>],
    plan: &Plan,
    layout: &Layout,
    cache: &A,
    cas: &C,
) -> Result<PutBack> {
    let mut put_back = PutBack {
        restored: Vec::with_capacity(targets.len()),
        unplaced: Vec::new(),
        not_cached: 0,
    };
    for ((node, target), verdict) in (0_u32..).map(NodeId::new).zip(targets).zip(plan.verdicts()) {
        let label = &target.node.label;
        let cached = match (verdict, cache.row(label)) {
            (Verdict::Clean { key }, Some(row)) => {
                cached_manifests(row, cas)?.map(|manifests| (key, row, manifests))
            }
            _ => None,
        };
        let Some((key, row, manifests)) = cached else {
            put_back.not_cached += 1;
            put_back.restored.push(None);
            continue;
        };
        let outcome = restore(
            label.clone(),
            target.node.role,
            row,
            &manifests,
            layout.outputs(node),
            cas,
        );
        if outcome.class.succeeded() {
            put_back.restored.push(Some(Restored {
                key: key.clone(),
                manifests,
            }));
        } else {
            let diagnostic = outcome
                .diagnostic
                .unwrap_or_else(|| Diagnostic::new("the cached outputs could not be put back"));
            put_back
                .unplaced
                .push((label.clone(), Check::Unplaced { diagnostic }));
            put_back.restored.push(None);
        }
    }
    Ok(put_back)
}

/// What each output of `row` was, in declared order, when `cas` still holds everything the row
/// names and each manifest hashes to the digest the row records for it; `None` otherwise.
///
/// A manifest that does not hash to its digest cannot be trusted to be what the row recorded.
/// Rejecting it here is what lets the walk that compares it with a re-run's manifest stand for
/// comparing their digests.
///
/// # Errors
///
/// Returns whatever `cas` returns when a manifest cannot be read, and [`Error::CanonicalJson`]
/// for a manifest that cannot be serialized.
fn cached_manifests<C: ContentStore>(
    row: &CacheRow,
    cas: &C,
) -> Result<Option<Vec<OutputManifest>>> {
    let Some(manifests) = restorable(row, |digest| cas.contains(digest), |digest| cas.get(digest))?
    else {
        return Ok(None);
    };
    for (manifest, digest) in manifests.iter().zip(row.outputs()) {
        if Digest::of(&manifest.to_bytes()?) != *digest {
            return Ok(None);
        }
    }
    Ok(Some(manifests))
}

/// The lowest of `deps` that is not in `in_place`, if any: the dependency that keeps a target
/// from re-running.
fn unmet<'a>(deps: &[&'a Label], in_place: &BTreeSet<&Label>) -> Option<&'a Label> {
    deps.iter()
        .copied()
        .filter(|dep| !in_place.contains(dep))
        .min()
}

/// Runs every task of `tasks` through `dispatcher`, none of them shown live, and returns the
/// ones that ended, in the order they came back.
///
/// Every task is handed out when the dispatcher begins, and nothing is handed out after. The
/// stop flag is never set, so a task is always begun; one that comes back as not begun is left
/// out of the result, like one the dispatcher lost.
///
/// # Errors
///
/// Returns whatever the dispatcher returns.
fn run<S, C, K, D>(
    tasks: Vec<Task>,
    strategy: &S,
    cas: &C,
    clock: &K,
    dispatcher: &mut D,
) -> Result<Vec<Performed>>
where
    S: ExecStrategy,
    C: ContentStore,
    K: Clock + Sync,
    D: Dispatcher,
{
    let stop = AtomicBool::new(false);
    let shared = Shared {
        stop: &stop,
        live: None,
    };
    let mut waiting = tasks;
    let mut performed = Vec::new();
    dispatcher.dispatch(
        |task, tell| perform(task, strategy, cas, clock, shared, tell),
        |turn: Turn<Told, Worked>| match turn {
            Turn::Begin => core::mem::take(&mut waiting),
            Turn::Told(_) | Turn::Done(Worked::NotBegun(_)) => Vec::new(),
            Turn::Done(Worked::Performed(done)) => {
                performed.push(*done);
                Vec::new()
            }
        },
    )?;
    Ok(performed)
}

/// How a re-run's outputs compare with the cached ones: verified when every output is equal, and
/// otherwise mismatched at the first output, in declared order, that differs.
///
/// `names` are the outputs the target declares, `cached` what each was when it was cached and
/// `kept` what each is now, all in declared order.
fn compared(names: &[OutputName], cached: &[OutputManifest], kept: &[Kept]) -> Check {
    names
        .iter()
        .zip(cached)
        .zip(kept)
        .find_map(|((name, cached), kept)| {
            first_difference(cached, &kept.manifest).map(|difference| Check::Mismatched {
                output: name.clone(),
                difference,
            })
        })
        .unwrap_or(Check::Verified)
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeSet;

    use super::{compared, unmet};
    use crate::execute::fixtures::{file, kept, label, outputs, tree};
    use crate::types::{Check, EntryChange, FileChange, OutputDifference, TreePath};

    #[test]
    fn the_lowest_dependency_not_in_place_is_the_one_that_is_unmet() {
        let (a, b, c) = (label("//:a"), label("//:b"), label("//:c"));
        let in_place = BTreeSet::from([&a]);
        assert_eq!(unmet(&[&c, &b, &a], &in_place), Some(&b));
        assert_eq!(unmet(&[&a], &in_place), None);
        assert_eq!(unmet(&[], &in_place), None);
    }

    #[test]
    fn equal_outputs_are_verified() {
        let check = compared(
            &outputs(&["x", "y"]),
            &[file("one"), tree(&[("a", "1")])],
            &[kept(file("one")), kept(tree(&[("a", "1")]))],
        );
        assert_eq!(check, Check::Verified);
    }

    #[test]
    fn the_first_output_in_declared_order_that_differs_is_named() {
        let names = outputs(&["x", "y", "z"]);
        let check = compared(
            &names,
            &[file("one"), tree(&[("a", "1")]), file("three")],
            &[
                kept(file("one")),
                kept(tree(&[("a", "2")])),
                kept(file("3")),
            ],
        );
        assert_eq!(
            check,
            Check::Mismatched {
                output: names[1].clone(),
                difference: OutputDifference::Entry {
                    path: TreePath::parse("a").unwrap(),
                    change: EntryChange::Bytes,
                },
            }
        );
    }

    #[test]
    fn a_file_with_other_bytes_is_mismatched_in_bytes() {
        let names = outputs(&["x"]);
        let check = compared(&names, &[file("one")], &[kept(file("two"))]);
        assert_eq!(
            check,
            Check::Mismatched {
                output: names[0].clone(),
                difference: OutputDifference::File(FileChange::Bytes),
            }
        );
    }
}
