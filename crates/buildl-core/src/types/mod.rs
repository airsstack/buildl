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
//!
//! Non-responsibilities: decisions. A type here validates and renders itself; logic that needs
//! two of them to decide something belongs to the module for the phase that decides it.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

pub mod digest;
pub mod directory;
pub mod label;
pub mod node_id;
pub mod provenance;
pub mod target_name;
pub mod timestamp;

pub use digest::Digest;
pub use directory::Directory;
pub use label::Label;
pub use node_id::NodeId;
pub use provenance::Provenance;
pub use target_name::TargetName;
pub use timestamp::Timestamp;
