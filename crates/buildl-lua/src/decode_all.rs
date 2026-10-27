//! `buildl.decode_all`: the JSON values of a stream, as a list.
//!
//! Its own file because, unlike the other `buildl` primitives, it neither stages nor refuses: its
//! errors are plain Lua errors, which a build file can catch with `pcall`. Output that is JSON values one after another, such as a tool's
//! `--message-format=json` stream, is not one JSON document, so `airsstack.json.decode` refuses
//! it. This splits the stream into its values and hands each to the `decode` airsl installed, so
//! a value comes out exactly as `airsstack.json.decode` would give it.
//!
//! Responsibilities: [`decode_all`].
//!
//! Non-responsibilities: the JSON-to-Lua conversion, which airsl's `json` module owns.

use airsl::mlua::{self, Function, Lua, Table, Value};
use serde::de::IgnoredAny;
use serde_json::Deserializer;

use crate::curated::CURATED_REGISTRY_KEY;

/// A list of the JSON values in `text`, in order.
///
/// Values are separated by whitespace, which may be left out beside an object, an array or a
/// string (`{"a":1}{"a":2}`); `12` is one number, not two. Each is decoded
/// by the `decode` function of the `json` module as it was installed, so a build file that
/// replaces `airsstack.json` afterwards changes nothing here. Empty or whitespace-only text is an
/// empty list.
///
/// # Errors
///
/// Returns a Lua error when `text` is not a string, or when the stream is not a sequence of JSON
/// values; the message names `buildl.decode_all` and carries the parser's, with the line and
/// column.
pub(crate) fn decode_all(lua: &Lua, text: &Value) -> mlua::Result<Table> {
    let Value::String(text) = text else {
        return Err(mlua::Error::RuntimeError(format!(
            "buildl.decode_all: text must be a string, got {}",
            text.type_name()
        )));
    };
    let text = text.to_str()?;
    let decode = json_decode(lua)?;

    let list = lua.create_table()?;
    let mut values = Deserializer::from_str(&text).into_iter::<IgnoredAny>();
    let mut start = 0;
    while let Some(read) = values.next() {
        read.map_err(|error| mlua::Error::RuntimeError(format!("buildl.decode_all: {error}")))?;
        let end = values.byte_offset();
        let span = text.get(start..end).ok_or_else(|| {
            mlua::Error::RuntimeError(
                "buildl.decode_all: a value ends inside a character".to_owned(),
            )
        })?;
        list.raw_push(decode.call::<Value>(span.trim_start())?)?;
        start = end;
    }
    Ok(list)
}

