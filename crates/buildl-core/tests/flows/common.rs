//! In-memory implementations of the ports, and builders for the staged values they return.
//!
//! Everything here uses only `buildl_core`'s public API, exactly as an adapter crate would.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use buildl_core::{
    ActionCache, BuildFile, CacheRow, Ceiling, Clock, ContentStore, DeclarationOrder,
    DeclarationSource, Diagnostic, Digest, Digester, Directory, Dispatcher, EnvName, EnvValue,
    Error, Evaluated, EvaluationFailure, EventLog, ExecStrategy, Exit, FileStat, Freshness, Host,
    InputKind, InputStat, Label, LogLine, Manifest, NetworkAccess, OutcomeEvent, OutputPath,
    Platform, PortName, PortSet, Ports, Program, RawOutcome, ReadyAction, Result, RunHistory,
    SourcePath, StagedAlias, StagedDeclaration, StagedFile, StagedItem, StagedRule, StagedSetting,
    StagedSubdir, StagedTarget, StatCache, StatEntry, Sweep, TargetRole, Timestamp, ToolResolver,
    Workspace, Written,
};
use buildl_core::{ActionOutcome, Pipeline};
use buildl_core::{
    Captured, Channel, GraphRecord, LiveSink, OutcomeClass, Reporter, Summary, TargetGraph, Turn,
    json,
};
use buildl_core::{FileMode, Found, FoundEntry, FoundKind, StoredFile};

/// What the fake holds for one directory.
#[derive(Debug, Clone)]
pub enum FakeFile {
    /// A build file that evaluates to these staged values.
    Staged(StagedFile),
    /// A build file whose evaluation fails this way.
    Fails(EvaluationFailure),
}

/// A build-file source holding fixed files per directory, recording every evaluation.
///
/// Built with [`FakeSource::new`] it holds one set of files. Built with
/// [`FakeSource::with_second_run`] it holds two, and switches from the first to the second when
/// the root directory is evaluated a second time, so every directory evaluated after that point
/// reads from the second set. A directory it holds no file for evaluates to
/// [`Evaluated::Absent`].
#[derive(Debug)]
pub struct FakeSource {
    first: BTreeMap<Directory, FakeFile>,
    second: Option<BTreeMap<Directory, FakeFile>>,
    root_evaluations: AtomicU32,
    evaluated: Mutex<Vec<Directory>>,
}

impl FakeSource {
    /// A source returning the same files on every evaluation.
    #[must_use]
    pub const fn new(files: BTreeMap<Directory, FakeFile>) -> Self {
        Self {
            first: files,
            second: None,
            root_evaluations: AtomicU32::new(0),
            evaluated: Mutex::new(Vec::new()),
        }
    }

    /// A source returning `first` until the root is evaluated a second time, and `second` from
    /// then on — a workspace that declares differently on its second load.
    #[must_use]
    pub const fn with_second_run(
        first: BTreeMap<Directory, FakeFile>,
        second: BTreeMap<Directory, FakeFile>,
    ) -> Self {
        Self {
            first,
            second: Some(second),
            root_evaluations: AtomicU32::new(0),
            evaluated: Mutex::new(Vec::new()),
        }
    }

