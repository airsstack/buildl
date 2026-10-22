//! What decides whether a build may do what it wants, behind the port a build asks through.
//!
//! Its own module because deciding is a matter for the workspace and the person using it, and
//! the pipeline only asks: which adapter answers is chosen where the ports are assembled, so the
//! pipeline never learns how an answer is reached.
//!
//! Responsibilities:
//! - [`ManifestApprover`] — the implementation of `buildl_core::Approver` for a workspace that
//!   leaves approval to its manifest's ceiling.
//! - [`DenyApprover`] — the implementation for a workspace that lets no build run.
//! - [`InteractiveApprover`] — the implementation that asks the person at a [`Console`] about
//!   what is new, and remembers a yes.
//! - [`LocalApprover`] — the one of the three a workspace names, as a single type.
//! - [`Console`] and [`NoTerminal`] — where the interactive approver asks, and a stand-in for
//!   where nobody is.
//! - [`DataDir`] — the user's data directory, or one given.
//! - [`Approvals`] — the approvals remembered for a workspace, in a file in that directory.
//!
//! Non-responsibilities: holding a want to the ceiling, which the plan does before an approver
//! is asked; and acting on a denial, which `buildl-core` does.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to
//! unit-test.

mod approvals;
mod console;
mod data_dir;
mod deny;
mod interactive;
mod local;
mod manifest;

pub use approvals::Approvals;
pub use console::{Console, NoTerminal};
pub use data_dir::DataDir;
pub use deny::DenyApprover;
pub use interactive::InteractiveApprover;
pub use local::LocalApprover;
pub use manifest::ManifestApprover;
