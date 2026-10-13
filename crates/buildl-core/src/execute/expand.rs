//! Expansion: a declared command's, and a rule's description's, placeholders replaced by what
//! they stand for.
//!
//! Its own file because a build file never holds a real path: a command is written with `$in`,
//! `$out`, `$deps` and `$opt:name`, and what an action is handed is the command with each of
//! them replaced. The replacement is workspace-relative text, the same wherever the action
//! runs, so it is known before anything does. A description is expanded by the same rules, so
//! that the line shown for an action names what the action was handed.
//!
//! Responsibilities: [`Lists`], [`expand`] and [`expand_description`].
//!
//! Non-responsibilities: whether the expanded command names a program that may run; and where a
//! reference to a setting starts and ends, which is read here through the scanner
//! [`Argument::setting_references`] and [`Description::setting_references`] list their names
//! with.

use std::collections::BTreeMap;

use crate::types::setting_reference;
use crate::types::{
    Command, Description, ExpandedDescription, OutputPath, SettingName, SettingValue, SourcePath,
};

/// The three lists of paths one target's placeholders stand for.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Lists<'a> {
    /// What `$in` stands for: the target's inputs, in the order they were written.
    pub(crate) inputs: &'a [SourcePath],
    /// What `$out` stands for: where the target's own outputs live, in the order they were
    /// written.
    pub(crate) outputs: &'a [OutputPath],
    /// What `$deps` stands for: where every output of every dependency lives.
    pub(crate) deps: &'a [OutputPath],
}

impl<'a> Lists<'a> {
    /// The list a placeholder's word names: `in`, `out` or `deps`, and nothing else.
    fn named(&self, word: &str) -> Option<Vec<&'a str>> {
        match word {
            "in" => Some(self.inputs.iter().map(SourcePath::as_str).collect()),
            "out" => Some(self.outputs.iter().map(OutputPath::as_str).collect()),
            "deps" => Some(self.deps.iter().map(OutputPath::as_str).collect()),
            _ => None,
        }
    }
}

/// `command` with its placeholders replaced, as the texts of its arguments.
///
/// Each argument is read once, left to right. At a `$`:
///
/// - `$opt:` and a setting's name becomes the value `settings` holds for it, and stays as it is
///   when `settings` holds none;
/// - `$` and a run of ASCII letters that is exactly `in`, `out` or `deps` becomes that list of
///   `lists`. The run is the longest one, so `$input` is text and `$out/app` is the outputs
///   followed by `/app`;
/// - anything else is text.
///
/// An argument that is one list and nothing else becomes one argument per path, and none when
/// the list is empty. Anywhere else a list becomes its paths joined by one space. Text that was
/// substituted is never read again, so a setting whose value is `$in` yields `$in`.
///
/// The result may hold no argument at all, and its first may not name a program: both are for
/// the caller to refuse.
pub(crate) fn expand(
    command: &Command,
    settings: &BTreeMap<SettingName, SettingValue>,
    lists: &Lists<'_>,
) -> Vec<String> {
    let mut expanded = Vec::new();
    for argument in command.arguments() {
        let alone = argument
            .as_str()
            .strip_prefix('$')
            .and_then(|word| lists.named(word));
        match alone {
            Some(paths) => expanded.extend(paths.into_iter().map(str::to_owned)),
            None => expanded.push(within(argument.as_str(), settings, lists)),
        }
    }
    expanded
}

/// `description` with its placeholders replaced, as the line to show for an action.
///
/// Placeholders are read as [`expand`] reads them within one argument: `$opt:` and a setting's
/// name becomes the value `settings` holds, and stays as it is when `settings` holds none, and
/// `$in`, `$out` and `$deps` become their list of `lists`. A list is always its paths joined by
/// one space, since a description is one line and has no arguments to split a list across. Text
/// that was substituted is never read again.
pub(crate) fn expand_description(
    description: &Description,
    settings: &BTreeMap<SettingName, SettingValue>,
    lists: &Lists<'_>,
) -> ExpandedDescription {
    ExpandedDescription::new(&within(description.as_str(), settings, lists))
}

/// One text with every placeholder inside it replaced.
///
/// The text is read `$` by `$`. One that starts a reference to a setting is replaced by the
/// setting's value, or left as written when `settings` holds none; one followed by the word of a
/// list is replaced by that list; any other stays a `$`.
fn within(text: &str, settings: &BTreeMap<SettingName, SettingValue>, lists: &Lists<'_>) -> String {
    let mut expanded = String::new();
    let mut rest = text;
    while let Some(at) = rest.find('$') {
        let (before, from) = rest.split_at(at);
        expanded.push_str(before);
        rest = if let Some((name, tail)) = setting_reference::leading(from) {
            match value(settings, name) {
                Some(value) => expanded.push_str(value.as_str()),
                None => expanded.extend([setting_reference::MARKER, name]),
            }
            tail
        } else {
            let after = from.strip_prefix('$').unwrap_or(from);
            let tail = after.trim_start_matches(|letter: char| letter.is_ascii_alphabetic());
            let word = after.strip_suffix(tail).unwrap_or_default();
            if let Some(paths) = lists.named(word) {
                expanded.push_str(&paths.join(" "));
                tail
            } else {
                expanded.push('$');
                after
            }
        };
    }
    expanded.push_str(rest);
    expanded
}

