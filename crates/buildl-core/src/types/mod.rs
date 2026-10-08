//! The domain vocabulary: the validated values the pipeline's phases hand to one another.
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
//!   [`SettingName`], [`SettingValue`], [`EntryName`], [`FieldName`] — the validated values a
//!   declaration's fields hold.
//! - [`Diagnostic`] — an adapter's message for a failure, carried for display only.
//! - [`Written`] — text exactly as a build file wrote it, typed by what it is meant to become.
//!
//! Non-responsibilities: decisions. A type here validates and renders itself; logic that needs
//! two of them to decide something belongs to the module for the phase that decides it.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

mod grammar;

pub mod argument;
pub mod build_file;
pub mod declaration;
pub mod description;
pub mod diagnostic;
pub mod digest;
pub mod directory;
pub mod entry_name;
pub mod env_name;
pub mod field_name;
pub mod label;
pub mod node_id;
pub mod output_name;
pub mod provenance;
pub mod setting;
pub mod source_path;
pub mod target_name;
pub mod timestamp;
pub mod written;

pub use argument::{Argument, Command};
pub use build_file::{
    BuildFile, DeclarationOrder, Evaluated, StagedAlias, StagedDeclaration, StagedFile, StagedItem,
    StagedRule, StagedSetting, StagedSubdir, StagedTarget,
};
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
pub use field_name::FieldName;
pub use label::Label;
pub use node_id::NodeId;
pub use output_name::OutputName;
pub use provenance::Provenance;
pub use setting::{SettingName, SettingValue};
pub use source_path::SourcePath;
pub use target_name::TargetName;
pub use timestamp::Timestamp;
pub use written::Written;