    /// Every directory evaluated so far, in evaluation order.
    #[must_use]
    pub fn evaluated(&self) -> Vec<Directory> {
        self.evaluated
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl DeclarationSource for FakeSource {
    fn evaluate(&self, file: &BuildFile) -> Result<Evaluated> {
        let directory = file.directory();
        if directory.is_root() {
            self.root_evaluations.fetch_add(1, Ordering::SeqCst);
        }
        self.evaluated
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(directory.clone());
        let files = match &self.second {
            Some(second) if self.root_evaluations.load(Ordering::SeqCst) > 1 => second,
            _ => &self.first,
        };
        match files.get(directory) {
            None => Ok(Evaluated::Absent),
            Some(FakeFile::Staged(staged)) => Ok(Evaluated::Staged(staged.clone())),
            Some(FakeFile::Fails(failure)) => Err(Error::Evaluation {
                provenance: file.provenance().clone(),
                failure: failure.clone(),
                diagnostic: Diagnostic::new("fake evaluation failure"),
            }),
        }
    }
}

/// A manifest holding one workspace.
#[derive(Debug, Clone)]
pub struct FakeManifest {
    /// What the manifest says.
    pub workspace: Workspace,
}

impl Manifest for FakeManifest {
    fn workspace(&self) -> Result<Workspace> {
        Ok(self.workspace.clone())
    }
}

/// A machine with a fixed platform and a fixed environment.
#[derive(Debug, Clone)]
pub struct FakeHost {
    /// The platform it reports.
    pub platform: Platform,
    /// The variables that are set.
    pub variables: BTreeMap<EnvName, EnvValue>,
    /// The variables whose value it fails to report.
    pub unreportable: BTreeSet<EnvName>,
}

impl Host for FakeHost {
    fn platform(&self) -> Platform {
        self.platform.clone()
    }

    fn env(&self, name: &EnvName) -> Result<Option<EnvValue>> {
        if self.unreportable.contains(name) {
            return Err(Error::Adapter {
                port: PortName::Host,
                diagnostic: Diagnostic::new(format!("{name} does not hold valid text")),
            });
        }
        Ok(self.variables.get(name).cloned())
    }
}

/// One file a [`FakeDigester`] holds.
#[derive(Debug, Clone)]
pub struct FakeContent {
    /// The file's bytes.
    pub bytes: Vec<u8>,
    /// The metadata reported for it.
    pub stat: FileStat,
}

/// Workspace files held in memory, counting every call made to it.
#[derive(Debug, Default)]
pub struct FakeDigester {
    /// The files that exist.
    pub files: BTreeMap<SourcePath, FakeContent>,
    /// The paths that hold something other than a regular file. One named here is reported as
    /// that, whatever `files` holds for it.
    pub others: BTreeMap<SourcePath, InputKind>,
    stat_calls: Cell<u32>,
    digest_calls: Cell<u32>,
    digested: RefCell<Vec<SourcePath>>,
}

impl FakeDigester {
    /// How many times metadata was asked for.
    #[must_use]
    pub const fn stat_calls(&self) -> u32 {
        self.stat_calls.get()
    }

    /// How many times digests were asked for.
    #[must_use]
    pub const fn digest_calls(&self) -> u32 {
        self.digest_calls.get()
    }

    /// Every path a digest was asked for, in the order asked.
    #[must_use]
    pub fn digested(&self) -> Vec<SourcePath> {
        self.digested.borrow().clone()
    }
}

impl Digester for FakeDigester {
    fn stat(&self, path: &SourcePath) -> Result<InputStat> {
        self.stat_calls.set(self.stat_calls.get() + 1);
        if let Some(kind) = self.others.get(path) {
            return Ok(InputStat::Other(*kind));
        }
        Ok(self
            .files
            .get(path)
            .map_or(InputStat::Missing, |file| InputStat::File(file.stat)))
    }

    fn digest(&self, paths: &[SourcePath]) -> Result<Vec<Digest>> {
        self.digest_calls.set(self.digest_calls.get() + 1);
        self.digested.borrow_mut().extend_from_slice(paths);
        Ok(paths
            .iter()
            .map(|path| {
                self.files
                    .get(path)
                    .map_or_else(|| Digest::of(b""), |file| Digest::of(&file.bytes))
            })
            .collect())
    }
}

/// A stat cache held in memory, counting how often it was committed.
#[derive(Debug, Default)]
pub struct FakeStatCache {
    /// What is recorded.
    pub entries: BTreeMap<SourcePath, StatEntry>,
    commits: u32,
}

impl FakeStatCache {
    /// How many times the entries were committed.
    #[must_use]
    pub const fn commits(&self) -> u32 {
        self.commits
    }
}

impl StatCache for FakeStatCache {
    fn entry(&self, path: &SourcePath) -> Option<StatEntry> {
        self.entries.get(path).copied()
    }

    fn record(&mut self, path: SourcePath, entry: StatEntry) {
        self.entries.insert(path, entry);
    }

    fn commit(&mut self) -> Result<()> {
        self.commits += 1;
        Ok(())
    }
}

/// Programs with fixed fingerprints, recording every lookup.
#[derive(Debug, Default)]
pub struct FakeTools {
    /// The programs that can be found.
    pub fingerprints: BTreeMap<Program, Digest>,
    lookups: RefCell<Vec<Program>>,
}

impl FakeTools {
    /// Every program looked up, in the order asked.
    #[must_use]
    pub fn lookups(&self) -> Vec<Program> {
        self.lookups.borrow().clone()
    }
}

impl ToolResolver for FakeTools {
    fn fingerprint(&self, program: &Program) -> Result<Option<Digest>> {
        self.lookups.borrow_mut().push(program.clone());
        Ok(self.fingerprints.get(program).copied())
    }
}

/// The rows of a previous build and of this one, held in memory.
#[derive(Debug, Default)]
pub struct FakeCache {
    /// The rows.
    pub rows: BTreeMap<Label, CacheRow>,
    /// The targets whose row it fails to record.
    pub failing: BTreeSet<Label>,
    /// The targets whose row it fails to forget.
    pub unforgettable: BTreeSet<Label>,
    recorded: Vec<Label>,
    forgotten: Vec<Label>,
}

impl FakeCache {
    /// Every target a row was recorded for, in the order recorded.
    #[must_use]
    pub fn recorded(&self) -> Vec<Label> {
        self.recorded.clone()
    }

    /// Every target whose row was removed, in the order removed.
    #[must_use]
    pub fn forgotten(&self) -> Vec<Label> {
        self.forgotten.clone()
    }
}

impl ActionCache for FakeCache {
    fn row(&self, label: &Label) -> Option<&CacheRow> {
        self.rows.get(label)
    }

    fn labels(&self) -> Vec<Label> {
        self.rows.keys().cloned().collect()
    }

    fn record(&mut self, label: Label, row: CacheRow) -> Result<()> {
        if self.failing.contains(&label) {
            return Err(Error::Adapter {
                port: PortName::ActionCache,
                diagnostic: Diagnostic::new(format!("{label} cannot be recorded")),
            });
        }
        self.recorded.push(label.clone());
        self.rows.insert(label, row);
        Ok(())
    }

    fn forget(&mut self, label: &Label) -> Result<()> {
        if self.unforgettable.contains(label) {
            return Err(Error::Adapter {
                port: PortName::ActionCache,
                diagnostic: Diagnostic::new(format!("{label} cannot be forgotten")),
            });
        }
        if self.rows.remove(label).is_some() {
            self.forgotten.push(label.clone());
        }
        Ok(())
    }
}

/// What a [`FakeCas`] holds, behind its lock.
#[derive(Debug, Default)]
struct CasState {
    blobs: BTreeMap<Digest, Vec<u8>>,
    placed: BTreeMap<OutputPath, Digest>,
    placements: Vec<(OutputPath, Digest)>,
    unstorable: BTreeSet<Vec<u8>>,
    unplaceable: BTreeSet<OutputPath>,
    modes: BTreeMap<OutputPath, FileMode>,
}

/// A content store held in memory, with the workspace paths content was placed at.
///
/// Actions store what they produce from the threads they run on, so everything it holds sits
/// behind one lock and every method takes `&self`.
#[derive(Debug, Default)]
pub struct FakeCas {
    state: Mutex<CasState>,
}

impl FakeCas {
    fn state(&self) -> MutexGuard<'_, CasState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The content held under `digest`, if any.
    #[must_use]
    pub fn blob(&self, digest: &Digest) -> Option<Vec<u8>> {
        self.state().blobs.get(digest).cloned()
    }

    /// The digest of everything held.
    #[must_use]
    pub fn digests(&self) -> BTreeSet<Digest> {
        self.state().blobs.keys().copied().collect()
    }

    /// What is at each workspace path now: the digest last placed there.
    #[must_use]
    pub fn placed(&self) -> BTreeMap<OutputPath, Digest> {
        self.state().placed.clone()
    }

    /// Every placement that succeeded, in the order made.
    #[must_use]
    pub fn placements(&self) -> Vec<(OutputPath, Digest)> {
        self.state().placements.clone()
    }

    /// Drops the content held under `digest`, as a store that was cleaned would.
    pub fn forget(&self, digest: &Digest) {
        self.state().blobs.remove(digest);
    }

    /// Makes every later attempt to place content at `path` fail.
    pub fn fail_place(&self, path: OutputPath) {
        self.state().unplaceable.insert(path);
    }

    /// Makes every later attempt to store exactly `bytes` fail.
    pub fn fail_put(&self, bytes: Vec<u8>) {
        self.state().unstorable.insert(bytes);
    }

    /// Whether the file at each workspace path now may be run: the mode it was last placed with.
    #[must_use]
    pub fn modes(&self) -> BTreeMap<OutputPath, FileMode> {
        self.state().modes.clone()
    }
}

impl ContentStore for FakeCas {
    fn put(&self, bytes: &[u8]) -> Result<Digest> {
        let mut state = self.state();
        if state.unstorable.contains(bytes) {
            return Err(Error::Adapter {
                port: PortName::ContentStore,
                diagnostic: Diagnostic::new("the content cannot be stored"),
            });
        }
        let digest = Digest::of(bytes);
        state.blobs.insert(digest, bytes.to_vec());
        drop(state);
        Ok(digest)
    }

    fn contains(&self, digest: &Digest) -> bool {
        self.state().blobs.contains_key(digest)
    }

    fn get(&self, digest: &Digest) -> Result<Option<Vec<u8>>> {
        Ok(self.state().blobs.get(digest).cloned())
    }

    fn clear(&self, at: &OutputPath) -> Result<()> {
        let below = format!("{at}/");
        let mut state = self.state();
        state
            .placed
            .retain(|path, _| path != at && !path.as_str().starts_with(&below));
        state
            .modes
            .retain(|path, _| path != at && !path.as_str().starts_with(&below));
        drop(state);
        Ok(())
    }

    fn place(&self, digest: &Digest, mode: FileMode, at: &OutputPath) -> Result<()> {
        let mut state = self.state();
        if !state.blobs.contains_key(digest) {
            return Err(Error::Adapter {
                port: PortName::ContentStore,
                diagnostic: Diagnostic::new(format!("{digest} is not held")),
            });
        }
        if state.unplaceable.contains(at) {
            return Err(Error::Adapter {
                port: PortName::ContentStore,
                diagnostic: Diagnostic::new(format!("{at} cannot be written")),
            });
        }
        state.placed.insert(at.clone(), *digest);
        state.placements.push((at.clone(), *digest));
        state.modes.insert(at.clone(), mode);
        drop(state);
        Ok(())
    }
}

impl Sweep for FakeCas {
    fn held(&self) -> Result<BTreeSet<Digest>> {
        Ok(self.digests())
    }

    fn remove(&self, digest: &Digest) -> Result<u64> {
        let removed = self.state().blobs.remove(digest);
        Ok(removed.map_or(0, |bytes| u64::try_from(bytes.len()).unwrap_or(u64::MAX)))
    }
}

/// A build log held in memory.
#[derive(Debug, Default)]
pub struct FakeLog {
    /// Every event appended, in order.
    pub events: Vec<OutcomeEvent>,
    /// Whether appending fails.
    pub broken: bool,
}

impl EventLog for FakeLog {
    fn append(&mut self, event: &OutcomeEvent) -> Result<()> {
        if self.broken {
            return Err(Error::Adapter {
                port: PortName::EventLog,
                diagnostic: Diagnostic::new("the log cannot be written"),
            });
        }
        self.events.push(event.clone());
        Ok(())
    }
}

/// A build log held in memory, as a reader gets it back.
#[derive(Debug, Default)]
pub struct FakeHistory {
    /// The lines of the log, oldest first.
    pub lines: Vec<LogLine>,
    /// Whether reading fails.
    pub broken: bool,
}

impl RunHistory for FakeHistory {
    fn lines(&self) -> Result<Vec<LogLine>> {
        if self.broken {
            return Err(Error::Adapter {
                port: PortName::RunHistory,
                diagnostic: Diagnostic::new("the log cannot be read"),
            });
        }
        Ok(self.lines.clone())
    }
}

/// A clock that moves one nanosecond each time it is read: the first read is `1`, the next `2`.
///
/// Actions read it from the threads they run on, so the count is atomic.
#[derive(Debug, Default)]
pub struct FakeClock {
    reads: AtomicU64,
}

impl FakeClock {
    /// How many times the clock was read.
    #[must_use]
    pub fn reads(&self) -> u64 {
        self.reads.load(Ordering::SeqCst)
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Timestamp {
        Timestamp::from_unix_nanos(self.reads.fetch_add(1, Ordering::SeqCst) + 1)
    }
}

/// How a scripted action goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ends {
    /// The command runs and exits with this status.
    Exits(i32),
    /// The command runs and this signal ends it.
    Signaled(i32),
    /// The command runs longer than it is allowed to.
    TimesOut,
    /// No directory can be made to run it in.
    PrepareFails,
    /// The command cannot be started.
    RunFails,
    /// The command exits with zero, and then its outputs cannot be read.
    CollectFails,
    /// The command exits with zero, and the strategy then answers for one output too few.
    CollectShort,
}

impl Default for Ends {
    fn default() -> Self {
        Self::Exits(0)
    }
}

/// What a scripted action leaves at one of its output paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Content {
    /// A file holding these bytes.
    Holds(Vec<u8>),
    /// Nothing.
    Missing,
    /// A file holding these bytes that may be run.
    Runs(Vec<u8>),
    /// A directory holding these things, each at its path below the directory, listed in this
    /// order.
    Tree(Vec<(String, Leaf)>),
    /// A symbolic link.
    Link,
    /// Something that is neither a file, a directory nor a symbolic link.
    Special,
}

/// One thing a scripted action leaves below an output path that is a directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Leaf {
    /// A file holding these bytes.
    Holds(Vec<u8>),
    /// A file holding these bytes that may be run.
    Runs(Vec<u8>),
    /// A directory holding nothing.
    Empty,
    /// A symbolic link.
    Link,
    /// Something that is neither a file, a directory nor a symbolic link.
    Special,
    /// Something whose name is not valid Unicode.
    NotText,
}