/// The value `settings` holds for the setting written as `name`.
fn value<'s>(
    settings: &'s BTreeMap<SettingName, SettingValue>,
    name: &str,
) -> Option<&'s SettingValue> {
    SettingName::parse(name)
        .ok()
        .and_then(|name| settings.get(&name))
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeMap;

    use super::{Lists, expand, expand_description};
    use crate::execute::fixtures::{command, output_path, path, settings};
    use crate::types::{Description, OutputPath, SettingName, SettingValue, SourcePath};

    /// `declared`, expanded with no setting and these three lists.
    fn expanded(
        declared: &[&str],
        inputs: &[SourcePath],
        outputs: &[OutputPath],
        deps: &[OutputPath],
    ) -> Vec<String> {
        with_settings(declared, &BTreeMap::new(), inputs, outputs, deps)
    }

    fn with_settings(
        declared: &[&str],
        settings: &BTreeMap<SettingName, SettingValue>,
        inputs: &[SourcePath],
        outputs: &[OutputPath],
        deps: &[OutputPath],
    ) -> Vec<String> {
        let lists = Lists {
            inputs,
            outputs,
            deps,
        };
        expand(&command(declared), settings, &lists)
    }

    #[test]
    fn a_compile_command_names_its_input_and_its_output() {
        assert_eq!(
            expanded(
                &["cc", "-c", "$in", "-o", "$out"],
                &[path("lib/a.c")],
                &[output_path("//lib:text", "text")],
                &[],
            ),
            ["cc", "-c", "lib/a.c", "-o", "out/lib/text"]
        );
    }

    #[test]
    fn a_link_command_names_every_output_of_its_dependencies() {
        assert_eq!(
            expanded(
                &["cc", "$deps", "-o", "$out"],
                &[],
                &[output_path("//:app", "app")],
                &[
                    output_path("//:main.o", "main.o"),
                    output_path("//lib:text", "text")
                ],
            ),
            ["cc", "out/main.o", "out/lib/text", "-o", "out/app"]
        );
    }

    #[test]
    fn a_test_command_runs_what_its_dependency_produced_with_a_setting() {
        assert_eq!(
            with_settings(
                &["$deps", "--filter=$opt:test_filter"],
                &settings(&[("test_filter", "Login")]),
                &[],
                &[],
                &[output_path("//:app", "app")],
            ),
            ["out/app", "--filter=Login"]
        );
    }

    #[test]
    fn an_empty_list_alone_leaves_no_argument() {
        assert_eq!(expanded(&["gen", "$in"], &[], &[], &[]), ["gen"]);
        assert_eq!(
            expanded(&["$deps"], &[], &[], &[]),
            Vec::<String>::new(),
            "a command may expand to nothing"
        );
    }

    #[test]
    fn a_list_alone_becomes_one_argument_per_path() {
        assert_eq!(
            expanded(
                &["cc", "$in"],
                &[path("src/b.c"), path("src/a.c")],
                &[],
                &[]
            ),
            ["cc", "src/b.c", "src/a.c"]
        );
    }

    #[test]
    fn a_list_inside_other_text_is_joined_by_one_space() {
        let inputs = [path("src/b.c"), path("src/a.c")];
        assert_eq!(
            expanded(&["--srcs=$in", "--none=$deps"], &inputs, &[], &[]),
            ["--srcs=src/b.c src/a.c", "--none="]
        );
    }

    #[test]
    fn two_placeholders_in_one_argument_are_both_expanded() {
        assert_eq!(
            expanded(
                &["$in:$out", "$out/app", "$in$deps"],
                &[path("a.c")],
                &[output_path("//:app", "bin")],
                &[output_path("//:lib", "lib")],
            ),
            ["a.c:out/bin", "out/bin/app", "a.cout/lib"]
        );
    }

    #[test]
    fn a_placeholder_is_the_longest_run_of_letters_after_the_dollar() {
        assert_eq!(
            expanded(
                &["$input", "$outs", "$dep", "$IN", "$in2", "$out.map", "$$in"],
                &[path("a.c")],
                &[output_path("//:app", "app")],
                &[],
            ),
            [
                "$input",
                "$outs",
                "$dep",
                "$IN",
                "a.c2",
                "out/app.map",
                "$a.c"
            ]
        );
    }

    #[test]
    fn a_dollar_that_starts_no_placeholder_stays_text() {
        assert_eq!(
            with_settings(
                &["$", "a$", "$1", "$opt:", "$opt:/x", "$opt", "é$é"],
                &settings(&[("x", "X")]),
                &[],
                &[],
                &[],
            ),
            ["$", "a$", "$1", "$opt:", "$opt:/x", "$opt", "é$é"]
        );
    }

    #[test]
    fn a_setting_reference_becomes_the_settings_value() {
        assert_eq!(
            with_settings(
                &["$opt:mode", "-O$opt:level-$opt:mode", "$opt:empty"],
                &settings(&[("mode", "debug"), ("level", "2"), ("empty", "")]),
                &[],
                &[],
                &[],
            ),
            ["debug", "-O$opt:level-debug", ""],
            "a name runs through `-`, so `level-` is one name, and it is not a setting"
        );
    }

    #[test]
    fn a_reference_to_a_setting_that_is_not_held_stays_text() {
        assert_eq!(
            with_settings(
                &["$opt:missing", "--a=$opt:missing,$opt:mode"],
                &settings(&[("mode", "debug")]),
                &[],
                &[],
                &[],
            ),
            ["$opt:missing", "--a=$opt:missing,debug"]
        );
    }

    #[test]
    fn substituted_text_is_never_expanded_again() {
        assert_eq!(
            with_settings(
                &["$opt:flag", "--x=$opt:flag", "$opt:nested"],
                &settings(&[("flag", "$in"), ("nested", "$opt:flag")]),
                &[path("a.c")],
                &[],
                &[],
            ),
            ["$in", "--x=$in", "$opt:flag"]
        );
    }

    /// `declared` as a description, expanded with these settings and lists.
    fn described(
        declared: &str,
        settings: &BTreeMap<SettingName, SettingValue>,
        inputs: &[SourcePath],
        outputs: &[OutputPath],
        deps: &[OutputPath],
    ) -> String {
        let lists = Lists {
            inputs,
            outputs,
            deps,
        };
        let description = Description::parse(declared).unwrap();
        expand_description(&description, settings, &lists)
            .as_str()
            .to_owned()
    }

    #[test]
    fn a_description_names_what_the_command_was_handed() {
        let inputs = [path("lib/a.c"), path("lib/b.c")];
        let outputs = [output_path("//lib:text", "text")];
        let deps = [
            output_path("//:gen", "gen.h"),
            output_path("//:gen", "gen.c"),
        ];
        assert_eq!(
            described("compile $in", &BTreeMap::new(), &inputs, &outputs, &deps),
            "compile lib/a.c lib/b.c",
            "a list alone is still one text, its paths joined by one space"
        );
        assert_eq!(
            described(
                "$in -> $out after $deps",
                &BTreeMap::new(),
                &inputs,
                &outputs,
                &deps
            ),
            "lib/a.c lib/b.c -> out/lib/text after out/gen.h out/gen.c"
        );
        assert_eq!(
            described("none: $in|$out|$deps", &BTreeMap::new(), &[], &[], &[]),
            "none: ||"
        );
    }

    #[test]
    fn a_description_follows_the_rules_of_a_command_for_everything_else() {
        let held = settings(&[("mode", "debug"), ("flag", "$in")]);
        assert_eq!(
            described(
                "$input $in2 $opt:mode-$opt:mode $opt:flag $opt:missing $ $opt:",
                &held,
                &[path("a.c")],
                &[],
                &[],
            ),
            "$input a.c2 $opt:mode-debug $in $opt:missing $ $opt:",
            "the longest run decides a placeholder, a substituted text is not read again, and an \
             unknown setting stays text"
        );
    }

    #[test]
    fn a_description_is_expanded_where_a_command_would_be_with_the_same_text() {
        let held = settings(&[("level", "2")]);
        let inputs = [path("a.c"), path("b.c")];
        let outputs = [output_path("//:app", "app")];
        let text = "cc -O$opt:level $in -o $out";
        let as_command = expand(
            &command(&[text]),
            &held,
            &Lists {
                inputs: &inputs,
                outputs: &outputs,
                deps: &[],
            },
        );
        assert_eq!(
            [described(text, &held, &inputs, &outputs, &[])],
            as_command.as_slice(),
            "the same text, inside other text, expands the same way in both"
        );
    }

    #[test]
    fn a_control_character_in_a_substituted_value_does_not_break_the_line() {
        assert_eq!(
            described(
                "mode $opt:mode.",
                &settings(&[("mode", "a\nb")]),
                &[],
                &[],
                &[]
            ),
            "mode a b."
        );
    }

    #[test]
    fn a_setting_named_like_a_list_is_still_a_setting() {
        assert_eq!(
            with_settings(
                &["$opt:in", "$opt:out$in"],
                &settings(&[("in", "I"), ("out", "O")]),
                &[path("a.c")],
                &[],
                &[],
            ),
            ["I", "Oa.c"]
        );
    }
}
