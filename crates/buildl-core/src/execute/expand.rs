//! Expansion: a declared command's placeholders replaced by what they stand for.
//!
//! Its own file because a build file never holds a real path: a command is written with `$in`,
//! `$out`, `$deps` and `$opt:name`, and what an action is handed is the command with each of
//! them replaced. The replacement is workspace-relative text, the same wherever the action
//! runs, so it is known before anything does.
//!
//! Responsibilities: [`Lists`] and [`expand`].
//!
//! Non-responsibilities: whether the expanded command names a program that may run; and which
//! characters a setting's name is made of, which [`Argument::setting_references`] decides.

use std::collections::BTreeMap;

use crate::types::{Argument, Command, OutputPath, SettingName, SettingValue, SourcePath};

/// What follows the `$` of a reference to a build setting, before the setting's name.
const SETTING: &str = "opt:";

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
            None => expanded.push(within(argument, settings, lists)),
        }
    }
    expanded
}

/// One argument with every placeholder inside it replaced.
///
/// The names of the settings it refers to are taken from [`Argument::setting_references`], in
/// the order they appear, and matched against the text as it is read: this function finds the
/// same `$opt:` occurrences in the same order, so the next name is always the one at hand.
fn within(
    argument: &Argument,
    settings: &BTreeMap<SettingName, SettingValue>,
    lists: &Lists<'_>,
) -> String {
    let mut references = argument.setting_references().into_iter().peekable();
    let mut text = String::new();
    let mut rest = argument.as_str();
    while let Some((before, after)) = rest.split_once('$') {
        text.push_str(before);
        rest = if let Some((name, tail)) = reference(after, references.peek().copied()) {
            references.next();
            match value(settings, name) {
                Some(value) => text.push_str(value.as_str()),
                None => text.extend(["$", SETTING, name]),
            }
            tail
        } else {
            let tail = after.trim_start_matches(|letter: char| letter.is_ascii_alphabetic());
            let word = after.strip_suffix(tail).unwrap_or_default();
            if let Some(paths) = lists.named(word) {
                text.push_str(&paths.join(" "));
                tail
            } else {
                text.push('$');
                after
            }
        };
    }
    text.push_str(rest);
    text
}

/// The setting name `after` starts a reference to, with the text that follows the name, when
/// that name is `next`: the next name the argument refers to.
///
/// A `$opt:` followed by no name is not a reference, and then `after` cannot continue with
/// `next`: a name starts with a character a name may hold, and the text here does not.
fn reference<'t>(after: &'t str, next: Option<&'t str>) -> Option<(&'t str, &'t str)> {
    let name = next?;
    let tail = after.strip_prefix(SETTING)?.strip_prefix(name)?;
    Some((name, tail))
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
    use std::collections::BTreeMap;

    use super::{Lists, expand};
    use crate::execute::fixtures::{command, output_path, path, settings};
    use crate::types::{OutputPath, SettingName, SettingValue, SourcePath};

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
