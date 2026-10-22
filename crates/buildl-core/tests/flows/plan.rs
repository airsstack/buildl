//! `Pipeline::plan`, driven through every port it reads by the fakes of [`crate::common`].

#![expect(
    clippy::unwrap_used,
    reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use buildl_core::{
    ActionKey, CacheRow, DepProducts, Difference, Digest, DirtyReason, EntryName, EnvName,
    EnvValue, Error, EvaluationFailure, FileStat, Freshness, InputKind, KeyComponents, Label,
    NodeId, OwnComponents, Pipeline, Plan, PlannedKey, Platform, PortName, SettingName,
    SettingValue, SourcePath, StagedItem, StagedSetting, TargetGraph, Timestamp, ToolIdentity,
    Verdict, Want, Written, json,
};
use buildl_core::{Execution, Isolation, Manifest, Scope, Timeout, Workers};

use crate::common::{
    Answer, FakeApprover, FakeContent, FakeFile, FakePorts, FakeSource, dir, file, ports, program,
    setting, target, target_with, workspace,
};

/// The serialized plan of [`crate::graph::fixture`] after a build and one edit, pinned.
const GOLDEN: &str = include_str!("golden/plan.json");

/// The values given to settings for one run.
type Overrides = BTreeMap<SettingName, SettingValue>;

fn entry() -> EntryName {
    EntryName::parse("build.lua").unwrap()
}

fn label(raw: &str) -> Label {
    Label::parse(raw).unwrap()
}

fn path(raw: &str) -> SourcePath {
    SourcePath::parse(raw).unwrap()
}

/// Writes or replaces a workspace file holding `text`, last modified at `modified`.
fn put(pipeline: &mut Pipeline<FakePorts>, raw: &str, text: &str, modified: u64) {
    let content = FakeContent {
        bytes: text.as_bytes().to_vec(),
        stat: FileStat {
            modified: Timestamp::from_unix_nanos(modified),
            size: u64::try_from(text.len()).unwrap(),
        },
    };
    pipeline
        .ports_mut()
        .digester
        .files
        .insert(path(raw), content);
}

/// A plan of the workspace with no setting overridden.
fn planned(pipeline: &mut Pipeline<FakePorts>) -> Plan {
    planned_in(pipeline, &Scope::Workspace)
}

/// A plan of what `scope` asks for, with no setting overridden.
fn planned_in(pipeline: &mut Pipeline<FakePorts>, scope: &Scope) -> Plan {
    pipeline.plan(&entry(), scope, &Overrides::new()).unwrap()
}

/// What a verdict says, without its key: the reason, and the labels of the targets awaited.
#[derive(Debug, PartialEq, Eq)]
enum Said {
    Clean,
    Dirty(DirtyReason, Vec<Label>),
    Conditional(Vec<Label>),
}

/// What `plan` says about the target `raw` names.
fn said(plan: &Plan, raw: &str) -> Said {
    let wanted = label(raw);
    let id = (0_u32..)
        .take(plan.len())
        .map(NodeId::new)
        .find(|id| plan.label(*id) == Some(&wanted))
        .unwrap();
    let labels = |awaiting: &[NodeId]| -> Vec<Label> {
        awaiting
            .iter()
            .map(|id| plan.label(*id).unwrap().clone())
            .collect()
    };
    match plan.verdict(id).unwrap() {
        Verdict::Clean { .. } => Said::Clean,
        Verdict::Dirty { reason, key } => {
            let awaiting = match key {
                PlannedKey::Finished(_) => Vec::new(),
                PlannedKey::Unfinished(unfinished) => labels(&unfinished.awaiting),
            };
            Said::Dirty(reason.clone(), awaiting)
        }
        Verdict::Conditional(unfinished) => Said::Conditional(labels(&unfinished.awaiting)),
    }
}

/// The components a verdict holds for its target, whatever the verdict.
const fn own_of(verdict: &Verdict) -> &OwnComponents {
    match verdict {
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
    }
}

/// Records one target as built, as an executor would: under the key `plan` holds for it, finished
/// with what its dependencies' rows say they produced, with every output's content derived from
/// `version`, for an action that printed nothing.
fn record(
    pipeline: &mut Pipeline<FakePorts>,
    graph: &TargetGraph,
    plan: &Plan,
    id: NodeId,
    version: &str,
) {
    let label = plan.label(id).unwrap().clone();
    let rows = &pipeline.ports().cache.rows;
    let deps = graph
        .deps(id)
        .unwrap()
        .iter()
        .map(|dep| {
            let dep_label = plan.label(*dep).unwrap();
            DepProducts {
                label: dep_label.clone(),
                outputs: rows.get(dep_label).unwrap().produced(),
            }
        })
        .collect();
    let own = own_of(plan.verdict(id).unwrap()).clone();
    let outputs = own
        .outputs
        .iter()
        .map(|name| Digest::of(format!("{label} {name} {version}").as_bytes()))
        .collect();
    let key = ActionKey::new(KeyComponents { own, deps }).unwrap();
    let nothing = Digest::of(b"");
    let row = CacheRow::new(key, outputs, nothing, nothing).unwrap();
    pipeline.ports_mut().cache.rows.insert(label, row);
}

/// Records a build of every target of the workspace, each after the targets it depends on.
///
/// The helper takes the targets in dependency order rather than planning until all are clean,
/// so it also works for a workspace holding a target that is never clean.
fn build(pipeline: &mut Pipeline<FakePorts>, overrides: &Overrides, version: &str) {
    build_in(pipeline, &Scope::Workspace, overrides, version);
}

