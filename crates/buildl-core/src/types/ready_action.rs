//! One action, ready to run: everything an execution strategy is handed for one target.
//!
//! Its own file because it is the value that crosses the execution port. A strategy is told the
//! command to run, the variables to run it with, and which workspace-relative paths the command
//! reads and writes; it learns nothing else about the build.
//!
//! Responsibilities: [`ReadyAction`].
//!
//! Non-responsibilities: expanding the command's and the description's placeholders and laying
//! out the output paths, which the phase that executes does before it builds this value; and
//! running or showing anything, which are ports'.
//!
//! This file holds only a type definition, so it carries no logic to unit-test.

use std::collections::BTreeMap;

use crate::types::{
    Command, EnvName, EnvValue, ExpandedDescription, Label, NetworkAccess, OutputPath, SourcePath,
};

/// One target's action as an execution strategy receives it.
///
/// Every path is relative to the workspace root, and the command already names those same
/// paths: no placeholder is left in it, nor in the description.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadyAction {
    /// The target, for the strategy's own diagnostics.
    pub label: Label,
    /// The command to run, as an argument vector with every placeholder expanded.
    pub command: Command,
    /// The text to show for the action, expanded as the command is; `None` for a target whose
    /// rule has no description and for one with an inline command. Nothing is made up in its
    /// place: whoever shows the action chooses what to show instead. It is display text and no
    /// part of the action's key.
    pub description: Option<ExpandedDescription>,
    /// The environment the command runs with: each variable the target reads that has a value.
    pub env: BTreeMap<EnvName, EnvValue>,
    /// The workspace files the command reads, in the order the target wrote them.
    pub inputs: Vec<SourcePath>,
    /// Every output of every dependency, in the order the dependencies were written, each
    /// dependency's outputs in the order it declares them.
    pub deps: Vec<OutputPath>,
    /// Where the command must leave each output the target declares, in declared order: at
    /// each path a regular file, or a directory of regular files.
    pub outputs: Vec<OutputPath>,
    /// Whether the command may reach the network.
    pub network: NetworkAccess,
}
