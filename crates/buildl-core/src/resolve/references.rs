//! What each reference in a declaration names.
//!
//! Its own file because every reference is answered the same way: look the name up in the index,
//! accept the kinds the field may name, and otherwise say whether the name is missing or names
//! the wrong kind of thing.
//!
//! Responsibilities: [`Holder`], and one function per kind of reference — [`dependency`],
//! [`rule`], [`alias_target`] and [`settings`].
//!
//! Non-responsibilities: the order references are checked in, and what is built from the
//! answers. Both belong to the function that walks the index.

use crate::error::{DeclarationField, DeclarationSite, DeclaredKind, Error, Result};
use crate::resolve::index::{Entry, Index, Item};
use crate::resolve::suggest::nearest;
use crate::types::{Command, Label, Provenance, Rule, SettingName};

/// The declaration holding a reference: the one an error about that reference names.
///
/// Borrowed from the index for the length of one check. `DeclarationSite` is the owned form an
/// error carries.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Holder<'a> {
    /// The declaration's label.
    pub(crate) label: &'a Label,
    /// The build file that declared it.
    pub(crate) provenance: &'a Provenance,
}

impl Holder<'_> {
    /// This declaration as an error names it.
    fn site(&self) -> DeclarationSite {
        DeclarationSite {
            label: self.label.clone(),
            provenance: self.provenance.clone(),
        }
    }
}

/// The label a `deps` entry resolves to: the target it names, or the one its alias names.
///
/// The label an alias names is returned as the alias wrote it. Whether that label is a target
/// is the alias's own reference to check, not the dependency's.
///
/// # Errors
///
/// Returns [`Error::WrongReferenceKind`] when `reference` names a rule, and
/// [`Error::UnknownReference`] when it names nothing.
pub(crate) fn dependency<'a>(
    index: &'a Index,
    holder: Holder<'_>,
    reference: &Label,
) -> Result<&'a Label> {
    const FIELD: DeclarationField = DeclarationField::Dep;
    match index.labels.get_key_value(reference) {
        Some((
            label,
            Entry {
                item: Item::Target(_),
                ..
            },
        )) => Ok(label),
        Some((
            _,
            Entry {
                item: Item::Alias(alias),
                ..
            },
        )) => Ok(&alias.target),
        Some((_, entry)) => Err(wrong_kind(holder, FIELD, reference, entry)),
        None => Err(unknown(
            index,
            holder,
            FIELD,
            reference,
            &[DeclaredKind::Target, DeclaredKind::Alias],
        )),
    }
}

/// The rule a target's `rule` reference names.
///
/// # Errors
///
/// Returns [`Error::WrongReferenceKind`] when `reference` names a target or an alias, and
/// [`Error::UnknownReference`] when it names nothing.
pub(crate) fn rule<'a>(
    index: &'a Index,
    holder: Holder<'_>,
    reference: &Label,
) -> Result<&'a Rule> {
    const FIELD: DeclarationField = DeclarationField::Rule;
    match index.labels.get(reference) {
        Some(Entry {
            item: Item::Rule(rule),
            ..
        }) => Ok(rule),
        Some(entry) => Err(wrong_kind(holder, FIELD, reference, entry)),
        None => Err(unknown(
            index,
            holder,
            FIELD,
            reference,
            &[DeclaredKind::Rule],
        )),
    }
}

/// The target an alias names.
///
/// # Errors
///
/// Returns [`Error::WrongReferenceKind`] when `reference` names another alias or a rule, and
/// [`Error::UnknownReference`] when it names nothing.
pub(crate) fn alias_target<'a>(
    index: &'a Index,
    holder: Holder<'_>,
    reference: &Label,
) -> Result<&'a Label> {
    const FIELD: DeclarationField = DeclarationField::Target;
    match index.labels.get_key_value(reference) {
        Some((
            label,
            Entry {
                item: Item::Target(_),
                ..
            },
        )) => Ok(label),
        Some((_, entry)) => Err(wrong_kind(holder, FIELD, reference, entry)),
        None => Err(unknown(
            index,
            holder,
            FIELD,
            reference,
            &[DeclaredKind::Target],
        )),
    }
}

/// Checks that every `$opt:name` in `command` names a declared build setting.
///
/// References are checked in argument order, then in the order they appear in an argument.
///
/// # Errors
///
/// Returns [`Error::UnknownSetting`] for the first reference that names no declared setting.
pub(crate) fn settings(index: &Index, holder: Holder<'_>, command: &Command) -> Result<()> {
    for argument in command.arguments() {
        for name in argument.setting_references() {
            let declared =
                SettingName::parse(name).is_ok_and(|setting| index.settings.contains_key(&setting));
            if !declared {
                return Err(Error::UnknownSetting {
                    site: Box::new(holder.site()),
                    name: name.to_owned(),
                    suggestion: nearest(name, index.settings.keys()).cloned(),
                });
            }
        }
    }
    Ok(())
}

