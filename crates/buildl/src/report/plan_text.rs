//! A plan as lines for a person: what a build would do with each target, and why.
//!
//! Its own file because it is the second form of a plan. The canonical JSON of a plan holds
//! both keys of every target, for a program to compare; this holds one line a target, saying in
//! words what the comparison found.
//!
//! Responsibilities: [`plan_text`], and the words for each verdict, each reason, and each
//! difference between two keys; the wants and the plugins behind them are shown as their own
//! types render them.
//!
//! Non-responsibilities: reaching a verdict, which the phase that plans does; and writing the
//! text anywhere.

use buildl_core::{Difference, DirtyReason, NodeId, OwnComponents, Plan, PlannedKey, Verdict};

/// One way a key differs from the recorded one, in words. `own` is the key as planned: it
/// names the program a changed tool is.
fn difference_text(difference: &Difference, own: &OwnComponents) -> String {
    match difference {
        Difference::Command => "command changed".to_owned(),
        Difference::Input(path) => format!("input {path} changed"),
        Difference::InputOrder => "inputs reordered".to_owned(),
        Difference::Outputs => "declared outputs changed".to_owned(),
        Difference::Env(name) => format!("environment variable {name} changed"),
        Difference::Tool => own.tool.as_ref().map_or_else(
            || "tool changed".to_owned(),
            |tool| format!("tool {} changed", tool.program),
        ),
        Difference::Setting(name) => format!("setting {name} changed"),
        Difference::Network => "network access changed".to_owned(),
        Difference::Platform => "platform changed".to_owned(),
        Difference::Dep(label) => format!("dependency {label} changed"),
        Difference::DepOrder => "dependencies reordered".to_owned(),
        _ => "something this buildl cannot name changed".to_owned(),
    }
}

/// Why a target will run, in words. `key` is the target's key as planned.
fn reason_text(reason: &DirtyReason, key: &PlannedKey) -> String {
    match reason {
        DirtyReason::Always => "declared to run on every build".to_owned(),
        DirtyReason::NeverBuilt => "never built".to_owned(),
        DirtyReason::Rerun => "asked to run again".to_owned(),
        DirtyReason::Changed(differences) => {
            let own = match key {
                PlannedKey::Finished(key) => &key.components().own,
                PlannedKey::Unfinished(unfinished) => &unfinished.own,
            };
            let changes: Vec<String> = differences
                .iter()
                .map(|difference| difference_text(difference, own))
                .collect();
            changes.join("; ")
        }
    }
}

/// The labels of `ids` in `plan`, joined by a comma and a space.
fn labels(plan: &Plan, ids: &[NodeId]) -> String {
    let names: Vec<String> = ids
        .iter()
        .filter_map(|id| plan.label(*id))
        .map(ToString::to_string)
        .collect();
    names.join(", ")
}

