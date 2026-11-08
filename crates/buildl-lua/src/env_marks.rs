//! Staging a target's `env` table: each variable's name beside the mark written for it.
//!
//! Its own file because `env` is the one option whose table keys are data. The entries are staged
//! as written — a name with a mark, a list entry, a key that is no name — and the core's load
//! refuses what is not a marked name, so every refusal names the target the way its other
//! refusals do.
//!
//! Responsibilities: [`stage`], and the order the entries come out in.
//!
//! Non-responsibilities: whether a name is a valid variable name or a word is a mark.

use airsl::mlua::{Table, Value};
use buildl_core::{EnvMark, EnvName, StagedEnv, StagedMark, Written};

use crate::refusal::Refusal;
use crate::values::{runtime, table};

/// Stages the `env` option, `nil` being no variables.
///
/// A string key with a string value is a name with a mark; a string key with any other value is a
/// name with a mark that is not a string; an integer key is a list entry, whatever it holds; a
/// string key that is not valid UTF-8 is a name that cannot be read; any other key is no name. The
/// entries come out marked names first, in name order, then list entries, then the unreadable
/// names, then keys that are no name, so what a table's `pairs` order is reaches nothing.
///
/// # Errors
///
/// Refuses a value that is not a table, and a table that cannot be read.
pub(crate) fn stage(value: &Value) -> Result<Vec<StagedEnv>, Refusal> {
    if matches!(value, Value::Nil) {
        return Ok(Vec::new());
    }
    entries(&table(value, "env")?)
}

fn entries(env: &Table) -> Result<Vec<StagedEnv>, Refusal> {
    let mut marked = Vec::new();
    let mut lists = 0_usize;
    let mut not_utf8 = 0_usize;
    let mut no_names = Vec::new();
    for pair in env.pairs::<Value, Value>() {
        let (key, mark) = pair.map_err(|error| runtime(&error))?;
        match key {
            Value::String(name) => match name.to_str() {
                Ok(name) => marked.push((name.to_owned(), written_mark(&mark))),
                Err(_) => not_utf8 += 1,
            },
            Value::Integer(_) => lists += 1,
            other => no_names.push(other.type_name().to_owned()),
        }
    }
    marked.sort_by(|(a, _), (b, _)| a.cmp(b));
    no_names.sort();
    let named = marked.into_iter().map(|(name, mark)| StagedEnv::Marked {
        name: Written::<EnvName>::new(name),
        mark,
    });
    let lists = core::iter::repeat_n(StagedEnv::ListEntry, lists);
    let not_utf8 = core::iter::repeat_n(StagedEnv::NotUtf8Name, not_utf8);
    let no_names = no_names
        .into_iter()
        .map(|found| StagedEnv::NotAName { found });
    Ok(named.chain(lists).chain(not_utf8).chain(no_names).collect())
}

/// The mark as written: a string as it is, anything else as a refusal shows it.
fn written_mark(value: &Value) -> StagedMark {
    match value {
        Value::String(word) => word.to_str().map_or_else(
            |_| StagedMark::NotAString {
                found: String::from("non-UTF-8 text"),
            },
            |word| StagedMark::Word(Written::<EnvMark>::new(&*word)),
        ),
        Value::Boolean(set) => StagedMark::NotAString {
            found: set.to_string(),
        },
        other => StagedMark::NotAString {
            found: with_article(other.type_name()),
        },
    }
}

