//! The domain vocabulary: the validated values the pipeline's phases hand to one another.
//!
//! Its own directory because these are the crate's values with a grammar to get wrong, and each
//! is parsed once at the edge so no later phase re-checks it. Validation lives in the sibling
//! file named for the type; this file is the index.
//!
//! Responsibilities:
//!
//! - [`Directory`] — a workspace-relative directory path.
//! - [`TargetName`] — the name half of a label.
//! - [`Label`] — a target's absolute name.
//!
//! Non-responsibilities: decisions. A type here validates and renders itself; logic that needs
//! two of them to decide something belongs to the module for the phase that decides it.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

pub mod directory;
pub mod label;
pub mod target_name;

pub use directory::Directory;
pub use label::Label;
pub use target_name::TargetName;
