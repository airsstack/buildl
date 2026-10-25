//! The workspace manifest, `buildl.toml`: finding it, and reading what it says.
//!
//! Its own directory because the manifest is the one file that is both the marker of a
//! workspace root and the ceiling on what a build may do, and no other module of this crate
//! knows its name or its format.
//!
//! Responsibilities:
//!
//! - [`WorkspaceRoot`] — the directory that holds the manifest, found from where a command was
//!   started.
//! - [`TomlManifest`] — the manifest, read and checked once: the workspace it describes, the
//!   file name of a build file, the ceilings a build file is evaluated under, and where each
//!   plugin it names comes from.
//! - [`DeclarationSettings`] — those ceilings, each absent when the manifest sets none.
//!
//! Non-responsibilities: deciding what a build wants, and holding a build to the ceiling. Both
//! belong to the pipeline, which is handed what the manifest says as a value.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

mod anchor;
mod interpret;
mod memory;
mod root;
mod schema;
mod toml_manifest;

pub use root::WorkspaceRoot;
pub use toml_manifest::{DeclarationSettings, TomlManifest};
