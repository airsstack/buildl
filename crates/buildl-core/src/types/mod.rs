//! The domain vocabulary: the validated values the pipeline's phases hand to one another, and the
//! as-written values a build file stages before they are validated.
//!
//! Its own directory because these are the crate's values with a grammar to get wrong, and each
//! is parsed once at the edge so no later phase re-checks it. Validation lives in the sibling
//! file named for the type; this file is the index.
//!
//! Responsibilities:
//!
//! - [`Digest`] — the single SHA-256 identity for files, outputs, keys and log blobs.
//! - [`Hasher`] — that digest, taken over bytes handed in pieces.
//! - [`Directory`] — a workspace-relative directory path.
//! - [`TargetName`] — the name half of a label.
//! - [`Label`] — a target's absolute name.
//! - [`NodeId`] — a target's handle inside the graph's arenas.
//! - [`Provenance`] — the build file and directory a declaration came from.
//! - [`Timestamp`] — an instant on the host wall clock.
//! - [`SourcePath`], [`OutputName`], [`EnvName`], [`Argument`], [`Command`], [`Description`],
//!   [`SettingName`], [`SettingValue`] — the validated values a declaration's fields hold;
//!   [`ExpandedDescription`] — a description once its placeholders are expanded.
//! - [`EntryName`] — the file name every build file has; [`FieldName`] — the name of a field in
//!   a declaration's option table.
//! - [`Program`] — the name of a program a command runs; [`EnvValue`] — an environment
//!   variable's value; [`Platform`] — the operating system and architecture actions run on.
//! - [`FileStat`] and [`StatEntry`] — a file's metadata, and a digest recorded beside it.
//! - [`InputStat`] and [`InputKind`] — what stands at the path of a declared input.
//! - [`Diagnostic`] — an adapter's message for a failure, carried for display only.
//! - [`Written`] — text exactly as a build file wrote it, typed by what it is meant to become.
//! - [`Declaration`] and its parts — what a build file declared, validated.
//! - [`BuildFile`], [`Evaluated`] and the staged values — what crosses the build-file port;
//!   [`Discovered`] — what discovery targets last produced, handed to the build file that asks.
//! - [`Loaded`] — the declarations a command loaded and the discovery targets that had no result.
//! - [`TargetGraph`] and [`Node`] — the runnable targets and the dependencies between them.
//! - [`ActionKey`], [`KeyComponents`] and their parts — what decides a target's output, and the
//!   digest that stands for it.
//! - [`CacheRow`] — what the last successful build of one target recorded.
//! - [`Ceiling`], [`WantedSet`] and [`Want`] — what a workspace allows, and what a build wants.
//! - [`Attribution`] — the plugins whose targets are behind each want.
//! - [`Approval`] and [`DenialReason`] — an approver's answer about what a build wants;
//!   [`ApproverMode`] — which approver a workspace names.
//! - [`Checkup`] — what a workspace allows and wants, as `doctor` reports it.
//! - [`Workspace`] — what a workspace's manifest tells the pipeline.
//! - [`Blob`] — a digest with the size of what it digests; [`RootPath`] — a file of the input
//!   root a remote action is given.
//! - [`RemoteAction`], [`RemoteFile`] and [`RemotePlatform`] — the action a remote cache is
//!   asked about; [`RemoteHit`] with [`RemoteOutputFile`], [`RemoteOutputDirectory`] and
//!   [`RemoteTreeFile`] — the result it answers with, unchecked; [`RemoteUpload`] with
//!   [`RemoteOutput`], [`RemoteContent`] and [`RemoteTree`], and [`Uploaded`] — what a run stores
//!   there and how that ended.
//! - [`RemoteUse`], [`RemoteFailure`] and [`RemotePolicy`] — what a run does with the remote;
//!   [`RemoteCall`] and [`RemoteNotice`] — what a build says about it.
//! - [`RemoteSettings`] — the remote cache a workspace names, built through
//!   [`RemoteSettingsBuilder`], with its parts [`RemoteUrl`], [`RemoteScheme`], [`InstanceName`],
//!   [`HostPath`], [`TokenSource`] and [`HeaderName`].
//! - [`PluginName`], [`PluginVersion`], [`PluginRef`], [`PluginCap`] and [`PluginNetwork`] — a
//!   plugin, what its manifest asks for, and what the workspace lets it have; [`PluginChain`]
//!   — the plugins that made a declaration; [`Origin`] — whose
//!   code declared a target; [`PluginViolation`] — a want outside a plugin's caps.
//! - [`PluginSource`] — where a plugin comes from, with [`GitUrl`], [`Rev`], [`TagName`] and
//!   [`CommitId`] — the parts of a git source.
//! - [`Execution`], with [`Workers`], [`Timeout`] and [`Isolation`] — how a workspace wants its
//!   actions run.
//! - [`Plan`], [`Verdict`] and their parts — what a build would do with each target, and why.
//! - [`OutputPath`] — where one output of one target lives in the workspace.
//! - [`OutputManifest`] — what one output is, said exactly; its parts [`StoredFile`],
//!   [`FileMode`], [`Tree`], [`TreeEntry`] and [`TreePath`].
//! - [`Found`], [`FoundEntry`] and [`FoundKind`] — what an execution strategy found at one
//!   output path.
//! - [`ReadyAction`] — one action as an execution strategy is handed it; [`RawOutcome`] and
//!   [`Exit`] — what the strategy reports back.
//! - [`Channel`] — which of its two outputs a command wrote to; [`LiveSink`] — where a strategy
//!   hands what a running command writes; [`Captured`] — what an action printed, as whoever
//!   shows it is handed it.
//! - [`OnFailure`] — what a build does with the rest of its targets once one has failed.
//! - [`Scope`] — which targets of a workspace one run works on; [`LiveOutput`] — whether a build
//!   shows what an action prints while the action is still running.
//! - [`Purpose`] — what a run is for, which decides the targets a whole workspace means.
//! - `Forcing` — what makes one target run whatever its row says.
//! - [`Rerun`] — whether a run runs its tests again whatever their keys.
//! - [`Query`] and [`Direction`] — a question about the target graph: what one target needs, or
//!   what needs it.
//! - [`ActionOutcome`], [`OutcomeClass`], [`InfraKind`] and [`OutputRefusal`] — how the build of
//!   one target ended; [`Tally`] — how many targets ended in each way;
//!   [`Summary`] — a build's tally, and the tally of its tests.
//! - [`Sample`] — how many of the cached targets a verify re-runs; [`Verification`], [`Check`] and
//!   [`OutputDifference`] with its parts [`FileChange`] and [`EntryChange`] — what it found.
//! - [`OutcomeEvent`] — one line of the build log: an outcome, with when and under which key.
//! - [`LoggedRun`] and [`LogLine`] — a line of the build log as it reads back: an event, or a
//!   damaged line.
//! - [`Turn`] — what a dispatcher hands the thread that schedules a build, each time it hands
//!   it something.
//!
//! Non-responsibilities: decisions between different concepts. A type here validates and renders
//! itself, and may combine values of its own kind; logic that needs two different concepts to
//! decide something belongs to the module for the phase that decides it.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

