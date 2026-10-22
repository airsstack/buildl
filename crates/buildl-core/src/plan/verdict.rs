//! Verdicts: what a build would do with each target.
//!
//! Its own file because this is the decision the whole phase exists to make: with a target's
//! own components, what its dependencies produced and what its last build recorded, say whether
//! it is clean, must run, or runs only if a dependency's output changes.
//!
//! Responsibilities: [`Dependency`], [`decide`] and [`all`].
//!
//! Non-responsibilities: listing how two keys differ, and the order targets are visited in.
//! Each has its own sibling file.

use crate::error::Result;
use crate::plan::difference::{Known, between};
use crate::types::{
    ActionKey, CacheRow, DepProducts, DirtyReason, Forcing, KeyComponents, Label, NodeId,
    OwnComponents, PlannedKey, Produced, Rerun, TargetGraph, Unfinished, Verdict,
};

/// One dependency of the target being decided.
///
/// The owned value: it carries the id a key awaits the dependency by, and the products read
/// from its cache row. A comparison borrows the label and the products from it and leaves the
/// id.
#[derive(Debug)]
pub(crate) struct Dependency<'a> {
    /// The dependency's id.
    pub(crate) id: NodeId,
    /// Its label.
    pub(crate) label: &'a Label,
    /// What its last build produced, when it is clean; `None` when it is not, because what it
    /// will produce is not known before it runs.
    pub(crate) products: Option<Vec<Produced>>,
}

