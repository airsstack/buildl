//! `Pipeline::verify`, driven through every port by the fakes of [`crate::common`].

#![expect(
    clippy::unwrap_used,
    reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
)]

use core::num::NonZeroUsize;
use std::collections::BTreeMap;

use buildl_core::{
    ActionOutcome, Check, Digest, EntryChange, EntryName, Error, FileChange, FileMode, FileStat,
    Freshness, Label, LiveOutput, OnFailure, OutcomeClass, OutputDifference, OutputManifest,
    OutputName, OutputPath, OutputRefusal, Pipeline, Rerun, Sample, Scope, SourcePath, TargetRole,
    Timestamp, TreePath, Verification, Written,
};

use crate::common::{
    Answer, Content, Ends, FakeApprover, FakeContent, FakePorts, FakeSource, Loss, Script, Touch,
    dir, ending, file, label, ports, program, root_only, script, target, target_with, tree,
    workspace,
};

fn entry() -> EntryName {
    EntryName::parse("build.lua").unwrap()
}

/// Where the output `name` of the target `raw` names is kept.
fn out(raw: &str, name: &str) -> OutputPath {
    OutputPath::of(&dir("out"), &label(raw), &OutputName::parse(name).unwrap())
}

/// A build of the whole workspace that is expected to succeed.
fn build(pipeline: &mut Pipeline<FakePorts>) -> Vec<ActionOutcome> {
    pipeline
        .build(
            &entry(),
            &Scope::Workspace,
            &BTreeMap::new(),
            OnFailure::Stop,
            LiveOutput::Off,
        )
        .unwrap()
}

/// A build of the targets `labels` name, which is expected to succeed.
fn build_named(pipeline: &mut Pipeline<FakePorts>, labels: &[&str]) {
    pipeline
        .build(
            &entry(),
            &Scope::Targets(labels.iter().map(|raw| label(raw)).collect()),
            &BTreeMap::new(),
            OnFailure::Stop,
            LiveOutput::Off,
        )
        .unwrap();
}

/// A verify of `scope`.
fn verify_scope(
    pipeline: &mut Pipeline<FakePorts>,
    scope: &Scope,
    sample: Sample,
) -> Result<Verification, Error> {
    pipeline.verify(&entry(), scope, &BTreeMap::new(), sample)
}

/// A verify of the whole workspace, re-running everything cached, that is expected to succeed.
fn verify(pipeline: &mut Pipeline<FakePorts>) -> Verification {
    verify_scope(pipeline, &Scope::Workspace, Sample::All).unwrap()
}

/// The check of the target `raw` names.
fn check<'v>(verification: &'v Verification, raw: &str) -> &'v Check {
    let wanted = label(raw);
    verification
        .checked()
        .iter()
        .find(|(label, _)| *label == wanted)
        .map(|(_, check)| check)
        .unwrap()
}

/// The labels of the targets that were checked, as text, in the order reported.
fn checked_labels(verification: &Verification) -> Vec<String> {
    verification
        .checked()
        .iter()
        .map(|(label, _)| label.to_string())
        .collect()
}

/// A script in which the action leaves `content` at its output `name`.
fn leaving(raw: &str, name: &str, content: Content) -> Script {
    Script {
        outputs: BTreeMap::from([(out(raw, name), content)]),
        ..Script::default()
    }
}

/// The digest of the manifest the row of the target `raw` names holds for its first output.
fn manifest_digest(pipeline: &Pipeline<FakePorts>, raw: &str) -> Digest {
    pipeline.ports().cache.rows[&label(raw)].outputs()[0]
}

/// A pipeline over a chain `a` <- `b` <- `c`, each target depending on the one before it.
fn chain() -> Pipeline<FakePorts> {
    root_only(vec![
        target("a", &[]),
        target("b", &["a"]),
        target("c", &["b"]),
    ])
}

#[test]
fn a_target_that_reproduces_its_outputs_is_verified() {
    let mut pipeline = root_only(vec![target("app", &[])]);
    build(&mut pipeline);

    let verification = verify(&mut pipeline);

    assert_eq!(checked_labels(&verification), ["//:app"]);
    assert_eq!(check(&verification, "//:app"), &Check::Verified);
    assert_eq!(verification.not_cached(), 0);
    assert_eq!(verification.not_sampled(), 0);
    assert_eq!(
        pipeline.ports().strategy.runs(&label("//:app")),
        2,
        "the build ran it once, the verify again"
    );
}