mod grammar;
pub(crate) mod setting_reference;

pub mod action_key;
pub mod approval;
pub mod approver_mode;
pub mod argument;
pub mod attribution;
pub mod authority;
pub mod blob;
pub mod build_file;
pub mod cache_row;
pub mod captured;
pub mod channel;
pub mod check;
pub mod checkup;
pub mod declaration;
pub mod description;
pub mod diagnostic;
pub mod digest;
pub mod directory;
pub mod discovered;
pub mod entry_name;
pub mod env_name;
pub mod env_value;
pub mod field_name;
pub mod file_stat;
pub(crate) mod forcing;
pub mod found;
pub mod input_stat;
pub mod isolation;
pub mod label;
pub mod live_output;
pub mod live_sink;
pub mod loaded;
pub mod log_line;
pub mod logged_run;
pub mod node_id;
pub mod on_failure;
pub mod outcome;
pub mod outcome_event;
pub mod output_manifest;
pub mod output_name;
pub mod output_path;
pub mod plan;
pub mod platform;
pub mod plugin;
pub mod plugin_source;
pub mod program;
pub mod provenance;
pub mod purpose;
pub mod query;
pub mod raw_outcome;
pub mod ready_action;
pub mod remote_action;
pub mod remote_hit;
pub mod remote_notice;
pub mod remote_policy;
pub mod remote_settings;
pub mod remote_upload;
pub mod rerun;
pub mod root_path;
pub mod sample;
pub mod scope;
pub mod setting;
pub mod source_path;
pub mod stored_file;
pub mod target_graph;
pub mod target_name;
pub mod timeout;
pub mod timestamp;
pub mod tree;
pub mod tree_path;
pub mod turn;
pub mod verification;
pub mod workers;
pub mod workspace;
pub mod written;