/// A type's name with its article: `a table`, `an integer`, `a userdata`.
///
/// `userdata` is the one type name starting with `u` and it is spoken with a consonant.
fn with_article(type_name: &str) -> String {
    let article = if type_name.starts_with(['a', 'e', 'i', 'o']) {
        "an"
    } else {
        "a"
    };
    format!("{article} {type_name}")
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid Lua snippets; a panic is the intended failure signal"
    )]

    use airsl::mlua::{Lua, Value};
    use buildl_core::{EnvMark, EnvName, StagedEnv, StagedMark, Written};

    use super::{stage, with_article};

    fn staged(source: &str) -> Vec<StagedEnv> {
        let lua = Lua::new();
        let value: Value = lua.load(source).eval().unwrap();
        stage(&value).unwrap()
    }

    fn word(name: &str, mark: &str) -> StagedEnv {
        StagedEnv::Marked {
            name: Written::<EnvName>::new(name),
            mark: StagedMark::Word(Written::<EnvMark>::new(mark)),
        }
    }

    #[test]
    fn env_table_of_marks_is_read() {
        assert_eq!(
            staged("return { RUSTFLAGS = 'key', HOME = 'unkeyed' }"),
            [word("HOME", "unkeyed"), word("RUSTFLAGS", "key")]
        );
    }

    #[test]
    fn empty_or_absent_env_declares_nothing() {
        let nothing: Vec<StagedEnv> = Vec::new();
        assert_eq!(staged("return {}"), nothing);
        assert_eq!(staged("return nil"), nothing);
    }

    #[test]
    fn env_list_entry_is_staged_without_a_mark() {
        assert_eq!(staged("return { 'HOME' }"), [StagedEnv::ListEntry]);
    }

    #[test]
    fn env_integer_key_is_a_list_entry() {
        assert_eq!(staged("return { [1] = 'key' }"), [StagedEnv::ListEntry]);
    }

    #[test]
    fn env_mixed_table_stages_both_kinds() {
        assert_eq!(
            staged("return { 'HOME', PATH = 'unkeyed' }"),
            [word("PATH", "unkeyed"), StagedEnv::ListEntry]
        );
    }

    #[test]
    fn env_marks_that_are_not_words_are_staged_as_found() {
        let found = |source: &str| match staged(source).as_slice() {
            [
                StagedEnv::Marked {
                    mark: StagedMark::NotAString { found },
                    ..
                },
            ] => found.clone(),
            other => unreachable!("expected one mark that is not a string, found {other:?}"),
        };
        assert_eq!(found("return { HOME = true }"), "true");
        assert_eq!(found("return { HOME = {} }"), "a table");
        assert_eq!(found("return { HOME = 1 }"), "an integer");
        assert_eq!(staged("return { HOME = 'pass' }"), [word("HOME", "pass")]);
    }

    #[test]
    fn env_key_that_is_no_name_is_staged_as_found() {
        assert_eq!(
            staged("return { [true] = 'key' }"),
            [StagedEnv::NotAName {
                found: "boolean".to_owned()
            }]
        );
    }

    #[test]
    fn env_key_that_is_not_utf8_is_staged_as_such() {
        assert_eq!(
            staged("return { ['\\xff'] = 'key', HOME = 'key' }"),
            [word("HOME", "key"), StagedEnv::NotUtf8Name]
        );
    }

    #[test]
    fn articles_follow_how_a_type_name_is_spoken() {
        assert_eq!(with_article("table"), "a table");
        assert_eq!(with_article("integer"), "an integer");
        assert_eq!(with_article("userdata"), "a userdata");
    }

    #[test]
    fn env_entries_come_out_in_name_order() {
        let names: Vec<String> = (0..26_u8)
            .map(|step| {
                let letter = char::from(b'A' + (step * 7) % 26);
                format!("VAR_{letter}")
            })
            .collect();
        let table = names
            .iter()
            .map(|name| format!("{name} = 'key'"))
            .collect::<Vec<_>>()
            .join(", ");
        let mut sorted: Vec<&str> = names.iter().map(String::as_str).collect();
        sorted.sort_unstable();
        let expected: Vec<StagedEnv> = sorted.into_iter().map(|name| word(name, "key")).collect();
        assert_eq!(staged(&format!("return {{ {table} }}")), expected);
    }

    #[test]
    fn env_that_is_not_a_table_is_refused() {
        let lua = Lua::new();
        let value: Value = lua.load("return 'HOME'").eval().unwrap();
        let refusal = stage(&value).unwrap_err();
        assert_eq!(
            refusal.diagnostic("target", None).as_str(),
            "buildl.target: field 'env' must be a table, found string"
        );
    }
}
