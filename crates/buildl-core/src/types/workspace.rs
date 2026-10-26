//! What a workspace's manifest tells the pipeline.
//!
//! Its own file because these are the facts about a workspace that no build file states: where
//! a build puts what it produces, where buildl keeps its own files, what the workspace allows,
//! and how it wants its actions run.
//!
//! Responsibilities: [`Workspace`] and [`Execution`].
//!
//! Non-responsibilities: reading a manifest, which is a port's; and the grammar of each
//! execution setting, which the file named for its type holds.
//!
//! This file holds only type definitions, so it carries no logic to unit-test.

use std::collections::BTreeMap;

use crate::types::{
    ApproverMode, Ceiling, Directory, Isolation, PluginCap, PluginName, RemoteSettings, Timeout,
    Workers,
};

/// A workspace as its manifest describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    /// The directory every output is placed under.
    pub out: Directory,
    /// The directory buildl keeps its own files in: its caches, its content store, its log.
    pub state: Directory,
    /// What the workspace allows a build to do.
    pub ceiling: Ceiling,
    /// How the workspace wants its actions run.
    pub execution: Execution,
    /// How the workspace settles whether a build may run. No phase reads it: whoever assembles
    /// the adapters picks the approver from it.
    pub approver: ApproverMode,
    /// What the workspace lets each plugin have, by plugin name. A plugin with no entry has the
    /// empty cap.
    pub plugins: BTreeMap<PluginName, PluginCap>,
    /// The remote cache server the workspace names. `None`: the workspace names no server.
    pub remote: Option<RemoteSettings>,
}

/// How a workspace wants its actions run: the manifest's `[execution]` table.
///
/// These settings are for whoever assembles the adapters that run a build: `workers` for the
/// one that dispatches actions, `timeout` and `strategy` for the one that runs them. None is
/// part of an action key: they decide how an action is run, never which action it is. The one
/// thing the Plan phase reads is the variables `strategy` pins, to refuse a target that
/// declares one.
///
/// The default is what each setting is when a manifest leaves it out: one worker per core, ten
/// minutes, and the input sandbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Execution {
    /// How many actions run at once.
    pub workers: Workers,
    /// How long one action may run.
    pub timeout: Timeout,
    /// How strongly actions are isolated.
    pub strategy: Isolation,
}
