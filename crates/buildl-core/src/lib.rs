//! Domain data, ports, and pure pipeline logic for the buildl build system.
//!
//! This crate is the centre buildl's other crates depend on. It depends on no other buildl
//! crate, on no Lua runtime, and on no filesystem, process, thread, network, environment or
//! clock API, so every pipeline flow can be exercised against in-memory implementations of its
//! ports.
//!
//! # Responsibilities
//!
//! - The domain values the pipeline phases hand to one another.
//! - The ports: traits through which the pipeline reaches build-file evaluation, storage,
//!   hashing, and execution.
//! - The pure logic of each phase: Load → Resolve → Plan → Execute → Record.
//!
//! # Non-responsibilities
//!
//! - Implementing any port. Concrete adapters live in the `buildl-lua` and `buildl` crates.
//! - Reading or writing anything. A value arrives from a caller or through a port.
//!
//! # Where things live
//!
//! | Module | Holds |
//! |---|---|
//! | [`error`] | the one error enum and the crate-wide `Result` alias |
//! | [`types`] | the validated domain values — names, identities, instants, declarations and the plugins, if any, whose code made each, the target graph, action keys, cache rows, the ceiling, the plugins a workspace names and what it lets each have, the plan, ready actions, outcomes, log events, what one run was asked for — and the as-written values a build file stages |
//! | [`json`] | the one canonical serializer every byte of output goes through, and the one reader that takes such bytes back |
//! | [`ports`] | every trait in the crate: the ports through which everything outside it is reached, and the bundle that names one implementation of each, with the set that holds one instance of each |
//! | [`load`](mod@load) | Load, the first phase: build files, and the last result of each discovery target, to validated, sorted declarations |
//! | [`resolve`](mod@resolve) | Resolve, the second phase: declarations to the target graph, that graph cut down to the targets one run was asked for, and the targets a query reaches |
//! | [`plan`](mod@plan) | Plan, the third phase: the target graph to a verdict per target, with what the build wants |
//! | [`execute`](mod@execute) | Execute and Record, the last phase: a plan to one outcome per target, each result recorded as the last step of its action and reported once it is logged |
//! | `placement` | where every output lives and what a command's placeholders stand for once they are placed; used by Plan, Execute and verify, and not a phase |
//! | [`grants`] | the build's authority: which programs a build made, everything a build wants, the wants a plugin's targets may not have, which plugins are behind each want, and the approver's answer; not a phase |
//! | [`pipeline`] | the phases run in sequence over one bundle of ports, one method per command, and the commands that need only some of the ports as functions over those ports alone: the three that reach only build files and the state that holds discovery results, `doctor`, and `log` |
//! | [`history`] | the read side of the build log: the newest run of a target that left output |
//! | [`verify`](mod@verify) | verify, which serves one command and is not a phase: cached targets put back in place, a sample of them re-run, and each re-run compared with what was cached |
//! | [`watch`](mod@watch) | what a watching run decides from the changes it sees: what a batch of changed paths starts, and the loop that builds, waits and reacts; serves one command and is not a phase |
//! | `discovery` | the last result of each discovery target, read back from the build log and the content store for a second load of the build files; several pipeline functions use it, and it is not a phase |
//! | `suggest` | the nearest declared name to one that names nothing, for any module that refuses a name |
//!
//! Each phase has its own module, named for the phase. Five rules keep that arrangement
//! navigable:
//!
//! 1. No phase module names another. Phases communicate through values, and [`pipeline`] owns
//!    the sequence, so a phase that imported the next would be bypassing it. [`history`],
//!    [`verify`](mod@verify), [`watch`](mod@watch) and `discovery` are not phases: each serves a command, or several,
//!    and may name a phase's internals. `placement` and [`grants`] are neither: several phases
//!    name them, and they name no phase.
//! 2. [`types`] holds no decisions between *different* concepts. A type there may decide things
//!    about its own kind — [`Timestamp::duration_since`] decides whether two instants of the
//!    same type went backwards, [`Label::resolve`] decides which of three reference forms it was
//!    handed — but logic needing two different concepts to decide something belongs to the
//!    phase that decides it.
//! 3. One concept, one type. [`types`] is the only place a domain value is declared, so nothing
//!    downstream invents a second spelling of a name that already exists.
//! 4. [`ports`] never gains a production implementation. A real implementation means doing I/O,
//!    which this crate does not do; the only implementations here are test fakes.
//! 5. Every phase module is exercised against in-memory implementations of the ports it uses.
//!    Its pure helpers, which need no port, are tested in the file under test; its flows through
//!    a port are tested in this crate's integration tests, against fakes built from the public
//!    API alone, so anything a fake does an adapter crate can do too.
//!
//! # Status
//!
//! Pre-release. Load is implemented: [`load`](fn@load) walks the workspace's build files
//! through the [`DeclarationSource`] port and returns their declarations validated and sorted,
//! and [`Pipeline::check`] runs it twice to catch a build file that declares differently on
//! each evaluation. Resolve is implemented: [`resolve`](fn@resolve) turns those declarations
//! into a [`TargetGraph`], refusing a name declared twice, a reference that names nothing or the
//! wrong kind of thing, and a dependency cycle; [`Pipeline::graph`] runs both phases. Plan is
//! implemented: [`plan`](fn@plan) refuses a build that wants more than its workspace's
//! [`Ceiling`] allows, gives every target an [`ActionKey`] from what the ports report, and
//! compares it with the key the target's last build recorded, so each target is clean, dirty
//! with a reason, or conditional on a dependency's output; the [`Plan`] also holds the value of
//! every build setting for the run, which is where Execute reads them; [`Pipeline::plan`] runs
//! all three phases. Execute and Record are implemented: [`execute`](fn@execute) gives every
//! output a path under the workspace's output directory, expands every command and holds its
//! program to the [`Ceiling`] before anything runs, then runs the plan through the
//! [`Dispatcher`] port. A target whose finished key equals the one its last build recorded,
//! with every recorded output still in the [`ContentStore`], is cached; any other runs through
//! the [`ExecStrategy`] port.
//! An output is a regular file or a directory of regular files: its [`OutputManifest`] is stored
//! beside the files it names, and the digest of that manifest is the output's digest.
//! Recording is the last step of each action — its outputs into the content store, then its
//! [`CacheRow`], then one [`OutcomeEvent`] in the [`EventLog`] — so a failure leaves no row.
//! [`Pipeline::build`] runs all four phases and returns one [`ActionOutcome`] per target; what
//! a failure does to the rest of the build is the caller's [`OnFailure`]. [`Pipeline::build`]
//! and [`Pipeline::test`] put the plan's wanted set to the [`Approver`] port after planning,
//! and [`negotiate`](fn@negotiate) turns a denial into [`Error::NotApproved`]: a refusal runs
//! nothing, even when every target is cached. [`Pipeline::plan`] never asks.
//!
//! A plugin's declarations carry an [`Origin`]: the chain of [`PluginRef`]s, outermost first,
//! whose code made them, and a graph [`Node`] keeps it. [`plan`](fn@plan) holds every want of a
//! node with plugins to each plugin in its chain, the plugin's manifest and the workspace's
//! [`PluginCap`] for it, before it asks the [`Ceiling`]: a want outside either is
//! [`Error::PluginRefused`], and so is a target that reaches the network when the cap does not
//! allow it. An [`Attribution`] says which plugins' targets are behind each want; the plan, the
//! [`Approver`] and a [`Checkup`] carry it for display, and it is no part of an [`ActionKey`].
//!
//! A run works on the whole workspace or on the targets a [`Scope`] names, with everything they
//! depend on: [`narrow`](fn@narrow) cuts the graph down before Plan, so [`Pipeline::plan`] and
//! [`Pipeline::build`] neither plan nor build a target outside it, while two outputs claiming one
//! place are still refused across the whole workspace. A build writes down the graph of the whole
//! workspace through the [`GraphRecord`] port before it plans, and is shown through the
//! [`Reporter`] port as it goes: that it began, each action as a worker begins it, each target's
//! outcome with what its action printed — for a cached target as for one that ran — and the count
//! last. Under [`LiveOutput::OneAction`] one running action at a time is shown as it prints. An
//! action a signal ended fails its target as [`OutcomeClass::Killed`], and an input that is not a
//! regular file is refused by Plan from what the [`Digester`] reports as an [`InputStat`].
//! [`Hasher`] takes the digest of bytes handed in pieces. A [`Query`], `deps` or `rdeps` of one
//! target, asks which targets that target needs or which need it: [`reach`](fn@reach) answers it
//! over a [`TargetGraph`], in the [`Direction`] it names. [`pipeline::check`], [`pipeline::graph`]
//! and [`pipeline::query`] run the three commands that reach only build files for a caller that
//! holds a [`DeclarationSource`], a [`RunHistory`] and a [`ContentStore`]. A workspace that
//! declares a discovery target is loaded again with the last result of each, read from those two
//! ports, and the second load may not declare different discovery targets than the first
//! ([`Error::DiscoveryChanged`]); [`Pipeline::plan`], [`Pipeline::verify`] and [`gc`](fn@gc) do
//! the same, and [`Plan::stale_discovery`] names the discovery targets a plan finds not clean.
//! [`Pipeline::build`] and [`Pipeline::test`] build the discovery targets first, as a build of
//! just them, and load the second time with the results that leaves.
//! A [`LoggedRun`] reads back from the build log through
//! [`RunHistory`], and [`pipeline::log`](fn@pipeline::log) returns a target's last run with what it
//! printed. [`pipeline::doctor`](fn@pipeline::doctor) reports what a workspace's build wants
//! against its ceiling as a [`Checkup`], every want outside the ceiling listed and none refused,
//! for a caller that holds a [`DeclarationSource`], a [`Manifest`], a [`RunHistory`] and a
//! [`ContentStore`].
//!
//! A target no cache row stands for may be served by a cache other machines share, reached
//! through the [`RemoteCache`] port. [`Pipeline::set_remote`] sets the [`RemotePolicy`] of a
//! build and a test run: a [`RemoteUse`], a [`RemoteFailure`] and a [`RemoteExecution`]. With
//! the remote on, a worker asks the port for the result of a [`RemoteAction`] built from the ready action before it runs
//! anything. A [`RemoteHit`] is used only when it is a clean success for the declared outputs;
//! every blob is checked against its digest and stored before the row is written, and the target
//! ends as [`OutcomeClass::Fetched`]. A miss runs the action, and under [`RemoteUse::ReadWrite`]
//! a run that succeeded is uploaded, its blobs read from the content store through
//! [`BlobSource`]. A failed call warns once and turns the remote off for the rest of the command
//! under [`RemoteFailure::Lenient`], and fails the command under [`RemoteFailure::Strict`]; a
//! result that is not usable is a failure of the same kind. [`NoRemote`] is the port of a
//! workspace that names none, and every command but `build` and `test` never calls it.
//!
//! A verify compares what cached targets produce when re-run with what was cached.
//! [`Pipeline::verify`] plans as a build does, asks the [`Approver`] as a build does, then
//! [`verify`](fn@verify) puts the outputs of every target whose last build still stands back in
//! place and re-runs a [`Sample`] of those targets through the [`ExecStrategy`] port. A
//! [`Verification`] holds a [`Check`] per target it looked at: the re-run reproduced the outputs,
//! differed at the first output and file an [`OutputDifference`] names, failed, or was never begun
//! because a target it needs was not put back. It counts the targets with nothing cached and the
//! ones the sample left out. A verify records no row, forgets none, logs no event and reports
//! nothing, and no output of a re-run is put in place.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to
//! unit-test.

