//! The two passes over the build files: one without discovery results, one with them.
//!
//! Its own file because every command that reads discovery results loads twice the same way, and
//! the second load is held to one rule: it may not declare different discovery targets than the
//! first, since a discovery target that depended on a discovery result would have no result to
//! start from.
//!
//! Responsibilities: [`load_twice`], [`second_pass`], and the labels of the discovery targets a
//! load declared.
//!
//! Non-responsibilities: reading the results, which `results` does; running a discovery target,
//! which a build does; and evaluating build files, which Load does.

use core::cmp::min_by;

use crate::error::{Error, Result};
use crate::load::{declaration_order, load};
use crate::ports::{DeclarationSource, Dispatcher};
use crate::types::{Declaration, Declared, Discovered, EntryName, Label, Loaded, TargetRole};

/// The label `declaration` declares when it declares a discovery target.
fn discovery_target(declaration: &Declaration) -> Option<&Label> {
    match declaration.item() {
        Declared::Target(target) if target.role == TargetRole::Discovery => Some(&target.label),
        _ => None,
    }
}

/// The labels of the discovery targets `declarations` hold, in the declarations' order.
pub(crate) fn discovery_labels(declarations: &[Declaration]) -> Vec<Label> {
    declarations
        .iter()
        .filter_map(discovery_target)
        .cloned()
        .collect()
}

/// The error naming the first discovery target the two sorted lists disagree on, if they do.
///
/// Only the discovery targets are compared, each whole: its label, every field, the build file
/// and whose code declared it. Both lists agree before the first differing index, so the lesser
/// of the two declarations there, in the order Load sorts by, is the one the other list lacks.
fn discovery_change(pass_one: &[Declaration], pass_two: &[Declaration]) -> Option<Error> {
    fn discovery(declarations: &[Declaration]) -> Vec<&Declaration> {
        declarations
            .iter()
            .filter(|declaration| discovery_target(declaration).is_some())
            .collect()
    }
    let (one, two) = (discovery(pass_one), discovery(pass_two));
    (0..one.len().max(two.len())).find_map(|index| {
        let (a, b) = (one.get(index).copied(), two.get(index).copied());
        if a == b {
            return None;
        }
        let lacking = match (a, b) {
            (Some(a), Some(b)) => min_by(a, b, |x, y| declaration_order(x, y)),
            (Some(only), None) | (None, Some(only)) => only,
            (None, None) => return None,
        };
        Some(Error::DiscoveryChanged {
            label: discovery_target(lacking)?.clone(),
            provenance: lacking.provenance().clone(),
        })
    })
}

/// Loads the workspace with `discovered` and returns its declarations, once its discovery targets
/// are the ones `pass_one` declared.
///
/// # Errors
///
/// Returns whatever the load returns, and [`Error::DiscoveryChanged`] when a discovery target was
/// added, removed or changed since `pass_one`.
pub(crate) fn second_pass<S: DeclarationSource, D: Dispatcher>(
    source: &S,
    dispatcher: &mut D,
    entry: &EntryName,
    pass_one: &[Declaration],
    discovered: &Discovered,
) -> Result<Vec<Declaration>> {
    let pass_two = load(source, dispatcher, entry, discovered)?;
    discovery_change(pass_one, &pass_two).map_or(Ok(pass_two), Err)
}

