//! Reading a primitive's Lua arguments into text as the build file wrote it.
//!
//! Its own file because every primitive reads the same four shapes — a string, a list of strings,
//! a boolean, a closed option table — and the shape rules must not differ between them. The rules
//! are strict: a string field takes a Lua string and nothing that Lua would coerce into one, a list
//! is a sequence `1..n` whose elements are strings or one level of nested sequences of strings
//! (spliced in order), and a flag is a boolean.
//!
//! Responsibilities: the value readers, and the closed-table check that names an unknown field.
//!
//! Non-responsibilities: validating what the text says. A name's grammar, a label's resolution and
//! a path's containment are decided after evaluation, not here.

use airsl::mlua::{self, Table, Value};
use buildl_core::{EvaluationFailure, Written};

use crate::refusal::Refusal;

const A_STRING: &str = "a string";
const A_LIST: &str = "a list of strings";
const A_BOOLEAN: &str = "a boolean";
const A_TABLE: &str = "a table";

/// Reads a required string argument as plain text.
pub(crate) fn string(value: &Value, field: &str) -> Result<String, Refusal> {
    utf8(value).ok_or_else(|| wrong(field, A_STRING, &describe(value)))
}

/// Reads a required string field.
pub(crate) fn text<T>(value: &Value, field: &str) -> Result<Written<T>, Refusal> {
    string(value, field).map(Written::new)
}

/// Reads an optional string field; `nil` is `None`.
pub(crate) fn optional_text<T>(value: &Value, field: &str) -> Result<Option<Written<T>>, Refusal> {
    match value {
        Value::Nil => Ok(None),
        other => text(other, field).map(Some),
    }
}

/// Reads a required list field, splicing one level of nested lists in order.
pub(crate) fn list<T>(value: &Value, field: &str) -> Result<Vec<Written<T>>, Refusal> {
    let Value::Table(table) = value else {
        return Err(wrong(field, A_LIST, &describe(value)));
    };
    let elements =
        sequence(table)?.ok_or_else(|| wrong(field, A_LIST, "a table that is not a list"))?;
    let mut written = Vec::with_capacity(elements.len());
    for element in &elements {
        if let Value::Table(nested) = element {
            let nested = sequence(nested)?
                .ok_or_else(|| wrong(field, A_LIST, "a nested table that is not a list"))?;
            for item in &nested {
                written.push(Written::new(
                    utf8(item).ok_or_else(|| wrong(field, A_LIST, &describe(item)))?,
                ));
            }
        } else {
            written.push(Written::new(
                utf8(element).ok_or_else(|| wrong(field, A_LIST, &describe(element)))?,
            ));
        }
    }
    Ok(written)
}

/// Reads an optional list field; `nil` is `None`.
pub(crate) fn optional_list<T>(
    value: &Value,
    field: &str,
) -> Result<Option<Vec<Written<T>>>, Refusal> {
    match value {
        Value::Nil => Ok(None),
        other => list(other, field).map(Some),
    }
}

/// Reads an optional boolean field; `nil` is `None`.
pub(crate) fn flag(value: &Value, field: &str) -> Result<Option<bool>, Refusal> {
    match value {
        Value::Nil => Ok(None),
        Value::Boolean(set) => Ok(Some(*set)),
        other => Err(wrong(field, A_BOOLEAN, &describe(other))),
    }
}

/// Reads a required table argument.
pub(crate) fn table(value: &Value, field: &str) -> Result<Table, Refusal> {
    match value {
        Value::Table(table) => Ok(table.clone()),
        other => Err(wrong(field, A_TABLE, &describe(other))),
    }
}

/// Refuses the first key of `table`, in sorted order, that `accepted` does not name.
///
/// Sorting makes the refused key the same whatever order `pairs` visits the table in.
pub(crate) fn closed(table: &Table, accepted: &[&str]) -> Result<(), Refusal> {
    let mut keys = Vec::new();
    for pair in table.pairs::<Value, Value>() {
        let (key, _) = pair.map_err(|error| runtime(&error))?;
        keys.push(render(&key));
    }
    keys.sort();
    keys.into_iter()
        .find(|key| !accepted.contains(&key.as_str()))
        .map_or(Ok(()), |key| {
            Err(Refusal::new(
                EvaluationFailure::UnknownField {
                    field: Written::new(key.as_str()),
                },
                format!("unknown field '{key}'"),
            ))
        })
}

/// The value stored under `name`, read without metamethods.
pub(crate) fn field(table: &Table, name: &str) -> Result<Value, Refusal> {
    table
        .raw_get::<Value>(name)
        .map_err(|error| runtime(&error))
}