/// The error for a reference to a declaration its field cannot name.
fn wrong_kind(
    holder: Holder<'_>,
    field: DeclarationField,
    reference: &Label,
    entry: &Entry,
) -> Error {
    Error::WrongReferenceKind {
        site: Box::new(holder.site()),
        field,
        reference: reference.clone(),
        found: entry.item.kind(),
        declared_in: entry.provenance.clone(),
    }
}

/// The error for a reference to nothing, offering the nearest label of a kind the field may
/// name.
fn unknown(
    index: &Index,
    holder: Holder<'_>,
    field: DeclarationField,
    reference: &Label,
    may_name: &[DeclaredKind],
) -> Error {
    let candidates = index
        .labels
        .iter()
        .filter(|(_, entry)| may_name.contains(&entry.item.kind()))
        .map(|(label, _)| label);
    Error::UnknownReference {
        site: Box::new(holder.site()),
        field,
        reference: reference.clone(),
        suggestion: nearest(&reference.to_string(), candidates).cloned(),
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::path::Path;

    use super::{Holder, alias_target, dependency, rule, settings};
    use crate::error::{DeclarationField, DeclaredKind, Error};
    use crate::resolve::fixtures::{alias, command, file, label, rule as a_rule, setting, target};
    use crate::resolve::index::{Index, index};
    use crate::types::{Label, Provenance};

    /// `//:app` and `//lib:text` are targets, `//:default` an alias for `//:app`,
    /// `//tools:cc` a rule, and `test_filter` a setting.
    fn workspace() -> Index {
        index(vec![
            target("//:app", &[]),
            target("//lib:text", &[]),
            alias("//:default", "//:app"),
            a_rule("//tools:cc", &["cc"]),
            setting("test_filter"),
        ])
        .unwrap()
    }

    fn held_by<'a>(label: &'a Label, provenance: &'a Provenance) -> Holder<'a> {
        Holder { label, provenance }
    }

    /// Asserts `error` is a wrong-kind error for `reference` in `field`, and returns what was
    /// found and the file that declared it.
    fn wrong_kind(
        error: Error,
        field: DeclarationField,
        reference: &str,
    ) -> (DeclaredKind, String) {
        match error {
            Error::WrongReferenceKind {
                site,
                field: in_field,
                reference: named,
                found,
                declared_in,
            } => {
                assert_eq!(site.label, label("//:holder"));
                assert_eq!(site.provenance.file(), Path::new("build.lua"));
                assert_eq!(in_field, field);
                assert_eq!(named, label(reference));
                (found, declared_in.to_string())
            }
            other => unreachable!("expected WrongReferenceKind, got {other:?}"),
        }
    }

    /// Asserts `error` is an unknown-reference error in `field`, and returns its suggestion.
    fn unknown(error: Error, field: DeclarationField) -> Option<Label> {
        match error {
            Error::UnknownReference {
                site,
                field: in_field,
                suggestion,
                ..
            } => {
                assert_eq!(site.label, label("//:holder"));
                assert_eq!(in_field, field);
                suggestion
            }
            other => unreachable!("expected UnknownReference, got {other:?}"),
        }
    }

    #[test]
    fn a_dependency_names_a_target_or_the_target_of_an_alias() {
        let index = workspace();
        let (holder, root) = (label("//:holder"), file(""));
        let holder = held_by(&holder, &root);
        assert_eq!(
            dependency(&index, holder, &label("//lib:text")).unwrap(),
            &label("//lib:text")
        );
        assert_eq!(
            dependency(&index, holder, &label("//:default")).unwrap(),
            &label("//:app")
        );
    }

    #[test]
    fn a_dependency_on_a_rule_is_the_wrong_kind() {
        let index = workspace();
        let (holder, root) = (label("//:holder"), file(""));
        let error = dependency(&index, held_by(&holder, &root), &label("//tools:cc")).unwrap_err();
        assert_eq!(
            wrong_kind(error, DeclarationField::Dep, "//tools:cc"),
            (DeclaredKind::Rule, "tools/build.lua".to_owned())
        );
    }

    #[test]
    fn an_unknown_dependency_is_offered_the_nearest_target_or_alias() {
        let index = workspace();
        let (holder, root) = (label("//:holder"), file(""));
        let holder = held_by(&holder, &root);
        let error = dependency(&index, holder, &label("//:ap")).unwrap_err();
        assert_eq!(unknown(error, DeclarationField::Dep), Some(label("//:app")));
        let error = dependency(&index, holder, &label("//:defualt")).unwrap_err();
        assert_eq!(
            unknown(error, DeclarationField::Dep),
            Some(label("//:default"))
        );
        // A rule is near, but a dependency cannot name one, so it is not offered.
        let error = dependency(&index, holder, &label("//tools:c")).unwrap_err();
        assert_eq!(unknown(error, DeclarationField::Dep), None);
    }

    #[test]
    fn a_rule_reference_names_a_rule() {
        let index = workspace();
        let (holder, root) = (label("//:holder"), file(""));
        let found = rule(&index, held_by(&holder, &root), &label("//tools:cc")).unwrap();
        assert_eq!(found.run, command(&["cc"]));
    }

    #[test]
    fn a_rule_reference_to_a_target_or_an_alias_is_the_wrong_kind() {
        let index = workspace();
        let (holder, root) = (label("//:holder"), file(""));
        let holder = held_by(&holder, &root);
        let error = rule(&index, holder, &label("//lib:text")).unwrap_err();
        assert_eq!(
            wrong_kind(error, DeclarationField::Rule, "//lib:text"),
            (DeclaredKind::Target, "lib/build.lua".to_owned())
        );
        let error = rule(&index, holder, &label("//:default")).unwrap_err();
        assert_eq!(
            wrong_kind(error, DeclarationField::Rule, "//:default"),
            (DeclaredKind::Alias, "build.lua".to_owned())
        );
    }

    #[test]
    fn an_unknown_rule_is_offered_the_nearest_rule_only() {
        let index = workspace();
        let (holder, root) = (label("//:holder"), file(""));
        let holder = held_by(&holder, &root);
        let error = rule(&index, holder, &label("//tools:c")).unwrap_err();
        assert_eq!(
            unknown(error, DeclarationField::Rule),
            Some(label("//tools:cc"))
        );
        let error = rule(&index, holder, &label("//:ap")).unwrap_err();
        assert_eq!(unknown(error, DeclarationField::Rule), None);
    }

    #[test]
    fn an_alias_names_a_target() {
        let index = workspace();
        let (holder, root) = (label("//:holder"), file(""));
        assert_eq!(
            alias_target(&index, held_by(&holder, &root), &label("//:app")).unwrap(),
            &label("//:app")
        );
    }

    #[test]
    fn an_alias_for_an_alias_or_a_rule_is_the_wrong_kind() {
        let index = workspace();
        let (holder, root) = (label("//:holder"), file(""));
        let holder = held_by(&holder, &root);
        let error = alias_target(&index, holder, &label("//:default")).unwrap_err();
        assert_eq!(
            wrong_kind(error, DeclarationField::Target, "//:default"),
            (DeclaredKind::Alias, "build.lua".to_owned())
        );
        let error = alias_target(&index, holder, &label("//tools:cc")).unwrap_err();
        assert_eq!(
            wrong_kind(error, DeclarationField::Target, "//tools:cc"),
            (DeclaredKind::Rule, "tools/build.lua".to_owned())
        );
    }

    #[test]
    fn an_alias_for_nothing_is_offered_the_nearest_target_only() {
        let index = workspace();
        let (holder, root) = (label("//:holder"), file(""));
        let holder = held_by(&holder, &root);
        let error = alias_target(&index, holder, &label("//:ap")).unwrap_err();
        assert_eq!(
            unknown(error, DeclarationField::Target),
            Some(label("//:app"))
        );
        let error = alias_target(&index, holder, &label("//:defualt")).unwrap_err();
        assert_eq!(unknown(error, DeclarationField::Target), None);
    }

    #[test]
    fn a_command_naming_only_declared_settings_passes() {
        let index = workspace();
        let (holder, root) = (label("//:holder"), file(""));
        let holder = held_by(&holder, &root);
        assert!(settings(&index, holder, &command(&["go", "-run=$opt:test_filter"])).is_ok());
        assert!(settings(&index, holder, &command(&["cc", "$in", "$opt:"])).is_ok());
    }

    #[test]
    fn the_first_undeclared_setting_is_reported_with_the_nearest_declared_one() {
        let index = workspace();
        let (holder, root) = (label("//:holder"), file(""));
        let holder = held_by(&holder, &root);
        let run = command(&[
            "go",
            "$opt:test_filter",
            "$opt:test_fliter.$opt:zzz",
            "$opt:yyy",
        ]);
        match settings(&index, holder, &run).unwrap_err() {
            Error::UnknownSetting {
                site,
                name,
                suggestion,
            } => {
                assert_eq!(site.label, label("//:holder"));
                assert_eq!(site.provenance.file(), Path::new("build.lua"));
                assert_eq!(name, "test_fliter");
                assert_eq!(suggestion.unwrap().as_str(), "test_filter");
            }
            other => unreachable!("expected UnknownSetting, got {other:?}"),
        }
    }

    #[test]
    fn a_reference_too_long_to_be_a_setting_name_is_undeclared() {
        let index = workspace();
        let (holder, root) = (label("//:holder"), file(""));
        let long = "a".repeat(257);
        let run = command(&[&format!("$opt:{long}")]);
        match settings(&index, held_by(&holder, &root), &run).unwrap_err() {
            Error::UnknownSetting {
                name, suggestion, ..
            } => {
                assert_eq!(name, long);
                assert!(suggestion.is_none());
            }
            other => unreachable!("expected UnknownSetting, got {other:?}"),
        }
    }
}