/// The verdict for one target.
///
/// `deps` holds the target's dependencies in the order they were written, and `row` what its
/// last build recorded. The key is finished when every dependency is clean; otherwise it is left
/// unfinished, awaiting the dependencies that are not. The rules, tried in order:
///
/// 1. a target declared to always run is dirty;
/// 2. a test the run asked to run again is dirty, with its key finished or not, so it is never
///    clean and never conditional;
/// 3. a target with no row is dirty: it was never built;
/// 4. with a finished key, it is clean when the key's digest equals the row's, and dirty with
///    every difference otherwise;
/// 5. with an unfinished key, it is dirty when something already known differs, and conditional
///    when nothing does.
///
/// `forcing` carries the first two rules: what makes the target run whatever its row says.
///
/// # Errors
///
/// Returns [`Error::CanonicalJson`](crate::Error::CanonicalJson) when a finished key cannot be
/// serialized.
pub(crate) fn decide(
    forcing: Forcing,
    own: OwnComponents,
    deps: &[Dependency<'_>],
    row: Option<&CacheRow>,
) -> Result<Verdict> {
    let known: Vec<Known<'_>> = deps
        .iter()
        .map(|dep| Known {
            label: dep.label,
            products: dep.products.as_deref(),
        })
        .collect();
    let differences = row.map(|row| between(&own, &known, row.key().components()));

    let awaiting: Vec<NodeId> = deps
        .iter()
        .filter(|dep| dep.products.is_none())
        .map(|dep| dep.id)
        .collect();
    let key = if awaiting.is_empty() {
        let deps = deps
            .iter()
            .map(|dep| DepProducts {
                label: dep.label.clone(),
                outputs: dep.products.clone().unwrap_or_default(),
            })
            .collect();
        PlannedKey::Finished(ActionKey::new(KeyComponents { own, deps })?)
    } else {
        PlannedKey::Unfinished(Unfinished { own, awaiting })
    };

    match forcing {
        Forcing::Always => {
            return Ok(Verdict::Dirty {
                reason: DirtyReason::Always,
                key,
            });
        }
        Forcing::Rerun => {
            return Ok(Verdict::Dirty {
                reason: DirtyReason::Rerun,
                key,
            });
        }
        Forcing::None => {}
    }
    let (Some(row), Some(differences)) = (row, differences) else {
        return Ok(Verdict::Dirty {
            reason: DirtyReason::NeverBuilt,
            key,
        });
    };
    Ok(match key {
        PlannedKey::Finished(key) if key.digest() == row.key().digest() => Verdict::Clean { key },
        PlannedKey::Unfinished(unfinished) if differences.is_empty() => {
            Verdict::Conditional(unfinished)
        }
        key => Verdict::Dirty {
            reason: DirtyReason::Changed(differences),
            key,
        },
    })
}

/// The verdict for every target of `graph`, in id order.
///
/// `own` holds one set of components per node, in id order; `order` is the graph's nodes with
/// each after all it depends on; `rerun` says whether the run's tests run again whatever their
/// rows say; `row` answers with what a target's last build recorded.
///
/// # Errors
///
/// Returns whatever [`decide`] returns.
pub(crate) fn all<'r>(
    graph: &TargetGraph,
    own: Vec<OwnComponents>,
    order: &[NodeId],
    rerun: Rerun,
    row: impl Fn(&Label) -> Option<&'r CacheRow>,
) -> Result<Vec<Verdict>> {
    let mut own: Vec<Option<OwnComponents>> = own.into_iter().map(Some).collect();
    let mut verdicts: Vec<Option<Verdict>> = vec![None; graph.len()];
    for id in order {
        let components = own.get_mut(id.index()).and_then(Option::take);
        let (Some(node), Some(components)) = (graph.node(*id), components) else {
            continue;
        };
        let deps: Vec<Dependency<'_>> = graph
            .deps(*id)
            .unwrap_or_default()
            .iter()
            .filter_map(|dep| {
                let label = &graph.node(*dep)?.label;
                let clean = matches!(verdicts.get(dep.index()), Some(Some(Verdict::Clean { .. })));
                let products = clean.then(|| row(label)).flatten().map(CacheRow::produced);
                Some(Dependency {
                    id: *dep,
                    label,
                    products,
                })
            })
            .collect();
        let forcing = Forcing::of(node.freshness, node.role, rerun);
        let verdict = decide(forcing, components, &deps, row(&node.label))?;
        if let Some(slot) = verdicts.get_mut(id.index()) {
            *slot = Some(verdict);
        }
    }
    Ok(verdicts.into_iter().flatten().collect())
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeMap;

    use super::{Dependency, all, decide};
    use crate::grants::fixtures::{command, graph, label, node};
    use crate::plan::fixtures::{components, produced};
    use crate::plan::order::topological;
    use crate::types::{
        ActionKey, CacheRow, DepProducts, Difference, Digest, DirtyReason, Forcing, Freshness,
        KeyComponents, Label, NodeId, OwnComponents, PlannedKey, Produced, Rerun, TargetRole,
        Unfinished, Verdict,
    };

    /// The key of `own` over `deps`, each a label and what it produced.
    fn key(own: OwnComponents, deps: &[(&str, Vec<Produced>)]) -> ActionKey {
        let deps = deps
            .iter()
            .map(|(raw, outputs)| DepProducts {
                label: label(raw),
                outputs: outputs.clone(),
            })
            .collect();
        ActionKey::new(KeyComponents { own, deps }).unwrap()
    }

    /// The row of a build under `key` whose one output had the content `content`, and whose
    /// action printed nothing.
    fn row(key: ActionKey, content: &str) -> CacheRow {
        let nothing = Digest::of(b"");
        CacheRow::new(key, vec![Digest::of(content.as_bytes())], nothing, nothing).unwrap()
    }

    fn clean<'a>(id: u32, label: &'a Label, name: &str, content: &str) -> Dependency<'a> {
        Dependency {
            id: NodeId::new(id),
            label,
            products: Some(vec![produced(name, content)]),
        }
    }

    const fn not_clean(id: u32, label: &Label) -> Dependency<'_> {
        Dependency {
            id: NodeId::new(id),
            label,
            products: None,
        }
    }

    fn app() -> OwnComponents {
        components("app", |_| {})
    }

    #[test]
    fn a_target_that_always_runs_is_dirty_whatever_was_recorded() {
        let recorded = row(key(app(), &[]), "app v1");
        let verdict = decide(Forcing::Always, app(), &[], Some(&recorded)).unwrap();
        assert_eq!(
            verdict,
            Verdict::Dirty {
                reason: DirtyReason::Always,
                key: PlannedKey::Finished(key(app(), &[])),
            }
        );
    }

    #[test]
    fn a_rerun_test_with_an_unchanged_key_is_dirty_for_a_rerun() {
        let recorded = row(key(app(), &[]), "app v1");
        let verdict = decide(Forcing::Rerun, app(), &[], Some(&recorded)).unwrap();
        assert_eq!(
            verdict,
            Verdict::Dirty {
                reason: DirtyReason::Rerun,
                key: PlannedKey::Finished(key(app(), &[])),
            }
        );
    }

    #[test]
    fn a_rerun_test_waiting_on_a_dep_is_dirty_not_conditional() {
        let text = label("//lib:text");
        let deps = [not_clean(1, &text)];
        let recorded = key(app(), &[("//lib:text", vec![produced("text", "text v1")])]);
        let verdict = decide(Forcing::Rerun, app(), &deps, Some(&row(recorded, "app v1"))).unwrap();
        assert_eq!(
            verdict,
            Verdict::Dirty {
                reason: DirtyReason::Rerun,
                key: PlannedKey::Unfinished(Unfinished {
                    own: app(),
                    awaiting: vec![NodeId::new(1)],
                }),
            }
        );
    }

    #[test]
    fn a_test_declared_always_keeps_always_under_a_rerun() {
        let recorded = row(key(app(), &[]), "app v1");
        let forcing = Forcing::of(Freshness::Always, TargetRole::Test, Rerun::Tests);
        let verdict = decide(forcing, app(), &[], Some(&recorded)).unwrap();
        assert_eq!(
            verdict,
            Verdict::Dirty {
                reason: DirtyReason::Always,
                key: PlannedKey::Finished(key(app(), &[])),
            }
        );
    }

    #[test]
    fn a_target_with_no_row_was_never_built() {
        let verdict = decide(Forcing::None, app(), &[], None).unwrap();
        assert_eq!(
            verdict,
            Verdict::Dirty {
                reason: DirtyReason::NeverBuilt,
                key: PlannedKey::Finished(key(app(), &[])),
            }
        );
    }

    #[test]
    fn a_never_built_target_awaits_its_dependencies_that_are_not_clean() {
        let text = label("//lib:text");
        let util = label("//lib:util");
        let deps = [not_clean(4, &text), clean(2, &util, "util", "util v1")];
        let verdict = decide(Forcing::None, app(), &deps, None).unwrap();
        assert_eq!(
            verdict,
            Verdict::Dirty {
                reason: DirtyReason::NeverBuilt,
                key: PlannedKey::Unfinished(Unfinished {
                    own: app(),
                    awaiting: vec![NodeId::new(4)],
                }),
            }
        );
    }

    #[test]
    fn a_target_whose_key_equals_the_recorded_one_is_clean() {
        let text = label("//lib:text");
        let deps = [clean(1, &text, "text", "text v1")];
        let recorded = key(app(), &[("//lib:text", vec![produced("text", "text v1")])]);
        let verdict = decide(
            Forcing::None,
            app(),
            &deps,
            Some(&row(recorded.clone(), "app v1")),
        )
        .unwrap();
        assert_eq!(verdict, Verdict::Clean { key: recorded });
    }

    #[test]
    fn a_settled_target_whose_key_differs_is_dirty_with_every_difference() {
        let recorded = row(key(app(), &[]), "app v1");
        let changed = components("app", |own| own.command = command(&["cc", "-O2"]));
        let verdict = decide(Forcing::None, changed.clone(), &[], Some(&recorded)).unwrap();
        assert_eq!(
            verdict,
            Verdict::Dirty {
                reason: DirtyReason::Changed(vec![Difference::Command]),
                key: PlannedKey::Finished(key(changed, &[])),
            }
        );
    }

    #[test]
    fn a_clean_dependency_that_produced_something_else_makes_its_dependent_dirty() {
        let text = label("//lib:text");
        let deps = [clean(1, &text, "text", "text v2")];
        let recorded = key(app(), &[("//lib:text", vec![produced("text", "text v1")])]);
        let verdict = decide(Forcing::None, app(), &deps, Some(&row(recorded, "app v1"))).unwrap();
        assert_eq!(
            verdict,
            Verdict::Dirty {
                reason: DirtyReason::Changed(vec![Difference::Dep(label("//lib:text"))]),
                key: PlannedKey::Finished(key(
                    app(),
                    &[("//lib:text", vec![produced("text", "text v2")])]
                )),
            }
        );
    }

    #[test]
    fn an_unsettled_target_with_a_known_difference_is_dirty_and_its_key_unfinished() {
        let text = label("//lib:text");
        let deps = [not_clean(1, &text)];
        let recorded = key(app(), &[("//lib:text", vec![produced("text", "text v1")])]);
        let changed = components("app", |own| own.command = command(&["cc", "-O2"]));
        let verdict = decide(
            Forcing::None,
            changed.clone(),
            &deps,
            Some(&row(recorded, "app v1")),
        )
        .unwrap();
        assert_eq!(
            verdict,
            Verdict::Dirty {
                reason: DirtyReason::Changed(vec![Difference::Command]),
                key: PlannedKey::Unfinished(Unfinished {
                    own: changed,
                    awaiting: vec![NodeId::new(1)],
                }),
            }
        );
    }

    #[test]
    fn an_unsettled_target_with_nothing_known_to_differ_is_conditional() {
        let text = label("//lib:text");
        let util = label("//lib:util");
        let other = label("//lib:other");
        let deps = [
            not_clean(7, &util),
            clean(1, &text, "text", "text v1"),
            not_clean(3, &other),
        ];
        let recorded = key(
            app(),
            &[
                ("//lib:util", vec![produced("util", "util v1")]),
                ("//lib:text", vec![produced("text", "text v1")]),
                ("//lib:other", vec![produced("other", "other v1")]),
            ],
        );
        let verdict = decide(Forcing::None, app(), &deps, Some(&row(recorded, "app v1"))).unwrap();
        assert_eq!(
            verdict,
            Verdict::Conditional(Unfinished {
                own: app(),
                awaiting: vec![NodeId::new(7), NodeId::new(3)],
            }),
            "awaited dependencies keep the order they were written in"
        );
    }

    #[test]
    fn an_always_target_with_an_unclean_dependency_keeps_an_unfinished_key() {
        let text = label("//lib:text");
        let deps = [not_clean(1, &text)];
        let verdict = decide(Forcing::Always, app(), &deps, None).unwrap();
        assert_eq!(
            verdict,
            Verdict::Dirty {
                reason: DirtyReason::Always,
                key: PlannedKey::Unfinished(Unfinished {
                    own: app(),
                    awaiting: vec![NodeId::new(1)],
                }),
            }
        );
    }

    /// `//:app` (0) depends on `//:lib` (1), which depends on `//:z` (3); `//:solo` (2) stands
    /// alone. Ids are in label order, so the chain runs against them.
    fn chain() -> crate::types::TargetGraph {
        graph(
            vec![
                node("//:app", |_| {}),
                node("//:lib", |_| {}),
                node("//:solo", |_| {}),
                node("//:z", |_| {}),
            ],
            &[&[1], &[3], &[], &[]],
        )
    }

    fn own_of_chain() -> Vec<OwnComponents> {
        ["app", "lib", "solo", "z"]
            .map(|output| components(output, |_| {}))
            .to_vec()
    }

    /// The rows a build of [`chain`] records, with `z`'s output content as given.
    fn rows_of_chain(z_content: &str) -> BTreeMap<Label, CacheRow> {
        let z = row(key(components("z", |_| {}), &[]), z_content);
        let lib = row(
            key(components("lib", |_| {}), &[("//:z", z.produced())]),
            "lib v1",
        );
        let app = row(
            key(components("app", |_| {}), &[("//:lib", lib.produced())]),
            "app v1",
        );
        let solo = row(key(components("solo", |_| {}), &[]), "solo v1");
        BTreeMap::from([
            (label("//:app"), app),
            (label("//:lib"), lib),
            (label("//:solo"), solo),
            (label("//:z"), z),
        ])
    }

    fn verdicts_of_chain(
        own: Vec<OwnComponents>,
        rows: &BTreeMap<Label, CacheRow>,
    ) -> Vec<Verdict> {
        let graph = chain();
        let order = topological(&graph);
        all(&graph, own, &order, Rerun::Off, |label| rows.get(label)).unwrap()
    }

    #[test]
    fn with_no_rows_every_target_was_never_built_and_awaits_its_dependency() {
        let verdicts = verdicts_of_chain(own_of_chain(), &BTreeMap::new());
        let never = |output: &str, awaiting: &[u32]| Verdict::Dirty {
            reason: DirtyReason::NeverBuilt,
            key: if awaiting.is_empty() {
                PlannedKey::Finished(key(components(output, |_| {}), &[]))
            } else {
                PlannedKey::Unfinished(Unfinished {
                    own: components(output, |_| {}),
                    awaiting: awaiting.iter().copied().map(NodeId::new).collect(),
                })
            },
        };
        assert_eq!(
            verdicts,
            [
                never("app", &[1]),
                never("lib", &[3]),
                never("solo", &[]),
                never("z", &[]),
            ]
        );
    }

    #[test]
    fn with_the_rows_of_a_build_every_target_is_clean() {
        let rows = rows_of_chain("z v1");
        let verdicts = verdicts_of_chain(own_of_chain(), &rows);
        for (verdict, raw) in verdicts.iter().zip(["//:app", "//:lib", "//:solo", "//:z"]) {
            let recorded = rows.get(&label(raw)).unwrap().key().clone();
            assert_eq!(verdict, &Verdict::Clean { key: recorded }, "{raw}");
        }
    }

    #[test]
    fn a_rerun_leaves_a_clean_build_target_clean() {
        let rows = rows_of_chain("z v1");
        let graph = chain();
        let order = topological(&graph);
        let verdicts = all(&graph, own_of_chain(), &order, Rerun::Tests, |label| {
            rows.get(label)
        })
        .unwrap();
        assert!(
            verdicts
                .iter()
                .all(|verdict| matches!(verdict, Verdict::Clean { .. })),
            "{verdicts:?}"
        );
    }

    #[test]
    fn a_rerun_makes_a_clean_test_dirty_and_leaves_the_rest_clean() {
        let rows = rows_of_chain("z v1");
        let graph = graph(
            vec![
                node("//:app", |_| {}),
                node("//:lib", |_| {}),
                node("//:solo", |node| node.role = TargetRole::Test),
                node("//:z", |_| {}),
            ],
            &[&[1], &[3], &[], &[]],
        );
        let order = topological(&graph);
        let verdicts_under = |rerun| {
            all(&graph, own_of_chain(), &order, rerun, |label| {
                rows.get(label)
            })
            .unwrap()
        };
        let solo = rows.get(&label("//:solo")).unwrap().key().clone();

        let rerun = verdicts_under(Rerun::Tests);
        assert_eq!(
            rerun[2],
            Verdict::Dirty {
                reason: DirtyReason::Rerun,
                key: PlannedKey::Finished(solo.clone()),
            }
        );
        for index in [0, 1, 3] {
            assert!(matches!(rerun[index], Verdict::Clean { .. }), "{rerun:?}");
        }
        assert_eq!(verdicts_under(Rerun::Off)[2], Verdict::Clean { key: solo });
    }

    #[test]
    fn everything_downstream_of_a_dirty_target_is_conditional() {
        let rows = rows_of_chain("z v1");
        let mut own = own_of_chain();
        own[3].command = command(&["cc", "-O2"]);
        let verdicts = verdicts_of_chain(own, &rows);
        assert!(matches!(
            &verdicts[3],
            Verdict::Dirty {
                reason: DirtyReason::Changed(differences),
                key: PlannedKey::Finished(_),
            } if differences == &[Difference::Command]
        ));
        assert_eq!(
            verdicts[1],
            Verdict::Conditional(Unfinished {
                own: components("lib", |_| {}),
                awaiting: vec![NodeId::new(3)],
            })
        );
        assert_eq!(
            verdicts[0],
            Verdict::Conditional(Unfinished {
                own: components("app", |_| {}),
                awaiting: vec![NodeId::new(1)],
            })
        );
        assert!(matches!(verdicts[2], Verdict::Clean { .. }));
    }

    #[test]
    fn a_dependency_rebuilt_to_the_same_output_leaves_its_dependents_clean() {
        let mut rows = rows_of_chain("z v1");
        let mut own = own_of_chain();
        own[3].command = command(&["cc", "-O2"]);
        // `//:z` was rebuilt under its new key and produced the same output.
        rows.insert(label("//:z"), row(key(own[3].clone(), &[]), "z v1"));
        let verdicts = verdicts_of_chain(own, &rows);
        assert!(
            verdicts
                .iter()
                .all(|verdict| matches!(verdict, Verdict::Clean { .. })),
            "{verdicts:?}"
        );
    }

    #[test]
    fn a_dependency_rebuilt_to_another_output_makes_its_dependent_dirty() {
        let mut rows = rows_of_chain("z v1");
        let own = own_of_chain();
        rows.insert(label("//:z"), row(key(own[3].clone(), &[]), "z v2"));
        let verdicts = verdicts_of_chain(own, &rows);
        assert!(matches!(verdicts[3], Verdict::Clean { .. }));
        assert!(matches!(
            &verdicts[1],
            Verdict::Dirty {
                reason: DirtyReason::Changed(differences),
                key: PlannedKey::Finished(_),
            } if differences == &[Difference::Dep(label("//:z"))]
        ));
        assert!(matches!(verdicts[0], Verdict::Conditional(_)));
    }
}
