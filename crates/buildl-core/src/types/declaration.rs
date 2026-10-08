//! What a build file declares, once every value in it has been validated.
//!
//! Its own file because a declaration is the value Load hands to the next phase: everything in it
//! is already a domain type, and nothing in it depends on the order the build file made its calls
//! in.
//!
//! Responsibilities: [`Declaration`], the four kinds in [`Declared`] — [`Target`], [`Rule`],
//! [`Alias`] and [`Setting`] — a target's [`Action`], and the three two-state flags
//! [`TargetRole`], [`NetworkAccess`] and [`Freshness`].
//!
//! Non-responsibilities: relationships between declarations. Whether a label is declared twice,
//! whether a dependency exists, and whether a rule reference names a rule all need every build
//! file, so they belong to the phase that builds the graph.

use serde::{Deserialize, Serialize};

use crate::types::{
    Command, Description, EnvName, Label, OutputName, Provenance, SettingName, SettingValue,
    SourcePath,
};

/// Whether a target is built for its outputs or run as a test.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum TargetRole {
    /// Declared with `b.target`.
    Build,
    /// Declared with `b.test`.
    Test,
}

/// Whether a target's action may reach the network.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum NetworkAccess {
    /// The default: no network.
    Sealed,
    /// `network = true`: a declared, plan-visible exception.
    Declared,
}

/// Whether a target's result may be reused from the cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Freshness {
    /// The default: reused while its action key is unchanged.
    Cached,
    /// `always = true`: run on every build.
    Always,
}

/// How a target produces its outputs.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Action {
    /// `rule = "..."`: run the named rule's command.
    UseRule(Label),
    /// `run = { ... }`: run this command.
    Run(Command),
}

/// A buildable or testable target, declared with `b.target` or `b.test`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Target {
    /// The target's name, in the directory that declared it.
    pub label: Label,
    /// Whether it builds or tests.
    pub role: TargetRole,
    /// How it produces its outputs.
    pub action: Action,
    /// The workspace files it reads.
    pub inputs: Vec<SourcePath>,
    /// The targets it depends on.
    pub deps: Vec<Label>,
    /// The files it produces, relative to its output directory.
    pub outputs: Vec<OutputName>,
    /// The environment variables it reads.
    pub env: Vec<EnvName>,
    /// Whether it may reach the network.
    pub network: NetworkAccess,
    /// Whether its result may be reused.
    pub freshness: Freshness,
}

/// A reusable command, declared with `b.rule`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Rule {
    /// The rule's name, in the directory that declared it.
    pub label: Label,
    /// The command a target using the rule runs.
    pub run: Command,
    /// The status-line text shown while it runs.
    pub description: Option<Description>,
}

/// A second name for a target, declared with `b.alias`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Alias {
    /// The alias's name, in the directory that declared it.
    pub label: Label,
    /// The target it names.
    pub target: Label,
}

/// A build setting, declared with `b.option`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Setting {
    /// The setting's workspace-wide name.
    pub name: SettingName,
    /// The value used when no `--set` overrides it.
    pub default: SettingValue,
}

/// The four things a build file can declare.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Declared {
    /// A target.
    Target(Target),
    /// A rule.
    Rule(Rule),
    /// An alias.
    Alias(Alias),
    /// A build setting.
    Setting(Setting),
}

impl Declared {
    /// The declared name: the target name of a target, rule or alias, or the setting's name.
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Target(target) => target.label.name().as_str(),
            Self::Rule(rule) => rule.label.name().as_str(),
            Self::Alias(alias) => alias.label.name().as_str(),
            Self::Setting(setting) => setting.name.as_str(),
        }
    }
}

/// One validated declaration and the build file that made it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Declaration {
    provenance: Provenance,
    item: Declared,
}

impl Declaration {
    /// Records `item` as declared by the build file `provenance` names.
    #[must_use]
    pub const fn new(provenance: Provenance, item: Declared) -> Self {
        Self { provenance, item }
    }

    /// The build file that made the declaration.
    #[must_use]
    pub const fn provenance(&self) -> &Provenance {
        &self.provenance
    }