/// How the action of one target behaves, where that differs from an unscripted one.
///
/// An unscripted action exits with zero, prints its label and a newline to standard output and
/// nothing to standard error, and leaves at each output path text made of its label, the path
/// and the digest of each of its inputs — so the output changes exactly when an input does.
#[derive(Debug, Clone, Default)]
pub struct Script {
    /// How the action goes.
    pub ends: Ends,
    /// What it writes to standard output; `None` for what an unscripted action writes.
    pub stdout: Option<Vec<u8>>,
    /// What it writes to standard error; `None` for nothing.
    pub stderr: Option<Vec<u8>>,
    /// What it leaves at an output path; a path not named holds what an unscripted action
    /// leaves there.
    pub outputs: BTreeMap<OutputPath, Content>,
}

/// The directory a [`FakeStrategy`] runs an action in: nothing, since nothing is run.
#[derive(Debug)]
pub struct FakeDir;

/// A strategy that runs nothing: each action ends, prints and produces what its script says.
///
/// It keeps no directory, so no output path holds anything before an action runs: what it
/// reports for a path is what the script leaves there on this run, and never what an earlier
/// run of the same target left.
///
/// Actions are run from several threads, so what it records sits behind locks.
#[derive(Debug, Default)]
pub struct FakeStrategy {
    /// The workspace files an action can read, as the digester holds them.
    pub files: BTreeMap<SourcePath, Vec<u8>>,
    /// The targets whose action is scripted.
    pub scripts: BTreeMap<Label, Script>,
    handed: Mutex<Vec<ReadyAction>>,
    ran: Mutex<Vec<Label>>,
}

impl FakeStrategy {
    /// Every action a directory was asked for, in the order asked.
    #[must_use]
    pub fn handed(&self) -> Vec<ReadyAction> {
        self.handed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The target of every action that was run, in the order run.
    #[must_use]
    pub fn ran(&self) -> Vec<Label> {
        self.ran
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// How many times the action of `label` was run.
    #[must_use]
    pub fn runs(&self, label: &Label) -> usize {
        self.ran().iter().filter(|ran| *ran == label).count()
    }

    fn script(&self, label: &Label) -> Script {
        self.scripts.get(label).cloned().unwrap_or_default()
    }

    /// What an unscripted action leaves at `path`.
    fn unscripted(&self, action: &ReadyAction, path: &OutputPath) -> Vec<u8> {
        let inputs: Vec<String> = action
            .inputs
            .iter()
            .map(|input| {
                let bytes = self.files.get(input).map_or(&[][..], Vec::as_slice);
                format!(" {input}={}", Digest::of(bytes))
            })
            .collect();
        format!("{} {path}{}", action.label, inputs.concat()).into_bytes()
    }
}

/// The failure of a [`FakeStrategy`] for the action of `label`.
fn strategy_failure(label: &Label, what: &str) -> Error {
    Error::Adapter {
        port: PortName::ExecStrategy,
        diagnostic: Diagnostic::new(format!("{label}: {what}")),
    }
}

impl ExecStrategy for FakeStrategy {
    type Dir = FakeDir;

    fn prepare(&self, action: &ReadyAction) -> Result<FakeDir> {
        self.handed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(action.clone());
        if self.script(&action.label).ends == Ends::PrepareFails {
            return Err(strategy_failure(&action.label, "no directory to run in"));
        }
        Ok(FakeDir)
    }

    fn run(
        &self,
        _dir: &FakeDir,
        action: &ReadyAction,
        live: Option<LiveSink<'_>>,
    ) -> Result<RawOutcome> {
        self.ran
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(action.label.clone());
        let script = self.script(&action.label);
        let exit = match script.ends {
            Ends::RunFails => {
                return Err(strategy_failure(
                    &action.label,
                    "the command cannot be started",
                ));
            }
            Ends::TimesOut => Exit::TimedOut,
            Ends::Exits(status) => Exit::Exited(status),
            Ends::Signaled(signal) => Exit::Signaled(signal),
            Ends::PrepareFails | Ends::CollectFails | Ends::CollectShort => Exit::Exited(0),
        };
        let stdout = script
            .stdout
            .unwrap_or_else(|| format!("{}\n", action.label).into_bytes());
        let stderr = script.stderr.unwrap_or_default();
        if let Some(live) = live {
            // Written a byte at a time and a line at a time, to show that only the joined
            // bytes are promised.
            let (first, rest) = stdout.split_at(stdout.len().min(1));
            for (channel, bytes) in [
                (Channel::Stdout, first),
                (Channel::Stderr, stderr.as_slice()),
                (Channel::Stdout, rest),
            ] {
                if !bytes.is_empty() {
                    live(channel, bytes);
                }
            }
        }
        Ok(RawOutcome {
            exit,
            stdout,
            stderr,
        })
    }

    fn collect<C: ContentStore>(
        &self,
        _dir: &FakeDir,
        action: &ReadyAction,
        cas: &C,
    ) -> Result<Vec<Found>> {
        let script = self.script(&action.label);
        if script.ends == Ends::CollectFails {
            return Err(strategy_failure(
                &action.label,
                "the outputs cannot be read",
            ));
        }
        let mut entries = Vec::new();
        for path in &action.outputs {
            let content = script
                .outputs
                .get(path)
                .cloned()
                .unwrap_or_else(|| Content::Holds(self.unscripted(action, path)));
            entries.push(match content {
                Content::Holds(bytes) => Found::File(stored(cas, &bytes, FileMode::Plain)?),
                Content::Missing => Found::Nothing,
                Content::Runs(bytes) => Found::File(stored(cas, &bytes, FileMode::Executable)?),
                Content::Tree(leaves) => Found::Tree(
                    leaves
                        .into_iter()
                        .map(|(path, leaf)| {
                            let kind = match leaf {
                                Leaf::Holds(bytes) => {
                                    FoundKind::File(stored(cas, &bytes, FileMode::Plain)?)
                                }
                                Leaf::Runs(bytes) => {
                                    FoundKind::File(stored(cas, &bytes, FileMode::Executable)?)
                                }
                                Leaf::Empty => FoundKind::EmptyDirectory,
                                Leaf::Link => FoundKind::Symlink,
                                Leaf::Special => FoundKind::Special,
                                Leaf::NotText => FoundKind::NotText,
                            };
                            Ok(FoundEntry { path, kind })
                        })
                        .collect::<Result<Vec<FoundEntry>>>()?,
                ),
                Content::Link => Found::Symlink,
                Content::Special => Found::Special,
            });
        }
        if script.ends == Ends::CollectShort {
            entries.pop();
        }
        Ok(entries)
    }
}

/// Which waiting task a [`FakeDispatcher`] takes next.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Order {
    /// The one that has waited longest.
    #[default]
    OldestFirst,
    /// The one handed over last.
    NewestFirst,
}

/// How a [`FakeDispatcher`] loses a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Loss {
    /// The task is never run, and no result is handed to `step`.
    Unrun,
    /// The task is run, and its result is dropped instead of being handed to `step`.
    Unreported,
}

