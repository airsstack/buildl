//! Plugins: the code a build file loads with `buildl.use`, under an environment of its own.
//!
//! Responsibilities: [`Plugins`], the plugins one evaluation may use.
//!
//! Non-responsibilities: deciding which plugins a workspace has, which the caller passes in.
//!
//! # Where things live
//!
//! | Module | Holds |
//! |---|---|
//! | `manifest` | `extension.toml` read with airsl and turned into a `PluginRef` |
//! | `loader` | `buildl.use`: refusals, instances per origin chain, the entry chunk |
//! | `environment` | the table a plugin instance runs under |
//! | `require` | a plugin instance's `require`, confined to its directory |
//!
//! This file holds only module declarations and re-exports, so it carries no logic to
//! unit-test.

mod environment;
mod loader;
mod manifest;
mod require;

pub(crate) use loader::Plugins;
