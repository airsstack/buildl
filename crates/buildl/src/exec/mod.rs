//! Running one action as a plain process: the first tier of isolation.
//!
//! Its own module because starting a process, waiting for it and reading what it left behind
//! are things only this crate may do: the pipeline hands over an action whose command and paths
//! are settled, through `buildl_core::ExecStrategy`, and learns how the run ended and what
//! stands at each output path.
//!
//! Responsibilities: [`Bare`], the implementation of `buildl_core::ExecStrategy` that runs an
//! action in the workspace itself.
//!
//! Non-responsibilities: confining an action, and judging what it left. This tier settles how
//! a command is started and nothing about what the command may reach. A missing output, a link
//! and an empty directory are reported as they are found; what each means for the build is the
//! pipeline's to decide. Files are stored through whatever `buildl_core::ContentStore` the
//! module is handed.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to
//! unit-test.

mod bare;
mod capture;
#[cfg(test)]
mod memory_store;
mod outputs;

pub use bare::Bare;
