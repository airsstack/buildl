//! The bundle that names one implementation of every port a pipeline uses.
//!
//! Its own file because it is a different kind of trait from the ports themselves: it has no
//! methods, only associated types, and it exists so that a pipeline takes one type parameter
//! instead of one per port.
//!
//! Responsibilities: [`Ports`].
//!
//! Non-responsibilities: choosing the implementations. A composition root does that, by
//! implementing this trait.
//!
//! This file holds only a trait definition, so it carries no logic to unit-test.

use crate::ports::{
    ActionCache, Approver, Clock, ContentStore, DeclarationSource, Digester, Dispatcher, EventLog,
    ExecStrategy, GraphRecord, Host, Manifest, Reporter, RunHistory, StatCache, ToolResolver,
};

/// One implementation of each port, chosen together.
///
/// # Examples
///
/// A bundle for a workspace with no build files, on a machine with nothing in it:
///
/// ```
/// use std::collections::BTreeSet;
/// use std::sync::{Mutex, PoisonError};
///
/// use buildl_core::{
///     ActionCache, Approval, Approver, Attribution, BuildFile, CacheRow, Captured, Ceiling,
///     Channel, Clock, ContentStore, DeclarationSource, Diagnostic, Digest, Digester, Directory,
///     Discovered, Dispatcher, EnvName, EnvValue, Error, Evaluated, EventLog, ExecStrategy,
///     FileMode, Found, GraphRecord, Host, InputStat, Label, LiveSink, LogLine, Manifest,
///     OutcomeEvent, OutputPath, Platform, PortName, Ports, Program, RawOutcome, ReadyAction,
///     Reporter, Result, RunHistory, SourcePath, StatCache, StatEntry, Summary, TargetGraph, Timestamp, ToolResolver, Turn, WantedSet,
///     Workspace,
/// };
///
/// /// Every port at once, each answering that there is nothing.
/// struct Nothing;
///
/// /// The failure of a port that has nothing to work with.
/// fn nothing(port: PortName) -> Error {
///     Error::Adapter {
///         port,
///         diagnostic: Diagnostic::new("there is nothing here"),
///     }
/// }
///
/// impl DeclarationSource for Nothing {
///     fn evaluate(&self, _file: &BuildFile, _discovered: &Discovered) -> Result<Evaluated> {
///         Ok(Evaluated::Absent)
///     }
/// }
///
/// impl Manifest for Nothing {
///     fn workspace(&self) -> Result<Workspace> {
///         Ok(Workspace {
///             out: Directory::parse("out")?,
///             state: Directory::parse(".buildl")?,
///             ceiling: Ceiling {
///                 run: BTreeSet::new(),
///                 env: BTreeSet::new(),
///                 read: BTreeSet::new(),
///                 write: BTreeSet::new(),
///             },
///             execution: buildl_core::Execution::default(),
///             approver: buildl_core::ApproverMode::Manifest,
///             plugins: std::collections::BTreeMap::new(),
///         })
///     }
/// }
///
/// impl Host for Nothing {
///     fn platform(&self) -> Platform {
///         Platform::parse("linux/x86_64").unwrap_or_else(|_| unreachable!("a valid platform"))
///     }
///
///     fn env(&self, _name: &EnvName) -> Result<Option<EnvValue>> {
///         Ok(None)
///     }
/// }
///
/// impl Digester for Nothing {
///     fn stat(&self, _path: &SourcePath) -> Result<InputStat> {
///         Ok(InputStat::Missing)
///     }
///
///     fn digest(&self, _paths: &[SourcePath]) -> Result<Vec<Digest>> {
///         Ok(Vec::new())
///     }
/// }
///
/// impl StatCache for Nothing {
///     fn entry(&self, _path: &SourcePath) -> Option<StatEntry> {
///         None
///     }
///
///     fn record(&mut self, _path: SourcePath, _entry: StatEntry) {}
///
///     fn commit(&mut self) -> Result<()> {
///         Ok(())
///     }
/// }
///
/// impl ToolResolver for Nothing {
///     fn fingerprint(&self, _program: &Program) -> Result<Option<Digest>> {
///         Ok(None)
///     }
/// }
///
/// impl ActionCache for Nothing {
///     fn row(&self, _label: &Label) -> Option<&CacheRow> {
///         None
///     }
///
///     fn labels(&self) -> Vec<Label> {
///         Vec::new()
///     }
///
///     fn record(&mut self, _label: Label, _row: CacheRow) -> Result<()> {
///         Err(nothing(PortName::ActionCache))
///     }
///
///     fn forget(&mut self, _label: &Label) -> Result<()> {
///         Ok(())
///     }
/// }
///
/// impl ContentStore for Nothing {
///     fn put(&self, _bytes: &[u8]) -> Result<Digest> {
///         Err(nothing(PortName::ContentStore))
///     }
///
///     fn contains(&self, _digest: &Digest) -> bool {
///         false
///     }
///
///     fn get(&self, _digest: &Digest) -> Result<Option<Vec<u8>>> {
///         Ok(None)
///     }
///
///     fn clear(&self, _at: &OutputPath) -> Result<()> {
///         Err(nothing(PortName::ContentStore))
///     }
///
///     fn place(&self, _digest: &Digest, _mode: FileMode, _at: &OutputPath) -> Result<()> {
///         Err(nothing(PortName::ContentStore))
///     }
/// }
///
/// impl EventLog for Nothing {
///     fn append(&mut self, _event: &OutcomeEvent) -> Result<()> {
///         Err(nothing(PortName::EventLog))
///     }
/// }
///
/// impl RunHistory for Nothing {
///     fn lines(&self) -> Result<Vec<LogLine>> {
///         Err(nothing(PortName::RunHistory))
///     }
/// }
///
/// impl ExecStrategy for Nothing {
///     type Dir = ();
///
///     fn prepare(&self, _action: &ReadyAction) -> Result<()> {
///         Err(nothing(PortName::ExecStrategy))
///     }
///
///     fn run(
///         &self,
///         _dir: &(),
///         _action: &ReadyAction,
///         _live: Option<LiveSink<'_>>,
///     ) -> Result<RawOutcome> {
///         Err(nothing(PortName::ExecStrategy))
///     }
///
///     fn collect<C: ContentStore>(
///         &self,
///         _dir: &(),
///         _action: &ReadyAction,
///         _cas: &C,
///     ) -> Result<Vec<Found>> {
///         Err(nothing(PortName::ExecStrategy))
///     }
/// }
///
/// /// Runs every task on the calling thread, one at a time, and hands over what a task said
/// /// once the task is done.
/// impl Dispatcher for Nothing {
///     fn dispatch<T, M, R, W, S>(&mut self, work: W, mut step: S) -> Result<()>
///     where
///         T: Send,
///         M: Send,
///         R: Send,
///         W: Fn(T, &(dyn Fn(M) + Sync)) -> R + Sync,
///         S: FnMut(Turn<M, R>) -> Vec<T>,
///     {
///         let mut waiting = step(Turn::Begin);
///         while let Some(task) = waiting.pop() {
///             let said = Mutex::new(Vec::new());
///             let result = work(task, &|message| {
///                 said.lock().unwrap_or_else(PoisonError::into_inner).push(message);
///             });
///             for message in said.into_inner().unwrap_or_else(PoisonError::into_inner) {
///                 waiting.extend(step(Turn::Told(message)));
///             }
///             waiting.extend(step(Turn::Done(result)));
///         }
///         Ok(())
///     }
/// }
///
/// impl GraphRecord for Nothing {
///     fn record(&mut self, _graph: &TargetGraph) -> Result<()> {
///         Err(nothing(PortName::GraphRecord))
///     }
/// }
///
/// impl Reporter for Nothing {
///     fn begun(&mut self, _targets: usize) {}
///
///     fn started(&mut self, _action: &ReadyAction) {}
///
///     fn printed(&mut self, _label: &Label, _channel: Channel, _bytes: &[u8]) {}
///
///     fn finished(&mut self, _event: &OutcomeEvent, _printed: Captured<'_>) {}
///
///     fn summary(&mut self, _summary: Summary) {}
/// }
///
/// impl Approver for Nothing {
///     fn approve(&mut self, _wanted: &WantedSet, _attribution: &Attribution) -> Result<Approval> {
///         Ok(Approval::Granted)
///     }
/// }
///
/// impl Clock for Nothing {
///     fn now(&self) -> Timestamp {
///         Timestamp::UNIX_EPOCH
///     }
/// }
///
/// /// A bundle choosing it for every port.
/// struct NothingPorts;
///
/// impl Ports for NothingPorts {
///     type Source = Nothing;
///     type Manifest = Nothing;
///     type Host = Nothing;
///     type Digester = Nothing;
///     type StatCache = Nothing;
///     type Tools = Nothing;
///     type Cache = Nothing;
///     type Cas = Nothing;
///     type Log = Nothing;
///     type Strategy = Nothing;
///     type Dispatcher = Nothing;
///     type Clock = Nothing;
///     type Graph = Nothing;
///     type Reporter = Nothing;
///     type Approver = Nothing;
/// }
/// ```
pub trait Ports {
    /// How build files are evaluated.
    type Source: DeclarationSource;
    /// How the workspace's manifest is read.
    type Manifest: Manifest;
    /// How the machine actions run on is asked about itself.
    type Host: Host;
    /// How a file's metadata and content digest are read.
    type Digester: Digester;
    /// Where the digests of files already hashed are kept.
    type StatCache: StatCache;
    /// How the programs commands name are found.
    type Tools: ToolResolver;
    /// Where each target's row is read from and written to.
    type Cache: ActionCache;
    /// Where what actions produce is stored, by content.
    type Cas: ContentStore;
    /// Where the record of how each target ended is written, and read back from.
    type Log: EventLog + RunHistory;
    /// How one action is run.
    type Strategy: ExecStrategy;
    /// How several actions are run at once.
    type Dispatcher: Dispatcher;
    /// How the wall clock is read. `Sync` because an action's start and end are read from the
    /// thread that runs it.
    type Clock: Clock + Sync;
    /// Where the graph a build resolved is kept.
    type Graph: GraphRecord;
    /// How a build shows what it is doing. Called only from the thread that called the build.
    type Reporter: Reporter;
    /// How a build is asked whether it may do what it wants. Called only from the thread that
    /// called the build.
    type Approver: Approver;
}
