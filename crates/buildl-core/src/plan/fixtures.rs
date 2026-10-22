//! Values built by hand, shared by this phase's unit tests.
//!
//! Its own file because several test modules need the same builders. Compiled for tests only,
//! so it carries no test module of its own.

#![expect(
    clippy::unwrap_used,
    reason = "fixtures unwrap known-valid text; a panic is the intended failure signal"
)]

use std::collections::BTreeMap;

use crate::grants::fixtures::command;
use crate::types::{Digest, NetworkAccess, OutputName, OwnComponents, Platform, Produced};

/// The own components of a target running `cc` on `linux/x86_64` and producing `output`, with
/// nothing else, after `change` is applied.
pub(crate) fn components(output: &str, change: impl FnOnce(&mut OwnComponents)) -> OwnComponents {
    let mut own = OwnComponents {
        command: command(&["cc"]),
        inputs: Vec::new(),
        outputs: vec![OutputName::parse(output).unwrap()],
        env: BTreeMap::new(),
        tool: None,
        settings: BTreeMap::new(),
        network: NetworkAccess::Sealed,
        platform: Platform::parse("linux/x86_64").unwrap(),
    };
    change(&mut own);
    own
}

/// An output named `name` whose content was `content`.
pub(crate) fn produced(name: &str, content: &str) -> Produced {
    Produced {
        name: OutputName::parse(name).unwrap(),
        digest: Digest::of(content.as_bytes()),
    }
}