#[test]
fn a_target_whose_rerun_writes_other_bytes_is_mismatched_naming_the_output() {
    let mut pipeline = root_only(vec![target("app", &[]), target("docs", &[])]);
    build(&mut pipeline);
    script(
        &mut pipeline,
        "//:app",
        leaving("//:app", "app", Content::Holds(b"other".to_vec())),
    );

    let verification = verify(&mut pipeline);

    assert_eq!(
        check(&verification, "//:app"),
        &Check::Mismatched {
            output: OutputName::parse("app").unwrap(),
            difference: OutputDifference::File(FileChange::Bytes),
        }
    );
    assert_eq!(check(&verification, "//:docs"), &Check::Verified);
}

#[test]
fn a_rerun_that_changes_only_a_mode_is_mismatched_in_mode() {
    let mut pipeline = root_only(vec![target("app", &[])]);
    script(
        &mut pipeline,
        "//:app",
        leaving("//:app", "app", Content::Holds(b"same".to_vec())),
    );
    build(&mut pipeline);
    script(
        &mut pipeline,
        "//:app",
        leaving("//:app", "app", Content::Runs(b"same".to_vec())),
    );

    let verification = verify(&mut pipeline);

    assert_eq!(
        check(&verification, "//:app"),
        &Check::Mismatched {
            output: OutputName::parse("app").unwrap(),
            difference: OutputDifference::File(FileChange::Mode),
        }
    );
}

#[test]
fn a_tree_output_mismatch_names_the_first_differing_file() {
    let mut pipeline = root_only(vec![target("site", &[])]);
    script(
        &mut pipeline,
        "//:site",
        leaving(
            "//:site",
            "site",
            tree(&[
                ("index.html", "page"),
                ("sub/b.h", "two"),
                ("z.css", "style"),
            ]),
        ),
    );
    build(&mut pipeline);
    script(
        &mut pipeline,
        "//:site",
        leaving(
            "//:site",
            "site",
            tree(&[
                ("index.html", "page"),
                ("sub/b.h", "TWO"),
                ("z.css", "STYLE"),
            ]),
        ),
    );

    let verification = verify(&mut pipeline);

    assert_eq!(
        check(&verification, "//:site"),
        &Check::Mismatched {
            output: OutputName::parse("site").unwrap(),
            difference: OutputDifference::Entry {
                path: TreePath::parse("sub/b.h".to_owned()).unwrap(),
                change: EntryChange::Bytes,
            },
        }
    );
}

#[test]
fn a_failed_rerun_carries_its_class_and_what_it_printed() {
    let mut pipeline = root_only(vec![target("a", &[]), target("b", &[]), target("c", &[])]);
    build(&mut pipeline);
    script(
        &mut pipeline,
        "//:b",
        Script {
            ends: Ends::Exits(3),
            stdout: Some(b"partial\n".to_vec()),
            stderr: Some(b"bad\n".to_vec()),
            ..Script::default()
        },
    );

    let verification = verify(&mut pipeline);

    assert_eq!(
        check(&verification, "//:b"),
        &Check::Failed {
            class: OutcomeClass::ActionFailure { status: 3 },
            diagnostic: None,
            stdout: b"partial\n".to_vec(),
            stderr: b"bad\n".to_vec(),
        }
    );
    assert_eq!(
        check(&verification, "//:a"),
        &Check::Verified,
        "the re-runs go on"
    );
    assert_eq!(check(&verification, "//:c"), &Check::Verified);
}

#[test]
fn a_discovery_target_whose_rerun_leaves_a_directory_is_refused_as_not_a_file() {
    let mut pipeline = root_only(vec![target_with("pkgs", |target| {
        target.role = TargetRole::Discovery;
    })]);
    build_named(&mut pipeline, &["//:pkgs"]);
    script(
        &mut pipeline,
        "//:pkgs",
        leaving("//:pkgs", "pkgs.json", tree(&[("a.json", "{}")])),
    );

    let verification = verify_scope(
        &mut pipeline,
        &Scope::Targets([label("//:pkgs")].into()),
        Sample::All,
    )
    .unwrap();

    let Check::Failed { class, .. } = check(&verification, "//:pkgs") else {
        unreachable!("expected a failed check, got {verification:?}");
    };
    assert_eq!(
        class,
        &OutcomeClass::OutputRefused {
            name: OutputName::parse("pkgs.json").unwrap(),
            refusal: OutputRefusal::NotAFile,
        }
    );
}

#[test]
fn a_rerun_that_cannot_begin_fails_with_what_the_builder_said() {
    let mut pipeline = root_only(vec![target("app", &[])]);
    build(&mut pipeline);
    script(&mut pipeline, "//:app", ending(Ends::PrepareFails));

    let verification = verify(&mut pipeline);

    match check(&verification, "//:app") {
        Check::Failed {
            class,
            diagnostic,
            stdout,
            stderr,
        } => {
            assert!(matches!(class, OutcomeClass::InfraFailure { .. }));
            assert!(diagnostic.is_some());
            assert!(stdout.is_empty() && stderr.is_empty());
        }
        other => unreachable!("expected a failed check, got {other:?}"),
    }
}