pub use action_key::{
    ActionKey, DepProducts, InputDigest, KeyComponents, OwnComponents, Produced, ToolIdentity,
};
pub use approval::{Approval, DenialReason};
pub use approver_mode::ApproverMode;
pub use argument::{Argument, Command};
pub use attribution::Attribution;
pub use authority::{Ceiling, Want, WantedSet};
pub use blob::Blob;
pub use build_file::{
    BuildFile, DeclarationOrder, Evaluated, StagedAlias, StagedDeclaration, StagedFile, StagedItem,
    StagedRule, StagedSetting, StagedSubdir, StagedTarget,
};
pub use cache_row::CacheRow;
pub use captured::Captured;
pub use channel::Channel;
pub use check::{Check, EntryChange, FileChange, OutputDifference};
pub use checkup::Checkup;
pub use declaration::{
    Action, Alias, Declaration, Declared, Freshness, NetworkAccess, Rule, Setting, Target,
    TargetRole,
};
pub use description::{Description, ExpandedDescription};
pub use diagnostic::Diagnostic;
pub use digest::{Digest, Hasher};
pub use directory::Directory;
pub use discovered::Discovered;
pub use entry_name::EntryName;
pub use env_name::EnvName;
pub use env_value::EnvValue;
pub use field_name::FieldName;
pub use file_stat::{FileStat, StatEntry};
pub(crate) use forcing::Forcing;
pub use found::{Found, FoundEntry, FoundKind};
pub use input_stat::{InputKind, InputStat};
pub use isolation::Isolation;
pub use label::Label;
pub use live_output::LiveOutput;
pub use live_sink::LiveSink;
pub use loaded::Loaded;
pub use log_line::LogLine;
pub use logged_run::LoggedRun;
pub use node_id::NodeId;
pub use on_failure::OnFailure;
pub use outcome::{ActionOutcome, InfraKind, OutcomeClass, OutputRefusal, Summary, Tally};
pub use outcome_event::OutcomeEvent;
pub use output_manifest::OutputManifest;
pub use output_name::OutputName;
pub use output_path::OutputPath;
pub use plan::{Difference, DirtyReason, Plan, PlannedKey, Unfinished, Verdict};
pub use platform::Platform;
pub use plugin::{
    Origin, PluginCap, PluginChain, PluginName, PluginNetwork, PluginRef, PluginVersion,
    PluginViolation,
};
pub use plugin_source::{CommitId, GitUrl, PluginSource, Rev, TagName};
pub use program::Program;
pub use provenance::Provenance;
pub use purpose::Purpose;
pub use query::{Direction, Query};
pub use raw_outcome::{Exit, RawOutcome};
pub use ready_action::ReadyAction;
pub use remote_action::{RemoteAction, RemoteFile, RemotePlatform};
pub use remote_hit::{RemoteHit, RemoteOutputDirectory, RemoteOutputFile, RemoteTreeFile};
pub use remote_notice::{RemoteCall, RemoteNotice};
pub use remote_policy::{RemoteFailure, RemotePolicy, RemoteUse};
pub use remote_settings::{
    HeaderName, HostPath, InstanceName, RemoteScheme, RemoteSettings, RemoteSettingsBuilder,
    RemoteUrl, TokenSource,
};
pub use remote_upload::{RemoteContent, RemoteOutput, RemoteTree, RemoteUpload, Uploaded};
pub use rerun::Rerun;
pub use root_path::RootPath;
pub use sample::Sample;
pub use scope::Scope;
pub use setting::{SettingName, SettingValue};
pub use source_path::SourcePath;
pub use stored_file::{FileMode, StoredFile};
pub use target_graph::{Node, TargetGraph};
pub use target_name::TargetName;
pub use timeout::Timeout;
pub use timestamp::Timestamp;
pub use tree::{Tree, TreeEntry};
pub use tree_path::TreePath;
pub use turn::Turn;
pub use verification::Verification;
pub use workers::Workers;
pub use workspace::{Execution, Workspace};
pub use written::Written;
