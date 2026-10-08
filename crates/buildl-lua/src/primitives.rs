//! What each `buildl` primitive reads from its arguments, and the staged record it produces.
//!
//! Its own file because the argument shape of each primitive is the build-file API's contract,
//! and keeping all of them together keeps them consistent: positional parameters are read left
//! to right, then an option table is checked for unknown keys, then its accepted fields are read
//! in the order each primitive lists them. A refusal after the name is known names it.
//!
//! Responsibilities: [`target`], [`rule`], [`alias`], [`option`], [`subdir`], [`pattern`].
//!
//! Non-responsibilities: staging the records and installing the functions, which the `buildl`
//! module does.

use airsl::mlua::Value;
use buildl_core::{
    Directory, Freshness, NetworkAccess, SettingName, StagedAlias, StagedRule, StagedSetting,
    StagedTarget, TargetName, TargetRole, Written,
};

use crate::refusal::Refusal;
use crate::values::{closed, field, flag, list, optional_list, optional_text, string, table, text};

/// The fields `buildl.target` and `buildl.test` accept, in the order they are read.
const TARGET_FIELDS: [&str; 8] = [
    "rule", "run", "inputs", "deps", "outputs", "env", "network", "always",
];
/// The fields `buildl.rule` accepts, in the order they are read.
const RULE_FIELDS: [&str; 2] = ["run", "desc"];
/// The fields `buildl.option` accepts.
const OPTION_FIELDS: [&str; 1] = ["default"];

/// `buildl.target(name, options)` and `buildl.test(name, options)`, told apart by `role`.
pub(crate) fn target(
    name: &Value,
    options: &Value,
    role: TargetRole,
) -> Result<StagedTarget, Refusal> {
    let name: Written<TargetName> = text(name, "name")?;
    let subject = name.as_written().to_owned();
    target_fields(name, options, role).map_err(|refusal| refusal.about(&subject))
}

fn target_fields(
    name: Written<TargetName>,
    options: &Value,
    role: TargetRole,
) -> Result<StagedTarget, Refusal> {
    let options = table(options, "options")?;
    closed(&options, &TARGET_FIELDS)?;
    Ok(StagedTarget {
        name,
        role,
        rule: optional_text(&field(&options, "rule")?, "rule")?,
        run: optional_list(&field(&options, "run")?, "run")?,
        inputs: optional_list(&field(&options, "inputs")?, "inputs")?.unwrap_or_default(),
        deps: optional_list(&field(&options, "deps")?, "deps")?.unwrap_or_default(),
        outputs: optional_list(&field(&options, "outputs")?, "outputs")?,
        env: optional_list(&field(&options, "env")?, "env")?.unwrap_or_default(),
        network: match flag(&field(&options, "network")?, "network")? {
            Some(true) => NetworkAccess::Declared,
            Some(false) | None => NetworkAccess::Sealed,
        },
        freshness: match flag(&field(&options, "always")?, "always")? {
            Some(true) => Freshness::Always,
            Some(false) | None => Freshness::Cached,
        },
    })
}

/// `buildl.rule(name, options)`.
pub(crate) fn rule(name: &Value, options: &Value) -> Result<StagedRule, Refusal> {
    let name: Written<TargetName> = text(name, "name")?;
    let subject = name.as_written().to_owned();
    rule_fields(name, options).map_err(|refusal| refusal.about(&subject))
}

fn rule_fields(name: Written<TargetName>, options: &Value) -> Result<StagedRule, Refusal> {
    let options = table(options, "options")?;
    closed(&options, &RULE_FIELDS)?;
    Ok(StagedRule {
        name,
        run: list(&field(&options, "run")?, "run")?,
        description: optional_text(&field(&options, "desc")?, "desc")?,
    })
}

/// `buildl.alias(name, target)`.
pub(crate) fn alias(name: &Value, target: &Value) -> Result<StagedAlias, Refusal> {
    let name: Written<TargetName> = text(name, "name")?;
    let target = text(target, "target").map_err(|refusal| refusal.about(name.as_written()))?;
    Ok(StagedAlias { name, target })
}

/// `buildl.option(name, options)`.
pub(crate) fn option(name: &Value, options: &Value) -> Result<StagedSetting, Refusal> {
    let name: Written<SettingName> = text(name, "name")?;
    let subject = name.as_written().to_owned();
    option_fields(name, options).map_err(|refusal| refusal.about(&subject))
}

fn option_fields(name: Written<SettingName>, options: &Value) -> Result<StagedSetting, Refusal> {
    let options = table(options, "options")?;
    closed(&options, &OPTION_FIELDS)?;
    Ok(StagedSetting {
        name,
        default: text(&field(&options, "default")?, "default")?,
    })
}

/// `buildl.subdir(path)`.
pub(crate) fn subdir(path: &Value) -> Result<Written<Directory>, Refusal> {
    text(path, "path")
}