/// Records a build of every target of the workspace, planned for what `scope` asks for, each
/// after the targets it depends on.
///
/// The plan's targets are matched to the whole graph's by number, so `scope` must ask for a part
/// that is numbered the same as the whole.
fn build_in(
    pipeline: &mut Pipeline<FakePorts>,
    scope: &Scope,
    overrides: &Overrides,
    version: &str,
) {
    let graph = pipeline.graph(&entry()).unwrap();
    let plan = pipeline.plan(&entry(), scope, overrides).unwrap();
    let mut built = BTreeSet::new();
    while built.len() < graph.len() {
        let ready: Vec<NodeId> = (0_u32..)
            .take(graph.len())
            .map(NodeId::new)
            .filter(|id| !built.contains(id))
            .filter(|id| {
                graph
                    .deps(*id)
                    .unwrap()
                    .iter()
                    .all(|dep| built.contains(dep))
            })
            .collect();
        assert!(!ready.is_empty(), "a graph has no cycle");
        for id in ready {
            record(pipeline, &graph, &plan, id, version);
            built.insert(id);
        }
    }
}

/// Records the target `raw` names as rebuilt under the key the current plan holds for it.
fn rebuild(pipeline: &mut Pipeline<FakePorts>, raw: &str, version: &str) {
    let graph = pipeline.graph(&entry()).unwrap();
    let plan = planned(pipeline);
    let id = graph.id_of(&label(raw)).unwrap();
    record(pipeline, &graph, &plan, id, version);
}

/// `//:app` needs `//:lib`, which needs `//:gen`; `//:docs` stands alone. Each reads one file and
/// runs `cc`, which the fake resolver can find.
fn chain() -> Pipeline<FakePorts> {
    let reading = |name: &str, input: &str, deps: &[&str]| {
        target_with(name, |target| {
            target.inputs = vec![Written::new(input)];
            target.deps = deps.iter().map(|dep| Written::new(*dep)).collect();
        })
    };
    let source = FakeSource::new(workspace(vec![(
        "",
        file(
            vec![
                reading("gen", "gen.c", &[]),
                reading("lib", "lib.c", &["gen"]),
                reading("app", "main.c", &["lib"]),
                reading("docs", "README", &[]),
            ],
            &[],
        ),
    )]));
    let mut pipeline = Pipeline::new(ports(source));
    for name in ["gen.c", "lib.c", "main.c", "README"] {
        put(&mut pipeline, name, name, 100);
    }
    pipeline
        .ports_mut()
        .tools
        .fingerprints
        .insert(program("cc"), Digest::of(b"cc 14.2"));
    pipeline
}

const fn changed(differences: Vec<Difference>) -> DirtyReason {
    DirtyReason::Changed(differences)
}

#[test]
fn a_workspace_never_built_is_dirty_throughout_and_awaits_its_dependencies() {
    let mut pipeline = chain();
    let plan = planned(&mut pipeline);
    let never = |awaiting: &[&str]| {
        Said::Dirty(
            DirtyReason::NeverBuilt,
            awaiting.iter().map(|raw| label(raw)).collect(),
        )
    };
    assert_eq!(plan.len(), 4);
    assert_eq!(said(&plan, "//:gen"), never(&[]));
    assert_eq!(said(&plan, "//:docs"), never(&[]));
    assert_eq!(said(&plan, "//:lib"), never(&["//:gen"]));
    assert_eq!(said(&plan, "//:app"), never(&["//:lib"]));
    assert_eq!(
        plan.wanted().programs,
        BTreeSet::from([program("cc")]),
        "the wanted set lists the one program every command names"
    );
}

#[test]
fn after_a_build_every_target_is_clean_and_nothing_is_hashed_or_written() {
    let mut pipeline = chain();
    build(&mut pipeline, &Overrides::new(), "v1");
    let digests = pipeline.ports().digester.digest_calls();
    let commits = pipeline.ports().stat_cache.commits();
    assert_eq!(
        (digests, commits),
        (1, 1),
        "the first plan hashed every input"
    );

    let plan = planned(&mut pipeline);

    for raw in ["//:app", "//:docs", "//:gen", "//:lib"] {
        assert_eq!(said(&plan, raw), Said::Clean, "{raw}");
    }
    assert_eq!(pipeline.ports().digester.digest_calls(), digests);
    assert_eq!(pipeline.ports().stat_cache.commits(), commits);
}

#[test]
fn a_changed_input_dirties_its_target_and_makes_everything_downstream_conditional() {
    let mut pipeline = chain();
    build(&mut pipeline, &Overrides::new(), "v1");
    put(&mut pipeline, "gen.c", "gen.c, edited", 200);

    let plan = planned(&mut pipeline);

    assert_eq!(
        said(&plan, "//:gen"),
        Said::Dirty(changed(vec![Difference::Input(path("gen.c"))]), Vec::new())
    );
    assert_eq!(
        said(&plan, "//:lib"),
        Said::Conditional(vec![label("//:gen")])
    );
    assert_eq!(
        said(&plan, "//:app"),
        Said::Conditional(vec![label("//:lib")])
    );
    assert_eq!(said(&plan, "//:docs"), Said::Clean);
}

