//! `pipeline::doctor`, driven through the build-file port by [`FakeSource`] and the manifest
//! port by [`FakeManifest`].

#![expect(
    clippy::unwrap_used,
    reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
)]

use std::collections::BTreeSet;

use buildl_core::{
    Checkup, Directory, EntryName, EnvName, Error, NetworkAccess, Origin, Result, StagedItem,
    StagedSetting, TargetRole, Want, Written, doctor,
};

use crate::common::{
    FakeCas, FakeDispatcher, FakeHistory, FakeManifest, FakeSource, dir, file, file_by, label,
    open_workspace, plugin, program, target, target_with, workspace,
};

fn entry() -> EntryName {
    EntryName::parse("build.lua").unwrap()
}

/// The checkup of a workspace whose root build file makes `items`, under [`open_workspace`].
fn checked(items: Vec<StagedItem>) -> Result<Checkup> {
    let source = FakeSource::new(workspace(vec![("", file(items, &[]))]));
    let manifest = FakeManifest {
        workspace: open_workspace(),
    };
    checked_by(&source, &manifest)
}

/// The checkup of `source` under `manifest`, with no run recorded.
fn checked_by(source: &FakeSource, manifest: &FakeManifest) -> Result<Checkup> {
    doctor(
        source,
        &mut FakeDispatcher::default(),
        manifest,
        &entry(),
        &FakeHistory::default(),
        &FakeCas::default(),
    )
    .map(|(checkup, _)| checkup)
}

fn env_name(raw: &str) -> EnvName {
    EnvName::parse(raw).unwrap()
}

#[test]
fn doctor_reports_wants_outside_the_ceiling_without_refusing() {
    let checkup = checked(vec![
        target("app", &[]),
        target_with("link", |target| {
            target.run = Some(vec![Written::new("ld")]);
            target.env = vec![Written::new("HOME")];
        }),
        target_with("fetch", |target| target.network = NetworkAccess::Declared),
    ])
    .unwrap();

    let wanted = checkup.wanted();
    assert_eq!(
        wanted.programs,
        BTreeSet::from([program("cc"), program("ld")])
    );
    assert_eq!(wanted.env, BTreeSet::from([env_name("HOME")]));
    assert_eq!(wanted.write, BTreeSet::from([dir(".buildl"), dir("out")]));
    assert_eq!(wanted.network, BTreeSet::from([label("//:fetch")]));

    let outside = checkup.outside();
    assert_eq!(outside.programs, BTreeSet::from([program("ld")]));
    assert_eq!(outside.env, BTreeSet::from([env_name("HOME")]));
    assert!(outside.write.is_empty());
    assert!(outside.network.is_empty());
    assert_eq!(checkup.workspace(), &open_workspace());
}

#[test]
fn doctor_uses_the_whole_graph_and_default_settings() {
    let checkup = checked(vec![
        StagedItem::Setting(StagedSetting {
            name: Written::new("tool"),
            default: Written::new("ld"),
        }),
        target_with("app", |target| {
            target.run = Some(["$opt:tool", "-o", "$out"].map(Written::new).to_vec());
        }),
        target_with("app_test", |target| {
            target.role = TargetRole::Test;
            target.run = Some(vec![Written::new("pytest")]);
        }),
    ])
    .unwrap();

    assert_eq!(
        checkup.wanted().programs,
        BTreeSet::from([program("ld"), program("pytest")])
    );
}

#[test]
fn doctor_lists_a_program_named_through_in() {
    let checkup = checked(vec![target_with("run", |target| {
        target.run = Some(["$in", "--fast"].map(Written::new).to_vec());
        target.inputs = vec![Written::new("tools/run.sh")];
    })])
    .unwrap();

    assert_eq!(
        checkup.wanted().programs,
        BTreeSet::from([program("tools/run.sh")])
    );
    assert_eq!(
        checkup.outside().programs,
        BTreeSet::from([program("tools/run.sh")])
    );
}

#[test]
fn doctor_leaves_out_a_built_program() {
    let checkup = checked(vec![
        target("gen", &[]),
        target_with("use", |target| {
            target.run = Some(["$deps", "$out"].map(Written::new).to_vec());
            target.deps = vec![Written::new("gen")];
        }),
    ])
    .unwrap();

    assert_eq!(checkup.wanted().programs, BTreeSet::from([program("cc")]));
    assert!(checkup.outside().is_empty());
}

#[test]
fn doctor_reports_an_output_conflict_as_an_error() {
    let writing_x = |name: &str| {
        target_with(name, |target| {
            target.outputs = Some(vec![Written::new("x")]);
        })
    };

    let error = checked(vec![writing_x("a"), writing_x("b")]).unwrap_err();

    assert!(matches!(error, Error::OutputConflict { .. }), "{error}");
}

#[test]
fn doctor_hashes_nothing_and_looks_up_no_tool() {
    // `doctor` is handed a source, a dispatcher and a manifest and nothing else, so no other
    // port can be reached; what is left to show is that the build files are read once.
    let source = FakeSource::new(workspace(vec![("", file(vec![target("app", &[])], &[]))]));
    let manifest = FakeManifest {
        workspace: open_workspace(),
    };

    checked_by(&source, &manifest).unwrap();

    assert_eq!(source.evaluated(), [Directory::root()]);
}

#[test]
fn doctor_reports_every_plugin_violation_and_refuses_nothing() {
    let origin = Origin::plugins(vec![plugin("go", "1.2.0")]).unwrap();
    let source = FakeSource::new(workspace(vec![(
        "",
        file_by(
            &origin,
            vec![
                target_with("app", |target| target.env = vec![Written::new("HOME")]),
                target_with("fetch", |target| target.network = NetworkAccess::Declared),
            ],
            &[],
        ),
    )]));
    let manifest = FakeManifest {
        workspace: open_workspace(),
    };

    let checkup = checked_by(&source, &manifest).unwrap();

    let found: Vec<(String, Want)> = checkup
        .plugin_violations()
        .iter()
        .map(|violation| (violation.site.label.to_string(), violation.want.clone()))
        .collect();
    assert_eq!(
        found,
        [
            ("//:app".to_owned(), Want::Program(program("cc"))),
            ("//:app".to_owned(), Want::Env(env_name("HOME"))),
            ("//:fetch".to_owned(), Want::Program(program("cc"))),
            ("//:fetch".to_owned(), Want::Network(label("//:fetch"))),
        ]
    );
    assert_eq!(
        checkup.attribution().to_string(),
        "  cc: declared by plugin go@1.2.0 for //:app\n  \
         HOME: declared by plugin go@1.2.0 for //:app\n  \
         //:fetch: declared by plugin go@1.2.0 for //:fetch"
    );
}

#[test]
fn doctor_of_build_file_targets_has_no_attribution_and_no_violation() {
    let checkup = checked(vec![target("app", &[])]).unwrap();

    assert!(checkup.attribution().is_empty());
    assert_eq!(checkup.plugin_violations(), []);
}
