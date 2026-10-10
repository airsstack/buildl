//! The index: every declaration found by the name it was declared under.
//!
//! Its own file because it is the first thing this phase does and the only place a name can
//! collide: targets, rules and aliases share one namespace of labels, and build settings have
//! their own.
//!
//! Responsibilities: [`Index`], [`Entry`], [`Item`], and [`index`], which refuses a name
//! declared more than once.
//!
//! Non-responsibilities: what a declaration refers to. An index says what each name is, not
//! whether the names it mentions exist.

use std::collections::BTreeMap;

use crate::error::{DeclaredKind, Error, Result};
use crate::types::{
    Alias, Declaration, Declared, Label, Provenance, Rule, SettingName, SettingValue, Target,
};

/// What a label was declared as, holding the declaration itself.
///
/// The four kinds a build file can declare, less the setting: a setting is named in its own
/// namespace, so no label ever holds one. `DeclaredKind` is the kind alone, for an error to
/// report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Item {
    /// A build or test target.
    Target(Target),
    /// A rule.
    Rule(Rule),
    /// An alias.
    Alias(Alias),
}

impl Item {
    /// Which kind of declaration this is.
    pub(crate) const fn kind(&self) -> DeclaredKind {
        match self {
            Self::Target(_) => DeclaredKind::Target,
            Self::Rule(_) => DeclaredKind::Rule,
            Self::Alias(_) => DeclaredKind::Alias,
        }
    }
}

/// One labelled declaration and the build file that made it.
///
/// A `Declaration` narrowed to what a label can name, with its parts open to this phase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    /// The build file.
    pub(crate) provenance: Provenance,
    /// What was declared.
    pub(crate) item: Item,
}

/// Every declaration of a workspace, each under the one name it was declared with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Index {
    /// Targets, rules and aliases, by label.
    pub(crate) labels: BTreeMap<Label, Entry>,
    /// Build settings and their defaults, by name.
    pub(crate) settings: BTreeMap<SettingName, SettingValue>,
}

/// Indexes `declarations` by name.
///
/// The result does not depend on the order of `declarations`: they are sorted first, and both
/// maps iterate in the order of their keys.
///
/// # Errors
///
/// Returns [`Error::DuplicateLabel`] for the lowest label declared more than once, in any mix
/// of target, rule and alias. With no such label, returns [`Error::DuplicateSetting`] for the
/// lowest setting name declared more than once. Either names the build file of every
/// declaration of that name.
pub(crate) fn index(mut declarations: Vec<Declaration>) -> Result<Index> {
    declarations.sort();

    let mut labels: BTreeMap<Label, Vec<Entry>> = BTreeMap::new();
    let mut settings: BTreeMap<SettingName, Vec<(Provenance, SettingValue)>> = BTreeMap::new();
    for declaration in declarations {
        let (provenance, declared) = declaration.into_parts();
        let (label, item) = match declared {
            Declared::Target(target) => (target.label.clone(), Item::Target(target)),
            Declared::Rule(rule) => (rule.label.clone(), Item::Rule(rule)),
            Declared::Alias(alias) => (alias.label.clone(), Item::Alias(alias)),
            Declared::Setting(setting) => {
                settings
                    .entry(setting.name)
                    .or_default()
                    .push((provenance, setting.default));
                continue;
            }
        };
        labels
            .entry(label)
            .or_default()
            .push(Entry { provenance, item });
    }

    Ok(Index {
        labels: unique(
            labels,
            |entry| &entry.provenance,
            |label, sites| Error::DuplicateLabel { label, sites },
        )?,
        settings: unique(
            settings,
            |(provenance, _)| provenance,
            |name, sites| Error::DuplicateSetting { name, sites },
        )?
        .into_iter()
        .map(|(name, (_, default))| (name, default))
        .collect(),
    })
}