    /// What was declared.
    #[must_use]
    pub const fn item(&self) -> &Declared {
        &self.item
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::path::PathBuf;

    use super::{
        Action, Alias, Declaration, Declared, Freshness, NetworkAccess, Rule, Setting, Target,
        TargetRole,
    };
    use crate::types::{
        Argument, Command, Description, Directory, Label, OutputName, Provenance, SettingName,
        SettingValue,
    };

    fn label(raw: &str) -> Label {
        Label::parse(raw).unwrap()
    }

    fn command(raw: &[&str]) -> Command {
        Command::new(raw.iter().map(|a| Argument::parse(*a).unwrap()).collect()).unwrap()
    }

    fn target() -> Target {
        Target {
            label: label("//:app"),
            role: TargetRole::Build,
            action: Action::Run(command(&["cc", "$deps", "-o", "$out"])),
            inputs: Vec::new(),
            deps: vec![label("//:main.o")],
            outputs: vec![OutputName::parse("app").unwrap()],
            env: Vec::new(),
            network: NetworkAccess::Sealed,
            freshness: Freshness::Cached,
        }
    }

    #[test]
    fn name_is_the_target_name_or_the_setting_name() {
        assert_eq!(Declared::Target(target()).name(), "app");
        let rule = Rule {
            label: label("//:cc"),
            run: command(&["cc"]),
            description: None,
        };
        assert_eq!(Declared::Rule(rule).name(), "cc");
        let alias = Alias {
            label: label("//:default"),
            target: label("//:app"),
        };
        assert_eq!(Declared::Alias(alias).name(), "default");
        let setting = Setting {
            name: SettingName::parse("test_filter").unwrap(),
            default: SettingValue::parse("").unwrap(),
        };
        assert_eq!(Declared::Setting(setting).name(), "test_filter");
    }

    #[test]
    fn exposes_its_provenance_and_item() {
        let provenance = Provenance::new(PathBuf::from("build.lua"), Directory::root());
        let declaration = Declaration::new(provenance.clone(), Declared::Target(target()));
        assert_eq!(declaration.provenance(), &provenance);
        assert_eq!(declaration.item(), &Declared::Target(target()));
    }

    #[test]
    fn serializes_as_exact_canonical_json_and_round_trips() {
        let declaration = Declaration::new(
            Provenance::new(PathBuf::from("build.lua"), Directory::root()),
            Declared::Target(target()),
        );
        let json = crate::json::canonical::to_string(&declaration).unwrap();
        assert_eq!(
            json,
            concat!(
                r#"{"item":{"Target":{"action":{"Run":["cc","$deps","-o","$out"]},"#,
                r#""deps":["//:main.o"],"env":[],"freshness":"Cached","inputs":[],"#,
                r#""label":"//:app","network":"Sealed","outputs":["app"],"role":"Build"}},"#,
                r#""provenance":{"directory":"","file":"build.lua"}}"#
            )
        );
        assert_eq!(
            serde_json::from_str::<Declaration>(&json).unwrap(),
            declaration
        );

        let mut variant = target();
        variant.role = TargetRole::Test;
        variant.action = Action::UseRule(label("//:cc"));
        variant.network = NetworkAccess::Declared;
        variant.freshness = Freshness::Always;
        let items = [
            Declared::Target(variant),
            Declared::Rule(Rule {
                label: label("//:cc"),
                run: command(&["cc"]),
                description: Some(Description::parse("compile").unwrap()),
            }),
            Declared::Rule(Rule {
                label: label("//:ld"),
                run: command(&["ld"]),
                description: None,
            }),
            Declared::Alias(Alias {
                label: label("//:default"),
                target: label("//:app"),
            }),
            Declared::Setting(Setting {
                name: SettingName::parse("test_filter").unwrap(),
                default: SettingValue::parse("").unwrap(),
            }),
        ];
        for item in items {
            let declaration = Declaration::new(
                Provenance::new(PathBuf::from("build.lua"), Directory::root()),
                item,
            );
            let json = crate::json::canonical::to_string(&declaration).unwrap();
            assert_eq!(
                serde_json::from_str::<Declaration>(&json).unwrap(),
                declaration
            );
        }
    }

    #[test]
    fn flags_order_their_default_first() {
        assert!(TargetRole::Build < TargetRole::Test);
        assert!(NetworkAccess::Sealed < NetworkAccess::Declared);
        assert!(Freshness::Cached < Freshness::Always);
    }
}