/// A workspace with a clean target, one never built, one that always runs, a dirty one and the
/// target above it, built except for the one never built.
fn mixed() -> Pipeline<FakePorts> {
    let reading = |name: &str, deps: &[&str]| {
        target_with(name, |staged| {
            staged.inputs = vec![Written::new("base.c")];
            staged.deps = deps.iter().map(|dep| Written::new(*dep)).collect();
        })
    };
    let source = FakeSource::new(workspace(vec![(
        "",
        file(
            vec![
                target("built", &[]),
                target("never", &[]),
                target_with("always", |staged| staged.freshness = Freshness::Always),
                reading("base", &[]),
                target("top", &["base"]),
            ],
            &[],
        ),
    )]));
    let mut pipeline = Pipeline::new(ports(source));
    pipeline
        .ports_mut()
        .tools
        .fingerprints
        .insert(program("cc"), Digest::of(b"cc 14.2"));
    edit(&mut pipeline, "base.c", "one", 100);
    build_named(
        &mut pipeline,
        &["//:built", "//:always", "//:base", "//:top"],
    );
    pipeline
}

/// Writes or replaces a workspace file holding `text`, last modified at `modified`.
fn edit(pipeline: &mut Pipeline<FakePorts>, raw: &str, text: &str, modified: u64) {
    let path = SourcePath::parse(raw).unwrap();
    let content = FakeContent {
        bytes: text.as_bytes().to_vec(),
        stat: FileStat {
            modified: Timestamp::from_unix_nanos(modified),
            size: u64::try_from(text.len()).unwrap(),
        },
    };
    let ports = pipeline.ports_mut();
    ports.digester.files.insert(path.clone(), content);
    ports.strategy.files.insert(path, text.as_bytes().to_vec());
}

#[test]
fn targets_that_are_not_clean_are_counted_not_cached() {
    let mut pipeline = mixed();
    edit(&mut pipeline, "base.c", "two", 200);

    let verification = verify(&mut pipeline);

    assert_eq!(
        checked_labels(&verification),
        ["//:built"],
        "only the clean target is checked"
    );
    assert_eq!(
        verification.not_cached(),
        4,
        "never built, always, dirty, and conditional on the dirty one"
    );
    assert_eq!(verification.not_sampled(), 0);
    let strategy = &pipeline.ports().strategy;
    assert_eq!(
        strategy.runs(&label("//:never")) + strategy.runs(&label("//:top")),
        1,
        "only `top` ran, in the build"
    );
}

#[test]
fn a_clean_target_whose_content_is_gone_is_counted_not_cached() {
    let mut pipeline = root_only(vec![target("a", &[]), target("b", &[])]);
    build(&mut pipeline);
    let gone = manifest_digest(&pipeline, "//:a");
    pipeline.ports().cas.forget(&gone);

    let verification = verify(&mut pipeline);

    assert_eq!(checked_labels(&verification), ["//:b"]);
    assert_eq!(verification.not_cached(), 1);
    assert_eq!(pipeline.ports().strategy.runs(&label("//:a")), 1);
}

#[test]
fn a_cached_manifest_that_does_not_hash_to_its_digest_is_counted_not_cached() {
    let mut pipeline = root_only(vec![target("a", &[]), target("b", &[])]);
    build(&mut pipeline);
    let (held_as_a, of_b) = (
        manifest_digest(&pipeline, "//:a"),
        manifest_digest(&pipeline, "//:b"),
    );
    let bytes_of_b = pipeline.ports().cas.blob(&of_b).unwrap();
    assert!(OutputManifest::from_bytes(&bytes_of_b).is_some());
    pipeline.ports().cas.corrupt(held_as_a, bytes_of_b);

    let verification = verify(&mut pipeline);

    assert_eq!(checked_labels(&verification), ["//:b"]);
    assert_eq!(verification.not_cached(), 1);
    assert_eq!(pipeline.ports().strategy.runs(&label("//:a")), 1);
}

