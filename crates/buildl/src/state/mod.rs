//! The state directory of a workspace: where its files are, who may use it, and how a file in
//! it is written and removed.
//!
//! Its own module because several adapters keep a file in the state directory and no adapter
//! may name another. What each of them would otherwise write out for itself lives here once.
//! This module implements no port, and names no other module of this crate.
//!
//! Responsibilities:
//! - [`StateLayout`] — the path of each file of a state directory.
//! - [`StateLock`] and [`LockError`] — one process using a state directory at a time.
//! - [`Scratch`] and [`Owner`] — the directory actions run in, outside the workspace.
//! - [`name_of`] — the name a workspace root or a label gives a directory or file of its own.
//! - [`write_whole`] and [`Access`] — a file written so that a reader finds it whole.
//! - `clear_path` and `clear_found` — removing whatever stands at a path.
//! - [`Cleaned`] and `clear_out` — emptying the output directory.
//!
//! Non-responsibilities: what any of those files holds. Each file's format belongs to the module
//! that keeps it.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to
//! unit-test.

mod clear;
mod clear_out;
mod layout;
mod lock;
mod name;
mod scratch;
mod write;

pub(crate) use clear::{clear_found, clear_path};
pub use clear_out::Cleaned;
pub(crate) use clear_out::clear_out;
pub use layout::StateLayout;
pub use lock::{LockError, StateLock};
pub use name::name_of;
pub use scratch::{Owner, Scratch};
pub(crate) use write::STAGED_PREFIX;
pub use write::{Access, write_whole};
