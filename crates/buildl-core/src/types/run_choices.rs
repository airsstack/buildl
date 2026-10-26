//! The choices a command makes about how one run of targets goes.
//!
//! Its own file because the phase that executes needs all of them at once and none is in the
//! workspace: they are made per command, so they travel together as one value rather than as
//! four arguments.
//!
//! Responsibilities: [`RunChoices`].
//!
//! Non-responsibilities: applying the choices, which the phase that executes does.
//!
//! This file holds only a type definition, so it carries no logic to unit-test.

use crate::types::{LiveOutput, OnFailure, RemotePolicy, Rerun};

/// What a run does once a target fails, whether it shows one action as it prints, whether tests
/// run again, and how it uses the remote cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunChoices {
    /// What a failed action does to the rest of the run.
    pub on_failure: OnFailure,
    /// Whether one running action at a time is shown as it prints.
    pub live: LiveOutput,
    /// Whether the run's tests run again whatever their rows say.
    pub rerun: Rerun,
    /// How the run uses the remote cache.
    pub remote: RemotePolicy,
}
