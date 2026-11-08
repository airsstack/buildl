//! The machine actions run on, as buildl itself sees it.
//!
//! Its own directory because the platform, the environment and the machine's folders are ambient:
//! nothing else in this crate reads them, so what an action key says about the machine has one
//! source.
//!
//! Responsibilities:
//!
//! - [`LocalHost`] — the platform this binary was compiled for, the value of an environment
//!   variable at the moment it is asked for, and the home, workspace and sandbox folders.
//! - [`written_root`] — the workspace root as the shell names it, found from `PWD`.
//!
//! Non-responsibilities: the ceiling on which variables a build may read, which is the
//! manifest's to state and the pipeline's to hold a build to.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

mod local_host;

pub use local_host::{LocalHost, written_root};
