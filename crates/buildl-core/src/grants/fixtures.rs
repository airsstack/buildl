//! Values built by hand, shared by this module's unit tests.
//!
//! Its own file because several test modules need the same builders. Compiled for tests only,
//! so it carries no test module of its own.

#![expect(
    clippy::unwrap_used,
    reason = "fixtures unwrap known-valid text; a panic is the intended failure signal"
)]

use crate::types::{Directory, Label, OutputName, OutputPath, Program};

/// A program name from known-valid text.
pub(crate) fn program(raw: &str) -> Program {
    Program::parse(raw).unwrap()
}

/// Where the output `name` of the target `raw` lives in a workspace whose outputs go under
/// `out`.
pub(crate) fn output_path(raw: &str, name: &str) -> OutputPath {
    OutputPath::of(
        &Directory::parse("out").unwrap(),
        &Label::parse(raw).unwrap(),
        &OutputName::parse(name).unwrap(),
    )
}