#[test]
fn a_target_whose_dependency_is_not_cached_is_skipped_naming_it() {
    let mut pipeline = chain();
    build(&mut pipeline);
    let gone = manifest_digest(&pipeline, "//:a");
    pipeline.ports().cas.forget(&gone);

    let verification = verify(&mut pipeline);

    assert_eq!(checked_labels(&verification), ["//:b", "//:c"]);
    assert_eq!(
        check(&verification, "//:b"),
        &Check::Skipped {
            because: label("//:a")
        }
    );
    assert_eq!(
        check(&verification, "//:c"),
        &Check::Verified,
        "b's outputs are in place, so c is not skipped for it"
    );
    assert_eq!(verification.not_cached(), 1);
    let strategy = &pipeline.ports().strategy;
    assert_eq!(strategy.runs(&label("//:b")), 1, "only the build ran it");
    assert_eq!(strategy.runs(&label("//:c")), 2);
}

#[test]
fn a_failed_restore_is_unplaced_and_skips_its_dependents() {
    let mut pipeline = chain();
    build(&mut pipeline);
    pipeline.ports().cas.fail_place(out("//:a", "a"));

    let verification = verify(&mut pipeline);

    match check(&verification, "//:a") {
        Check::Unplaced { diagnostic } => {
            assert!(diagnostic.to_string().contains("cannot be written"));
        }
        other => unreachable!("expected an unplaced check, got {other:?}"),
    }
    assert_eq!(
        check(&verification, "//:b"),
        &Check::Skipped {
            because: label("//:a")
        }
    );
    assert_eq!(check(&verification, "//:c"), &Check::Verified);
    assert_eq!(verification.not_cached(), 0);
}

#[test]
fn a_sample_reruns_only_the_lowest_keys_and_counts_the_rest() {
    let names = ["a", "b", "c", "d", "e"];
    let mut pipeline = root_only(names.iter().map(|name| target(name, &[])).collect());
    build(&mut pipeline);
    let mut by_key: Vec<(Digest, String)> = names
        .iter()
        .map(|name| {
            let raw = format!("//:{name}");
            (
                pipeline.ports().cache.rows[&label(&raw)].key().digest(),
                raw,
            )
        })
        .collect();
    by_key.sort();
    let mut lowest: Vec<String> = by_key.into_iter().take(2).map(|(_, raw)| raw).collect();
    lowest.sort();

    let verification = verify_scope(
        &mut pipeline,
        &Scope::Workspace,
        Sample::Lowest(NonZeroUsize::new(2).unwrap()),
    )
    .unwrap();

    assert_eq!(checked_labels(&verification), lowest);
    assert_eq!(verification.not_sampled(), 3);
    assert_eq!(verification.not_cached(), 0);
    assert_eq!(
        pipeline.ports().strategy.ran().len(),
        names.len() + 2,
        "the build ran every target, the verify two"
    );
}

#[test]
fn every_clean_target_is_put_back_in_place_before_anything_reruns() {
    let mut pipeline = chain();
    build(&mut pipeline);
    let before = pipeline.ports().cas.touches().len();

    verify(&mut pipeline);

    let touches = pipeline.ports().cas.touches();
    let after = touches.get(before..).unwrap();
    let last_placed = after.iter().rposition(|touch| *touch == Touch::Placed);
    let first_stored = after.iter().position(|touch| *touch == Touch::Stored);
    assert_eq!(
        after
            .iter()
            .filter(|touch| **touch == Touch::Placed)
            .count(),
        3
    );
    assert!(
        last_placed.unwrap() < first_stored.unwrap(),
        "no re-run stored anything before the last target was put back: {after:?}"
    );
}

#[test]
fn verify_records_no_row_forgets_no_row_and_logs_nothing() {
    let mut pipeline = root_only(vec![target("a", &[]), target("b", &[])]);
    build(&mut pipeline);
    script(
        &mut pipeline,
        "//:a",
        leaving("//:a", "a", Content::Holds(b"other".to_vec())),
    );
    script(&mut pipeline, "//:b", ending(Ends::Exits(1)));
    let rows = pipeline.ports().cache.rows.clone();
    let events = pipeline.ports().log.events.clone();
    let reported = pipeline.ports().reporter.calls.clone();
    let recorded_graphs = pipeline.ports().graph.recorded.clone();
    let ports = pipeline.ports_mut();
    ports.cache.failing = rows.keys().cloned().collect();
    ports.cache.unforgettable = rows.keys().cloned().collect();
    ports.log.broken = true;
    ports.graph.broken = true;

    let verification = verify(&mut pipeline);

    assert!(matches!(
        check(&verification, "//:a"),
        Check::Mismatched { .. }
    ));
    assert!(matches!(check(&verification, "//:b"), Check::Failed { .. }));
    let ports = pipeline.ports();
    assert_eq!(ports.cache.recorded().len(), 2, "only the build recorded");
    assert_eq!(ports.cache.forgotten(), Vec::<Label>::new());
    assert_eq!(ports.cache.rows, rows);
    assert_eq!(ports.log.events, events);
    assert_eq!(ports.reporter.calls, reported);
    assert_eq!(ports.graph.recorded, recorded_graphs);
}