/// The elements of `table` when its keys are exactly `1..n`, in order.
fn sequence(table: &Table) -> Result<Option<Vec<Value>>, Refusal> {
    let len = table.raw_len();
    let mut count = 0_usize;
    for pair in table.pairs::<Value, Value>() {
        let (key, _) = pair.map_err(|error| runtime(&error))?;
        let in_range = match key {
            Value::Integer(index) => usize::try_from(index).is_ok_and(|i| (1..=len).contains(&i)),
            _ => false,
        };
        if !in_range {
            return Ok(None);
        }
        count += 1;
    }
    if count != len {
        return Ok(None);
    }
    (1..=len)
        .map(|index| table.raw_get::<Value>(index))
        .collect::<mlua::Result<Vec<_>>>()
        .map(Some)
        .map_err(|error| runtime(&error))
}

/// The value's text when it is a Lua string holding valid UTF-8.
fn utf8(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => text.to_str().ok().map(|text| text.to_owned()),
        _ => None,
    }
}

/// How a refusal names what it found.
fn describe(value: &Value) -> String {
    match value {
        Value::String(text) if text.to_str().is_err() => String::from("non-UTF-8 text"),
        other => other.type_name().to_owned(),
    }
}

/// How an option table's key is named in a refusal.
fn render(key: &Value) -> String {
    match key {
        Value::String(text) => String::from_utf8_lossy(&text.as_bytes()).into_owned(),
        other => format!("<{}>", other.type_name()),
    }
}

fn wrong(field: &str, wanted: &str, found: &str) -> Refusal {
    Refusal::new(
        EvaluationFailure::WrongFieldType {
            field: Written::new(field),
        },
        format!("field '{field}' must be {wanted}, found {found}"),
    )
}

