//! The `buildl` binary, run as a process in workspaces written for each test.
//!
//! Every test runs the built binary with its standard output and standard error piped, one
//! action at a time, and reads its status as the number a shell sees.

#![expect(
    clippy::unwrap_used,
    reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
)]

mod arguments;
mod common;
mod exit_codes;
mod output;
mod verify;