/// How many tasks a [`FakeDispatcher`] has under way before it hands anything back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Pace {
    /// One: what a task said and its result are handed over before the next task is taken.
    #[default]
    OneAtATime,
    /// Every waiting task: all are run, then what each said is handed over in the order they
    /// were taken, then their results, the task taken last first. It is what a build sees when
    /// several actions run at once and the first to begin is the last to end.
    Together,
}

/// A dispatcher that runs every task on the calling thread.
///
/// It breaks the port's contract only where told to: `lose` names tasks by the position at which
/// they are taken, counting from zero.
#[derive(Debug, Default)]
pub struct FakeDispatcher {
    /// Which waiting task is taken next.
    pub order: Order,
    /// How many tasks are under way at once.
    pub pace: Pace,
    /// The tasks to lose, by the position at which they are taken, and how.
    pub lose: BTreeMap<usize, Loss>,
}

impl Dispatcher for FakeDispatcher {
    fn dispatch<T, M, R, W, S>(&mut self, work: W, mut step: S) -> Result<()>
    where
        T: Send,
        M: Send,
        R: Send,
        W: Fn(T, &(dyn Fn(M) + Sync)) -> R + Sync,
        S: FnMut(Turn<M, R>) -> Vec<T>,
    {
        let mut waiting: VecDeque<T> = step(Turn::Begin).into();
        let mut taken = 0_usize;
        loop {
            let mut ran = Vec::new();
            loop {
                let next = match self.order {
                    Order::OldestFirst => waiting.pop_front(),
                    Order::NewestFirst => waiting.pop_back(),
                };
                let Some(task) = next else {
                    break;
                };
                let loss = self.lose.get(&taken).copied();
                taken += 1;
                if loss == Some(Loss::Unrun) {
                    continue;
                }
                let said = Mutex::new(Vec::new());
                let result = work(task, &|message| {
                    said.lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push(message);
                });
                if loss == Some(Loss::Unreported) {
                    continue;
                }
                ran.push((
                    said.into_inner().unwrap_or_else(PoisonError::into_inner),
                    result,
                ));
                if self.pace == Pace::OneAtATime {
                    break;
                }
            }
            if ran.is_empty() {
                return Ok(());
            }
            let mut results = Vec::new();
            for (said, result) in ran {
                for message in said {
                    waiting.extend(step(Turn::Told(message)));
                }
                results.push(result);
            }
            if self.pace == Pace::Together {
                results.reverse();
            }
            for result in results {
                waiting.extend(step(Turn::Done(result)));
            }
        }
    }
}

/// A graph record held in memory: the bytes of every graph handed to it, in order.
#[derive(Debug, Default)]
pub struct FakeGraphRecord {
    /// The canonical bytes of each graph recorded.
    pub recorded: Vec<Vec<u8>>,
    /// Whether recording fails.
    pub broken: bool,
}

impl GraphRecord for FakeGraphRecord {
    fn record(&mut self, graph: &TargetGraph) -> Result<()> {
        if self.broken {
            return Err(Error::Adapter {
                port: PortName::GraphRecord,
                diagnostic: Diagnostic::new("the graph cannot be written"),
            });
        }
        self.recorded.push(json::canonical::to_vec(graph)?);
        Ok(())
    }
}

/// One call a [`FakeReporter`] received.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reported {
    /// `begun`, with the number of targets.
    Begun(usize),
    /// `started`, with the action's label and its description.
    Started(Label, Option<String>),
    /// `printed`.
    Printed(Label, Channel, Vec<u8>),
    /// `finished`, with the outcome's label and class and what was left to show.
    Finished {
        /// The target.
        label: Label,
        /// How it ended.
        class: OutcomeClass,
        /// What it wrote to standard output, as handed over.
        stdout: Vec<u8>,
        /// What it wrote to standard error, as handed over.
        stderr: Vec<u8>,
    },
    /// `summary`.
    Summary(Summary),
}

/// A reporter that shows nothing and keeps every call it received, in order.
#[derive(Debug, Default)]
pub struct FakeReporter {
    /// The calls.
    pub calls: Vec<Reported>,
}

impl Reporter for FakeReporter {
    fn begun(&mut self, targets: usize) {
        self.calls.push(Reported::Begun(targets));
    }

    fn started(&mut self, action: &ReadyAction) {
        self.calls.push(Reported::Started(
            action.label.clone(),
            action
                .description
                .as_ref()
                .map(|description| description.as_str().to_owned()),
        ));
    }

    fn printed(&mut self, label: &Label, channel: Channel, bytes: &[u8]) {
        self.calls
            .push(Reported::Printed(label.clone(), channel, bytes.to_vec()));
    }

    fn finished(&mut self, event: &OutcomeEvent, printed: Captured<'_>) {
        self.calls.push(Reported::Finished {
            label: event.outcome.label.clone(),
            class: event.outcome.class.clone(),
            stdout: printed.stdout.to_vec(),
            stderr: printed.stderr.to_vec(),
        });
    }

    fn summary(&mut self, summary: Summary) {
        self.calls.push(Reported::Summary(summary));
    }
}

/// The bundle choosing the fakes of this file.
#[derive(Debug)]
pub struct FakePorts;

impl Ports for FakePorts {
    type Source = FakeSource;
    type Manifest = FakeManifest;
    type Host = FakeHost;
    type Digester = FakeDigester;
    type StatCache = FakeStatCache;
    type Tools = FakeTools;
    type Cache = FakeCache;
    type Cas = FakeCas;
    type Log = FakeLog;
    type Strategy = FakeStrategy;
    type Dispatcher = FakeDispatcher;
    type Clock = FakeClock;
    type Graph = FakeGraphRecord;
    type Reporter = FakeReporter;
}

/// A workspace that writes to `out` and `.buildl`, under a ceiling allowing `cc` to run, any
/// workspace file to be read, and those two directories to be written.
#[must_use]
pub fn open_workspace() -> Workspace {
    Workspace {
        out: dir("out"),
        state: dir(".buildl"),
        ceiling: Ceiling {
            run: BTreeSet::from([program("cc")]),
            env: BTreeSet::new(),
            read: BTreeSet::from([Directory::root()]),
            write: BTreeSet::from([dir("out"), dir(".buildl")]),
        },
        execution: buildl_core::Execution::default(),
    }
}

/// Every port around `source`, each holding nothing.
///
/// An [`open_workspace`]; a `linux/x86_64` machine with no variable set; no workspace file,
/// nothing recorded, no program and no row; an empty content store and an empty log; a strategy
/// with no script; a dispatcher taking the oldest task first, one at a time; a clock that has
/// not been read; no graph recorded; and a reporter that has received no call.
#[must_use]
pub fn ports(source: FakeSource) -> PortSet<FakePorts> {
    PortSet {
        source,
        manifest: FakeManifest {
            workspace: open_workspace(),
        },
        host: FakeHost {
            platform: Platform::parse("linux/x86_64")
                .unwrap_or_else(|error| unreachable!("fixture platform: {error}")),
            variables: BTreeMap::new(),
            unreportable: BTreeSet::new(),
        },
        digester: FakeDigester::default(),
        stat_cache: FakeStatCache::default(),
        tools: FakeTools::default(),
        cache: FakeCache::default(),
        cas: FakeCas::default(),
        log: FakeLog::default(),
        strategy: FakeStrategy::default(),
        dispatcher: FakeDispatcher::default(),
        clock: FakeClock::default(),
        graph: FakeGraphRecord::default(),
        reporter: FakeReporter::default(),
    }
}

/// A program from known-valid text.
///
/// # Panics
///
/// Panics when `raw` is not a valid program name; a test fixture is wrong then.
#[must_use]
pub fn program(raw: &str) -> Program {
    Program::parse(raw).unwrap_or_else(|error| unreachable!("fixture program: {error}"))
}

/// A directory from known-valid text.
///
/// # Panics
///
/// Panics when `raw` is not a valid directory; a test fixture is wrong then.
#[must_use]
pub fn dir(raw: &str) -> Directory {
    Directory::parse(raw).unwrap_or_else(|error| unreachable!("fixture directory: {error}"))
}

/// A build file making `items` as its declaration calls, then `subdirs` as its `subdir` calls.
///
/// Call order counts declarations first, then subdirs.
#[must_use]
pub fn file(items: Vec<StagedItem>, subdirs: &[&str]) -> FakeFile {
    let declarations: Vec<StagedDeclaration> = items
        .into_iter()
        .zip(0_u32..)
        .map(|(item, index)| StagedDeclaration {
            order: DeclarationOrder::new(index),
            item,
        })
        .collect();
    let first_subdir = u32::try_from(declarations.len()).unwrap_or(u32::MAX);
    let subdirs = subdirs
        .iter()
        .zip(first_subdir..)
        .map(|(path, index)| StagedSubdir {
            path: Written::new(*path),
            order: DeclarationOrder::new(index),
        })
        .collect();
    FakeFile::Staged(StagedFile {
        declarations,
        subdirs,
    })
}

