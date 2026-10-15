//! The facade over a wired workspace: one method per command.
//!
//! Its own module because it is what a caller holds: the binary and an embedder alike open a
//! [`Workspace`] and call a command on it, and neither names an adapter. What each command
//! needs of the workspace is decided here.
//!
//! Responsibilities: [`Workspace`]: opening one, reading a target or a query as a person writes
//! it, and the commands.
//!
//! Non-responsibilities: the sequence of phases, which is the pipeline's; making the adapters,
//! which is the wiring's; and showing anything, which is the caller's.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use buildl_core::{
    ActionOutcome, Declaration, Directory, Label, LiveOutput, OnFailure, Pipeline, Plan, Query,
    Reporter, Result, Scope, SettingName, SettingValue, TargetGraph, pipeline,
};

use crate::local_ports::Wiring;
use crate::report::Silent;

/// A workspace wired with the real adapters.
///
/// Opening one reads its manifest and nothing else. The state directory is taken by
/// [`Workspace::plan`] and [`Workspace::build`] only, and only while one of them runs, so a
/// workspace kept open holds nothing while it is idle.
#[derive(Debug)]
pub struct Workspace {
    wiring: Wiring,
}

impl Workspace {
    /// The workspace at or above `start`. Reads `buildl.toml`; writes nothing.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Adapter`](buildl_core::Error::Adapter), naming
    /// [`PortName::Manifest`](buildl_core::PortName::Manifest), when no directory at or above
    /// `start` holds a `buildl.toml`, and when the one found cannot be read or is refused.
    pub fn open(start: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            wiring: Wiring::read(start.as_ref())?,
        })
    }

    /// The workspace root: canonical and absolute.
    #[must_use]
    pub fn root(&self) -> &Path {
        self.wiring.root().as_path()
    }

    /// A target as a person names it: `//dir:name`, or `:name` / `name` in the directory the
    /// workspace was opened from.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidName`](buildl_core::Error::InvalidName) when `raw` fails the
    /// label grammar, and, for the two relative forms only, when the directory the workspace
    /// was opened from has a path below the root that no label can hold.
    pub fn label(&self, raw: &str) -> Result<Label> {
        if raw.starts_with("//") {
            // A full label names its own directory, so where the workspace was opened from is
            // not read.
            return Label::parse(raw);
        }
        let base = Directory::parse(self.wiring.root().within().to_string_lossy())?;
        Label::resolve(raw, &base)
    }

    /// A query as a person writes it: `deps(<target>)` or `rdeps(<target>)`, with the target read
    /// as [`Workspace::label`] reads one.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidQuery`](buildl_core::Error::InvalidQuery) when `raw` is in neither
    /// form, and whatever [`Workspace::label`] returns for the target.
    pub fn query_of(&self, raw: &str) -> Result<Query> {
        Query::parse(raw, |target| self.label(target))
    }

    /// Evaluates every build file twice and returns the declarations when both runs agree.
    ///
    /// Takes no lock and opens no state file, so it is stopped neither by a running build nor
    /// by a state directory that cannot be used.
    ///
    /// # Errors
    ///
    /// Returns whatever [`pipeline::check`] returns.
    pub fn check(&self) -> Result<Vec<Declaration>> {
        pipeline::check(&self.wiring.source(), self.wiring.entry())
    }

    /// Evaluates every build file once and resolves the declarations into the target graph.
    ///
    /// Takes no lock and opens no state file, as [`Workspace::check`].
    ///
    /// # Errors
    ///
    /// Returns whatever [`pipeline::graph`] returns.
    pub fn graph(&self) -> Result<TargetGraph> {
        pipeline::graph(&self.wiring.source(), self.wiring.entry())
    }

    /// Evaluates every build file once and answers `query`: the targets it reaches, in label
    /// order, without the target it names.
    ///
    /// Takes no lock and opens no state file, as [`Workspace::check`].
    ///
    /// # Errors
    ///
    /// Returns whatever [`pipeline::query`] returns.
    pub fn query(&self, query: &Query) -> Result<BTreeSet<Label>> {
        pipeline::query(&self.wiring.source(), self.wiring.entry(), query)
    }

    /// Plans a build of `scope` with `overrides` given to build settings, and runs nothing.
    ///
    /// Holds the state directory while it runs. It writes the lock file, the stat cache when a
    /// digest was added to it, and the action cache when an interrupted run left rows behind;
    /// it writes no output and no graph.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Refused`](buildl_core::Error::Refused) when the state directory is
    /// outside what the workspace allows a build to write into, and
    /// [`Error::Adapter`](buildl_core::Error::Adapter), naming
    /// [`PortName::ActionCache`](buildl_core::PortName::ActionCache), when another process
    /// holds the workspace. Otherwise returns whatever making the adapters returns, then
    /// whatever [`Pipeline::plan`] returns.
    pub fn plan(
        &self,
        scope: &Scope,
        overrides: &BTreeMap<SettingName, SettingValue>,
    ) -> Result<Plan> {
        let lock = self.wiring.lock()?;
        Pipeline::new(self.wiring.ports(&lock, Silent)?).plan(self.wiring.entry(), scope, overrides)
    }

    /// Builds `scope`, telling `reporter` how it goes.
    ///
    /// Holds the state directory while it runs. Whatever the pipeline returns, the rows the
    /// build recorded are folded into the action cache before the directory is let go. The
    /// reporter is the caller's: it is lent for this one build, and holds every call once this
    /// returns.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Adapter`](buildl_core::Error::Adapter), naming
    /// [`PortName::Manifest`](buildl_core::PortName::Manifest), when the workspace asks for a
    /// strategy other than `bare`: nothing has been made then, and nothing was run. After that,
    /// as [`Workspace::plan`], then whatever [`Pipeline::build`] returns. A build that
    /// succeeded and whose rows could not be folded returns the fold's error.
    pub fn build<R: Reporter>(
        &self,
        scope: &Scope,
        overrides: &BTreeMap<SettingName, SettingValue>,
        on_failure: OnFailure,
        live: LiveOutput,
        reporter: &mut R,
    ) -> Result<Vec<ActionOutcome>> {
        self.wiring.runs()?;
        let lock = self.wiring.lock()?;
        let mut pipeline = Pipeline::new(self.wiring.ports(&lock, reporter)?);
        let built = pipeline.build(self.wiring.entry(), scope, overrides, on_failure, live);
        let folded = pipeline.ports_mut().cache.fold();
        // The adapters are let go before the lock that kept the directory theirs.
        drop(pipeline);
        concluded(built, folded)
    }
}

