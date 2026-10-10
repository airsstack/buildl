//! Declarations built by hand, shared by this phase's unit tests.
//!
//! Its own file because three test modules need the same builders. Compiled for tests only, so
//! it carries no test module of its own.

#![expect(
    clippy::unwrap_used,
    reason = "fixtures unwrap known-valid text; a panic is the intended failure signal"
)]

use std::path::PathBuf;

use crate::types::{
    Action, Alias, Argument, Command, Declaration, Declared, Directory, Freshness, Label,
    NetworkAccess, OutputName, Provenance, Rule, Setting, SettingName, SettingValue, Target,
    TargetRole,
};

/// A label from known-valid text.
pub(crate) fn label(raw: &str) -> Label {
    Label::parse(raw).unwrap()
}

/// The build file of `directory`.
pub(crate) fn file(directory: &str) -> Provenance {
    let directory = Directory::parse(directory).unwrap();
    let path = if directory.is_root() {
        PathBuf::from("build.lua")
    } else {
        PathBuf::from(format!("{directory}/build.lua"))
    };
    Provenance::new(path, directory)
}

/// A command from known-valid arguments.
pub(crate) fn command(arguments: &[&str]) -> Command {
    Command::new(
        arguments
            .iter()
            .map(|argument| Argument::parse(*argument).unwrap())
            .collect(),
    )
    .unwrap()
}

/// `item`, declared by the build file of `directory`.
pub(crate) fn declared_in(directory: &str, item: Declared) -> Declaration {
    Declaration::new(file(directory), item)
}

/// `item`, declared by the build file of its label's own directory.
fn declared(label: &Label, item: Declared) -> Declaration {
    Declaration::new(file(label.directory().as_str()), item)
}

/// A build target running `cc`, with one output named after it, before `change` is applied.
pub(crate) fn target_item(raw: &str, change: impl FnOnce(&mut Target)) -> Declared {
    let label = label(raw);
    let mut target = Target {
        outputs: vec![OutputName::parse(label.name().as_str()).unwrap()],
        label,
        role: TargetRole::Build,
        action: Action::Run(command(&["cc"])),
        inputs: Vec::new(),
        deps: Vec::new(),
        env: Vec::new(),
        network: NetworkAccess::Sealed,
        freshness: Freshness::Cached,
    };
    change(&mut target);
    Declared::Target(target)
}

/// A build target running `cc` and depending on `deps`, in that order.
pub(crate) fn target(raw: &str, deps: &[&str]) -> Declaration {
    target_with(raw, |target| {
        target.deps = deps.iter().map(|dep| label(dep)).collect();
    })
}

/// A build target running `cc`, after `change` is applied.
pub(crate) fn target_with(raw: &str, change: impl FnOnce(&mut Target)) -> Declaration {
    declared(&label(raw), target_item(raw, change))
}

/// A rule running `run`.
pub(crate) fn rule(raw: &str, run: &[&str]) -> Declaration {
    let label = label(raw);
    declared(
        &label.clone(),
        Declared::Rule(Rule {
            label,
            run: command(run),
            description: None,
        }),
    )
}

/// An alias for `target`.
pub(crate) fn alias(raw: &str, target: &str) -> Declaration {
    let alias_label = label(raw);
    declared(
        &alias_label.clone(),
        Declared::Alias(Alias {
            label: alias_label,
            target: label(target),
        }),
    )
}

/// A build setting with an empty default, declared by the root build file.
pub(crate) fn setting(name: &str) -> Declaration {
    declared_in("", setting_item(name))
}

/// A build setting with an empty default.
pub(crate) fn setting_item(name: &str) -> Declared {
    Declared::Setting(Setting {
        name: SettingName::parse(name).unwrap(),
        default: SettingValue::parse("").unwrap(),
    })
}
