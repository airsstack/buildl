//! Running one action as a process: the first two tiers of isolation.
//!
//! Its own module because starting a process, waiting for it and reading what it left behind
//! are things only this crate may do: the pipeline hands over an action whose command and paths
//! are settled, through `buildl_core::ExecStrategy`, and learns how the run ended and what
//! stands at each output path.
//!
//! Responsibilities:
//! - [`Bare`] — the implementation of `buildl_core::ExecStrategy` that runs an action in the
//!   workspace itself.
//! - [`InputSandbox`] and [`ExecDir`] — the implementation that runs an action in a directory of
//!   its own outside the workspace, holding copies of only what the action declared.
//! - [`Local`] and [`LocalDir`] — the one strategy the real ports name, which runs an action
//!   under whichever of the two the workspace chose.
//!
//! Non-responsibilities: confining an action beyond that, and judging what it left. The first
//! tier settles how a command is started and nothing about what the command may reach; the
//! second keeps an undeclared workspace file out of the directory the command runs in, not out
//! of its reach by an absolute path. A missing output, a link and an empty directory are
//! reported as they are found; what each means for the build is the pipeline's to decide. Files
//! are stored through whatever `buildl_core::ContentStore` the module is handed.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to
//! unit-test.

mod bare;
mod capture;
mod local;
#[cfg(test)]
mod memory_store;
mod outcome;
mod outputs;
mod run;
mod sandbox;

pub use bare::Bare;
pub use local::{Local, LocalDir};
pub use sandbox::{ExecDir, InputSandbox};