/// `plan` as text: one line per target in label order, then one line of counts, each ending
/// with a newline.
///
/// A target that needs nothing reads `clean  <label>`. One that will run reads
/// `run    <label>: ` and the reason: `never built`, `declared to run on every build`, or
/// every way its key differs from the one its last build recorded, joined by `; `. One that
/// runs only if a dependency it waits on produces something else reads
/// `maybe  <label>: runs only if <those dependencies> produces something else`. The last line
/// counts the three: `1 to run, 1 conditional, 2 clean`.
///
/// Between the targets and the counts come the plan's wants, worded as the approval prompt words
/// them, as in `  run programs: go, protoc`. When plugins' targets are behind them, one line for
/// each chain of each want follows, as in
/// `  go: declared by plugin go@1.2.0 for //services/api:server`.
///
/// Each discovery target that is not up to date follows, as `<label> is not up to date; targets
/// declared from it may change`: the targets declared from its last result may differ once it
/// has run.
#[must_use]
pub fn plan_text(plan: &Plan) -> String {
    let mut lines = Vec::new();
    let (mut run, mut maybe, mut clean) = (0_usize, 0_usize, 0_usize);
    for (verdict, id) in plan.verdicts().iter().zip((0_u32..).map(NodeId::new)) {
        let Some(label) = plan.label(id) else {
            continue;
        };
        match verdict {
            Verdict::Clean { .. } => {
                clean += 1;
                lines.push(format!("clean  {label}\n"));
            }
            Verdict::Dirty { reason, key } => {
                run += 1;
                lines.push(format!("run    {label}: {}\n", reason_text(reason, key)));
            }
            Verdict::Conditional(unfinished) => {
                maybe += 1;
                lines.push(format!(
                    "maybe  {label}: runs only if {} produces something else\n",
                    labels(plan, &unfinished.awaiting)
                ));
            }
        }
    }
    if !plan.wanted().is_empty() {
        lines.push(format!("{}\n", plan.wanted()));
    }
    if !plan.attribution().is_empty() {
        lines.push(format!("{}\n", plan.attribution()));
    }
    for label in plan.stale_discovery() {
        lines.push(format!(
            "{label} is not up to date; targets declared from it may change\n"
        ));
    }
    lines.push(format!(
        "{run} to run, {maybe} conditional, {clean} clean\n"
    ));
    lines.concat()
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::{BTreeMap, BTreeSet};
    use std::path::PathBuf;

    use buildl_core::{
        Action, ActionCache, ActionKey, Approval, Approver, ApproverMode, Argument, Attribution,
        BuildFile, CacheRow, Ceiling, Clock, Command, ContentStore, Declaration, DeclarationSource,
        Declared, DepProducts, Diagnostic, Digest, Digester, Directory, DirtyReason, Discovered,
        Dispatcher, EnvName, EnvValue, Error, Evaluated, EventLog, ExecStrategy, Execution,
        FileMode, FileStat, Found, Freshness, GraphRecord, Host, InputDigest, InputStat,
        KeyComponents, Label, LiveSink, LogLine, Manifest, NetworkAccess, Origin, OutcomeEvent,
        OutputName, OutputPath, OwnComponents, Plan, PlannedKey, Platform, PluginCap, PluginName,
        PluginRef, PluginVersion, PortName, PortSet, Ports, Produced, Program, Provenance,
        RawOutcome, ReadyAction, Rerun, Result, RunHistory, Setting, SettingName, SettingValue,
        SourcePath, StatCache, StatEntry, Target, TargetGraph, TargetRole, Timestamp, ToolIdentity,
        ToolResolver, Turn, Verdict, WantedSet, Workspace, plan, resolve,
    };

    use super::{plan_text, reason_text};
    use crate::report::silent::Silent;

    /// The machine a plan asks about: every file exists and holds its own path, every program
    /// is found, and every environment variable has the value `-Wall`.
    #[derive(Debug)]
    struct Machine;

    impl Manifest for Machine {
        fn workspace(&self) -> Result<Workspace> {
            Ok(Workspace {
                out: Directory::parse("out")?,
                state: Directory::parse(".buildl")?,
                ceiling: Ceiling {
                    run: BTreeSet::from([Program::parse("cc")?]),
                    env: BTreeSet::from([EnvName::parse("CC_FLAGS")?]),
                    read: BTreeSet::from([Directory::root()]),
                    write: BTreeSet::from([Directory::parse("out")?, Directory::parse(".buildl")?]),
                },
                execution: Execution::default(),
                approver: ApproverMode::Manifest,
                plugins: BTreeMap::from([(
                    PluginName::parse("go")?,
                    PluginCap {
                        run: BTreeSet::from([Program::parse("cc")?]),
                        ..PluginCap::default()
                    },
                )]),
            })
        }
    }

    impl Host for Machine {
        fn platform(&self) -> Platform {
            Platform::parse("linux/x86_64").unwrap()
        }

        fn env(&self, _name: &EnvName) -> Result<Option<EnvValue>> {
            Ok(Some(EnvValue::parse("-Wall")?))
        }
    }

    impl Digester for Machine {
        fn stat(&self, _path: &SourcePath) -> Result<InputStat> {
            Ok(InputStat::File(FileStat {
                modified: Timestamp::UNIX_EPOCH,
                size: 1,
            }))
        }

        fn digest(&self, paths: &[SourcePath]) -> Result<Vec<Digest>> {
            Ok(paths
                .iter()
                .map(|path| Digest::of(path.as_str().as_bytes()))
                .collect())
        }
    }

    impl StatCache for Machine {
        fn entry(&self, _path: &SourcePath) -> Option<StatEntry> {
            None
        }

        fn record(&mut self, _path: SourcePath, _entry: StatEntry) {}

        fn commit(&mut self) -> Result<()> {
            Ok(())
        }
    }

    impl ToolResolver for Machine {
        fn fingerprint(&self, program: &Program) -> Result<Option<Digest>> {
            Ok(Some(Digest::of(program.as_str().as_bytes())))
        }
    }

    /// What earlier builds recorded.
    #[derive(Debug, Default)]
    struct Rows(BTreeMap<Label, CacheRow>);

    impl ActionCache for Rows {
        fn row(&self, label: &Label) -> Option<&CacheRow> {
            self.0.get(label)
        }

        fn labels(&self) -> Vec<Label> {
            self.0.keys().cloned().collect()
        }

        fn record(&mut self, label: Label, row: CacheRow) -> Result<()> {
            self.0.insert(label, row);
            Ok(())
        }

        fn forget(&mut self, label: &Label) -> Result<()> {
            self.0.remove(label);
            Ok(())
        }
    }

    /// Stands in for every port a plan never asks.
    #[derive(Debug)]
    struct Unused;

    /// The failure of a port a plan was not expected to ask.
    fn unused(port: PortName) -> Error {
        Error::Adapter {
            port,
            diagnostic: Diagnostic::new("a plan does not ask this port"),
        }
    }

    impl DeclarationSource for Unused {
        fn evaluate(&self, _file: &BuildFile, _discovered: &Discovered) -> Result<Evaluated> {
            Ok(Evaluated::Absent)
        }
    }

    impl ContentStore for Unused {
        fn put(&self, _bytes: &[u8]) -> Result<Digest> {
            Err(unused(PortName::ContentStore))
        }

        fn contains(&self, _digest: &Digest) -> bool {
            false
        }

        fn get(&self, _digest: &Digest) -> Result<Option<Vec<u8>>> {
            Err(unused(PortName::ContentStore))
        }

        fn clear(&self, _at: &OutputPath) -> Result<()> {
            Err(unused(PortName::ContentStore))
        }

        fn place(&self, _digest: &Digest, _mode: FileMode, _at: &OutputPath) -> Result<()> {
            Err(unused(PortName::ContentStore))
        }
    }

    impl EventLog for Unused {
        fn append(&mut self, _event: &OutcomeEvent) -> Result<()> {
            Err(unused(PortName::EventLog))
        }
    }

    impl RunHistory for Unused {
        fn lines(&self) -> Result<Vec<LogLine>> {
            Err(unused(PortName::RunHistory))
        }
    }

    impl ExecStrategy for Unused {
        type Dir = ();

        fn prepare(&self, _action: &ReadyAction) -> Result<()> {
            Err(unused(PortName::ExecStrategy))
        }

        fn run(
            &self,
            _dir: &(),
            _action: &ReadyAction,
            _live: Option<LiveSink<'_>>,
        ) -> Result<RawOutcome> {
            Err(unused(PortName::ExecStrategy))
        }

        fn collect<C: ContentStore>(
            &self,
            _dir: &(),
            _action: &ReadyAction,
            _cas: &C,
        ) -> Result<Vec<Found>> {
            Err(unused(PortName::ExecStrategy))
        }
    }

    impl Dispatcher for Unused {
        fn dispatch<T, M, R, W, S>(&mut self, _work: W, _step: S) -> Result<()>
        where
            T: Send,
            M: Send,
            R: Send,
            W: Fn(T, &(dyn Fn(M) + Sync)) -> R + Sync,
            S: FnMut(Turn<M, R>) -> Vec<T>,
        {
            Err(unused(PortName::Dispatcher))
        }
    }

    impl Clock for Unused {
        fn now(&self) -> Timestamp {
            Timestamp::UNIX_EPOCH
        }
    }

    impl GraphRecord for Unused {
        fn record(&mut self, _graph: &TargetGraph) -> Result<()> {
            Err(unused(PortName::GraphRecord))
        }
    }

    impl Approver for Unused {
        fn approve(&mut self, _wanted: &WantedSet, _attribution: &Attribution) -> Result<Approval> {
            Err(unused(PortName::Approver))
        }
    }

    /// The bundle a plan is made over here.
    struct Planning;

    impl Ports for Planning {
        type Source = Unused;
        type Manifest = Machine;
        type Host = Machine;
        type Digester = Machine;
        type StatCache = Machine;
        type Tools = Machine;
        type Cache = Rows;
        type Cas = Unused;
        type Log = Unused;
        type Strategy = Unused;
        type Dispatcher = Unused;
        type Clock = Unused;
        type Graph = Unused;
        type Reporter = Silent;
        type Approver = Unused;
    }

    /// The plan of `graph` on the machine, against what `rows` recorded.
    fn planned(graph: &TargetGraph, rows: Rows) -> Plan {
        let mut ports = PortSet::<Planning> {
            source: Unused,
            manifest: Machine,
            host: Machine,
            digester: Machine,
            stat_cache: Machine,
            tools: Machine,
            cache: rows,
            cas: Unused,
            log: Unused,
            strategy: Unused,
            dispatcher: Unused,
            clock: Unused,
            graph: Unused,
            reporter: Silent,
            approver: Unused,
        };
        plan(graph, &BTreeMap::new(), Rerun::Off, &mut ports).unwrap()
    }

    fn label(raw: &str) -> Label {
        Label::parse(raw).unwrap()
    }

    fn command(arguments: &[&str]) -> Command {
        Command::new(
            arguments
                .iter()
                .map(|argument| Argument::parse(*argument).unwrap())
                .collect(),
        )
        .unwrap()
    }

    fn root_file() -> Provenance {
        Provenance::new(PathBuf::from("build.lua"), Directory::root())
    }

    /// A target of the root build file running `run`, with nothing else declared.
    fn target(raw: &str, run: &[&str]) -> Target {
        Target {
            label: label(raw),
            role: TargetRole::Build,
            action: Action::Run(command(run)),
            inputs: Vec::new(),
            deps: Vec::new(),
            outputs: Vec::new(),
            env: Vec::new(),
            network: NetworkAccess::Sealed,
            freshness: Freshness::Cached,
        }
    }

    fn declared(target: Target) -> Declaration {
        Declaration::new(root_file(), Origin::BuildFile, Declared::Target(target))
    }

    fn paths(raw: &[&str]) -> Vec<SourcePath> {
        raw.iter()
            .map(|path| SourcePath::parse(*path).unwrap())
            .collect()
    }

    fn deps(raw: &[&str]) -> Vec<Label> {
        raw.iter().map(|dep| label(dep)).collect()
    }

    /// A graph with a target for every verdict a plan can reach.
    fn graph() -> TargetGraph {
        resolve(vec![
            Declaration::new(
                root_file(),
                Origin::BuildFile,
                Declared::Setting(Setting {
                    name: SettingName::parse("mode").unwrap(),
                    default: SettingValue::parse("debug").unwrap(),
                }),
            ),
            declared(Target {
                freshness: Freshness::Always,
                ..target("//:always", &["cc", "--version"])
            }),
            declared(Target {
                deps: deps(&["//:main.o"]),
                ..target("//:app", &["cc", "$deps", "-o", "$out"])
            }),
            declared(Target {
                inputs: paths(&["changed.c"]),
                deps: deps(&["//:docs"]),
                outputs: vec![OutputName::parse("changed").unwrap()],
                env: vec![EnvName::parse("CC_FLAGS").unwrap()],
                ..target("//:changed", &["cc", "-O$opt:mode", "$in", "-o", "$out"])
            }),
            declared(target("//:docs", &["cc", "--doc"])),
            declared(Target {
                outputs: vec![OutputName::parse("leaf").unwrap()],
                ..target("//:leaf", &["cc", "--leaf"])
            }),
            declared(Target {
                inputs: paths(&["main.c"]),
                ..target("//:main.o", &["cc", "-c", "$in"])
            }),
            declared(target("//:new", &["cc", "--new"])),
            declared(Target {
                inputs: paths(&["b.txt", "a.txt"]),
                deps: deps(&["//:leaf", "//:docs"]),
                ..target("//:reordered", &["cc", "$in"])
            }),
            declared(Target {
                deps: deps(&["//:leaf"]),
                ..target("//:script", &["$deps", "--run"])
            }),
        ])
        .unwrap()
    }

    /// What a plan made with nothing recorded knows of the key of `raw`, without its
    /// dependencies.
    fn own(first: &Plan, graph: &TargetGraph, raw: &str) -> OwnComponents {
        let id = graph.id_of(&label(raw)).unwrap();
        match first.verdict(id).unwrap() {
            Verdict::Dirty {
                key: PlannedKey::Finished(key),
                ..
            } => key.components().own.clone(),
            Verdict::Dirty {
                key: PlannedKey::Unfinished(unfinished),
                ..
            } => unfinished.own.clone(),
            other => unreachable!("with nothing recorded every target is dirty, not {other:?}"),
        }
    }

    /// The row of a build under the key made of `own` and `deps`, which printed nothing. Every
    /// output, a dependency's included, is recorded as the digest of its name.
    fn row(graph: &TargetGraph, own: OwnComponents, deps: &[&str]) -> CacheRow {
        let outputs = own
            .outputs
            .iter()
            .map(|name| Digest::of(name.as_str().as_bytes()))
            .collect();
        let deps = deps
            .iter()
            .map(|dep| {
                let label = label(dep);
                let node = graph.id_of(&label).and_then(|id| graph.node(id)).unwrap();
                DepProducts {
                    outputs: node
                        .outputs
                        .iter()
                        .map(|name| Produced {
                            name: name.clone(),
                            digest: Digest::of(name.as_str().as_bytes()),
                        })
                        .collect(),
                    label,
                }
            })
            .collect();
        let key = ActionKey::new(KeyComponents { own, deps }).unwrap();
        CacheRow::new(key, outputs, Digest::of(b""), Digest::of(b"")).unwrap()
    }

    #[test]
    fn every_reason_has_its_words() {
        let graph = graph();
        let first = planned(&graph, Rows::default());
        let own = |raw: &str| own(&first, &graph, raw);
        let mut rows = Rows::default();
        let mut record = |raw: &str, row: CacheRow| {
            rows.0.insert(label(raw), row);
        };

        // Built before, and nothing changed since.
        record("//:always", row(&graph, own("//:always"), &[]));
        record("//:docs", row(&graph, own("//:docs"), &[]));
        record("//:leaf", row(&graph, own("//:leaf"), &[]));
        // Built before from the same declaration; `//:main.o` is about to run again.
        record("//:app", row(&graph, own("//:app"), &["//:main.o"]));
        // Built before from another `main.c`.
        let mut main = own("//:main.o");
        main.inputs = vec![InputDigest {
            path: SourcePath::parse("main.c").unwrap(),
            digest: Digest::of(b"an older main.c"),
        }];
        record("//:main.o", row(&graph, main, &[]));
        // Built before with the same inputs and dependencies, each list in the other order.
        let mut reordered = own("//:reordered");
        reordered.inputs.reverse();
        record(
            "//:reordered",
            row(&graph, reordered, &["//:docs", "//:leaf"]),
        );
        // Built before by a program that was found on the machine.
        let mut script = own("//:script");
        script.tool = Some(ToolIdentity {
            program: Program::parse("cc").unwrap(),
            fingerprint: Digest::of(b"cc"),
        });
        record("//:script", row(&graph, script, &["//:leaf"]));
        // Built before with every part of its key different, and no dependency.
        let before = OwnComponents {
            command: command(&["cc", "$in"]),
            inputs: vec![InputDigest {
                path: SourcePath::parse("changed.c").unwrap(),
                digest: Digest::of(b"an older changed.c"),
            }],
            outputs: vec![OutputName::parse("older").unwrap()],
            env: BTreeMap::new(),
            tool: Some(ToolIdentity {
                program: Program::parse("cc").unwrap(),
                fingerprint: Digest::of(b"an older cc"),
            }),
            settings: BTreeMap::from([(
                SettingName::parse("mode").unwrap(),
                SettingValue::parse("release").unwrap(),
            )]),
            network: NetworkAccess::Declared,
            platform: Platform::parse("macos/aarch64").unwrap(),
        };
        record("//:changed", row(&graph, before, &[]));

        assert_eq!(
            plan_text(&planned(&graph, rows)),
            concat!(
                "run    //:always: declared to run on every build\n",
                "maybe  //:app: runs only if //:main.o produces something else\n",
                "run    //:changed: command changed; input changed.c changed; ",
                "declared outputs changed; environment variable CC_FLAGS changed; ",
                "tool cc changed; setting mode changed; network access changed; ",
                "platform changed; dependency //:docs changed\n",
                "clean  //:docs\n",
                "clean  //:leaf\n",
                "run    //:main.o: input main.c changed\n",
                "run    //:new: never built\n",
                "run    //:reordered: inputs reordered; dependencies reordered\n",
                "run    //:script: tool changed\n",
                "  run programs:   cc\n",
                "  read variables: CC_FLAGS\n",
                "  write into:     .buildl, out\n",
                "6 to run, 1 conditional, 2 clean\n",
            )
        );
    }

    #[test]
    fn a_rerun_reads_as_asked_to_run_again() {
        let graph = graph();
        let first = planned(&graph, Rows::default());
        let id = graph.id_of(&label("//:new")).unwrap();
        let Some(Verdict::Dirty { key, .. }) = first.verdict(id) else {
            unreachable!("with nothing recorded every target is dirty");
        };
        assert_eq!(reason_text(&DirtyReason::Rerun, key), "asked to run again");
    }

    #[test]
    fn plan_text_lists_attribution_under_the_wanted_set() {
        let go = PluginRef {
            name: PluginName::parse("go").unwrap(),
            version: PluginVersion::parse("1.2.0").unwrap(),
            run: BTreeSet::from([Program::parse("cc").unwrap()]),
            env: BTreeSet::new(),
        };
        let graph = resolve(vec![
            declared(target("//:docs", &["cc", "--doc"])),
            Declaration::new(
                root_file(),
                Origin::plugins(vec![go]).unwrap(),
                Declared::Target(target("//:app", &["cc", "-c"])),
            ),
        ])
        .unwrap();

        assert_eq!(
            plan_text(&planned(&graph, Rows::default())),
            concat!(
                "run    //:app: never built\n",
                "run    //:docs: never built\n",
                "  run programs: cc\n",
                "  write into:   .buildl, out\n",
                "  cc: declared by plugin go@1.2.0 for //:app\n",
                "2 to run, 0 conditional, 0 clean\n",
            )
        );
    }

    #[test]
    fn plan_text_notes_a_stale_discovery() {
        let graph = resolve(vec![declared(target("//svc:pkgs", &["cc", "--list"]))]).unwrap();
        let plan = planned(&graph, Rows::default())
            .with_stale_discovery(vec![label("//svc:pkgs"), label("//svc:more")]);

        assert_eq!(
            plan_text(&plan),
            concat!(
                "run    //svc:pkgs: never built\n",
                "  run programs: cc\n",
                "  write into:   .buildl, out\n",
                "//svc:pkgs is not up to date; targets declared from it may change\n",
                "//svc:more is not up to date; targets declared from it may change\n",
                "1 to run, 0 conditional, 0 clean\n",
            )
        );
    }

    #[test]
    fn a_plan_of_nothing_is_the_wants_of_every_build_and_the_counts() {
        let graph = resolve(Vec::new()).unwrap();
        assert_eq!(
            plan_text(&planned(&graph, Rows::default())),
            "  write into: .buildl, out\n0 to run, 0 conditional, 0 clean\n"
        );
    }
}