/// The `decode` function of the `json` module as it was installed.
fn json_decode(lua: &Lua) -> mlua::Result<Function> {
    let curated: Table = lua.named_registry_value(CURATED_REGISTRY_KEY)?;
    let json: Table = curated.get("json")?;
    json.get("decode")
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    use airsl::{Engine, Script};
    use buildl_core::Directory;

    use crate::engine::build;
    use crate::limits::DeclarationLimits;
    use crate::module::BuildlModule;
    use crate::results::Results;
    use crate::sources::Sources;
    use crate::staging::Staging;

    fn engine() -> (tempfile::TempDir, Engine) {
        let limits = DeclarationLimits::default();
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let module = BuildlModule::new(
            Arc::new(Mutex::new(Staging::new(u64::MAX))),
            Sources::new(&root, &Directory::root(), limits.walk_entries(), &[]),
            Results::none(),
            &root,
            BTreeMap::new(),
        )
        .unwrap();
        (dir, build(&limits, module).unwrap())
    }

    fn eval(source: &str) -> airsl::Result<String> {
        let (_dir, engine) = engine();
        engine.eval_to::<String>(&Script::from_source(source, "build.lua").unwrap())
    }

    /// The compact JSON of `buildl.decode_all(<stream>)`, `stream` being a Lua expression.
    fn decoded(stream: &str) -> String {
        eval(&format!(
            "return airsstack.json.encode(buildl.decode_all({stream}))"
        ))
        .unwrap()
    }

    #[test]
    fn two_objects_on_two_lines_are_two_values() {
        assert_eq!(
            decoded(r#""{\"a\":1}\n{\"a\":2}\n""#),
            r#"[{"a":1},{"a":2}]"#
        );
    }

    #[test]
    fn adjacent_objects_are_two_values() {
        assert_eq!(decoded(r#"'{"a":1}{"a":2}'"#), r#"[{"a":1},{"a":2}]"#);
    }

    #[test]
    fn one_array_is_a_list_of_one_value() {
        assert_eq!(decoded("'[1,2]'"), "[[1,2]]");
    }

    #[test]
    fn spaced_numbers_are_three_values() {
        assert_eq!(decoded("'1 2 3'"), "[1,2,3]");
    }

    #[test]
    fn strings_null_and_true_are_three_values() {
        let result = eval(
            r#"local all = buildl.decode_all('"x" null true')
               return table.concat({
                 tostring(#all), all[1], tostring(all[2] == airsstack.json.decode('null')),
                 tostring(all[3]),
               }, ',')"#,
        )
        .unwrap();
        assert_eq!(result, "3,x,true,true");
    }

    #[test]
    fn a_number_then_an_object_are_two_values() {
        assert_eq!(decoded(r#"'1{"a":2}'"#), r#"[1,{"a":2}]"#);
    }

    #[test]
    fn empty_text_is_an_empty_list() {
        assert_eq!(
            eval("return tostring(#buildl.decode_all(''))").unwrap(),
            "0"
        );
    }

    #[test]
    fn whitespace_only_is_an_empty_list() {
        assert_eq!(
            eval("return tostring(#buildl.decode_all(' \\n\\t '))").unwrap(),
            "0"
        );
    }

    #[test]
    fn unicode_survives() {
        let result = eval(
            "local all = buildl.decode_all('\"é\" \"😀\"')\n\
             return all[1] .. '|' .. all[2]",
        )
        .unwrap();
        assert_eq!(result, "é|😀");
    }

    #[test]
    fn a_truncated_stream_raises_a_catchable_error() {
        let result = eval(
            r#"local ok, err = pcall(buildl.decode_all, '{"a":1}' .. '\n' .. '{"a":')
               return tostring(ok) .. '|' .. tostring(err)"#,
        )
        .unwrap();
        assert!(result.starts_with("false|"), "{result}");
        assert!(
            result.contains("EOF while parsing a value at line 2 column 5"),
            "{result}"
        );
    }

    #[test]
    fn garbage_between_values_raises() {
        let message =
            eval(r#"return airsstack.json.encode(buildl.decode_all('{"a":1} xx {"a":2}'))"#)
                .unwrap_err()
                .to_string();
        assert!(
            message.contains("buildl.decode_all: expected value at line 1 column 9"),
            "{message}"
        );
    }

    #[test]
    fn a_non_string_raises_naming_the_argument() {
        let message = eval("return buildl.decode_all(5)").unwrap_err().to_string();
        assert!(message.contains("buildl.decode_all"), "{message}");
        assert!(message.contains("text must be a string"), "{message}");
    }

    #[test]
    fn values_equal_airsstack_json_decode() {
        let result = eval(
            r#"local spans = { '{"a":[1,2.5,"x",null],"b":{"c":false}}', '[]', '"s"', '-7', '1.5e2' }
               local all = buildl.decode_all(table.concat(spans, '\n'))
               local json = airsstack.json
               local same = #all == #spans
               for i, span in ipairs(spans) do
                 same = same and json.encode(all[i]) == json.encode(json.decode(span))
               end
               return tostring(same)"#,
        )
        .unwrap();
        assert_eq!(result, "true");
    }

    #[test]
    fn a_build_file_replacing_airsstack_json_does_not_change_decode_all() {
        let result = eval(
            r"airsstack.json.decode = function() error('replaced') end
               local first = buildl.decode_all('1 2')
               airsstack.json = { decode = function() error('replaced') end }
               local second = buildl.decode_all('3')
               return tostring(#first) .. tostring(first[1]) .. tostring(second[1])",
        )
        .unwrap();
        assert_eq!(result, "213");
    }
}
