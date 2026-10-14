//! Where a strategy hands what a running command writes, as it writes it.
//!
//! Its own file because it is the one value that crosses the execution port while a command is
//! still running. A name for it keeps the port's signature readable.
//!
//! Responsibilities: [`LiveSink`].
//!
//! This file holds only a type alias, so it carries no logic to unit-test.

use crate::types::Channel;

/// Takes a run of bytes a running command just wrote, with the output it wrote them to.
///
/// Borrowed for one run of one command, and callable from any thread during it: a strategy
/// that reads the two outputs on two threads may call it from both.
pub type LiveSink<'a> = &'a (dyn Fn(Channel, &[u8]) + Sync);