mod discovery;
pub mod error;
pub mod execute;
pub mod grants;
pub mod history;
pub mod json;
pub mod load;
pub mod pipeline;
mod placement;
pub mod plan;
pub mod ports;
pub mod resolve;
mod suggest;
pub mod types;
pub mod verify;
pub mod watch;

pub use error::{
    ActionFound, DeclarationField, DeclarationSite, DeclaredKind, Error, EvaluationFailure,
    EvaluationLimit, NameKind, PortName, RemoteFlagConflict, RemoteVariable, Result, TreeFault,
};
pub use execute::execute;
pub use grants::negotiate;
pub use history::LastRun;
pub use load::load;
pub use pipeline::{Collected, Pipeline, Shown, doctor, gc, log};
pub use plan::plan;
pub use ports::{
    ActionCache, Approver, BlobSource, Changes, Clock, ContentStore, Cycles, DeclarationSource,
    Digester, Dispatcher, EventLog, ExecStrategy, GraphRecord, Host, Manifest, NoRemote, PortSet,
    Ports, RemoteCache, Reporter, RunHistory, StatCache, Sweep, ToolResolver, WatchReport,
};
pub use resolve::{narrow, reach, resolve};
pub use types::{
    Action, ActionKey, ActionOutcome, Alias, Approval, ApproverMode, Argument, Attribution, Blob,
    BuildFile, Built, CacheRow, Captured, Ceiling, ChangeBatch, ChangedPath, Channel, Check,
    Checkup, Command, CommitId, Cycle, Declaration, DeclarationOrder, Declared, DenialReason,
    DepProducts, Description, Diagnostic, Difference, Digest, Direction, Directory, DirtyReason,
    Discovered, EntryChange, EntryName, EnvName, EnvValue, Evaluated, Executed, Execution, Exit,
    ExpandedDescription, FieldName, FileChange, FileMode, FileStat, Found, FoundEntry, FoundKind,
    Freshness, GitUrl, Hasher, HeaderName, HostPath, InfraKind, InputDigest, InputKind, InputStat,
    InstanceName, Isolation, KeyComponents, Label, LiveOutput, LiveSink, Loaded, LogLine,
    LoggedRun, NetworkAccess, Node, NodeId, OnFailure, Origin, OutcomeClass, OutcomeEvent,
    OutputDifference, OutputManifest, OutputName, OutputPath, OutputRefusal, OwnComponents, Plan,
    PlannedKey, Platform, PlatformProperties, PlatformPropertyName, PluginCap, PluginChain,
    PluginName, PluginNetwork, PluginRef, PluginSource, PluginVersion, PluginViolation, Produced,
    Program, Provenance, Purpose, Query, RawOutcome, Reaction, ReadyAction, Reference,
    RemoteAction, RemoteCall, RemoteContent, RemoteExecution, RemoteFailure, RemoteFile, RemoteHit,
    RemoteJobs, RemoteNotice, RemoteOutput, RemoteOutputDirectory, RemoteOutputFile,
    RemotePlatform, RemotePolicy, RemoteScheme, RemoteSettings, RemoteSettingsBuilder,
    RemoteSwitch, RemoteTree, RemoteTreeFile, RemoteUpload, RemoteUrl, RemoteUse, Rerun,
    RetryCount, Rev, RootPath, Rule, RunChoices, Sample, Scope, Setting, SettingName, SettingValue,
    SourcePath, StagedAlias, StagedDeclaration, StagedFile, StagedItem, StagedRule, StagedSetting,
    StagedSubdir, StagedTarget, StatEntry, StoredFile, Summary, TagName, Tally, Target,
    TargetGraph, TargetName, TargetRole, Timeout, Timestamp, TokenSource, ToolIdentity, Tree,
    TreeEntry, TreePath, Trigger, Turn, Unfinished, Uploaded, Verdict, Verification, Want,
    WantedSet, WatchLayout, Watched, Workers, Workspace, Written,
};
pub use verify::verify;
pub use watch::{react, watch};
