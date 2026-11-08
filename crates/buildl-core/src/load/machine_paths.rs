//! The refusal of a declaration, or a discovery output, that holds one of this machine's folders.
//!
//! Its own file because it is a check over what Load returned that belongs to no single load: the
//! commands are searched after every load, the discovery outputs when the second pass reads them,
//! and both by the same rule, [`MachineFolders::find_in`].
//!
//! Responsibilities: [`refuse_commands`] and [`refuse_discovered`].
//!
//! Non-responsibilities: learning the folders, which the host does; the match rule, which
//! [`MachineFolders`] holds; and the check of a `key` variable's value, which Plan makes as it
//! gathers the value.

use crate::error::{DeclarationSite, Error, Result};
use crate::types::{
    Action, Command, Declaration, Declared, Discovered, Label, MachineFolders, Target, TargetRole,
};

/// The site of `declaration`, labelled `label`.
fn site(declaration: &Declaration, label: &Label) -> DeclarationSite {
    DeclarationSite {
        label: label.clone(),
        provenance: declaration.provenance().clone(),
        origin: declaration.origin().clone(),
    }
}

/// The longest of `folders` that a word of `command` holds, placeholders unexpanded.
fn held_by<'f>(command: &Command, folders: &'f MachineFolders) -> Option<&'f str> {
    command
        .arguments()
        .iter()
        .find_map(|word| folders.find_in(word.as_str()))
}

/// Refuses the first declaration, in the order given, whose command holds one of `folders`.
///
/// A target's own command and a rule's command are searched word by word with their placeholders
/// unexpanded, so a path a setting would bring in is not looked for. A target that names a rule
/// has no command of its own: the rule's declaration is the one searched.
///
/// # Errors
///
/// Returns [`Error::MachinePathInCommand`] naming the target or rule and the folder.
pub(crate) fn refuse_commands(
    declarations: &[Declaration],
    folders: &MachineFolders,
) -> Result<()> {
    for declaration in declarations {
        let (label, command) = match declaration.item() {
            Declared::Target(Target {
                label,
                action: Action::Run(command),
                ..
            }) => (label, command),
            Declared::Rule(rule) => (&rule.label, &rule.run),
            _ => continue,
        };
        if let Some(folder) = held_by(command, folders) {
            return Err(Error::MachinePathInCommand {
                site: Box::new(site(declaration, label)),
                folder: folder.to_owned(),
            });
        }
    }
    Ok(())
}

