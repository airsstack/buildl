//! Checked-in workspaces driven through the facade, from the outside.
//!
//! Everything here uses only the public API of `buildl`, as the binary and an embedder do: a
//! workspace is opened with `Workspace::open`, and every command runs with real files, real
//! tools and a real state directory. Each test works in a temporary copy of a fixture under
//! `tests/fixtures`, so no test sees what another one did.
//!
//! One test binary, so the harness is shared by every scenario without being compiled once per
//! file.

#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
)]

mod clean_gc;
mod common;
mod facade;
mod failure;
mod identical;
mod incremental;
mod log;
mod named;
mod parallel_load;
mod recovery;
mod sandbox;
mod test;
