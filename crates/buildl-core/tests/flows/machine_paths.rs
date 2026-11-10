//! What the pipeline refuses for holding this machine's folders: a command, a `key` variable's
//! value, a setting a command names and a discovery output, driven through the ports by the fakes
//! of [`crate::common`].

#![expect(
    clippy::unwrap_used,
    reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
)]

use std::collections::BTreeMap;

use buildl_core::{
    Digest, EntryName, EnvMark, EnvName, EnvValue, Error, Label, LiveOutput, MachineFolders,
    NodeId, NothingIgnored, OnFailure, OutputSource, Pipeline, PlannedKey, Scope, SettingName,
    SettingValue, StagedEnv, StagedItem, StagedMark, StagedSetting, TargetRole, Verdict, Written,
    pipeline,
};

use crate::common::{
    FakeDispatcher, FakeHistory, FakePorts, FakeSource, Script, file, label, ports, program,
    root_only, script, target_with, workspace,
};

fn entry() -> EntryName {
    EntryName::parse("build.lua").unwrap()
}

/// The folders of a machine whose home is `/home/al` and whose workspace is `/home/al/shop`.
fn folders() -> MachineFolders {
    MachineFolders::new(&["/home/al"], &["/home/al/shop"], &[])
}

/// A pipeline over `source` that can find `cc` and whose host reports [`folders`].
fn pipeline_over(source: FakeSource) -> Pipeline<FakePorts> {
    let mut pipeline = Pipeline::new(ports(source));
    let ports = pipeline.ports_mut();
    ports
        .tools
        .fingerprints
        .insert(program("cc"), Digest::of(b"cc 14.2"));
    ports.host.folders = folders();
    pipeline
}

/// A target `name` running `words`.
fn running(name: &str, words: &[&str]) -> StagedItem {
    target_with(name, |target| {
        target.run = Some(words.iter().map(|word| Written::new(*word)).collect());
    })
}

fn build(pipeline: &mut Pipeline<FakePorts>) -> Result<(), Error> {
    pipeline
        .build(
            &entry(),
            &Scope::Workspace,
            &BTreeMap::new(),
            OnFailure::Stop,
            LiveOutput::Off,
        )
        .map(|_| ())
}

#[test]
fn a_command_holding_home_is_refused() {
    let mut pipeline = pipeline_over(FakeSource::new(workspace(vec![(
        "",
        file(
            vec![running("note", &["cc", "-I", "/home/al/include"])],
            &[],
        ),
    )])));
    let error = build(&mut pipeline).unwrap_err();
    assert_eq!(
        error.to_string(),
        "build.lua: //:note: the command holds this machine's path /home/al; it would give the \
         target another key on every machine"
    );
    assert_eq!(pipeline.ports().strategy.ran(), Vec::<Label>::new());
    assert!(matches!(
        pipeline.check(&entry()),
        Err(Error::MachinePathInCommand { .. })
    ));
}

#[test]
fn a_command_that_names_a_longer_name_is_not_refused() {
    let mut pipeline = pipeline_over(FakeSource::new(workspace(vec![(
        "",
        file(vec![running("note", &["cc", "/home/alice/include"])], &[]),
    )])));
    build(&mut pipeline).unwrap();
}

/// A source declaring the discovery target `//:meta`, whose output is its standard output, and
/// `//:app`, running `app_words`, once it is handed a result.
fn discovering_running(app_words: &[&str]) -> FakeSource {
    let meta = || {
        target_with("meta", |target| {
            target.role = TargetRole::Discovery;
            target.output_source = OutputSource::Stdout;
        })
    };
    FakeSource::declaring_from_results(
        workspace(vec![("", file(vec![meta()], &[]))]),
        workspace(vec![(
            "",
            file(vec![meta(), running("app", app_words)], &[]),
        )]),
    )
}

/// [`discovering_running`] with an `//:app` whose command holds no folder.
fn discovering() -> FakeSource {
    discovering_running(&["cc"])
}