/// Loads the workspace without discovery results and, when that declares a discovery target,
/// again with the results `results_for` reads for the labels of those targets.
///
/// With no discovery target declared this is one load and nothing is read. Otherwise the
/// declarations are those of the second load, and `missing` is what `results_for` reports.
///
/// # Errors
///
/// Returns whatever either load returns, and [`Error::DiscoveryChanged`] when the second load
/// declares different discovery targets than the first.
pub(crate) fn load_twice<S, D, R>(
    source: &S,
    dispatcher: &mut D,
    entry: &EntryName,
    results_for: R,
) -> Result<Loaded>
where
    S: DeclarationSource,
    D: Dispatcher,
    R: FnOnce(&[Label]) -> (Discovered, Vec<Label>),
{
    let pass_one = load(source, dispatcher, entry, &Discovered::empty())?;
    let labels = discovery_labels(&pass_one);
    if labels.is_empty() {
        return Ok(Loaded {
            declarations: pass_one,
            missing: Vec::new(),
        });
    }
    let (discovered, missing) = results_for(&labels);
    Ok(Loaded {
        declarations: second_pass(source, dispatcher, entry, &pass_one, &discovered)?,
        missing,
    })
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::path::PathBuf;

    use super::{discovery_change, discovery_labels};
    use crate::error::Error;
    use crate::types::{
        Action, Argument, Command, Declaration, Declared, Directory, Freshness, Label,
        NetworkAccess, Origin, OutputSource, Provenance, Setting, SettingName, SettingValue,
        Target, TargetRole,
    };

    fn file_in(directory: &str) -> Provenance {
        let directory = Directory::parse(directory).unwrap();
        let file = if directory.is_root() {
            PathBuf::from("build.lua")
        } else {
            PathBuf::from(format!("{directory}/build.lua"))
        };
        Provenance::new(file, directory)
    }

    /// A target of `role` named `name` in `directory`, running `program`.
    fn target(directory: &str, name: &str, role: TargetRole, program: &str) -> Declaration {
        let label = Label::parse(&format!("//{directory}:{name}")).unwrap();
        Declaration::new(
            file_in(directory),
            Origin::BuildFile,
            Declared::Target(Target {
                label,
                role,
                action: Action::Run(Command::new(vec![Argument::parse(program).unwrap()]).unwrap()),
                inputs: Vec::new(),
                deps: Vec::new(),
                outputs: Vec::new(),
                env: Vec::new(),
                network: NetworkAccess::Sealed,
                output_source: OutputSource::Command,
                freshness: Freshness::Cached,
            }),
        )
    }

    fn discovery(directory: &str, name: &str, program: &str) -> Declaration {
        target(directory, name, TargetRole::Discovery, program)
    }

    fn built(directory: &str, name: &str) -> Declaration {
        target(directory, name, TargetRole::Build, "cc")
    }

    fn setting() -> Declaration {
        Declaration::new(
            file_in(""),
            Origin::BuildFile,
            Declared::Setting(Setting {
                name: SettingName::parse("mode").unwrap(),
                default: SettingValue::parse("").unwrap(),
            }),
        )
    }

    fn named(error: &Error) -> (String, String) {
        match error {
            Error::DiscoveryChanged { label, provenance } => {
                (label.to_string(), provenance.to_string())
            }
            other => unreachable!("expected DiscoveryChanged, got {other:?}"),
        }
    }

    #[test]
    fn only_discovery_targets_are_listed_in_the_order_given() {
        let declarations = [
            built("", "app"),
            discovery("", "pkgs", "go"),
            setting(),
            discovery("svc", "mods", "go"),
        ];
        let labels = discovery_labels(&declarations);
        assert_eq!(
            labels,
            [
                Label::parse("//:pkgs").unwrap(),
                Label::parse("//svc:mods").unwrap()
            ]
        );
    }

    #[test]
    fn no_discovery_target_lists_no_label() {
        assert_eq!(
            discovery_labels(&[built("", "app"), setting()]),
            Vec::<Label>::new()
        );
        assert_eq!(discovery_labels(&[]), Vec::<Label>::new());
    }

    #[test]
    fn the_same_discovery_targets_do_not_differ_whatever_else_is_declared() {
        let one = [discovery("", "pkgs", "go")];
        let two = [
            built("", "a"),
            discovery("", "pkgs", "go"),
            built("", "b"),
            setting(),
        ];
        assert!(discovery_change(&one, &two).is_none());
        assert!(discovery_change(&[], &[built("", "a")]).is_none());
    }

    #[test]
    fn an_added_discovery_target_is_named() {
        let one = [discovery("", "pkgs", "go")];
        let two = [discovery("", "more", "go"), discovery("", "pkgs", "go")];
        let error = discovery_change(&one, &two).unwrap();
        assert_eq!(
            named(&error),
            ("//:more".to_owned(), "build.lua".to_owned())
        );
    }

    #[test]
    fn a_removed_discovery_target_is_named() {
        let one = [discovery("", "pkgs", "go"), discovery("svc", "mods", "go")];
        let two = [discovery("", "pkgs", "go")];
        let error = discovery_change(&one, &two).unwrap();
        assert_eq!(
            named(&error),
            ("//svc:mods".to_owned(), "svc/build.lua".to_owned())
        );
    }

    #[test]
    fn a_changed_discovery_target_is_named_once() {
        let one = [discovery("", "pkgs", "go"), discovery("", "zzz", "go")];
        let two = [discovery("", "pkgs", "go"), discovery("", "zzz", "cargo")];
        let error = discovery_change(&one, &two).unwrap();
        assert_eq!(named(&error).0, "//:zzz");
    }

    #[test]
    fn a_discovery_target_that_became_a_build_target_is_a_change() {
        let one = [discovery("", "pkgs", "go")];
        let two = [target("", "pkgs", TargetRole::Build, "go")];
        let error = discovery_change(&one, &two).unwrap();
        assert_eq!(named(&error).0, "//:pkgs");
    }
}