/// A `b.target(name, { run = { "cc" }, deps = deps })` call.
#[must_use]
pub fn target(name: &str, deps: &[&str]) -> StagedItem {
    target_with(name, |target| {
        target.deps = deps.iter().map(|dep| Written::new(*dep)).collect();
    })
}

/// A `b.target(name, { run = { "cc" } })` call, after `change` is applied to it.
#[must_use]
pub fn target_with(name: &str, change: impl FnOnce(&mut StagedTarget)) -> StagedItem {
    let mut target = StagedTarget {
        name: Written::new(name),
        role: TargetRole::Build,
        rule: None,
        run: Some(vec![Written::new("cc")]),
        inputs: Vec::new(),
        deps: Vec::new(),
        outputs: None,
        env: Vec::new(),
        network: NetworkAccess::Sealed,
        freshness: Freshness::Cached,
    };
    change(&mut target);
    StagedItem::Target(target)
}

/// A `b.rule(name, { run = run, desc = description })` call.
#[must_use]
pub fn rule(name: &str, run: &[&str], description: Option<&str>) -> StagedItem {
    StagedItem::Rule(StagedRule {
        name: Written::new(name),
        run: run.iter().map(|argument| Written::new(*argument)).collect(),
        description: description.map(Written::new),
    })
}

/// A `b.alias(name, target)` call.
#[must_use]
pub fn alias(name: &str, target: &str) -> StagedItem {
    StagedItem::Alias(StagedAlias {
        name: Written::new(name),
        target: Written::new(target),
    })
}

/// A `b.option(name, { default = "" })` call.
#[must_use]
pub fn setting(name: &str) -> StagedItem {
    StagedItem::Setting(StagedSetting {
        name: Written::new(name),
        default: Written::new(""),
    })
}

/// A workspace from `(directory, file)` pairs.
#[must_use]
pub fn workspace(files: Vec<(&str, FakeFile)>) -> BTreeMap<Directory, FakeFile> {
    files
        .into_iter()
        .map(|(directory, file)| (dir(directory), file))
        .collect()
}

/// Stores `bytes` in `cas`, and returns the file a strategy reports for them.
fn stored<C: ContentStore>(cas: &C, bytes: &[u8], mode: FileMode) -> Result<StoredFile> {
    Ok(StoredFile {
        digest: cas.put(bytes)?,
        mode,
    })
}

/// A directory of plain files, each a `(path, text)`, listed in the order given.
#[must_use]
pub fn tree(files: &[(&str, &str)]) -> Content {
    Content::Tree(
        files
            .iter()
            .map(|(path, text)| ((*path).to_owned(), Leaf::Holds(text.as_bytes().to_vec())))
            .collect(),
    )
}

/// A label from known-valid text.
///
/// # Panics
///
/// Panics when `raw` is not a valid label; a test fixture is wrong then.
#[must_use]
pub fn label(raw: &str) -> Label {
    Label::parse(raw).unwrap_or_else(|error| unreachable!("fixture label: {error}"))
}

/// Every target's class, by label.
#[must_use]
pub fn classes(outcomes: &[ActionOutcome]) -> BTreeMap<String, OutcomeClass> {
    outcomes
        .iter()
        .map(|outcome| (outcome.label.to_string(), outcome.class.clone()))
        .collect()
}

/// The classes a test expects, by label.
#[must_use]
pub fn expected(pairs: &[(&str, OutcomeClass)]) -> BTreeMap<String, OutcomeClass> {
    pairs
        .iter()
        .map(|(raw, class)| ((*raw).to_owned(), class.clone()))
        .collect()
}

/// A pipeline over one root build file making `items`, with `cc` findable.
#[must_use]
pub fn root_only(items: Vec<StagedItem>) -> Pipeline<FakePorts> {
    let source = FakeSource::new(workspace(vec![("", file(items, &[]))]));
    let mut pipeline = Pipeline::new(ports(source));
    pipeline
        .ports_mut()
        .tools
        .fingerprints
        .insert(program("cc"), Digest::of(b"cc 14.2"));
    pipeline
}

/// The class of a target skipped because the target `raw` names failed.
#[must_use]
pub fn skipped(raw: &str) -> OutcomeClass {
    OutcomeClass::Skipped {
        because: label(raw),
    }
}

/// A script in which the action goes as `ends` says and everything else is unscripted.
#[must_use]
pub fn ending(ends: Ends) -> Script {
    Script {
        ends,
        ..Script::default()
    }
}

/// Scripts the action of the target `raw` names.
pub fn script(pipeline: &mut Pipeline<FakePorts>, raw: &str, script: Script) {
    pipeline
        .ports_mut()
        .strategy
        .scripts
        .insert(label(raw), script);
}