/// Keeps the one declaration of each name, or fails on the lowest name declared more than once.
fn unique<K: Ord, V>(
    grouped: BTreeMap<K, Vec<V>>,
    site: impl Fn(&V) -> &Provenance,
    duplicate: impl FnOnce(K, Vec<Provenance>) -> Error,
) -> Result<BTreeMap<K, V>> {
    let mut single = BTreeMap::new();
    for (name, mut declared) in grouped {
        if declared.len() > 1 {
            let sites = declared.iter().map(|entry| site(entry).clone()).collect();
            return Err(duplicate(name, sites));
        }
        if let Some(only) = declared.pop() {
            single.insert(name, only);
        }
    }
    Ok(single)
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::path::Path;

    use super::{Item, index};
    use crate::error::{DeclaredKind, Error};
    use crate::resolve::fixtures::{
        alias, declared_in, label, rule, setting, setting_item, target, target_item,
    };
    use crate::types::{Provenance, SettingName};

    fn files(sites: &[Provenance]) -> Vec<&Path> {
        sites.iter().map(Provenance::file).collect()
    }

    #[test]
    fn every_declaration_is_found_under_its_name() {
        let found = index(vec![
            setting("test_filter"),
            alias("//:default", "//:app"),
            rule("//tools:cc", &["cc"]),
            target("//:app", &[]),
        ])
        .unwrap();
        let kinds: Vec<(String, DeclaredKind)> = found
            .labels
            .iter()
            .map(|(label, entry)| (label.to_string(), entry.item.kind()))
            .collect();
        assert_eq!(
            kinds,
            [
                ("//:app".to_owned(), DeclaredKind::Target),
                ("//:default".to_owned(), DeclaredKind::Alias),
                ("//tools:cc".to_owned(), DeclaredKind::Rule),
            ]
        );
        assert_eq!(
            found.labels[&label("//tools:cc")].provenance.file(),
            Path::new("tools/build.lua")
        );
        assert!(matches!(
            found.labels[&label("//:app")].item,
            Item::Target(_)
        ));
        let default = &found.settings[&SettingName::parse("test_filter").unwrap()];
        assert_eq!(default.as_str(), "");
    }

    #[test]
    fn nothing_declared_indexes_to_nothing() {
        let found = index(Vec::new()).unwrap();
        assert!(found.labels.is_empty());
        assert!(found.settings.is_empty());
    }

    #[test]
    fn a_label_declared_twice_in_one_file_names_that_file_twice() {
        let err = index(vec![target("//:a", &[]), target("//:a", &[])]).unwrap_err();
        match err {
            Error::DuplicateLabel {
                label: found,
                sites,
            } => {
                assert_eq!(found, label("//:a"));
                assert_eq!(
                    files(&sites),
                    [Path::new("build.lua"), Path::new("build.lua")]
                );
            }
            other => unreachable!("expected DuplicateLabel, got {other:?}"),
        }
    }

    #[test]
    fn a_label_declared_as_two_kinds_is_a_duplicate() {
        let err = index(vec![target("//:cc", &[]), rule("//:cc", &["cc"])]).unwrap_err();
        assert!(matches!(err, Error::DuplicateLabel { .. }), "{err:?}");
        let err = index(vec![alias("//:x", "//:y"), rule("//:x", &["cc"])]).unwrap_err();
        assert!(matches!(err, Error::DuplicateLabel { .. }), "{err:?}");
    }

    #[test]
    fn every_site_of_a_duplicate_is_named_in_the_declarations_own_order() {
        // Hand-built: Load cannot deliver one label from two files, but the index takes any
        // declarations and reports whatever sites they hold.
        let err = index(vec![
            declared_in("lib", target_item("//:a", |_| {})),
            declared_in("", target_item("//:a", |_| {})),
            declared_in("app", target_item("//:a", |_| {})),
        ])
        .unwrap_err();
        match err {
            Error::DuplicateLabel { sites, .. } => assert_eq!(
                files(&sites),
                [
                    Path::new("app/build.lua"),
                    Path::new("build.lua"),
                    Path::new("lib/build.lua"),
                ]
            ),
            other => unreachable!("expected DuplicateLabel, got {other:?}"),
        }
    }

    #[test]
    fn the_lowest_duplicated_label_is_the_one_reported() {
        let declarations = vec![
            target("//:z", &[]),
            target("//:z", &[]),
            target("//:b", &[]),
            target("//:b", &[]),
            target("//:a", &[]),
        ];
        match index(declarations).unwrap_err() {
            Error::DuplicateLabel { label: found, .. } => assert_eq!(found, label("//:b")),
            other => unreachable!("expected DuplicateLabel, got {other:?}"),
        }
    }

    #[test]
    fn a_setting_declared_in_two_files_names_both() {
        let err = index(vec![
            declared_in("lib", setting_item("test_filter")),
            declared_in("", setting_item("test_filter")),
        ])
        .unwrap_err();
        match err {
            Error::DuplicateSetting { name, sites } => {
                assert_eq!(name.as_str(), "test_filter");
                assert_eq!(
                    files(&sites),
                    [Path::new("build.lua"), Path::new("lib/build.lua")]
                );
            }
            other => unreachable!("expected DuplicateSetting, got {other:?}"),
        }
    }

    #[test]
    fn a_duplicated_label_is_reported_before_a_duplicated_setting() {
        let err = index(vec![
            setting("s"),
            setting("s"),
            target("//:a", &[]),
            target("//:a", &[]),
        ])
        .unwrap_err();
        assert!(matches!(err, Error::DuplicateLabel { .. }), "{err:?}");
    }

    #[test]
    fn a_setting_and_a_label_may_share_a_name() {
        let found = index(vec![setting("app"), target("//:app", &[])]).unwrap();
        assert_eq!(found.labels.len(), 1);
        assert_eq!(found.settings.len(), 1);
    }
}
