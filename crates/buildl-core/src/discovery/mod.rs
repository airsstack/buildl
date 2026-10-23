//! Discovery: the results of discovery targets, read back for a second load of the build files.
//!
//! Its own directory because several pipeline functions need it and it is not a phase: a command
//! that loads build files twice reads the last result of each discovery target here, loads again
//! with them, and holds the second load to the discovery targets of the first. No phase names it.
//!
//! Responsibilities: `results`, the last result of each discovery target read from the build log
//! and the content store; `load_twice` and `second_pass`, the two loads and the check between
//! them; and `discovery_labels`, the discovery targets a load declared.
//!
//! Non-responsibilities: running a discovery target, which a build does; and evaluating build
//! files, which Load does.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

mod passes;
mod results;

pub(crate) use passes::{discovery_labels, load_twice, second_pass};
pub(crate) use results::results;