/// The fakes held to the contracts of the ports they stand in for: a flow is only as good as
/// the fake it runs against.
#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::{BTreeMap, BTreeSet};
    use std::path::PathBuf;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU32, Ordering};

    use buildl_core::{
        ActionCache, ActionKey, ActionOutcome, Argument, CacheRow, Captured, Channel, Clock,
        Command, ContentStore, Digest, Directory, Dispatcher, EntryName, Error, EventLog,
        ExecStrategy, Exit, GraphRecord, KeyComponents, Label, NetworkAccess, OutcomeClass,
        OutcomeEvent, OutputName, OutputPath, OwnComponents, Platform, PortName, Provenance,
        ReadyAction, Reporter, SourcePath, Summary, Tally, TargetGraph, TargetRole, Timestamp,
        json, load, resolve,
    };

    use super::{
        Content, Ends, FakeCache, FakeCas, FakeClock, FakeDispatcher, FakeGraphRecord, FakeLog,
        FakeReporter, FakeSource, FakeStrategy, Loss, Order, Pace, Reported, Script, dir, file,
        target, workspace,
    };
    use super::{FakeContent, FakeDigester, Leaf, tree};
    use buildl_core::{
        Digester, FileMode, FileStat, Found, FoundEntry, FoundKind, InputKind, InputStat,
        StoredFile, TreePath, Turn,
    };

    fn label(raw: &str) -> Label {
        Label::parse(raw).unwrap()
    }

    /// A plain file with that digest, as a strategy reports one.
    const fn plain(digest: Digest) -> Found {
        Found::File(StoredFile {
            digest,
            mode: FileMode::Plain,
        })
    }

    fn path(raw: &str) -> SourcePath {
        SourcePath::parse(raw).unwrap()
    }

    /// The action of a target that runs `cc` over `inputs` and declares `outputs`.
    fn action(raw: &str, inputs: &[&str], outputs: &[&str]) -> ReadyAction {
        ReadyAction {
            label: label(raw),
            command: Command::new(vec![Argument::parse("cc").unwrap()]).unwrap(),
            description: None,
            env: BTreeMap::new(),
            inputs: inputs.iter().map(|input| path(input)).collect(),
            deps: Vec::new(),
            outputs: outputs.iter().map(|name| out_path(raw, name)).collect(),
            network: NetworkAccess::Sealed,
        }
    }

    /// A strategy holding one script, for `//:app`.
    fn scripted(script: Script) -> FakeStrategy {
        FakeStrategy {
            scripts: BTreeMap::from([(label("//:app"), script)]),
            ..FakeStrategy::default()
        }
    }

    /// Where the output `name` of the target `raw` lives, under `out`.
    fn out_path(raw: &str, name: &str) -> OutputPath {
        OutputPath::of(&dir("out"), &label(raw), &OutputName::parse(name).unwrap())
    }

    /// The event of a target declared in the root build file that was found cached.
    fn event(raw: &str) -> OutcomeEvent {
        OutcomeEvent {
            at: Timestamp::from_unix_nanos(1),
            provenance: Provenance::new(PathBuf::from("build.lua"), Directory::root()),
            command: Command::new(vec![Argument::parse("cc").unwrap()]).unwrap(),
            key: None,
            started: None,
            finished: None,
            outcome: ActionOutcome {
                label: label(raw),
                role: TargetRole::Build,
                class: OutcomeClass::Cached,
                diagnostic: None,
                outputs: Vec::new(),
                stdout: None,
                stderr: None,
            },
        }
    }

    /// A row for a target that ran `cc` and produced one output, `app`, holding `content`.
    fn row(content: &str) -> CacheRow {
        let key = ActionKey::new(KeyComponents {
            own: OwnComponents {
                command: Command::new(vec![Argument::parse("cc").unwrap()]).unwrap(),
                inputs: Vec::new(),
                outputs: vec![OutputName::parse("app").unwrap()],
                env: BTreeMap::new(),
                tool: None,
                settings: BTreeMap::new(),
                network: NetworkAccess::Sealed,
                platform: Platform::parse("linux/x86_64").unwrap(),
            },
            deps: Vec::new(),
        })
        .unwrap();
        let nothing = Digest::of(b"");
        CacheRow::new(key, vec![Digest::of(content.as_bytes())], nothing, nothing).unwrap()
    }

    #[test]
    fn a_digester_reports_a_file_nothing_and_what_is_not_a_file() {
        let stat = FileStat {
            modified: Timestamp::from_unix_nanos(7),
            size: 4,
        };
        let mut digester = FakeDigester::default();
        digester.files.insert(
            path("main.c"),
            FakeContent {
                bytes: b"main".to_vec(),
                stat,
            },
        );
        digester.others.insert(path("src"), InputKind::Directory);
        digester
            .others
            .insert(path("main.c.link"), InputKind::Symlink);

        assert_eq!(
            digester.stat(&path("main.c")).unwrap(),
            InputStat::File(stat)
        );
        assert_eq!(digester.stat(&path("gone.c")).unwrap(), InputStat::Missing);
        assert_eq!(
            digester.stat(&path("src")).unwrap(),
            InputStat::Other(InputKind::Directory)
        );
        assert_eq!(
            digester.stat(&path("main.c.link")).unwrap(),
            InputStat::Other(InputKind::Symlink)
        );
        assert_eq!(digester.stat_calls(), 4);
    }

    /// The graph of a workspace whose root build file declares a target for each of `names`.
    fn graph_of(names: &[&str]) -> TargetGraph {
        let targets = names.iter().map(|name| target(name, &[])).collect();
        let source = FakeSource::new(workspace(vec![("", file(targets, &[]))]));
        let entry = EntryName::parse("build.lua").unwrap();
        resolve(load(&source, &mut FakeDispatcher::default(), &entry).unwrap()).unwrap()
    }

    #[test]
    fn a_recorded_row_is_the_one_read_back() {
        let mut cache = FakeCache::default();
        assert_eq!(cache.row(&label("//:app")), None);

        cache.record(label("//:app"), row("first")).unwrap();
        cache.record(label("//:app"), row("second")).unwrap();

        assert_eq!(cache.row(&label("//:app")), Some(&row("second")));
        assert_eq!(cache.recorded(), [label("//:app"), label("//:app")]);
    }

    #[test]
    fn a_row_that_cannot_be_recorded_leaves_what_was_there() {
        let mut cache = FakeCache::default();
        cache.record(label("//:app"), row("first")).unwrap();
        cache.failing.insert(label("//:app"));

        let error = cache.record(label("//:app"), row("second")).unwrap_err();

        assert_eq!(error.to_string(), "action cache: //:app cannot be recorded");
        match error {
            Error::Adapter { port, .. } => assert_eq!(port, PortName::ActionCache),
            other => unreachable!("expected Adapter, got {other:?}"),
        }
        assert_eq!(cache.row(&label("//:app")), Some(&row("first")));
        assert_eq!(cache.recorded(), [label("//:app")]);
    }

    #[test]
    fn stored_content_is_held_under_its_digest() {
        let cas = FakeCas::default();

        let digest = cas.put(b"app").unwrap();

        assert_eq!(digest, Digest::of(b"app"));
        assert_eq!(cas.put(b"app").unwrap(), digest, "storing it again");
        assert!(cas.contains(&digest));
        assert!(!cas.contains(&Digest::of(b"other")));
        assert_eq!(cas.blob(&digest), Some(b"app".to_vec()));
        assert_eq!(cas.digests(), BTreeSet::from([digest]));
    }

    #[test]
    fn content_it_is_told_to_refuse_is_not_stored() {
        let cas = FakeCas::default();
        cas.fail_put(b"app".to_vec());

        let error = cas.put(b"app").unwrap_err();

        assert_eq!(
            error.to_string(),
            "content store: the content cannot be stored"
        );
        assert!(cas.digests().is_empty());
        assert!(cas.put(b"other").is_ok());
    }

    #[test]
    fn placed_content_becomes_the_file_at_its_path() {
        let cas = FakeCas::default();
        let first = cas.put(b"first").unwrap();
        let second = cas.put(b"second").unwrap();
        let app = out_path("//:app", "app");

        cas.place(&first, FileMode::Executable, &app).unwrap();
        cas.place(&second, FileMode::Plain, &app).unwrap();

        assert_eq!(cas.placed(), BTreeMap::from([(app.clone(), second)]));
        assert_eq!(
            cas.modes(),
            BTreeMap::from([(app.clone(), FileMode::Plain)]),
            "the mode it was last placed with"
        );
        assert_eq!(cas.placements(), [(app.clone(), first), (app, second)]);
    }

    #[test]
    fn held_content_is_read_back_and_content_not_held_is_not() {
        let cas = FakeCas::default();
        let digest = cas.put(b"app").unwrap();

        assert_eq!(cas.get(&digest).unwrap(), Some(b"app".to_vec()));
        assert_eq!(cas.get(&Digest::of(b"other")).unwrap(), None);

        cas.forget(&digest);
        assert_eq!(cas.get(&digest).unwrap(), None);
    }

    #[test]
    fn clearing_a_path_removes_the_file_there_or_everything_below_it() {
        let cas = FakeCas::default();
        let digest = cas.put(b"app").unwrap();
        let site = out_path("//:site", "site");
        let page = site.entry(&TreePath::parse("index.html").unwrap());
        let script = site.entry(&TreePath::parse("assets/app.js").unwrap());
        let sibling = out_path("//:site", "site2");
        for path in [&page, &script, &sibling] {
            cas.place(&digest, FileMode::Plain, path).unwrap();
        }

        cas.clear(&site).unwrap();

        assert_eq!(
            cas.placed(),
            BTreeMap::from([(sibling.clone(), digest)]),
            "a path that only starts like the cleared one is left alone"
        );
        assert_eq!(
            cas.modes(),
            BTreeMap::from([(sibling.clone(), FileMode::Plain)])
        );

        cas.clear(&sibling).unwrap();
        cas.clear(&sibling).unwrap();
        assert!(cas.placed().is_empty(), "clearing nothing is not an error");
    }

    #[test]
    fn content_that_is_not_held_cannot_be_placed() {
        let cas = FakeCas::default();
        let app = out_path("//:app", "app");
        let digest = cas.put(b"app").unwrap();
        cas.forget(&digest);

        let error = cas.place(&digest, FileMode::Plain, &app).unwrap_err();

        assert!(!cas.contains(&digest));
        assert_eq!(
            error.to_string(),
            format!("content store: {digest} is not held")
        );
        assert!(cas.placed().is_empty());
    }

    #[test]
    fn a_path_it_is_told_to_fail_on_is_not_written() {
        let cas = FakeCas::default();
        let app = out_path("//:app", "app");
        let map = out_path("//:app", "app.map");
        let digest = cas.put(b"app").unwrap();
        cas.fail_place(app.clone());

        let error = cas.place(&digest, FileMode::Plain, &app).unwrap_err();
        cas.place(&digest, FileMode::Plain, &map).unwrap();

        assert_eq!(
            error.to_string(),
            "content store: out/app cannot be written"
        );
        match error {
            Error::Adapter { port, .. } => assert_eq!(port, PortName::ContentStore),
            other => unreachable!("expected Adapter, got {other:?}"),
        }
        assert_eq!(cas.placements(), [(map, digest)]);
    }

    #[test]
    fn a_log_keeps_every_event_until_it_is_broken() {
        let mut log = FakeLog::default();
        log.append(&event("//:a")).unwrap();
        log.append(&event("//:b")).unwrap();
        assert_eq!(log.events, [event("//:a"), event("//:b")]);

        log.broken = true;
        let error = log.append(&event("//:c")).unwrap_err();

        assert_eq!(error.to_string(), "event log: the log cannot be written");
        assert_eq!(log.events.len(), 2);
    }

    #[test]
    fn a_graph_record_keeps_the_bytes_of_every_graph_until_it_is_broken() {
        let one = graph_of(&["app"]);
        let two = graph_of(&["app", "lib"]);
        let mut record = FakeGraphRecord::default();

        record.record(&one).unwrap();
        record.record(&two).unwrap();

        assert_eq!(
            record.recorded,
            [
                json::canonical::to_vec(&one).unwrap(),
                json::canonical::to_vec(&two).unwrap()
            ],
            "the canonical bytes of each graph, in the order recorded"
        );

        record.broken = true;
        let error = record.record(&one).unwrap_err();

        assert_eq!(
            error.to_string(),
            "graph record: the graph cannot be written"
        );
        match error {
            Error::Adapter { port, .. } => assert_eq!(port, PortName::GraphRecord),
            other => unreachable!("expected Adapter, got {other:?}"),
        }
        assert_eq!(record.recorded.len(), 2, "a broken record keeps nothing");
    }

    #[test]
    fn a_reporter_keeps_every_call_it_receives_in_order() {
        let counts = Summary {
            all: Tally {
                failed: 0,
                skipped: 0,
                built: 1,
                cached: 1,
            },
            tests: Tally::default(),
        };
        let mut reporter = FakeReporter::default();

        reporter.begun(2);
        reporter.started(&action("//:app", &[], &["app"]));
        reporter.printed(&label("//:app"), Channel::Stderr, b"warn\n");
        reporter.finished(
            &event("//:lib"),
            Captured {
                stdout: b"out",
                stderr: b"err",
            },
        );
        reporter.summary(counts);

        assert_eq!(
            reporter.calls,
            [
                Reported::Begun(2),
                Reported::Started(label("//:app"), None),
                Reported::Printed(label("//:app"), Channel::Stderr, b"warn\n".to_vec()),
                Reported::Finished {
                    label: label("//:lib"),
                    class: OutcomeClass::Cached,
                    stdout: b"out".to_vec(),
                    stderr: b"err".to_vec(),
                },
                Reported::Summary(counts),
            ]
        );
    }

    #[test]
    fn every_read_of_the_clock_is_one_tick_later() {
        let clock = FakeClock::default();
        assert_eq!(clock.reads(), 0);

        assert_eq!(clock.now(), Timestamp::from_unix_nanos(1));
        assert_eq!(clock.now(), Timestamp::from_unix_nanos(2));

        assert_eq!(clock.reads(), 2);
    }

    #[test]
    fn an_unscripted_action_succeeds_prints_its_label_and_leaves_every_output() {
        let strategy = FakeStrategy::default();
        let cas = FakeCas::default();
        let app = action("//:app", &[], &["app", "app.map"]);

        let dir = strategy.prepare(&app).unwrap();
        let ran = strategy.run(&dir, &app, None).unwrap();
        let collected = strategy.collect(&dir, &app, &cas).unwrap();

        assert_eq!(ran.exit, Exit::Exited(0));
        assert_eq!(ran.stdout, b"//:app\n");
        assert_eq!(ran.stderr, b"");
        let binary = Digest::of(b"//:app out/app");
        let map = Digest::of(b"//:app out/app.map");
        assert_eq!(collected, [plain(binary), plain(map)]);
        assert_eq!(cas.digests(), BTreeSet::from([binary, map]));
        assert_eq!(strategy.handed(), [app]);
        assert_eq!(strategy.ran(), [label("//:app")]);
        assert_eq!(strategy.runs(&label("//:app")), 1);
        assert_eq!(strategy.runs(&label("//:other")), 0);
    }

    #[test]
    fn an_unscripted_output_changes_exactly_when_an_input_does() {
        let cas = FakeCas::default();
        let app = action("//:app", &["src/a.c", "src/b.c"], &["app"]);
        let produce = |a: &str| {
            let strategy = FakeStrategy {
                files: BTreeMap::from([(path("src/a.c"), a.as_bytes().to_vec())]),
                ..FakeStrategy::default()
            };
            let dir = strategy.prepare(&app).unwrap();
            strategy.collect(&dir, &app, &cas).unwrap()
        };

        let first = produce("one");

        assert_eq!(produce("one"), first);
        assert_ne!(produce("two"), first);
        let text = format!(
            "//:app out/app src/a.c={} src/b.c={}",
            Digest::of(b"one"),
            Digest::of(b"")
        );
        assert_eq!(first, [plain(Digest::of(text.as_bytes()))]);
    }

    #[test]
    fn a_scripted_action_ends_and_prints_as_its_script_says() {
        let app = action("//:app", &[], &["app"]);
        let failing = scripted(Script {
            ends: Ends::Exits(3),
            stdout: Some(b"compiling".to_vec()),
            stderr: Some(b"no such file".to_vec()),
            ..Script::default()
        });
        let slow = scripted(Script {
            ends: Ends::TimesOut,
            ..Script::default()
        });
        let killed = scripted(Script {
            ends: Ends::Signaled(11),
            ..Script::default()
        });

        let failed = failing
            .run(&failing.prepare(&app).unwrap(), &app, None)
            .unwrap();
        let timed_out = slow.run(&slow.prepare(&app).unwrap(), &app, None).unwrap();
        let signaled = killed
            .run(&killed.prepare(&app).unwrap(), &app, None)
            .unwrap();

        assert_eq!(failed.exit, Exit::Exited(3));
        assert_eq!(failed.stdout, b"compiling");
        assert_eq!(failed.stderr, b"no such file");
        assert_eq!(timed_out.exit, Exit::TimedOut);
        assert_eq!(timed_out.stdout, b"//:app\n");
        assert_eq!(signaled.exit, Exit::Signaled(11));
        assert_eq!(signaled.stdout, b"//:app\n");
    }

    #[test]
    fn what_a_run_hands_over_live_joins_to_exactly_what_it_returns() {
        let app = action("//:app", &[], &["app"]);
        let strategy = scripted(Script {
            stdout: Some(b"one\ntwo\n".to_vec()),
            stderr: Some(b"warn\n".to_vec()),
            ..Script::default()
        });
        let handed = Mutex::new(Vec::new());
        let live = |channel: Channel, bytes: &[u8]| {
            handed.lock().unwrap().push((channel, bytes.to_vec()));
        };

        let dir = strategy.prepare(&app).unwrap();
        let shown = strategy.run(&dir, &app, Some(&live)).unwrap();
        let quiet = strategy.run(&dir, &app, None).unwrap();

        let handed = handed.into_inner().unwrap();
        assert_eq!(
            handed,
            [
                (Channel::Stdout, b"o".to_vec()),
                (Channel::Stderr, b"warn\n".to_vec()),
                (Channel::Stdout, b"ne\ntwo\n".to_vec()),
            ],
            "a run of bytes at a time, each with the output it was written to"
        );
        let joined = |wanted: Channel| -> Vec<u8> {
            handed
                .iter()
                .filter(|(channel, _)| *channel == wanted)
                .flat_map(|(_, bytes)| bytes.clone())
                .collect()
        };
        assert_eq!(joined(Channel::Stdout), shown.stdout);
        assert_eq!(joined(Channel::Stderr), shown.stderr);
        assert_eq!(
            shown, quiet,
            "what is returned is the same with or without it"
        );
    }

    #[test]
    fn a_strategy_fails_at_the_step_its_script_names() {
        let cas = FakeCas::default();
        let app = action("//:app", &[], &["app"]);
        let with = |ends: Ends| {
            scripted(Script {
                ends,
                ..Script::default()
            })
        };

        let no_dir = with(Ends::PrepareFails);
        let error = no_dir.prepare(&app).unwrap_err();
        assert_eq!(
            error.to_string(),
            "exec strategy: //:app: no directory to run in"
        );
        match error {
            Error::Adapter { port, .. } => assert_eq!(port, PortName::ExecStrategy),
            other => unreachable!("expected Adapter, got {other:?}"),
        }
        assert_eq!(no_dir.handed(), std::slice::from_ref(&app));
        assert_eq!(no_dir.runs(&label("//:app")), 0);

        let no_start = with(Ends::RunFails);
        let dir = no_start.prepare(&app).unwrap();
        assert_eq!(
            no_start.run(&dir, &app, None).unwrap_err().to_string(),
            "exec strategy: //:app: the command cannot be started"
        );
        assert_eq!(no_start.runs(&label("//:app")), 1);

        let unreadable = with(Ends::CollectFails);
        let dir = unreadable.prepare(&app).unwrap();
        assert_eq!(
            unreadable.run(&dir, &app, None).unwrap().exit,
            Exit::Exited(0)
        );
        assert_eq!(
            unreadable
                .collect(&dir, &app, &cas)
                .unwrap_err()
                .to_string(),
            "exec strategy: //:app: the outputs cannot be read"
        );
        assert!(cas.digests().is_empty());
    }

    #[test]
    fn scripted_outputs_are_stored_a_missing_one_is_not_and_a_short_reply_drops_the_last() {
        let cas = FakeCas::default();
        let app = action("//:app", &[], &["app", "app.map"]);
        let outputs = BTreeMap::from([
            (
                out_path("//:app", "app"),
                Content::Holds(b"binary".to_vec()),
            ),
            (out_path("//:app", "app.map"), Content::Missing),
        ]);
        let partial = scripted(Script {
            outputs: outputs.clone(),
            ..Script::default()
        });
        let short = scripted(Script {
            ends: Ends::CollectShort,
            outputs,
            ..Script::default()
        });

        let dir = partial.prepare(&app).unwrap();
        assert_eq!(
            partial.collect(&dir, &app, &cas).unwrap(),
            [plain(Digest::of(b"binary")), Found::Nothing]
        );
        assert_eq!(cas.digests(), BTreeSet::from([Digest::of(b"binary")]));

        let dir = short.prepare(&app).unwrap();
        assert_eq!(short.run(&dir, &app, None).unwrap().exit, Exit::Exited(0));
        assert_eq!(
            short.collect(&dir, &app, &cas).unwrap(),
            [plain(Digest::of(b"binary"))]
        );
    }

    #[test]
    fn a_scripted_output_is_reported_as_what_it_is_and_only_its_files_are_stored() {
        let cas = FakeCas::default();
        let app = action("//:app", &[], &["app", "dist", "link", "pipe"]);
        let leaves = vec![
            ("index.html".to_owned(), Leaf::Holds(b"page".to_vec())),
            ("bin/serve".to_owned(), Leaf::Runs(b"server".to_vec())),
            ("cache".to_owned(), Leaf::Empty),
            ("latest".to_owned(), Leaf::Link),
            ("socket".to_owned(), Leaf::Special),
            ("caf\u{fffd}".to_owned(), Leaf::NotText),
        ];
        let strategy = scripted(Script {
            outputs: BTreeMap::from([
                (out_path("//:app", "app"), Content::Runs(b"binary".to_vec())),
                (out_path("//:app", "dist"), Content::Tree(leaves)),
                (out_path("//:app", "link"), Content::Link),
                (out_path("//:app", "pipe"), Content::Special),
            ]),
            ..Script::default()
        });

        let dir = strategy.prepare(&app).unwrap();
        let collected = strategy.collect(&dir, &app, &cas).unwrap();

        let file = |bytes: &[u8], mode| StoredFile {
            digest: Digest::of(bytes),
            mode,
        };
        let entry = |path: &str, kind| FoundEntry {
            path: path.to_owned(),
            kind,
        };
        assert_eq!(
            collected,
            [
                Found::File(file(b"binary", FileMode::Executable)),
                Found::Tree(vec![
                    entry(
                        "index.html",
                        FoundKind::File(file(b"page", FileMode::Plain))
                    ),
                    entry(
                        "bin/serve",
                        FoundKind::File(file(b"server", FileMode::Executable))
                    ),
                    entry("cache", FoundKind::EmptyDirectory),
                    entry("latest", FoundKind::Symlink),
                    entry("socket", FoundKind::Special),
                    entry("caf\u{fffd}", FoundKind::NotText),
                ]),
                Found::Symlink,
                Found::Special,
            ],
            "a directory is listed in the order it was scripted in"
        );
        assert_eq!(
            cas.digests(),
            BTreeSet::from([
                Digest::of(b"binary"),
                Digest::of(b"page"),
                Digest::of(b"server")
            ])
        );
        assert_eq!(
            tree(&[("a", "one"), ("b/c", "two")]),
            Content::Tree(vec![
                ("a".to_owned(), Leaf::Holds(b"one".to_vec())),
                ("b/c".to_owned(), Leaf::Holds(b"two".to_vec())),
            ])
        );
    }

    #[test]
    fn an_output_the_store_refuses_fails_the_collection_as_the_stores_error() {
        let cas = FakeCas::default();
        cas.fail_put(b"//:app out/app".to_vec());
        let strategy = FakeStrategy::default();
        let app = action("//:app", &[], &["app"]);

        let dir = strategy.prepare(&app).unwrap();
        let error = strategy.collect(&dir, &app, &cas).unwrap_err();

        match error {
            Error::Adapter { port, .. } => assert_eq!(port, PortName::ContentStore),
            other => unreachable!("expected Adapter, got {other:?}"),
        }
    }

    /// Dispatches three tasks, the first of which yields a fourth, and returns the results in
    /// the order `step` received them. Both closures borrow locals, as a build's closures do.
    fn dispatched(order: Order) -> Vec<u32> {
        let worked = AtomicU32::new(0);
        let mut results = Vec::new();
        let mut said = Vec::new();
        let mut dispatcher = FakeDispatcher {
            order,
            ..FakeDispatcher::default()
        };

        dispatcher
            .dispatch(
                |task: u32, tell: &(dyn Fn(u32) + Sync)| {
                    worked.fetch_add(1, Ordering::SeqCst);
                    tell(task);
                    task * 10
                },
                |turn| match turn {
                    Turn::Begin => vec![1, 2, 3],
                    Turn::Told(task) => {
                        said.push(task);
                        Vec::new()
                    }
                    Turn::Done(result) => {
                        results.push(result);
                        assert_eq!(
                            said.last().map(|task| task * 10),
                            Some(result),
                            "what a task said comes before its result"
                        );
                        if result == 10 { vec![4] } else { Vec::new() }
                    }
                },
            )
            .unwrap();

        assert_eq!(worked.load(Ordering::SeqCst), 4, "every task is run");
        results
    }

    #[test]
    fn a_lost_task_is_either_never_run_or_run_without_its_result_being_handed_over() {
        for (loss, worked) in [(Loss::Unrun, 2), (Loss::Unreported, 3)] {
            let ran = AtomicU32::new(0);
            let mut results = Vec::new();
            let mut dispatcher = FakeDispatcher {
                lose: BTreeMap::from([(1, loss)]),
                ..FakeDispatcher::default()
            };

            dispatcher
                .dispatch(
                    |task: u32, _tell: &(dyn Fn(()) + Sync)| {
                        ran.fetch_add(1, Ordering::SeqCst);
                        task * 10
                    },
                    |turn| match turn {
                        Turn::Begin => vec![1, 2, 3],
                        Turn::Told(()) => Vec::new(),
                        Turn::Done(result) => {
                            results.push(result);
                            Vec::new()
                        }
                    },
                )
                .unwrap();

            assert_eq!(ran.load(Ordering::SeqCst), worked, "{loss:?}");
            assert_eq!(results, [10, 30], "{loss:?}: the second task has no result");
        }
    }

    #[test]
    fn tasks_run_one_at_a_time_oldest_first_unless_told_otherwise() {
        assert_eq!(FakeDispatcher::default().order, Order::OldestFirst);
        assert_eq!(dispatched(Order::OldestFirst), [10, 20, 30, 40]);
        assert_eq!(dispatched(Order::NewestFirst), [30, 20, 10, 40]);
    }

    #[test]
    fn tasks_run_together_say_everything_first_and_end_last_taken_first() {
        let mut turns = Vec::new();
        let mut dispatcher = FakeDispatcher {
            pace: Pace::Together,
            ..FakeDispatcher::default()
        };

        dispatcher
            .dispatch(
                |task: u32, tell: &(dyn Fn(String) + Sync)| {
                    tell(format!("{task} began"));
                    tell(format!("{task} printed"));
                    task * 10
                },
                |turn| match turn {
                    Turn::Begin => vec![1, 2, 3],
                    Turn::Told(said) => {
                        turns.push(said);
                        Vec::new()
                    }
                    Turn::Done(result) => {
                        turns.push(format!("{result} done"));
                        if result == 10 { vec![4] } else { Vec::new() }
                    }
                },
            )
            .unwrap();

        assert_eq!(FakeDispatcher::default().pace, Pace::OneAtATime);
        assert_eq!(
            turns,
            [
                "1 began",
                "1 printed",
                "2 began",
                "2 printed",
                "3 began",
                "3 printed",
                "30 done",
                "20 done",
                "10 done",
                "4 began",
                "4 printed",
                "40 done",
            ]
        );
    }
}
