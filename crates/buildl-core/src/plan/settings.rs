//! Build settings for one run: the value each has, and which of them a command refers to.
//!
//! Its own file because a setting reaches a key in two steps that have nothing else in common
//! with the rest of the phase: an override replaces a default, and a target keys only on the
//! settings its own command names.
//!
//! Responsibilities: [`effective`] and [`referenced`].
//!
//! Non-responsibilities: whether a command refers to a setting that is declared. The graph
//! guarantees it does.

use std::collections::BTreeMap;

use crate::error::{Error, Result};
use crate::types::{Argument, Command, SettingName, SettingValue};

/// The value every declared setting has for this run: its override, or else its default.
///
/// # Errors
///
/// Returns [`Error::UnknownOverride`] for an override that names no declared setting; when
/// several do, the one whose name sorts first.
pub(crate) fn effective(
    declared: &BTreeMap<SettingName, SettingValue>,
    overrides: &BTreeMap<SettingName, SettingValue>,
) -> Result<BTreeMap<SettingName, SettingValue>> {
    if let Some(name) = overrides.keys().find(|name| !declared.contains_key(*name)) {
        return Err(Error::UnknownOverride { name: name.clone() });
    }
    let mut values = declared.clone();
    values.extend(
        overrides
            .iter()
            .map(|(name, value)| (name.clone(), value.clone())),
    );
    Ok(values)
}

/// The settings `command` refers to, each with its value from `values`.
///
/// Only these reach a target's key, so changing a setting rebuilds exactly the targets whose
/// commands name it. A reference to a setting `values` does not hold is left out.
pub(crate) fn referenced(
    command: &Command,
    values: &BTreeMap<SettingName, SettingValue>,
) -> BTreeMap<SettingName, SettingValue> {
    command
        .arguments()
        .iter()
        .flat_map(Argument::setting_references)
        .filter_map(|name| SettingName::parse(name).ok())
        .filter_map(|name| values.get(&name).map(|value| (name, value.clone())))
        .collect()
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeMap;

    use super::{effective, referenced};
    use crate::error::Error;
    use crate::plan::fixtures::{command, settings};

    #[test]
    fn with_no_override_every_setting_keeps_its_default() {
        let declared = settings(&[("mode", "debug"), ("test_filter", "")]);
        assert_eq!(effective(&declared, &BTreeMap::new()).unwrap(), declared);
    }

    #[test]
    fn an_override_replaces_the_default_of_the_setting_it_names() {
        let declared = settings(&[("mode", "debug"), ("test_filter", "")]);
        let overrides = settings(&[("test_filter", "TestLogin")]);
        assert_eq!(
            effective(&declared, &overrides).unwrap(),
            settings(&[("mode", "debug"), ("test_filter", "TestLogin")])
        );
    }

    #[test]
    fn an_override_naming_no_setting_is_refused() {
        let declared = settings(&[("test_filter", "")]);
        let overrides = settings(&[("test_fliter", "TestLogin")]);
        match effective(&declared, &overrides).unwrap_err() {
            Error::UnknownOverride { name } => assert_eq!(name.as_str(), "test_fliter"),
            other => unreachable!("expected UnknownOverride, got {other:?}"),
        }
    }

    #[test]
    fn of_several_unknown_overrides_the_lowest_name_is_reported() {
        let declared = settings(&[("mode", "debug")]);
        let overrides = settings(&[("zeta", "1"), ("mode", "release"), ("alpha", "2")]);
        match effective(&declared, &overrides).unwrap_err() {
            Error::UnknownOverride { name } => assert_eq!(name.as_str(), "alpha"),
            other => unreachable!("expected UnknownOverride, got {other:?}"),
        }
    }

    #[test]
    fn only_the_settings_a_command_names_are_kept() {
        let values = settings(&[("arch", "arm"), ("mode", "debug"), ("test_filter", "T")]);
        let run = command(&["go", "test", "--run=$opt:test_filter", "-tags=$opt:mode"]);
        assert_eq!(
            referenced(&run, &values),
            settings(&[("mode", "debug"), ("test_filter", "T")])
        );
        assert!(referenced(&command(&["cc", "$in"]), &values).is_empty());
    }

    #[test]
    fn a_setting_named_twice_is_kept_once() {
        let values = settings(&[("mode", "debug")]);
        let run = command(&["tool", "$opt:mode", "--again=$opt:mode"]);
        assert_eq!(referenced(&run, &values), settings(&[("mode", "debug")]));
    }

    #[test]
    fn a_reference_to_a_setting_without_a_value_is_left_out() {
        let values = settings(&[("mode", "debug")]);
        let long = format!("$opt:{}", "a".repeat(257));
        let run = command(&["tool", "$opt:other", long.as_str(), "$opt:mode"]);
        assert_eq!(referenced(&run, &values), settings(&[("mode", "debug")]));
    }
}