/// A pipeline over [`discovering`] whose `//:meta` prints `output`.
fn discovering_with(output: &str) -> Pipeline<FakePorts> {
    let mut pipeline = pipeline_over(discovering());
    script(
        &mut pipeline,
        "//:meta",
        Script {
            stdout: Some(output.as_bytes().to_vec()),
            ..Script::default()
        },
    );
    pipeline
}

#[test]
fn a_discovery_output_holding_the_workspace_is_refused() {
    let mut pipeline = discovering_with(r#"{"workspace_root":"/home/al/shop"}"#);
    let error = build(&mut pipeline).unwrap_err();
    assert_eq!(
        error.to_string(),
        "build.lua: //:meta: its output holds this machine's path /home/al/shop"
    );
    assert_eq!(pipeline.ports().strategy.runs(&label("//:meta")), 1);
    assert_eq!(
        pipeline.ports().cache.rows.len(),
        1,
        "the refused output stays recorded"
    );
}

#[test]
fn a_cached_discovery_output_is_checked_too() {
    let mut pipeline = discovering_with(r#"{"workspace_root":"/home/al/shop"}"#);
    build(&mut pipeline).unwrap_err();

    let error = build(&mut pipeline).unwrap_err();

    assert!(
        matches!(error, Error::MachinePathInDiscovery { .. }),
        "{error:?}"
    );
    assert_eq!(
        pipeline.ports().strategy.runs(&label("//:meta")),
        1,
        "served from the cache the second time"
    );
}

#[test]
fn a_command_declared_only_after_discovery_is_refused() {
    let mut pipeline = pipeline_over(discovering_running(&["cc", "-I", "/home/al/include"]));
    script(
        &mut pipeline,
        "//:meta",
        Script {
            stdout: Some(b"{}".to_vec()),
            ..Script::default()
        },
    );

    let error = build(&mut pipeline).unwrap_err();

    assert_eq!(
        error.to_string(),
        "build.lua: //:app: the command holds this machine's path /home/al; it would give the \
         target another key on every machine"
    );
    assert_eq!(pipeline.ports().strategy.runs(&label("//:app")), 0);
}

#[test]
fn a_discovery_output_that_is_relative_is_allowed() {
    let mut pipeline = discovering_with(r#"{"workspace_root":"."}"#);
    build(&mut pipeline).unwrap();
}

/// A pipeline over one target `//:app` reading `RUSTFLAGS` as `mark`, set to `-L /home/al/lib`.
fn reading_rustflags(mark: EnvMark) -> Pipeline<FakePorts> {
    let mut pipeline = root_only(vec![target_with("app", |target| {
        target.env = vec![StagedEnv::Marked {
            name: Written::new("RUSTFLAGS"),
            mark: StagedMark::Word(Written::new(mark.as_str())),
        }];
    })]);
    let name = EnvName::parse("RUSTFLAGS").unwrap();
    let ports = pipeline.ports_mut();
    ports.host.folders = folders();
    ports.manifest.workspace.ceiling.env.insert(name.clone());
    ports
        .manifest
        .workspace
        .ceiling
        .unkeyed
        .insert(name.clone());
    ports
        .host
        .variables
        .insert(name, EnvValue::parse("-L /home/al/lib").unwrap());
    pipeline
}

#[test]
fn a_key_value_holding_home_is_refused() {
    let mut pipeline = reading_rustflags(EnvMark::Key);
    let error = pipeline
        .plan(&entry(), &Scope::Workspace, &BTreeMap::new())
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "build.lua: //:app: the variable RUSTFLAGS holds this machine's path /home/al; mark it \
         \"unkeyed\" if it only finds files"
    );
}

#[test]
fn an_unkeyed_value_holding_home_is_allowed() {
    let mut pipeline = reading_rustflags(EnvMark::Unkeyed);
    pipeline
        .plan(&entry(), &Scope::Workspace, &BTreeMap::new())
        .unwrap();
}

/// A pipeline over `//:app`, which runs `cc $opt:prefix`, and the setting `prefix` with the
/// default `default`.
fn naming_prefix(default: &str) -> Pipeline<FakePorts> {
    pipeline_over(FakeSource::new(workspace(vec![(
        "",
        file(
            vec![
                StagedItem::Setting(StagedSetting {
                    name: Written::new("prefix"),
                    default: Written::new(default),
                }),
                running("app", &["cc", "$opt:prefix"]),
            ],
            &[],
        ),
    )])))
}

fn overriding(name: &str, value: &str) -> BTreeMap<SettingName, SettingValue> {
    BTreeMap::from([(
        SettingName::parse(name).unwrap(),
        SettingValue::parse(value).unwrap(),
    )])
}

#[test]
fn a_setting_holding_home_is_refused() {
    let mut pipeline = naming_prefix("/opt");
    let error = pipeline
        .plan(
            &entry(),
            &Scope::Workspace,
            &overriding("prefix", "/home/al/opt"),
        )
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "build.lua: //:app: the setting prefix holds this machine's path /home/al; it would give \
         the target another key on every machine"
    );
    assert!(matches!(error, Error::MachinePathInSetting { .. }));
}

#[test]
fn a_default_holding_home_is_refused() {
    let mut pipeline = naming_prefix("/home/al/opt");
    let error = pipeline
        .plan(&entry(), &Scope::Workspace, &BTreeMap::new())
        .unwrap_err();
    assert!(
        matches!(error, Error::MachinePathInSetting { .. }),
        "{error:?}"
    );
}

#[test]
fn a_setting_no_command_names_is_not_searched() {
    let mut pipeline = pipeline_over(FakeSource::new(workspace(vec![(
        "",
        file(
            vec![
                StagedItem::Setting(StagedSetting {
                    name: Written::new("prefix"),
                    default: Written::new("/home/al/opt"),
                }),
                running("app", &["cc"]),
            ],
            &[],
        ),
    )])));
    pipeline
        .plan(&entry(), &Scope::Workspace, &BTreeMap::new())
        .unwrap();
}

#[test]
fn a_setting_without_a_folder_is_keyed() {
    let mut pipeline = pipeline_over(FakeSource::new(workspace(vec![(
        "",
        file(
            vec![
                StagedItem::Setting(StagedSetting {
                    name: Written::new("jobs"),
                    default: Written::new("4"),
                }),
                running("app", &["cc", "$opt:jobs"]),
            ],
            &[],
        ),
    )])));
    let plan = pipeline
        .plan(&entry(), &Scope::Workspace, &overriding("jobs", "8"))
        .unwrap();
    let wanted = label("//:app");
    let id = (0_u32..)
        .take(plan.len())
        .map(NodeId::new)
        .find(|id| plan.label(*id) == Some(&wanted))
        .unwrap();
    let own = match plan.verdict(id).unwrap() {
        Verdict::Clean { key }
        | Verdict::Dirty {
            key: PlannedKey::Finished(key),
            ..
        } => &key.components().own,
        Verdict::Dirty {
            key: PlannedKey::Unfinished(unfinished),
            ..
        }
        | Verdict::Conditional(unfinished) => &unfinished.own,
    };
    assert_eq!(
        own.settings,
        BTreeMap::from([(
            SettingName::parse("jobs").unwrap(),
            SettingValue::parse("8").unwrap()
        )])
    );
}

#[test]
fn gc_refuses_a_machine_path_too() {
    let source = FakeSource::new(workspace(vec![(
        "",
        file(vec![running("note", &["cc", "/home/al/include"])], &[]),
    )]));
    let mut pipeline = pipeline_over(source);
    let ports = pipeline.ports_mut();
    let error = pipeline::gc(
        &ports.source,
        &NothingIgnored,
        &mut FakeDispatcher::default(),
        &entry(),
        &folders(),
        &mut ports.cache,
        &ports.cas,
        &ports.cas,
        &FakeHistory::default(),
    )
    .unwrap_err();
    assert!(
        matches!(error, Error::MachinePathInCommand { .. }),
        "{error:?}"
    );
}
