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
//! - [`Directory`] — a workspace-relative directory path.
//! - [`TargetName`] — the name half of a label.
//! - [`Label`] — a target's absolute name.
//! - [`NodeId`] — a target's handle inside the graph's arenas.
//! - [`Provenance`] — the build file and directory a declaration came from.
//! - [`Timestamp`] — an instant on the host wall clock.
//! - [`SourcePath`], [`OutputName`], [`EnvName`], [`Argument`], [`Command`], [`Description`],
//!   [`SettingName`], [`SettingValue`] — the validated values a declaration's fields hold.
//! - [`EntryName`] — the file name every build file has; [`FieldName`] — the name of a field in
//!   a declaration's option table.
//! - [`Program`] — the name of a program a command runs; [`EnvValue`] — an environment
//!   variable's value; [`Platform`] — the operating system and architecture actions run on.
//! - [`FileStat`] and [`StatEntry`] — a file's metadata, and a digest recorded beside it.
//! - [`Diagnostic`] — an adapter's message for a failure, carried for display only.
//! - [`Written`] — text exactly as a build file wrote it, typed by what it is meant to become.
//! - [`Declaration`] and its parts — what a build file declared, validated.
//! - [`BuildFile`], [`Evaluated`] and the staged values — what crosses the build-file port.
//! - [`TargetGraph`] and [`Node`] — the runnable targets and the dependencies between them.
//! - [`ActionKey`], [`KeyComponents`] and their parts — what decides a target's output, and the
//!   digest that stands for it.
//! - [`CacheRow`] — what the last successful build of one target recorded.
//! - [`Ceiling`], [`WantedSet`] and [`Want`] — what a workspace allows, and what a build wants.
//! - [`Workspace`] — what a workspace's manifest tells the pipeline.
//! - [`Plan`], [`Verdict`] and their parts — what a build would do with each target, and why.
//! - [`OutputPath`] — where one output of one target lives in the workspace.
//! - [`ReadyAction`] — one action as an execution strategy is handed it; [`RawOutcome`] and
//!   [`Exit`] — what the strategy reports back.
//! - [`OnFailure`] — what a build does with the rest of its targets once one has failed.
//! - [`ActionOutcome`], [`OutcomeClass`] and [`InfraKind`] — how the build of one target ended;
//!   [`Summary`] — the count of a build's outcomes.
//! - [`OutcomeEvent`] — one line of the build log: an outcome, with when and under which key.
//!
//! Non-responsibilities: decisions between different concepts. A type here validates and renders
//! itself, and may combine values of its own kind; logic that needs two different concepts to
//! decide something belongs to the module for the phase that decides it.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

mod grammar;

pub mod action_key;
pub mod argument;
pub mod authority;
pub mod build_file;
pub mod cache_row;
pub mod declaration;
pub mod description;
pub mod diagnostic;
pub mod digest;
pub mod directory;
pub mod entry_name;
pub mod env_name;
pub mod env_value;
pub mod field_name;
pub mod file_stat;
pub mod label;
pub mod node_id;
pub mod on_failure;
pub mod outcome;
pub mod outcome_event;
pub mod output_name;
pub mod output_path;
pub mod plan;
pub mod platform;
pub mod program;
pub mod provenance;
pub mod raw_outcome;
pub mod ready_action;
pub mod setting;
pub mod source_path;
pub mod target_graph;
pub mod target_name;
pub mod timestamp;
pub mod workspace;
pub mod written;

pub use action_key::{
    ActionKey, DepProducts, InputDigest, KeyComponents, OwnComponents, Produced, ToolIdentity,
};
pub use argument::{Argument, Command};
pub use authority::{Ceiling, Want, WantedSet};
pub use build_file::{
    BuildFile, DeclarationOrder, Evaluated, StagedAlias, StagedDeclaration, StagedFile, StagedItem,
    StagedRule, StagedSetting, StagedSubdir, StagedTarget,
};
pub use cache_row::CacheRow;
pub use declaration::{
    Action, Alias, Declaration, Declared, Freshness, NetworkAccess, Rule, Setting, Target,
    TargetRole,
};
pub use description::Description;
pub use diagnostic::Diagnostic;
pub use digest::Digest;
pub use directory::Directory;
pub use entry_name::EntryName;
pub use env_name::EnvName;
pub use env_value::EnvValue;
pub use field_name::FieldName;
pub use file_stat::{FileStat, StatEntry};
pub use label::Label;
pub use node_id::NodeId;
pub use on_failure::OnFailure;
pub use outcome::{ActionOutcome, InfraKind, OutcomeClass, Summary};
pub use outcome_event::OutcomeEvent;
pub use output_name::OutputName;
pub use output_path::OutputPath;
pub use plan::{Difference, DirtyReason, Plan, PlannedKey, Unfinished, Verdict};
pub use platform::Platform;
pub use program::Program;
pub use provenance::Provenance;
pub use raw_outcome::{Exit, RawOutcome};
pub use ready_action::ReadyAction;
pub use setting::{SettingName, SettingValue};
pub use source_path::SourcePath;
pub use target_graph::{Node, TargetGraph};
pub use target_name::TargetName;
pub use timestamp::Timestamp;
pub use workspace::Workspace;
pub use written::Written;