#[test]
fn a_target_rebuilt_to_the_same_output_leaves_its_dependents_clean() {
    let mut pipeline = chain();
    build(&mut pipeline, &Overrides::new(), "v1");
    put(&mut pipeline, "gen.c", "gen.c, edited", 200);
    let old_key = pipeline.ports().cache.rows[&label("//:gen")].key().digest();

    rebuild(&mut pipeline, "//:gen", "v1");

    let new_key = pipeline.ports().cache.rows[&label("//:gen")].key().digest();
    assert_ne!(new_key, old_key, "the edit changed the key of `//:gen`");
    let plan = planned(&mut pipeline);
    for raw in ["//:app", "//:docs", "//:gen", "//:lib"] {
        assert_eq!(said(&plan, raw), Said::Clean, "{raw}");
    }
}

#[test]
fn a_target_rebuilt_to_another_output_dirties_the_target_that_depends_on_it() {
    let mut pipeline = chain();
    build(&mut pipeline, &Overrides::new(), "v1");
    put(&mut pipeline, "gen.c", "gen.c, edited", 200);

    rebuild(&mut pipeline, "//:gen", "v2");

    let plan = planned(&mut pipeline);
    assert_eq!(said(&plan, "//:gen"), Said::Clean);
    assert_eq!(
        said(&plan, "//:lib"),
        Said::Dirty(changed(vec![Difference::Dep(label("//:gen"))]), Vec::new())
    );
    assert_eq!(
        said(&plan, "//:app"),
        Said::Conditional(vec![label("//:lib")])
    );
    assert_eq!(said(&plan, "//:docs"), Said::Clean);
}

#[test]
fn a_file_touched_without_changing_is_hashed_once_and_its_target_stays_clean() {
    let mut pipeline = chain();
    build(&mut pipeline, &Overrides::new(), "v1");
    put(&mut pipeline, "gen.c", "gen.c", 200);

    let plan = planned(&mut pipeline);

    for raw in ["//:app", "//:docs", "//:gen", "//:lib"] {
        assert_eq!(said(&plan, raw), Said::Clean, "{raw}");
    }
    assert_eq!(pipeline.ports().digester.digest_calls(), 2);
    assert_eq!(
        pipeline.ports().digester.digested(),
        [
            path("README"),
            path("gen.c"),
            path("lib.c"),
            path("main.c"),
            path("gen.c"),
        ],
        "every input on the first plan, in path order, then the touched file alone"
    );
    assert_eq!(pipeline.ports().stat_cache.commits(), 2);

    planned(&mut pipeline);
    assert_eq!(
        pipeline.ports().digester.digest_calls(),
        2,
        "the new metadata was recorded"
    );
}

fn overrides(pairs: &[(&str, &str)]) -> Overrides {
    pairs
        .iter()
        .map(|(name, value)| {
            (
                SettingName::parse(*name).unwrap(),
                SettingValue::parse(*value).unwrap(),
            )
        })
        .collect()
}

fn env_name(raw: &str) -> EnvName {
    EnvName::parse(raw).unwrap()
}

/// A `b.target(name, { run = run })` call.
fn running(name: &str, run: &[&str]) -> StagedItem {
    target_with(name, |target| {
        target.run = Some(run.iter().map(|argument| Written::new(*argument)).collect());
    })
}

/// A pipeline over one root build file making `items`, with `cc` findable.
fn root_only(items: Vec<StagedItem>) -> Pipeline<FakePorts> {
    let source = FakeSource::new(workspace(vec![("", file(items, &[]))]));
    let mut pipeline = Pipeline::new(ports(source));
    pipeline
        .ports_mut()
        .tools
        .fingerprints
        .insert(program("cc"), Digest::of(b"cc 14.2"));
    pipeline
}

/// The label and build file an error names as its site.
fn site_of(error: &Error) -> Option<(String, &Path)> {
    let site = match error {
        Error::Refused { site, .. } => site.as_deref(),
        Error::MissingTool { site, .. }
        | Error::MissingInput { site, .. }
        | Error::InputNotAFile { site, .. } => Some(&**site),
        _ => None,
    };
    site.map(|site| (site.label.to_string(), site.provenance.file()))
}

#[test]
fn an_override_dirties_exactly_the_targets_whose_commands_name_the_setting() {
    let mut pipeline = root_only(vec![
        setting("mode"),
        running("a", &["cc", "-m=$opt:mode"]),
        running("b", &["cc", "$opt:mode"]),
        running("c", &["cc"]),
    ]);
    build(&mut pipeline, &Overrides::new(), "v1");

    let plan = pipeline
        .plan(
            &entry(),
            &Scope::Workspace,
            &overrides(&[("mode", "release")]),
        )
        .unwrap();

    let mode = SettingName::parse("mode").unwrap();
    for raw in ["//:a", "//:b"] {
        assert_eq!(
            said(&plan, raw),
            Said::Dirty(changed(vec![Difference::Setting(mode.clone())]), Vec::new()),
            "{raw}"
        );
    }
    assert_eq!(said(&plan, "//:c"), Said::Clean);
    assert_eq!(said(&planned(&mut pipeline), "//:a"), Said::Clean);
}

