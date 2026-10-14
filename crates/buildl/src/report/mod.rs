//! What a build shows a person, and every rendering of a value into text.
//!
//! Its own directory because showing is the one thing here that is for a person and not for a
//! later build: nothing it writes is read back, and nothing a build stores or returns depends on
//! it. It holds the adapter of the reporting port and every piece of text a command prints, so
//! that a caller formats nothing itself, and a caller with a reporter of its own can print what
//! this crate's does.
//!
//! Responsibilities:
//!
//! - [`Paint`] — whether what is shown is coloured.
//! - [`command_text`] — a command on one line.
//! - [`summary_line`] — the line that closes a build.
//! - [`trailer`] — what closes the report of a failed target.
//! - [`block`] — everything shown for one target once it has its outcome.
//! - [`TerminalReporter`] — the reporter that writes blocks, a status line and the summary to a
//!   stream it is given, with [`Mode`] saying whether a status line is kept.
//! - [`Silent`] — the reporter that shows nothing.
//! - [`plan_text`] — a plan as lines for a person.
//! - [`graph_dot`] — a target graph as Graphviz DOT.
//! - [`check_line`] — what a check of the build files says when they are sound.
//! - [`error_text`] — an error as a command shows it.
//!
//! Non-responsibilities: deciding what is reported and when, which the build does; choosing
//! the stream, the mode and the colour, which the caller does; and the canonical JSON of a
//! graph or a plan, which is the value's own serialized form.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

mod block;
mod dot;
mod message;
mod paint;
mod plan_text;
mod silent;
mod terminal;

pub use block::{block, command_text, summary_line, trailer};
pub use dot::graph_dot;
pub use message::{check_line, error_text};
pub use paint::Paint;
pub use plan_text::plan_text;
pub use silent::Silent;
pub use terminal::{Mode, TerminalReporter};
