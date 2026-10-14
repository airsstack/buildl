//! The machine actions run on, as buildl itself sees it.
//!
//! Its own directory because the platform and the environment are ambient: nothing else in this
//! crate reads either, so what an action key says about the machine has one source.
//!
//! Responsibilities:
//!
//! - [`LocalHost`] — the platform this binary was compiled for, and the value of an environment
//!   variable at the moment it is asked for.
//!
//! Non-responsibilities: the ceiling on which variables a build may read, which is the
//! manifest's to state and the pipeline's to hold a build to.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

mod local_host;

pub use local_host::LocalHost;