#[test]
fn an_environment_value_a_tool_and_the_platform_each_reach_the_key() {
    let mut pipeline = root_only(vec![target_with("app", |target| {
        target.env = vec![Written::new("PATH")];
    })]);
    let path_name = env_name("PATH");
    let value = |text: &str| EnvValue::parse(text).unwrap();
    {
        let ports = pipeline.ports_mut();
        ports
            .manifest
            .workspace
            .ceiling
            .env
            .insert(path_name.clone());
        ports
            .host
            .variables
            .insert(path_name.clone(), value("/bin"));
    }
    build(&mut pipeline, &Overrides::new(), "v1");
    let dirty = |difference: Difference| Said::Dirty(changed(vec![difference]), Vec::new());

    let host = &mut pipeline.ports_mut().host;
    host.variables.insert(path_name.clone(), value("/usr/bin"));
    assert_eq!(
        said(&planned(&mut pipeline), "//:app"),
        dirty(Difference::Env(path_name.clone()))
    );
    pipeline.ports_mut().host.variables.remove(&path_name);
    assert_eq!(
        said(&planned(&mut pipeline), "//:app"),
        dirty(Difference::Env(path_name.clone())),
        "an unset variable is not the value it had"
    );
    let host = &mut pipeline.ports_mut().host;
    host.variables.insert(path_name, value("/bin"));
    assert_eq!(said(&planned(&mut pipeline), "//:app"), Said::Clean);

    let tools = &mut pipeline.ports_mut().tools;
    tools
        .fingerprints
        .insert(program("cc"), Digest::of(b"cc 15.1"));
    assert_eq!(
        said(&planned(&mut pipeline), "//:app"),
        dirty(Difference::Tool)
    );
    let tools = &mut pipeline.ports_mut().tools;
    tools
        .fingerprints
        .insert(program("cc"), Digest::of(b"cc 14.2"));

    pipeline.ports_mut().host.platform = Platform::parse("macos/aarch64").unwrap();
    assert_eq!(
        said(&planned(&mut pipeline), "//:app"),
        dirty(Difference::Platform)
    );
}

#[test]
fn a_target_that_always_runs_is_dirty_on_every_plan_and_its_dependent_conditional() {
    let mut pipeline = root_only(vec![
        target_with("gen", |target| target.freshness = Freshness::Always),
        target("lib", &["gen"]),
    ]);
    build(&mut pipeline, &Overrides::new(), "v1");

    for _ in 0..2 {
        let plan = planned(&mut pipeline);
        assert_eq!(
            said(&plan, "//:gen"),
            Said::Dirty(DirtyReason::Always, Vec::new())
        );
        assert_eq!(
            said(&plan, "//:lib"),
            Said::Conditional(vec![label("//:gen")])
        );
    }
}

#[test]
fn a_program_outside_the_ceiling_is_refused_before_anything_is_read() {
    let mut pipeline = chain();
    pipeline.ports_mut().manifest.workspace.ceiling.run.clear();

    let error = pipeline
        .plan(&entry(), &Scope::Workspace, &Overrides::new())
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        "build.lua: //:app: program cc is outside the ceiling (proc.run allows: nothing)"
    );
    assert_eq!(
        site_of(&error),
        Some(("//:app".to_owned(), Path::new("build.lua")))
    );
    match error {
        Error::Refused { want, ceiling, .. } => {
            assert_eq!(want, Want::Program(program("cc")));
            assert_eq!(*ceiling, pipeline.ports().manifest.workspace.ceiling);
        }
        other => unreachable!("expected Refused, got {other:?}"),
    }
    let ports = pipeline.ports();
    assert_eq!(ports.digester.stat_calls(), 0);
    assert_eq!(ports.digester.digest_calls(), 0);
    assert_eq!(ports.tools.lookups(), []);
    assert_eq!(ports.stat_cache.commits(), 0);
}

#[test]
fn a_variable_an_input_and_a_directory_outside_the_ceiling_are_each_refused() {
    let mut reads_secret = root_only(vec![target_with("app", |target| {
        target.env = vec![Written::new("AWS_SECRET")];
    })]);
    let error = reads_secret
        .plan(&entry(), &Scope::Workspace, &Overrides::new())
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "build.lua: //:app: environment variable AWS_SECRET is outside the ceiling \
         (env.read allows: nothing)"
    );

    let mut reads_outside = chain();
    let ceiling = &mut reads_outside.ports_mut().manifest.workspace.ceiling;
    ceiling.read = BTreeSet::from([dir("src")]);
    let error = reads_outside
        .plan(&entry(), &Scope::Workspace, &Overrides::new())
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "build.lua: //:docs: input README is outside the ceiling (fs.read allows: src)"
    );

    let mut writes_outside = chain();
    let ceiling = &mut writes_outside.ports_mut().manifest.workspace.ceiling;
    ceiling.write = BTreeSet::from([dir(".buildl")]);
    let error = writes_outside
        .plan(&entry(), &Scope::Workspace, &Overrides::new())
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "directory out is outside the ceiling (fs.write allows: .buildl)"
    );
    assert_eq!(site_of(&error), None);
}

#[test]
fn a_pinned_variable_is_refused_after_the_ceiling_and_before_any_tool() {
    let mut pipeline = root_only(vec![target_with("app", |target| {
        target.env = vec![Written::new("TZ")];
        target.inputs = vec![Written::new("missing.c")];
    })]);
    pipeline.ports_mut().tools.fingerprints.clear();

    let error = pipeline
        .plan(&entry(), &Scope::Workspace, &Overrides::new())
        .unwrap_err();
    assert!(matches!(error, Error::Refused { .. }), "{error}");

    pipeline.ports_mut().manifest.workspace.ceiling.env = BTreeSet::from([env_name("TZ")]);
    let error = pipeline
        .plan(&entry(), &Scope::Workspace, &Overrides::new())
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "build.lua: //:app: env TZ is set to UTC by the sandbox and cannot be declared"
    );
    assert!(matches!(error, Error::PinnedEnv { .. }), "{error}");

    pipeline.ports_mut().manifest.workspace.execution.strategy = Isolation::Bare;
    let error = pipeline
        .plan(&entry(), &Scope::Workspace, &Overrides::new())
        .unwrap_err();
    assert!(matches!(error, Error::MissingTool { .. }), "{error}");
}