/// Refuses the first discovery target of `declarations` whose output in `discovered` holds one of
/// `folders`.
///
/// A discovery target with no output in `discovered` is not searched. The output is read as text,
/// with any byte that is not UTF-8 replaced, which no folder contains.
///
/// # Errors
///
/// Returns [`Error::MachinePathInDiscovery`] naming the discovery target and the folder.
pub(crate) fn refuse_discovered(
    declarations: &[Declaration],
    discovered: &Discovered,
    folders: &MachineFolders,
) -> Result<()> {
    for declaration in declarations {
        let Declared::Target(target) = declaration.item() else {
            continue;
        };
        if target.role != TargetRole::Discovery {
            continue;
        }
        let Some(bytes) = discovered.get(&target.label) else {
            continue;
        };
        if let Some(folder) = folders.find_in(&String::from_utf8_lossy(bytes)) {
            return Err(Error::MachinePathInDiscovery {
                site: Box::new(site(declaration, &target.label)),
                folder: folder.to_owned(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use super::{refuse_commands, refuse_discovered};
    use crate::error::Error;
    use crate::types::{
        Action, Argument, Command, Declaration, Declared, Directory, Discovered, Freshness, Label,
        MachineFolders, NetworkAccess, Origin, OutputSource, Provenance, Rule, Target, TargetRole,
    };

    fn folders() -> MachineFolders {
        MachineFolders::new(&["/home/al"], &["/home/al/shop"], &[])
    }

    fn file() -> Provenance {
        Provenance::new(PathBuf::from("build.lua"), Directory::parse("").unwrap())
    }

    fn command(words: &[&str]) -> Command {
        Command::new(words.iter().map(|w| Argument::parse(*w).unwrap()).collect()).unwrap()
    }

    fn target(name: &str, role: TargetRole, words: &[&str]) -> Declaration {
        Declaration::new(
            file(),
            Origin::BuildFile,
            Declared::Target(Target {
                label: Label::parse(&format!("//:{name}")).unwrap(),
                role,
                action: Action::Run(command(words)),
                inputs: Vec::new(),
                deps: Vec::new(),
                outputs: Vec::new(),
                env: BTreeMap::new(),
                network: NetworkAccess::Sealed,
                output_source: OutputSource::Command,
                freshness: Freshness::Cached,
            }),
        )
    }

    fn rule(name: &str, words: &[&str]) -> Declaration {
        Declaration::new(
            file(),
            Origin::BuildFile,
            Declared::Rule(Rule {
                label: Label::parse(&format!("//:{name}")).unwrap(),
                run: command(words),
                description: None,
            }),
        )
    }

    fn discovered(label: &str, bytes: &[u8]) -> Discovered {
        Discovered::new(BTreeMap::from([(
            Label::parse(label).unwrap(),
            bytes.to_vec(),
        )]))
    }

    #[test]
    fn a_command_word_holding_a_folder_is_refused_with_the_longest() {
        let declarations = [target(
            "note",
            TargetRole::Build,
            &["sh", "-c", "cd /home/al/shop && make"],
        )];
        match refuse_commands(&declarations, &folders()).unwrap_err() {
            Error::MachinePathInCommand { site, folder } => {
                assert_eq!(site.to_string(), "build.lua: //:note");
                assert_eq!(folder, "/home/al/shop");
            }
            other => unreachable!("expected MachinePathInCommand, got {other:?}"),
        }
    }

    #[test]
    fn a_rules_command_is_searched_too() {
        let declarations = [rule("compile", &["cc", "-I/x", "/home/al/inc"])];
        assert!(matches!(
            refuse_commands(&declarations, &folders()),
            Err(Error::MachinePathInCommand { .. })
        ));
    }

    #[test]
    fn a_command_that_holds_no_folder_is_allowed() {
        let declarations = [
            target("a", TargetRole::Build, &["cc", "$in", "/home/alice/x"]),
            rule("r", &["cc"]),
        ];
        assert!(refuse_commands(&declarations, &folders()).is_ok());
    }

    #[test]
    fn a_host_that_reports_no_folders_refuses_nothing() {
        let declarations = [target("a", TargetRole::Build, &["cat", "/home/al/x"])];
        assert!(refuse_commands(&declarations, &MachineFolders::default()).is_ok());
    }

    #[test]
    fn a_discovery_output_holding_a_folder_is_refused() {
        let declarations = [target("meta", TargetRole::Discovery, &["cargo"])];
        let output = discovered("//:meta", br#"{"workspace_root":"/home/al/shop"}"#);
        match refuse_discovered(&declarations, &output, &folders()).unwrap_err() {
            Error::MachinePathInDiscovery { site, folder } => {
                assert_eq!(site.to_string(), "build.lua: //:meta");
                assert_eq!(folder, "/home/al/shop");
            }
            other => unreachable!("expected MachinePathInDiscovery, got {other:?}"),
        }
    }

    #[test]
    fn a_relative_a_missing_and_a_build_targets_output_are_allowed() {
        let declarations = [
            target("meta", TargetRole::Discovery, &["cargo"]),
            target("app", TargetRole::Build, &["cc"]),
        ];
        let relative = discovered("//:meta", b"src/main.c");
        assert!(refuse_discovered(&declarations, &relative, &folders()).is_ok());
        assert!(refuse_discovered(&declarations, &Discovered::empty(), &folders()).is_ok());
        let build = discovered("//:app", b"/home/al/x");
        assert!(refuse_discovered(&declarations, &build, &folders()).is_ok());
    }
}
