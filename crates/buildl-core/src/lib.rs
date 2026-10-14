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
//! | [`types`] | the validated domain values — names, identities, instants, declarations, the target graph, action keys, cache rows, the ceiling, the plan, ready actions, outcomes, log events, what one run was asked for — and the as-written values a build file stages |
//! | [`json`] | the one canonical serializer every byte of output goes through, and the one reader that takes such bytes back |
//! | [`ports`] | every trait in the crate: the ports through which everything outside it is reached, and the bundle that names one implementation of each, with the set that holds one instance of each |
//! | [`load`](mod@load) | Load, the first phase: build files to validated, sorted declarations |
//! | [`resolve`](mod@resolve) | Resolve, the second phase: declarations to the target graph, that graph cut down to the targets one run was asked for, and the targets a query reaches |
//! | [`plan`](mod@plan) | Plan, the third phase: the target graph to a verdict per target, with what the build wants |
//! | [`execute`](mod@execute) | Execute and Record, the last phase: a plan to one outcome per target, each result recorded as the last step of its action and reported once it is logged |
//! | [`pipeline`] | the phases run in sequence over one bundle of ports, one method per command, and the three commands that reach only build files as functions over a source alone |
//!
//! Each phase has its own module, named for the phase. Five rules keep that arrangement
//! navigable:
//!
//! 1. No phase module names another. Phases communicate through values, and [`pipeline`] owns
//!    the sequence, so a phase that imported the next would be bypassing it.
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
//! a failure does to the rest of the build is the caller's [`OnFailure`].
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
//! holds only a [`DeclarationSource`].
//!
//! This file holds only module declarations and re-exports, so it carries no logic to
//! unit-test.

pub mod error;
pub mod execute;
pub mod json;
pub mod load;
pub mod pipeline;
pub mod plan;
pub mod ports;
pub mod resolve;
pub mod types;

pub use error::{
    ActionFound, DeclarationField, DeclarationSite, DeclaredKind, Error, EvaluationFailure,
    EvaluationLimit, NameKind, PortName, Result, TreeFault,
};
pub use execute::execute;
pub use load::load;
pub use pipeline::Pipeline;
pub use plan::plan;
pub use ports::{
    ActionCache, Clock, ContentStore, DeclarationSource, Digester, Dispatcher, EventLog,
    ExecStrategy, GraphRecord, Host, Manifest, PortSet, Ports, Reporter, StatCache, ToolResolver,
};
pub use resolve::{narrow, reach, resolve};
pub use types::{
    Action, ActionKey, ActionOutcome, Alias, Argument, BuildFile, CacheRow, Captured, Ceiling,
    Channel, Command, Declaration, DeclarationOrder, Declared, DepProducts, Description,
    Diagnostic, Difference, Digest, Direction, Directory, DirtyReason, EntryName, EnvName,
    EnvValue, Evaluated, Execution, Exit, ExpandedDescription, FieldName, FileMode, FileStat,
    Found, FoundEntry, FoundKind, Freshness, Hasher, InfraKind, InputDigest, InputKind, InputStat,
    Isolation, KeyComponents, Label, LiveOutput, LiveSink, NetworkAccess, Node, NodeId, OnFailure,
    OutcomeClass, OutcomeEvent, OutputManifest, OutputName, OutputPath, OutputRefusal,
    OwnComponents, Plan, PlannedKey, Platform, Produced, Program, Provenance, Query, RawOutcome,
    ReadyAction, Rule, Scope, Setting, SettingName, SettingValue, SourcePath, StagedAlias,
    StagedDeclaration, StagedFile, StagedItem, StagedRule, StagedSetting, StagedSubdir,
    StagedTarget, StatEntry, StoredFile, Summary, Target, TargetGraph, TargetName, TargetRole,
    Timeout, Timestamp, ToolIdentity, Tree, TreeEntry, TreePath, Turn, Unfinished, Verdict, Want,
    WantedSet, Workers, Workspace, Written,
};