#[test]
fn a_program_that_cannot_be_found_names_the_lowest_labelled_target_running_it() {
    let mut pipeline = chain();
    pipeline.ports_mut().tools.fingerprints.clear();

    let error = pipeline
        .plan(&entry(), &Scope::Workspace, &Overrides::new())
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        "build.lua: //:app: program cc was not found"
    );
    assert_eq!(
        site_of(&error),
        Some(("//:app".to_owned(), Path::new("build.lua")))
    );
    assert!(
        matches!(error, Error::MissingTool { program: missing, .. } if missing == program("cc"))
    );
}

#[test]
fn an_input_that_is_not_a_regular_file_names_the_target_declaring_it_and_what_it_is() {
    for (found, text) in [
        (
            InputKind::Directory,
            "build.lua: //:lib: input lib.c is a directory; an input must be a regular file",
        ),
        (
            InputKind::Symlink,
            "build.lua: //:lib: input lib.c is a symbolic link; an input must be a regular file",
        ),
        (
            InputKind::Special,
            "build.lua: //:lib: input lib.c is a pipe, a socket or a device; an input must be a regular file",
        ),
    ] {
        let mut pipeline = chain();
        pipeline
            .ports_mut()
            .digester
            .others
            .insert(path("lib.c"), found);

        let error = pipeline
            .plan(&entry(), &Scope::Workspace, &Overrides::new())
            .unwrap_err();

        assert_eq!(error.to_string(), text);
        assert_eq!(
            site_of(&error),
            Some(("//:lib".to_owned(), Path::new("build.lua")))
        );
        assert!(matches!(
            error,
            Error::InputNotAFile { path: named, found: kind, .. }
                if named == path("lib.c") && kind == found
        ));
        assert_eq!(pipeline.ports().digester.digest_calls(), 0);
    }
}

#[test]
fn an_input_that_does_not_exist_names_the_target_declaring_it() {
    let mut pipeline = chain();
    pipeline.ports_mut().digester.files.remove(&path("lib.c"));

    let error = pipeline
        .plan(&entry(), &Scope::Workspace, &Overrides::new())
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        "build.lua: //:lib: input lib.c does not exist"
    );
    assert_eq!(
        site_of(&error),
        Some(("//:lib".to_owned(), Path::new("build.lua")))
    );
    assert!(matches!(error, Error::MissingInput { path: missing, .. } if missing == path("lib.c")));
    assert_eq!(pipeline.ports().digester.digest_calls(), 0);
}

#[test]
fn a_program_named_by_several_targets_is_looked_up_once() {
    let mut pipeline = chain();
    planned(&mut pipeline);
    assert_eq!(pipeline.ports().tools.lookups(), [program("cc")]);
}

#[test]
fn of_two_variables_the_host_cannot_report_the_lower_name_is_the_error() {
    let mut pipeline = root_only(vec![target_with("app", |target| {
        target.env = vec![Written::new("ZED"), Written::new("ALPHA")];
    })]);
    let names = BTreeSet::from([env_name("ALPHA"), env_name("ZED")]);
    {
        let ports = pipeline.ports_mut();
        ports.manifest.workspace.ceiling.env = names.clone();
        ports.host.unreportable = names;
    }

    let error = pipeline
        .plan(&entry(), &Scope::Workspace, &Overrides::new())
        .unwrap_err();

    assert_eq!(error.to_string(), "host: ALPHA does not hold valid text");
    assert!(matches!(error, Error::Adapter { port, .. } if port == PortName::Host));
}

#[test]
fn several_faults_are_reported_one_at_a_time_in_a_fixed_order() {
    let source = FakeSource::new(workspace(vec![(
        "",
        file(
            vec![
                running("image", &["docker", "build"]),
                target_with("app", |target| {
                    target.inputs = vec![Written::new("missing.c")];
                }),
            ],
            &[],
        ),
    )]));
    let mut pipeline = Pipeline::<FakePorts>::new(ports(source));

    let unknown = overrides(&[("bogus", "1")]);
    let error = pipeline
        .plan(&entry(), &Scope::Workspace, &unknown)
        .unwrap_err();
    assert!(matches!(error, Error::UnknownOverride { .. }), "{error}");

    let error = pipeline
        .plan(&entry(), &Scope::Workspace, &Overrides::new())
        .unwrap_err();
    assert!(
        matches!(&error, Error::Refused { want, .. } if want == &Want::Program(program("docker"))),
        "{error}"
    );

    let ceiling = &mut pipeline.ports_mut().manifest.workspace.ceiling;
    ceiling.run.insert(program("docker"));
    let error = pipeline
        .plan(&entry(), &Scope::Workspace, &Overrides::new())
        .unwrap_err();
    assert!(
        matches!(&error, Error::MissingTool { program: missing, .. } if missing == &program("cc")),
        "{error}"
    );

    for name in ["cc", "docker"] {
        let tools = &mut pipeline.ports_mut().tools;
        tools
            .fingerprints
            .insert(program(name), Digest::of(name.as_bytes()));
    }
    let error = pipeline
        .plan(&entry(), &Scope::Workspace, &Overrides::new())
        .unwrap_err();
    assert!(
        matches!(&error, Error::MissingInput { path: missing, .. } if missing == &path("missing.c")),
        "{error}"
    );

    put(&mut pipeline, "missing.c", "found", 100);
    assert_eq!(planned(&mut pipeline).len(), 2);
}

