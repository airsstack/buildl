//! The programs commands name: finding each one, and what identifies it.
//!
//! Its own directory because a tool is part of what an action is: when the program a command
//! names changes, every target that runs it must be rebuilt, and this module is what says a
//! program changed.
//!
//! Responsibilities:
//!
//! - [`PathTools`] — the content digest of the file a program's name leads to, or nothing when
//!   the name leads to no file.
//!
//! Non-responsibilities: the rule for finding a program, which is the search path's and is
//! shared with whatever runs it; and deciding which programs a build may run.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

mod path_tools;

pub use path_tools::PathTools;
