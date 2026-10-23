//! Plugins: reading a plugin's manifest and, in the modules to come, loading its code.
//!
//! Responsibilities: [`read_manifest`].
//!
//! Non-responsibilities: deciding which plugins a workspace has, which the caller passes in.
//!
//! # Where things live
//!
//! | Module | Holds |
//! |---|---|
//! | `manifest` | `extension.toml` read with airsl and turned into a `PluginRef` |
//!
//! This file holds only module declarations and re-exports, so it carries no logic to
//! unit-test.

mod manifest;

#[expect(
    unused_imports,
    reason = "no code loads a plugin yet, so nothing imports the manifest reader"
)]
pub(crate) use manifest::read_manifest;