#[test]
fn a_reruns_outputs_are_never_placed() {
    let mut pipeline = root_only(vec![target("a", &[]), target("b", &[])]);
    build(&mut pipeline);
    script(
        &mut pipeline,
        "//:a",
        leaving("//:a", "a", Content::Holds(b"other".to_vec())),
    );
    let placed = pipeline.ports().cas.placed();
    let before = pipeline.ports().cas.placements().len();

    let verification = verify(&mut pipeline);

    assert!(matches!(
        check(&verification, "//:a"),
        Check::Mismatched { .. }
    ));
    let cas = &pipeline.ports().cas;
    let made = cas.placements().split_off(before);
    assert_eq!(made.len(), 2, "one restore per cached target, nothing more");
    assert_eq!(cas.placed(), placed, "out/ holds what the build left");
    assert_eq!(
        cas.modes().values().copied().collect::<Vec<_>>(),
        [FileMode::Plain, FileMode::Plain]
    );
}

#[test]
fn a_task_the_dispatcher_loses_is_a_dispatcher_fault() {
    for loss in [Loss::Unrun, Loss::Unreported] {
        let mut pipeline = root_only(vec![target("a", &[]), target("b", &[])]);
        build(&mut pipeline);
        // The re-runs are taken in label order: the second task is `//:b`.
        pipeline.ports_mut().dispatcher.lose = BTreeMap::from([(1, loss)]);

        let error = verify_scope(&mut pipeline, &Scope::Workspace, Sample::All).unwrap_err();

        match error {
            Error::DispatcherFault { targets } => {
                assert_eq!(targets, [label("//:b")], "{loss:?}");
            }
            other => unreachable!("{loss:?}: expected a dispatcher fault, got {other:?}"),
        }
    }
}

#[test]
fn a_plan_of_another_graph_is_refused() {
    let mut small = root_only(vec![target("a", &[])]);
    let plan = small
        .plan(&entry(), &Scope::Workspace, &BTreeMap::new())
        .unwrap();
    let mut large = root_only(vec![target("a", &[]), target("b", &[])]);
    let graph = large.graph(&entry()).unwrap();

    let refused =
        buildl_core::verify::verify(&graph, &graph, &plan, Sample::All, large.ports_mut())
            .unwrap_err();

    assert!(matches!(refused, Error::ForeignPlan));
    assert_eq!(large.ports().strategy.ran(), Vec::<Label>::new());
}

#[test]
fn pipeline_verify_plans_like_build() {
    let mut pipeline = root_only(vec![
        target("app", &[]),
        target_with("check", |staged| staged.role = TargetRole::Test),
    ]);
    build(&mut pipeline);
    pipeline
        .test(
            &entry(),
            &Scope::Workspace,
            &BTreeMap::new(),
            OnFailure::Stop,
            LiveOutput::Off,
            Rerun::Off,
        )
        .unwrap();

    let everything = verify(&mut pipeline);
    assert_eq!(
        checked_labels(&everything),
        ["//:app"],
        "a test is left out when nothing names it"
    );

    let named = verify_scope(
        &mut pipeline,
        &Scope::Targets(vec![label("//:check")].into_iter().collect()),
        Sample::All,
    )
    .unwrap();
    assert_eq!(checked_labels(&named), ["//:check"]);
    assert_eq!(check(&named, "//:check"), &Check::Verified);
}

#[test]
fn a_verify_asks_the_approver_once() {
    let mut pipeline = root_only(vec![target("app", &[])]);
    build(&mut pipeline);
    let asked_by_build = pipeline.ports().approver.asked.len();

    verify(&mut pipeline);

    assert_eq!(pipeline.ports().approver.asked.len(), asked_by_build + 1);
}

#[test]
fn a_denied_verify_reruns_nothing() {
    let mut pipeline = root_only(vec![target("app", &[])]);
    build(&mut pipeline);
    let runs = pipeline.ports().strategy.runs(&label("//:app"));
    pipeline.ports_mut().approver = FakeApprover::answering(Answer::Deny("no".to_owned()));

    let error = verify_scope(&mut pipeline, &Scope::Workspace, Sample::All).unwrap_err();

    assert!(matches!(error, Error::NotApproved { .. }), "{error:?}");
    assert_eq!(pipeline.ports().approver.asked.len(), 1);
    assert_eq!(pipeline.ports().strategy.runs(&label("//:app")), runs);
}