#[test]
fn an_override_naming_no_setting_is_refused() {
    let mut pipeline = root_only(vec![setting("mode"), running("a", &["cc", "$opt:mode"])]);

    let error = pipeline
        .plan(
            &entry(),
            &Scope::Workspace,
            &overrides(&[("mod", "release")]),
        )
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        "setting mod is overridden but not declared"
    );
    assert!(matches!(error, Error::UnknownOverride { name } if name.as_str() == "mod"));
}

#[test]
fn a_plan_holds_every_declared_setting_with_its_override_or_else_its_default() {
    let mut pipeline = root_only(vec![
        setting_of("mode", "debug"),
        setting_of("level", "1"),
        setting_of("unused", "idle"),
        running("a", &["cc", "-m=$opt:mode", "-O$opt:level"]),
    ]);

    assert_eq!(
        planned(&mut pipeline).settings(),
        &overrides(&[("level", "1"), ("mode", "debug"), ("unused", "idle")]),
        "with nothing overridden, every default"
    );

    let plan = pipeline
        .plan(
            &entry(),
            &Scope::Workspace,
            &overrides(&[("mode", "release"), ("unused", "busy")]),
        )
        .unwrap();

    assert_eq!(
        plan.settings(),
        &overrides(&[("level", "1"), ("mode", "release"), ("unused", "busy")]),
        "a setting no command names is held too"
    );
    let json = json::canonical::to_string(&plan).unwrap();
    assert!(
        json.starts_with(
            r#"{"settings":{"level":"1","mode":"release","unused":"busy"},"targets":{"#
        ),
        "{json}"
    );
}

#[test]
fn the_execution_settings_are_read_through_the_manifest_and_reach_no_key() {
    let mut pipeline = chain();
    assert_eq!(
        pipeline.ports().manifest.workspace().unwrap().execution,
        Execution::default()
    );
    build(&mut pipeline, &Overrides::new(), "v1");

    let execution = Execution {
        workers: Workers::parse("2").unwrap(),
        timeout: Timeout::parse("30s").unwrap(),
        strategy: Isolation::parse("bare").unwrap(),
    };
    assert_ne!(execution.workers, Execution::default().workers);
    assert_ne!(execution.timeout, Execution::default().timeout);
    assert_ne!(execution.strategy, Execution::default().strategy);
    pipeline.ports_mut().manifest.workspace.execution = execution;

    assert_eq!(
        pipeline.ports().manifest.workspace().unwrap().execution,
        execution
    );
    let plan = planned(&mut pipeline);
    for raw in ["//:app", "//:docs", "//:gen", "//:lib"] {
        assert_eq!(said(&plan, raw), Said::Clean, "{raw}");
    }
}

/// A `b.option(name, { default = default })` call.
fn setting_of(name: &str, default: &str) -> StagedItem {
    StagedItem::Setting(StagedSetting {
        name: Written::new(name),
        default: Written::new(default),
    })
}

/// `//:app` and `//:lib`, each running the program the setting `tool` names, which is `cc`
/// unless overridden. The workspace also allows `clang`, and both programs can be found.
fn tool_by_setting() -> Pipeline<FakePorts> {
    let mut pipeline = root_only(vec![
        setting_of("tool", "cc"),
        running("app", &["$opt:tool", "-o", "$out"]),
        running("lib", &["$opt:tool", "-c"]),
    ]);
    let ports = pipeline.ports_mut();
    ports
        .manifest
        .workspace
        .ceiling
        .run
        .insert(program("clang"));
    ports
        .tools
        .fingerprints
        .insert(program("clang"), Digest::of(b"clang 19.1"));
    pipeline
}

/// The tool `plan` keyed the target `raw` names on.
fn tool_of(plan: &Plan, raw: &str) -> Option<ToolIdentity> {
    let wanted = label(raw);
    let id = (0_u32..)
        .take(plan.len())
        .map(NodeId::new)
        .find(|id| plan.label(*id) == Some(&wanted))
        .unwrap();
    own_of(plan.verdict(id).unwrap()).tool.clone()
}

#[test]
fn a_tool_named_through_a_setting_is_wanted_looked_up_once_and_keyed_like_a_written_one() {
    let mut pipeline = tool_by_setting();

    let plan = planned(&mut pipeline);

    assert_eq!(plan.wanted().programs, BTreeSet::from([program("cc")]));
    assert_eq!(pipeline.ports().tools.lookups(), [program("cc")]);
    for raw in ["//:app", "//:lib"] {
        assert_eq!(
            tool_of(&plan, raw),
            Some(ToolIdentity {
                program: program("cc"),
                fingerprint: Digest::of(b"cc 14.2"),
            }),
            "{raw}"
        );
    }
}

#[test]
fn replacing_a_tool_named_through_a_setting_dirties_the_targets_running_it() {
    let mut pipeline = tool_by_setting();
    build(&mut pipeline, &Overrides::new(), "v1");
    assert_eq!(said(&planned(&mut pipeline), "//:app"), Said::Clean);

    let tools = &mut pipeline.ports_mut().tools;
    tools
        .fingerprints
        .insert(program("cc"), Digest::of(b"cc 15.1"));

    let plan = planned(&mut pipeline);
    for raw in ["//:app", "//:lib"] {
        assert_eq!(
            said(&plan, raw),
            Said::Dirty(changed(vec![Difference::Tool]), Vec::new()),
            "{raw}"
        );
    }
}