/// The pattern argument of `buildl.sources(pattern)`.
pub(crate) fn pattern(pattern: &Value) -> Result<String, Refusal> {
    string(pattern, "pattern")
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid Lua snippets; a panic is the intended failure signal"
    )]

    use airsl::mlua::{Lua, Value};
    use buildl_core::{
        EvaluationFailure, Freshness, NetworkAccess, StagedAlias, StagedRule, StagedSetting,
        StagedTarget, TargetRole, Written,
    };

    use super::{alias, option, pattern, rule, subdir, target};

    fn eval(lua: &Lua, source: &str) -> Value {
        lua.load(source).eval::<Value>().unwrap()
    }

    fn strings<T>(texts: &[&str]) -> Vec<Written<T>> {
        texts.iter().map(|text| Written::new(*text)).collect()
    }

    #[test]
    fn target_reads_every_field() {
        let lua = Lua::new();
        let staged = target(
            &eval(&lua, "return 'app'"),
            &eval(
                &lua,
                "return { rule = 'cc', run = { 'cc', '$in' }, inputs = { { 'a.c' }, 'b.c' }, \
                 deps = { '//lib:text' }, outputs = { 'app' }, env = { 'HOME' }, \
                 network = true, always = true }",
            ),
            TargetRole::Build,
        )
        .unwrap();
        assert_eq!(
            staged,
            StagedTarget {
                name: Written::new("app"),
                role: TargetRole::Build,
                rule: Some(Written::new("cc")),
                run: Some(strings(&["cc", "$in"])),
                inputs: strings(&["a.c", "b.c"]),
                deps: strings(&["//lib:text"]),
                outputs: Some(strings(&["app"])),
                env: strings(&["HOME"]),
                network: NetworkAccess::Declared,
                freshness: Freshness::Always,
            }
        );
    }

    #[test]
    fn target_defaults_every_absent_field() {
        let lua = Lua::new();
        let staged = target(
            &eval(&lua, "return 'app_test'"),
            &eval(&lua, "return { network = false, always = false }"),
            TargetRole::Test,
        )
        .unwrap();
        assert_eq!(
            staged,
            StagedTarget {
                name: Written::new("app_test"),
                role: TargetRole::Test,
                rule: None,
                run: None,
                inputs: Vec::new(),
                deps: Vec::new(),
                outputs: None,
                env: Vec::new(),
                network: NetworkAccess::Sealed,
                freshness: Freshness::Cached,
            }
        );
    }

    #[test]
    fn a_bad_name_is_refused_before_the_options_are_looked_at() {
        let refusal = target(&Value::Nil, &Value::Nil, TargetRole::Build).unwrap_err();
        assert_eq!(
            refusal.diagnostic("target", None).as_str(),
            "buildl.target: field 'name' must be a string, found nil"
        );
    }

    #[test]
    fn an_unknown_field_is_refused_before_a_wrongly_typed_one() {
        let lua = Lua::new();
        let refusal = target(
            &eval(&lua, "return 'app'"),
            &eval(&lua, "return { run = 1, srcs = {} }"),
            TargetRole::Build,
        )
        .unwrap_err();
        assert_eq!(
            refusal.failure(),
            &EvaluationFailure::UnknownField {
                field: Written::new("srcs"),
            }
        );
        assert_eq!(
            refusal.diagnostic("target", Some(4)).as_str(),
            "line 4: buildl.target 'app': unknown field 'srcs'"
        );
    }

    #[test]
    fn accepted_fields_are_checked_in_their_listed_order() {
        let lua = Lua::new();
        let refusal = target(
            &eval(&lua, "return 'app'"),
            &eval(&lua, "return { always = 1, run = 1 }"),
            TargetRole::Build,
        )
        .unwrap_err();
        assert_eq!(
            refusal.failure(),
            &EvaluationFailure::WrongFieldType {
                field: Written::new("run"),
            }
        );
    }

    #[test]
    fn rule_requires_run_and_reads_desc() {
        let lua = Lua::new();
        let staged = rule(
            &eval(&lua, "return 'cc'"),
            &eval(
                &lua,
                "return { run = { 'cc', '-c' }, desc = 'compile $in' }",
            ),
        )
        .unwrap();
        assert_eq!(
            staged,
            StagedRule {
                name: Written::new("cc"),
                run: strings(&["cc", "-c"]),
                description: Some(Written::new("compile $in")),
            }
        );
        let refusal = rule(&eval(&lua, "return 'cc'"), &eval(&lua, "return {}")).unwrap_err();
        assert_eq!(
            refusal.diagnostic("rule", None).as_str(),
            "buildl.rule 'cc': field 'run' must be a list of strings, found nil"
        );
    }

    #[test]
    fn alias_reads_two_strings() {
        let lua = Lua::new();
        assert_eq!(
            alias(&eval(&lua, "return 'default'"), &eval(&lua, "return 'app'")).unwrap(),
            StagedAlias {
                name: Written::new("default"),
                target: Written::new("app"),
            }
        );
        let refusal = alias(&eval(&lua, "return 'default'"), &Value::Nil).unwrap_err();
        assert_eq!(
            refusal.diagnostic("alias", None).as_str(),
            "buildl.alias 'default': field 'target' must be a string, found nil"
        );
    }

    #[test]
    fn option_requires_a_string_default() {
        let lua = Lua::new();
        assert_eq!(
            option(
                &eval(&lua, "return 'mode'"),
                &eval(&lua, "return { default = 'debug' }")
            )
            .unwrap(),
            StagedSetting {
                name: Written::new("mode"),
                default: Written::new("debug"),
            }
        );
        let refusal = option(
            &eval(&lua, "return 'jobs'"),
            &eval(&lua, "return { default = 4 }"),
        )
        .unwrap_err();
        assert_eq!(
            refusal.diagnostic("option", None).as_str(),
            "buildl.option 'jobs': field 'default' must be a string, found integer"
        );
    }

    #[test]
    fn subdir_and_pattern_read_one_string() {
        let lua = Lua::new();
        assert_eq!(
            subdir(&eval(&lua, "return 'lib'")).unwrap(),
            Written::new("lib")
        );
        assert_eq!(pattern(&eval(&lua, "return '*.c'")).unwrap(), "*.c");
        assert_eq!(
            pattern(&Value::Nil)
                .unwrap_err()
                .diagnostic("sources", None)
                .as_str(),
            "buildl.sources: field 'pattern' must be a string, found nil"
        );
    }
}