/// What a build returns once its rows were folded: its own error first, then the fold's.
fn concluded<T>(built: Result<T>, folded: Result<()>) -> Result<T> {
    match (built, folded) {
        (Err(error), _) | (Ok(_), Err(error)) => Err(error),
        (Ok(value), Ok(())) => Ok(value),
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::{BTreeMap, BTreeSet};
    use std::fs;
    use std::path::{Path, PathBuf};

    use buildl_core::json::canonical;
    use buildl_core::{
        Captured, Channel, Diagnostic, Error, Label, LiveOutput, NameKind, OnFailure, OutcomeClass,
        OutcomeEvent, PortName, ReadyAction, Reporter, Result, Scope, Summary, Tally, Verdict,
        Want,
    };

    use crate::report::Silent;
    use crate::workspace::{Workspace, concluded};

    /// A temporary workspace holding each of `files`, and its canonical root.
    fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        for (path, text) in files {
            let path = root.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        (dir, root)
    }

    /// A workspace with a `lib/` and a directory no label can name.
    fn directories() -> (tempfile::TempDir, PathBuf) {
        let (dir, root) = workspace(&[("buildl.toml", "")]);
        fs::create_dir(root.join("lib")).unwrap();
        fs::create_dir(root.join("my dir")).unwrap();
        (dir, root)
    }

    fn label(text: &str) -> Label {
        Label::parse(text).unwrap()
    }

    /// A manifest that allows a build to write its outputs and its state, to read every file,
    /// and to run `cp` and `false`, one action at a time.
    const BARE: &str = "[ceiling]\nproc.run = [\"cp\", \"false\"]\nfs.read = [\".\"]\nfs.write = [\"out\", \".buildl\"]\n\n[execution]\nworkers = 1\nstrategy = \"bare\"\n";

    /// One target, `//:copy`, that copies `a.txt`.
    const COPY: &str = "buildl.target(\"copy\", { inputs = { \"a.txt\" }, run = { \"cp\", \"$in\", \"$out\" } })\n";

    /// The workspace of [`COPY`] under [`BARE`].
    fn copying() -> (tempfile::TempDir, PathBuf) {
        workspace(&[("buildl.toml", BARE), ("build.lua", COPY), ("a.txt", "a\n")])
    }

    /// The names of everything in `dir`, sorted.
    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    /// The names of everything in the state directory of `root` but the stat cache, sorted.
    /// Whether the stat cache is written depends on how long ago an input was last changed.
    fn kept(root: &Path) -> Vec<String> {
        let mut names = names(&root.join(".buildl"));
        names.retain(|name| name != "statcache.json");
        names
    }

    /// A whole build of `workspace`, stopping at the first failure, shown to nobody.
    fn build(workspace: &Workspace) -> Result<Vec<(String, OutcomeClass)>> {
        let outcomes = workspace.build(
            &Scope::Workspace,
            &BTreeMap::new(),
            OnFailure::Stop,
            LiveOutput::Off,
            &mut Silent,
        )?;
        Ok(outcomes
            .into_iter()
            .map(|outcome| (outcome.label.to_string(), outcome.class))
            .collect())
    }

    /// Keeps what a build tells its reporter.
    #[derive(Debug, Default)]
    struct Told {
        begun: Vec<usize>,
        started: Vec<String>,
        finished: Vec<String>,
        summaries: Vec<Summary>,
    }

    impl Reporter for Told {
        fn begun(&mut self, targets: usize) {
            self.begun.push(targets);
        }

        fn started(&mut self, action: &ReadyAction) {
            self.started.push(action.label.to_string());
        }

        fn printed(&mut self, _label: &Label, _channel: Channel, _bytes: &[u8]) {}

        fn finished(&mut self, event: &OutcomeEvent, _printed: Captured<'_>) {
            self.finished.push(event.outcome.label.to_string());
        }

        fn summary(&mut self, summary: Summary) {
            self.summaries.push(summary);
        }
    }

    /// An error no adapter makes, told apart by its text.
    fn failure(text: &str) -> Error {
        Error::Adapter {
            port: PortName::ActionCache,
            diagnostic: Diagnostic::new(text),
        }
    }

    #[test]
    fn the_root_is_canonical_wherever_the_workspace_was_opened_from() {
        let (dir, root) = directories();

        assert_eq!(Workspace::open(dir.path()).unwrap().root(), root);
        assert_eq!(Workspace::open(root.join("lib")).unwrap().root(), root);
    }

    #[test]
    fn a_relative_target_is_read_against_the_directory_the_workspace_was_opened_from() {
        let (_dir, root) = directories();

        let below = Workspace::open(root.join("lib")).unwrap();
        assert_eq!(below.label("words").unwrap(), label("//lib:words"));
        assert_eq!(below.label(":words").unwrap(), label("//lib:words"));

        let at_root = Workspace::open(&root).unwrap();
        assert_eq!(at_root.label("words").unwrap(), label("//:words"));
        assert_eq!(at_root.label(":words").unwrap(), label("//:words"));
    }

    #[test]
    fn a_full_label_is_read_the_same_from_any_directory() {
        let (_dir, root) = directories();

        for start in [root.clone(), root.join("lib"), root.join("my dir")] {
            let workspace = Workspace::open(&start).unwrap();
            assert_eq!(
                workspace.label("//lib:words").unwrap(),
                label("//lib:words"),
                "{}",
                start.display()
            );
        }
    }

    #[test]
    fn a_start_directory_outside_the_directory_grammar_fails_only_the_relative_forms() {
        let (_dir, root) = directories();
        let workspace = Workspace::open(root.join("my dir")).unwrap();

        for raw in ["words", ":words"] {
            let error = workspace.label(raw).unwrap_err();
            assert!(
                matches!(
                    &error,
                    Error::InvalidName { kind: NameKind::Directory, value, .. } if value == "my dir"
                ),
                "{raw}: {error:?}"
            );
        }
        assert_eq!(
            workspace.label("//lib:words").unwrap(),
            label("//lib:words")
        );
    }

    #[test]
    fn a_target_that_fails_the_label_grammar_is_refused() {
        let (_dir, root) = directories();
        let workspace = Workspace::open(&root).unwrap();

        for raw in ["//lib", "a b"] {
            let error = workspace.label(raw).unwrap_err();
            assert!(
                matches!(&error, Error::InvalidName { .. }),
                "{raw}: {error:?}"
            );
        }
    }

    #[test]
    fn check_and_graph_read_build_files_and_make_nothing() {
        let (_dir, root) = copying();
        let workspace = Workspace::open(&root).unwrap();

        let declarations = workspace.check().unwrap();
        let graph = workspace.graph().unwrap();

        assert_eq!(declarations.len(), 1);
        assert_eq!(
            canonical::to_string(&graph).unwrap(),
            "{\"aliases\":{},\"settings\":{},\"targets\":{\"//:copy\":{\"deps\":[],\
             \"description\":null,\"env\":[],\"freshness\":\"Cached\",\"inputs\":[\"a.txt\"],\
             \"network\":\"Sealed\",\"outputs\":[\"copy\"],\"provenance\":{\"directory\":\"\",\
             \"file\":\"build.lua\"},\"role\":\"Build\",\"run\":[\"cp\",\"$in\",\"$out\"]}}}"
        );
        assert_eq!(names(&root), ["a.txt", "build.lua", "buildl.toml"]);
    }

    #[test]
    fn source_only_commands_mind_neither_a_held_lock_nor_a_cache_that_is_not_canonical() {
        let (_dir, root) = copying();
        let workspace = Workspace::open(&root).unwrap();
        let _held = workspace.wiring.lock().unwrap();
        fs::write(root.join(".buildl/cache.json"), "not a cache").unwrap();

        assert_eq!(workspace.check().unwrap().len(), 1);
        workspace.graph().unwrap();
        let query = workspace.query_of("deps(//:copy)").unwrap();
        assert_eq!(workspace.query(&query).unwrap(), BTreeSet::new());
    }

    #[test]
    fn plan_takes_the_lock_and_builds_nothing() {
        let (_dir, root) = copying();
        let workspace = Workspace::open(&root).unwrap();

        let plan = workspace.plan(&Scope::Workspace, &BTreeMap::new()).unwrap();

        assert!(
            matches!(plan.verdicts(), [Verdict::Dirty { .. }]),
            "{:?}",
            plan.verdicts()
        );
        // The lock file and nothing else: no output, no graph, no row.
        assert_eq!(kept(&root), ["lock"]);
        assert!(!root.join("out").exists());
        // The directory was let go when the plan returned.
        workspace.wiring.lock().unwrap();
    }

    #[test]
    fn plan_is_refused_while_another_buildl_holds_the_workspace() {
        let (_dir, root) = copying();
        let workspace = Workspace::open(&root).unwrap();
        let _held = workspace.wiring.lock().unwrap();

        let error = workspace
            .plan(&Scope::Workspace, &BTreeMap::new())
            .unwrap_err();

        assert!(
            matches!(
                &error,
                Error::Adapter {
                    port: PortName::ActionCache,
                    ..
                }
            ),
            "{error:?}"
        );
    }

    #[test]
    fn plan_is_refused_before_the_state_directory_is_made_when_the_ceiling_leaves_it_out() {
        let manifest = BARE.replace("\"out\", \".buildl\"", "\"out\"");
        let (_dir, root) = workspace(&[
            ("buildl.toml", &manifest),
            ("build.lua", COPY),
            ("a.txt", "a\n"),
        ]);
        let workspace = Workspace::open(&root).unwrap();

        let error = workspace
            .plan(&Scope::Workspace, &BTreeMap::new())
            .unwrap_err();

        assert!(
            matches!(&error, Error::Refused { want: Want::Write(directory), site: None, .. } if directory.as_str() == ".buildl"),
            "{error:?}"
        );
        assert!(!root.join(".buildl").exists());
    }

    #[test]
    fn plan_works_under_a_strategy_this_version_cannot_run() {
        let manifest = BARE.replace("strategy = \"bare\"\n", "");
        let (_dir, root) = workspace(&[
            ("buildl.toml", &manifest),
            ("build.lua", COPY),
            ("a.txt", "a\n"),
        ]);
        let workspace = Workspace::open(&root).unwrap();

        let plan = workspace.plan(&Scope::Workspace, &BTreeMap::new()).unwrap();

        assert_eq!(plan.len(), 1);
    }

    #[test]
    fn a_build_runs_its_actions_and_leaves_the_graph_it_resolved() {
        let (_dir, root) = copying();
        let workspace = Workspace::open(&root).unwrap();

        let outcomes = build(&workspace).unwrap();

        assert_eq!(outcomes, [("//:copy".to_owned(), OutcomeClass::Built)]);
        assert_eq!(fs::read(root.join("out/copy")).unwrap(), b"a\n");
        assert_eq!(
            fs::read(root.join(".buildl/graph.json")).unwrap(),
            canonical::to_vec(&workspace.graph().unwrap()).unwrap()
        );
        // The directory was let go when the build returned.
        workspace.wiring.lock().unwrap();
    }

    #[test]
    fn a_named_target_is_all_a_build_runs_and_the_graph_it_leaves_is_whole() {
        let (_dir, root) = workspace(&[
            ("buildl.toml", BARE),
            (
                "build.lua",
                "buildl.target(\"copy\", { inputs = { \"a.txt\" }, run = { \"cp\", \"$in\", \"$out\" } })\n\
                 buildl.target(\"other\", { inputs = { \"a.txt\" }, run = { \"cp\", \"$in\", \"$out\" } })\n",
            ),
            ("a.txt", "a\n"),
        ]);
        let workspace = Workspace::open(&root).unwrap();
        let scope = Scope::Targets(BTreeSet::from([workspace.label("copy").unwrap()]));

        let outcomes = workspace
            .build(
                &scope,
                &BTreeMap::new(),
                OnFailure::Stop,
                LiveOutput::Off,
                &mut Silent,
            )
            .unwrap();

        assert_eq!(outcomes.len(), 1);
        assert_eq!(names(&root.join("out")), ["copy"]);
        assert_eq!(
            fs::read(root.join(".buildl/graph.json")).unwrap(),
            canonical::to_vec(&workspace.graph().unwrap()).unwrap()
        );
    }

    #[test]
    fn the_reporter_a_build_is_lent_holds_every_call_once_the_build_returns() {
        let (_dir, root) = copying();
        let workspace = Workspace::open(&root).unwrap();
        let mut told = Told::default();

        for _ in 0..2 {
            workspace
                .build(
                    &Scope::Workspace,
                    &BTreeMap::new(),
                    OnFailure::Stop,
                    LiveOutput::Off,
                    &mut told,
                )
                .unwrap();
        }

        assert_eq!(told.begun, [1, 1]);
        // Started once: the second build found the target cached and ran nothing.
        assert_eq!(told.started, ["//:copy"]);
        assert_eq!(told.finished, ["//:copy", "//:copy"]);
        assert_eq!(
            told.summaries,
            [
                Summary {
                    all: Tally {
                        failed: 0,
                        skipped: 0,
                        built: 1,
                        cached: 0
                    },
                    tests: Tally::default()
                },
                Summary {
                    all: Tally {
                        failed: 0,
                        skipped: 0,
                        built: 0,
                        cached: 1
                    },
                    tests: Tally::default()
                }
            ]
        );
    }

    #[test]
    fn build_is_refused_while_another_buildl_holds_the_workspace() {
        let (_dir, root) = copying();
        let workspace = Workspace::open(&root).unwrap();
        let _held = workspace.wiring.lock().unwrap();

        let error = build(&workspace).unwrap_err();

        assert!(
            matches!(
                &error,
                Error::Adapter {
                    port: PortName::ActionCache,
                    ..
                }
            ),
            "{error:?}"
        );
        assert!(!root.join("out").exists());
    }

    #[test]
    fn concluded_returns_the_builds_error_before_the_folds() {
        let both: Result<()> = concluded(Err(failure("the build")), Err(failure("the fold")));
        assert_eq!(both.unwrap_err().to_string(), "action cache: the build");

        let one: Result<()> = concluded(Err(failure("the build")), Ok(()));
        assert_eq!(one.unwrap_err().to_string(), "action cache: the build");
    }

    #[test]
    fn concluded_returns_the_folds_error_for_a_build_that_succeeded() {
        let folded = concluded(Ok(7), Err(failure("the fold")));
        assert_eq!(folded.unwrap_err().to_string(), "action cache: the fold");
    }

    #[test]
    fn concluded_passes_the_value_on_when_both_succeeded() {
        assert_eq!(concluded(Ok(7), Ok(())).unwrap(), 7);
    }

    #[test]
    fn a_build_folds_its_rows_into_the_snapshot_and_removes_the_journal() {
        let (_dir, root) = copying();
        let workspace = Workspace::open(&root).unwrap();

        build(&workspace).unwrap();

        assert_eq!(
            kept(&root),
            ["cache.json", "cas", "graph.json", "lock", "log.jsonl"]
        );
        assert!(
            fs::read_to_string(root.join(".buildl/cache.json"))
                .unwrap()
                .starts_with("{\"//:copy\":{")
        );
    }

    #[test]
    fn a_build_finds_the_rows_the_build_before_it_folded() {
        let (_dir, root) = copying();
        let workspace = Workspace::open(&root).unwrap();
        build(&workspace).unwrap();
        let snapshot = fs::read(root.join(".buildl/cache.json")).unwrap();

        let again = build(&workspace).unwrap();

        assert_eq!(again, [("//:copy".to_owned(), OutcomeClass::Cached)]);
        assert_eq!(fs::read(root.join(".buildl/cache.json")).unwrap(), snapshot);
    }

    #[test]
    fn a_build_with_a_failed_action_folds_the_rows_of_what_was_built() {
        let (_dir, root) = workspace(&[
            ("buildl.toml", BARE),
            (
                "build.lua",
                "buildl.target(\"a-good\", { inputs = { \"a.txt\" }, run = { \"cp\", \"$in\", \"$out\" } })\n\
                 buildl.target(\"bad\", { run = { \"false\" } })\n",
            ),
            ("a.txt", "a\n"),
        ]);
        let workspace = Workspace::open(&root).unwrap();

        let outcomes = build(&workspace).unwrap();

        assert_eq!(outcomes[0], ("//:a-good".to_owned(), OutcomeClass::Built));
        assert_eq!(outcomes[1].0, "//:bad");
        assert!(
            matches!(outcomes[1].1, OutcomeClass::ActionFailure { .. }),
            "{:?}",
            outcomes[1].1
        );
        let state = root.join(".buildl");
        assert!(!state.join("cache.journal").exists());
        let snapshot = fs::read_to_string(state.join("cache.json")).unwrap();
        assert!(snapshot.starts_with("{\"//:a-good\":{"), "{snapshot}");
        assert!(!snapshot.contains("//:bad"), "{snapshot}");
    }

    #[test]
    fn a_build_that_returns_an_error_folds_the_rows_recorded_before_it() {
        let (_dir, root) = copying();
        // A log that cannot be appended to: the build stops once its first target is recorded.
        fs::create_dir_all(root.join(".buildl/log.jsonl")).unwrap();
        let workspace = Workspace::open(&root).unwrap();

        let error = build(&workspace).unwrap_err();

        assert!(
            matches!(
                &error,
                Error::Adapter {
                    port: PortName::EventLog,
                    ..
                }
            ),
            "{error:?}"
        );
        let state = root.join(".buildl");
        assert!(!state.join("cache.journal").exists());
        assert!(
            fs::read_to_string(state.join("cache.json"))
                .unwrap()
                .starts_with("{\"//:copy\":{")
        );
        // The directory was let go although the build failed.
        workspace.wiring.lock().unwrap();
    }

    #[test]
    fn build_refuses_a_strategy_it_cannot_run_before_anything_is_made() {
        let manifest = BARE.replace("strategy = \"bare\"\n", "");
        let (_dir, root) = workspace(&[
            ("buildl.toml", &manifest),
            ("build.lua", COPY),
            ("a.txt", "a\n"),
        ]);
        let workspace = Workspace::open(&root).unwrap();

        let error = build(&workspace).unwrap_err();

        assert!(
            matches!(
                &error,
                Error::Adapter {
                    port: PortName::Manifest,
                    ..
                }
            ),
            "{error:?}"
        );
        assert_eq!(
            error.to_string(),
            "manifest: buildl.toml: [execution] strategy is \"sandbox\" (the default when it is \
             not written), and this version of buildl runs \"bare\" only: write strategy = \
             \"bare\" under [execution]"
        );
        // Refused before the lock was taken: no state directory, no output.
        assert_eq!(names(&root), ["a.txt", "build.lua", "buildl.toml"]);
    }
}