#[test]
fn a_setting_changed_to_another_program_dirties_its_targets_for_the_tool_and_the_setting() {
    let mut pipeline = tool_by_setting();
    build(&mut pipeline, &Overrides::new(), "v1");

    let plan = pipeline
        .plan(
            &entry(),
            &Scope::Workspace,
            &overrides(&[("tool", "clang")]),
        )
        .unwrap();

    let tool = SettingName::parse("tool").unwrap();
    assert_eq!(
        said(&plan, "//:app"),
        Said::Dirty(
            changed(vec![Difference::Tool, Difference::Setting(tool)]),
            Vec::new()
        )
    );
    assert_eq!(
        tool_of(&plan, "//:app"),
        Some(ToolIdentity {
            program: program("clang"),
            fingerprint: Digest::of(b"clang 19.1"),
        })
    );
    assert_eq!(plan.wanted().programs, BTreeSet::from([program("clang")]));
}

#[test]
fn a_tool_named_through_a_setting_is_held_to_the_ceiling_before_anything_is_read() {
    let mut pipeline = tool_by_setting();

    let error = pipeline
        .plan(
            &entry(),
            &Scope::Workspace,
            &overrides(&[("tool", "docker")]),
        )
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        "build.lua: //:app: program docker is outside the ceiling (proc.run allows: cc, clang)"
    );
    assert_eq!(
        site_of(&error),
        Some(("//:app".to_owned(), Path::new("build.lua")))
    );
    assert!(
        matches!(&error, Error::Refused { want, .. } if want == &Want::Program(program("docker"))),
        "{error}"
    );
    assert_eq!(pipeline.ports().tools.lookups(), []);
}

#[test]
fn a_tool_named_through_a_setting_that_cannot_be_found_names_the_lowest_labelled_target() {
    let mut pipeline = tool_by_setting();
    let tools = &mut pipeline.ports_mut().tools;
    tools.fingerprints.remove(&program("clang"));

    let error = pipeline
        .plan(
            &entry(),
            &Scope::Workspace,
            &overrides(&[("tool", "clang")]),
        )
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        "build.lua: //:app: program clang was not found"
    );
    assert_eq!(
        site_of(&error),
        Some(("//:app".to_owned(), Path::new("build.lua")))
    );
    assert!(
        matches!(error, Error::MissingTool { program: missing, .. } if missing == program("clang"))
    );
}

#[test]
fn a_first_argument_that_runs_what_the_build_produced_names_no_tool() {
    let mut pipeline = root_only(vec![
        setting_of("suite", "unit"),
        target("app", &[]),
        target_with("self_test", |target| {
            target.run = Some(["$deps", "--self-test"].map(Written::new).to_vec());
            target.deps = vec![Written::new("app")];
        }),
        target_with("suite_test", |target| {
            target.run = Some(["$deps/$opt:suite"].map(Written::new).to_vec());
            target.deps = vec![Written::new("app")];
        }),
    ]);

    let plan = planned(&mut pipeline);

    assert_eq!(tool_of(&plan, "//:self_test"), None);
    assert_eq!(tool_of(&plan, "//:suite_test"), None);
    assert_eq!(plan.wanted().programs, BTreeSet::from([program("cc")]));
    assert_eq!(pipeline.ports().tools.lookups(), [program("cc")]);
}

#[test]
fn a_build_file_that_fails_to_evaluate_is_the_load_error_unchanged() {
    let source = FakeSource::new(workspace(vec![
        ("", file(vec![target("app", &[])], &["lib"])),
        ("lib", FakeFile::Fails(EvaluationFailure::Syntax)),
    ]));
    let mut pipeline = Pipeline::<FakePorts>::new(ports(source));

    match pipeline
        .plan(&entry(), &Scope::Workspace, &Overrides::new())
        .unwrap_err()
    {
        Error::Evaluation {
            provenance,
            failure,
            ..
        } => {
            assert_eq!(provenance.file(), Path::new("lib/build.lua"));
            assert_eq!(failure, EvaluationFailure::Syntax);
        }
        other => unreachable!("expected Evaluation, got {other:?}"),
    }
}

#[test]
fn a_built_workspace_with_one_edit_plans_to_the_pinned_json() {
    let mut pipeline = Pipeline::<FakePorts>::new(ports(crate::graph::fixture()));
    put(&mut pipeline, "main.c", "main.c", 100);
    put(&mut pipeline, "lib/text.c", "lib/text.c", 100);
    let tools = &mut pipeline.ports_mut().tools;
    tools
        .fingerprints
        .insert(program("cc"), Digest::of(b"cc 14.2"));
    let app_test = Scope::Targets(BTreeSet::from([label("//:app_test")]));
    build_in(&mut pipeline, &app_test, &Overrides::new(), "v1");
    put(&mut pipeline, "main.c", "main.c, edited", 200);

    let plan = planned_in(&mut pipeline, &app_test);

    assert_eq!(
        said(&plan, "//:main.o"),
        Said::Dirty(changed(vec![Difference::Input(path("main.c"))]), Vec::new())
    );
    assert_eq!(
        said(&plan, "//:app"),
        Said::Conditional(vec![label("//:main.o")])
    );
    assert_eq!(
        said(&plan, "//:app_test"),
        Said::Conditional(vec![label("//:app")])
    );
    assert_eq!(said(&plan, "//lib:text"), Said::Clean);
    let json = json::canonical::to_string(&plan).unwrap();
    assert_eq!(json, GOLDEN.trim_end());
}