fn runtime(error: &mlua::Error) -> Refusal {
    Refusal::new(EvaluationFailure::Runtime, error.to_string())
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid Lua snippets; a panic is the intended failure signal"
    )]

    use airsl::mlua::{Lua, Value};
    use buildl_core::{EvaluationFailure, Written};

    use super::{closed, field, flag, list, optional_list, optional_text, string, table, text};
    use crate::refusal::Refusal;

    fn eval(lua: &Lua, source: &str) -> Value {
        lua.load(source).eval::<Value>().unwrap()
    }

    fn texts(written: &[Written<()>]) -> Vec<&str> {
        written.iter().map(Written::as_written).collect()
    }

    fn wrong_type(field: &str) -> EvaluationFailure {
        EvaluationFailure::WrongFieldType {
            field: Written::new(field),
        }
    }

    fn failure(refusal: &Refusal) -> &EvaluationFailure {
        refusal.failure()
    }

    #[test]
    fn text_reads_a_string_as_written() {
        let lua = Lua::new();
        let written: Written<()> = text(&eval(&lua, "return 'src/a.c'"), "name").unwrap();
        assert_eq!(written.as_written(), "src/a.c");
    }

    #[test]
    fn text_refuses_a_number_rather_than_coercing_it() {
        let lua = Lua::new();
        let refusal = text::<()>(&eval(&lua, "return 1"), "name").unwrap_err();
        assert_eq!(failure(&refusal), &wrong_type("name"));
        assert_eq!(
            refusal.diagnostic("target", None).as_str(),
            "buildl.target: field 'name' must be a string, found integer"
        );
    }

    #[test]
    fn text_refuses_non_utf8_bytes() {
        let lua = Lua::new();
        let refusal = text::<()>(&eval(&lua, "return '\\255'"), "name").unwrap_err();
        assert_eq!(
            refusal.diagnostic("target", None).as_str(),
            "buildl.target: field 'name' must be a string, found non-UTF-8 text"
        );
    }

    #[test]
    fn string_reads_plain_text() {
        let lua = Lua::new();
        assert_eq!(
            string(&eval(&lua, "return '**/*.c'"), "pattern").unwrap(),
            "**/*.c"
        );
    }

    #[test]
    fn text_refuses_nil() {
        let refusal = text::<()>(&Value::Nil, "default").unwrap_err();
        assert_eq!(failure(&refusal), &wrong_type("default"));
    }

    #[test]
    fn optional_text_reads_nil_as_none() {
        assert_eq!(optional_text::<()>(&Value::Nil, "desc").unwrap(), None);
    }

    #[test]
    fn list_reads_a_sequence_in_order() {
        let lua = Lua::new();
        let written: Vec<Written<()>> =
            list(&eval(&lua, "return { 'cc', '-c', '$in' }"), "run").unwrap();
        assert_eq!(texts(&written), ["cc", "-c", "$in"]);
    }

    #[test]
    fn list_splices_one_level_of_nested_lists_in_order() {
        let lua = Lua::new();
        let written: Vec<Written<()>> = list(
            &eval(&lua, "return { { 'a.c', 'b.c' }, 'go.mod', {}, { 'c.c' } }"),
            "inputs",
        )
        .unwrap();
        assert_eq!(texts(&written), ["a.c", "b.c", "go.mod", "c.c"]);
    }

    #[test]
    fn list_refuses_two_levels_of_nesting() {
        let lua = Lua::new();
        let refusal = list::<()>(&eval(&lua, "return { { { 'a.c' } } }"), "inputs").unwrap_err();
        assert_eq!(
            refusal.diagnostic("target", None).as_str(),
            "buildl.target: field 'inputs' must be a list of strings, found table"
        );
    }

    #[test]
    fn list_refuses_a_table_with_holes_or_named_keys() {
        let lua = Lua::new();
        for source in [
            "return { [1] = 'a', [3] = 'c' }",
            "return { 'a', name = 'b' }",
            "return { [0] = 'a' }",
        ] {
            let refusal = list::<()>(&eval(&lua, source), "deps").unwrap_err();
            assert_eq!(
                refusal.diagnostic("target", None).as_str(),
                "buildl.target: field 'deps' must be a list of strings, found a table that is not a list",
                "{source}"
            );
        }
    }

    #[test]
    fn list_refuses_a_non_string_element() {
        let lua = Lua::new();
        let refusal = list::<()>(&eval(&lua, "return { 'cc', 1 }"), "run").unwrap_err();
        assert_eq!(failure(&refusal), &wrong_type("run"));
    }

    #[test]
    fn list_refuses_a_bare_string() {
        let lua = Lua::new();
        let refusal = list::<()>(&eval(&lua, "return 'cc'"), "run").unwrap_err();
        assert_eq!(
            refusal.diagnostic("target", None).as_str(),
            "buildl.target: field 'run' must be a list of strings, found string"
        );
    }

    #[test]
    fn optional_list_reads_nil_as_none_and_an_empty_table_as_empty() {
        let lua = Lua::new();
        assert_eq!(optional_list::<()>(&Value::Nil, "outputs").unwrap(), None);
        assert_eq!(
            optional_list::<()>(&eval(&lua, "return {}"), "outputs").unwrap(),
            Some(Vec::new())
        );
    }

    #[test]
    fn flag_reads_booleans_and_nil() {
        assert_eq!(flag(&Value::Boolean(true), "network").unwrap(), Some(true));
        assert_eq!(
            flag(&Value::Boolean(false), "network").unwrap(),
            Some(false)
        );
        assert_eq!(flag(&Value::Nil, "network").unwrap(), None);
    }

    #[test]
    fn flag_refuses_a_truthy_non_boolean() {
        let refusal = flag(&Value::Integer(1), "always").unwrap_err();
        assert_eq!(
            refusal.diagnostic("target", None).as_str(),
            "buildl.target: field 'always' must be a boolean, found integer"
        );
    }

    #[test]
    fn table_refuses_anything_but_a_table() {
        let refusal = table(&Value::Nil, "options").unwrap_err();
        assert_eq!(failure(&refusal), &wrong_type("options"));
    }

    #[test]
    fn closed_accepts_known_keys_only() {
        let lua = Lua::new();
        let options = table(&eval(&lua, "return { run = {}, desc = 'x' }"), "options").unwrap();
        assert!(closed(&options, &["run", "desc"]).is_ok());
    }

    #[test]
    fn closed_refuses_the_first_unknown_key_in_sorted_order() {
        // Lua seeds its string hash per state, so `pairs` order varies between
        // states; many fresh states make an unsorted walk fail reliably.
        for _ in 0..64 {
            let lua = Lua::new();
            let options = table(
                &eval(&lua, "return { zeta = 1, run = {}, alpha = 2, mu = 3 }"),
                "options",
            )
            .unwrap();
            let refusal = closed(&options, &["run"]).unwrap_err();
            assert_eq!(
                failure(&refusal),
                &EvaluationFailure::UnknownField {
                    field: Written::new("alpha"),
                }
            );
            assert_eq!(
                refusal.diagnostic("rule", None).as_str(),
                "buildl.rule: unknown field 'alpha'"
            );
        }
    }

    #[test]
    fn closed_renders_a_non_string_key_by_its_type() {
        let lua = Lua::new();
        let options = table(&eval(&lua, "return { 'positional' }"), "options").unwrap();
        let refusal = closed(&options, &["run"]).unwrap_err();
        assert_eq!(
            failure(&refusal),
            &EvaluationFailure::UnknownField {
                field: Written::new("<integer>"),
            }
        );
    }

    #[test]
    fn closed_renders_a_non_utf8_key_lossily() {
        let lua = Lua::new();
        let options = table(&eval(&lua, "return { ['\\255'] = 1 }"), "options").unwrap();
        let refusal = closed(&options, &["run"]).unwrap_err();
        assert_eq!(
            failure(&refusal),
            &EvaluationFailure::UnknownField {
                field: Written::new("\u{fffd}"),
            }
        );
    }

    #[test]
    fn field_reads_without_metamethods() {
        let lua = Lua::new();
        let options = table(
            &eval(
                &lua,
                "return setmetatable({}, { __index = function() return 'x' end })",
            ),
            "options",
        )
        .unwrap();
        assert!(matches!(field(&options, "run").unwrap(), Value::Nil));
    }
}