#[test]
fn a_plan_of_the_workspace_leaves_tests_out() {
    let mut pipeline = Pipeline::<FakePorts>::new(ports(crate::graph::fixture()));
    put(&mut pipeline, "main.c", "main.c", 100);
    put(&mut pipeline, "lib/text.c", "lib/text.c", 100);
    let tools = &mut pipeline.ports_mut().tools;
    tools
        .fingerprints
        .insert(program("cc"), Digest::of(b"cc 14.2"));

    let plan = planned(&mut pipeline);

    let planned_labels: Vec<String> = (0_u32..)
        .take(plan.len())
        .map(NodeId::new)
        .map(|id| plan.label(id).unwrap().to_string())
        .collect();
    assert_eq!(planned_labels, ["//:app", "//:main.o", "//lib:text"]);
}

/// `//:gen`, and `//:use`, which depends on it and runs `command`.
fn generating(command: &[&str]) -> Pipeline<FakePorts> {
    root_only(vec![
        target("gen", &[]),
        target_with("use", |target| {
            target.run = Some(
                command
                    .iter()
                    .map(|argument| Written::new(*argument))
                    .collect(),
            );
            target.deps = vec![Written::new("gen")];
        }),
    ])
}

#[test]
fn a_program_named_through_in_outside_the_ceiling_is_refused_at_plan() {
    let mut pipeline = root_only(vec![target_with("gen", |target| {
        target.run = Some(vec![Written::new("$in")]);
        target.inputs = vec![Written::new("scripts/gen.sh")];
    })]);

    let error = pipeline
        .plan(&entry(), &Scope::Workspace, &Overrides::new())
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        "build.lua: //:gen: program scripts/gen.sh is outside the ceiling (proc.run allows: cc)"
    );
    assert!(
        matches!(&error, Error::Refused { want, .. } if want == &Want::Program(program("scripts/gen.sh"))),
        "{error}"
    );
    let ports = pipeline.ports();
    assert_eq!(ports.digester.digest_calls(), 0);
    assert_eq!(ports.tools.lookups(), []);
}

#[test]
fn an_output_conflict_is_refused_at_plan() {
    let writing_x = |name: &str| {
        target_with(name, |target| {
            target.outputs = Some(vec![Written::new("x")]);
        })
    };
    let mut pipeline = root_only(vec![writing_x("a"), writing_x("b")]);

    let error = pipeline
        .plan(&entry(), &Scope::Workspace, &Overrides::new())
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        "out/x: //:a (build.lua) and //:b (build.lua) cannot both write here"
    );
    assert!(matches!(error, Error::OutputConflict { .. }));
    assert_eq!(pipeline.ports().tools.lookups(), []);
}

#[test]
fn an_output_conflict_comes_before_a_refusal() {
    let writing_x = |name: &str, run: &str| {
        target_with(name, |target| {
            target.run = Some(vec![Written::new(run)]);
            target.outputs = Some(vec![Written::new("x")]);
        })
    };
    let mut pipeline = root_only(vec![writing_x("a", "cc"), writing_x("b", "docker")]);

    let error = pipeline
        .plan(&entry(), &Scope::Workspace, &Overrides::new())
        .unwrap_err();

    assert!(matches!(error, Error::OutputConflict { .. }), "{error}");
}

#[test]
fn a_built_program_is_never_looked_up() {
    let mut pipeline = generating(&["out/gen", "$out"]);

    let plan = planned(&mut pipeline);

    assert_eq!(pipeline.ports().tools.lookups(), [program("cc")]);
    assert_eq!(plan.wanted().programs, BTreeSet::from([program("cc")]));
    assert_eq!(tool_of(&plan, "//:use"), None);
    assert_eq!(
        tool_of(&plan, "//:gen"),
        Some(ToolIdentity {
            program: program("cc"),
            fingerprint: Digest::of(b"cc 14.2"),
        })
    );
}

#[test]
fn plan_json_lists_in_programs_and_not_built_ones() {
    let mut pipeline = root_only(vec![
        target("gen", &[]),
        target_with("script", |target| {
            target.run = Some(vec![Written::new("$in")]);
            target.inputs = vec![Written::new("scripts/run.sh")];
        }),
        target_with("use", |target| {
            target.run = Some(["out/gen", "$out"].map(Written::new).to_vec());
            target.deps = vec![Written::new("gen")];
        }),
    ]);
    let ceiling = &mut pipeline.ports_mut().manifest.workspace.ceiling;
    ceiling.run.insert(program("scripts/run.sh"));
    ceiling.read.insert(dir("scripts"));
    put(&mut pipeline, "scripts/run.sh", "echo run", 100);

    let plan = planned(&mut pipeline);

    assert_eq!(
        plan.wanted().programs,
        BTreeSet::from([program("cc"), program("scripts/run.sh")])
    );
    let json = json::canonical::to_string(&plan).unwrap();
    assert!(
        json.contains(r#""programs":["cc","scripts/run.sh"]"#),
        "{json}"
    );
}

#[test]
fn plan_never_asks_the_approver() {
    let mut pipeline = Pipeline::new(ports(FakeSource::new(workspace(vec![(
        "",
        file(vec![target("app", &[])], &[]),
    )]))));
    pipeline.ports_mut().approver = FakeApprover::answering(Answer::Fail("asked".to_owned()));
    pipeline
        .ports_mut()
        .tools
        .fingerprints
        .insert(program("cc"), Digest::of(b"cc 14.2"));

    planned(&mut pipeline);

    assert_eq!(pipeline.ports().approver.asked, []);
}
