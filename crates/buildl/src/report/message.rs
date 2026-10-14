//! The lines a command says about itself.
//!
//! Its own file because none of these lines belongs to a build: each is what a command says
//! about how it went, for a person, written beside whatever the command was asked to produce
//! and never in it.
//!
//! Responsibilities:
//!
//! - [`check_line`] — that the build files are sound.
//! - [`error_text`] — why the command failed.
//!
//! Non-responsibilities: writing the text anywhere; the words of an error, which are the
//! error's own `Display`; and choosing an exit status, which reads the error's variant, never
//! this text.

use std::collections::BTreeSet;
use std::path::Path;

use buildl_core::json::canonical;
use buildl_core::{Declaration, Error};

use crate::report::paint::Paint;

/// `count` of a thing, named by `one` for exactly one and by `many` otherwise.
fn counted(count: usize, one: &str, many: &str) -> String {
    if count == 1 {
        format!("{count} {one}")
    } else {
        format!("{count} {many}")
    }
}

/// What a check of the build files says when every one of them evaluated: how many
/// declarations were found, and in how many build files, as `ok: 7 declarations in 2 build
/// files`. Ends with a newline.
///
/// The count of build files says that subdirectories were reached. A build file that declares
/// nothing is not counted, since only a declaration names its file.
#[must_use]
pub fn check_line(declarations: &[Declaration]) -> String {
    let files: BTreeSet<&Path> = declarations
        .iter()
        .map(|declaration| declaration.provenance().file())
        .collect();
    format!(
        "ok: {} in {}\n",
        counted(declarations.len(), "declaration", "declarations"),
        counted(files.len(), "build file", "build files")
    )
}

/// One evaluation's declaration on one line, or `nothing` when that evaluation had none there.
fn evaluated(declaration: Option<&Declaration>) -> String {
    declaration.map_or_else(
        || "nothing".to_owned(),
        |declaration| {
            canonical::to_string(declaration).unwrap_or_else(|_| format!("{declaration:?}"))
        },
    )
}

/// An error as a command shows it: `error: ` and the error's own words, ending with a newline.
///
/// Two errors get more lines, each indented by four spaces. Build files that declared
/// differently on two evaluations are followed by the declaration each evaluation had at the
/// first difference, as canonical JSON. A state file that is not what buildl writes is followed
/// by what to do about it: remove the file and run the command again.
///
/// Under [`Paint::Ansi`] the word `error` is bold red.
#[must_use]
pub fn error_text(error: &Error, paint: Paint) -> String {
    let mut lines = vec![format!("{}: {error}\n", paint.failure("error"))];
    match error {
        Error::Nondeterministic {
            first_run,
            second_run,
            ..
        } => {
            lines.push(format!(
                "    first evaluation:  {}\n",
                evaluated(first_run.as_deref())
            ));
            lines.push(format!(
                "    second evaluation: {}\n",
                evaluated(second_run.as_deref())
            ));
        }
        Error::CorruptState { file, .. } => {
            lines.push(format!(
                "    remove {} and run the command again: every target is then built anew\n",
                file.display()
            ));
        }
        _ => {}
    }
    lines.concat()
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::path::PathBuf;

    use buildl_core::{
        Action, Argument, Command, Declaration, Declared, Directory, Error, Freshness, Label,
        NetworkAccess, Provenance, Setting, SettingName, SettingValue, Target, TargetRole,
    };

    use super::{check_line, error_text};
    use crate::report::paint::Paint;

    /// The build file of `directory`.
    fn build_file(directory: &str) -> Provenance {
        let file = if directory.is_empty() {
            PathBuf::from("build.lua")
        } else {
            PathBuf::from(directory).join("build.lua")
        };
        Provenance::new(file, Directory::parse(directory).unwrap())
    }

    /// A setting named `name`, declared in the build file of `directory`.
    fn setting(directory: &str, name: &str) -> Declaration {
        Declaration::new(
            build_file(directory),
            Declared::Setting(Setting {
                name: SettingName::parse(name).unwrap(),
                default: SettingValue::parse("debug").unwrap(),
            }),
        )
    }

    #[test]
    fn a_check_counts_declarations_and_the_build_files_they_came_from() {
        let declarations = [
            setting("", "mode"),
            setting("", "opt"),
            setting("lib", "a"),
            setting("lib", "b"),
            setting("lib", "c"),
            setting("", "d"),
            setting("", "e"),
        ];
        assert_eq!(
            check_line(&declarations),
            "ok: 7 declarations in 2 build files\n"
        );
    }

    #[test]
    fn one_declaration_in_one_build_file_is_counted_in_the_singular() {
        assert_eq!(
            check_line(&[setting("", "mode")]),
            "ok: 1 declaration in 1 build file\n"
        );
    }

    #[test]
    fn a_workspace_that_declares_nothing_counts_nothing() {
        assert_eq!(check_line(&[]), "ok: 0 declarations in 0 build files\n");
    }

    /// The target `raw`, running `cc`, depending on `deps`.
    fn target(raw: &str, deps: &[&str]) -> Declaration {
        let label = Label::parse(raw).unwrap();
        Declaration::new(
            build_file(label.directory().as_str()),
            Declared::Target(Target {
                label,
                role: TargetRole::Build,
                action: Action::Run(Command::new(vec![Argument::parse("cc").unwrap()]).unwrap()),
                inputs: Vec::new(),
                deps: deps.iter().map(|dep| Label::parse(dep).unwrap()).collect(),
                outputs: Vec::new(),
                env: Vec::new(),
                network: NetworkAccess::Sealed,
                freshness: Freshness::Cached,
            }),
        )
    }

    #[test]
    fn an_error_is_its_own_words_after_the_word_error() {
        let error = Error::UnknownOverride {
            name: SettingName::parse("optimise").unwrap(),
        };
        assert_eq!(
            error_text(&error, Paint::Plain),
            "error: setting optimise is overridden but not declared\n"
        );
    }

    #[test]
    fn the_word_error_is_bold_red_when_painted() {
        let error = Error::UnknownOverride {
            name: SettingName::parse("optimise").unwrap(),
        };
        assert_eq!(
            error_text(&error, Paint::Ansi),
            "\x1b[1;31merror\x1b[0m: setting optimise is overridden but not declared\n"
        );
    }

    #[test]
    fn a_corrupt_state_error_says_what_to_remove() {
        let error = Error::CorruptState {
            file: PathBuf::from(".buildl/cache.json"),
            source: Box::new(Error::NonCanonicalJson { offset: 586 }),
        };
        assert_eq!(
            error_text(&error, Paint::Plain),
            concat!(
                "error: .buildl/cache.json is not a file buildl wrote: ",
                "JSON is not in canonical form: it differs from it at byte 586\n",
                "    remove .buildl/cache.json and run the command again: ",
                "every target is then built anew\n",
            )
        );
    }

    #[test]
    fn a_nondeterministic_error_shows_both_declarations() {
        let error = Error::Nondeterministic {
            provenance: build_file("lib"),
            first_run: Some(Box::new(target("//lib:text", &[]))),
            second_run: Some(Box::new(target("//lib:text", &["//:main.o"]))),
        };
        assert_eq!(
            error_text(&error, Paint::Plain),
            concat!(
                "error: lib/build.lua: declarations differ between two evaluations\n",
                "    first evaluation:  {\"item\":{\"Target\":{\"action\":{\"Run\":[\"cc\"]},",
                "\"deps\":[],\"env\":[],\"freshness\":\"Cached\",\"inputs\":[],",
                "\"label\":\"//lib:text\",\"network\":\"Sealed\",\"outputs\":[],",
                "\"role\":\"Build\"}},",
                "\"provenance\":{\"directory\":\"lib\",\"file\":\"lib/build.lua\"}}\n",
                "    second evaluation: {\"item\":{\"Target\":{\"action\":{\"Run\":[\"cc\"]},",
                "\"deps\":[\"//:main.o\"],\"env\":[],\"freshness\":\"Cached\",\"inputs\":[],",
                "\"label\":\"//lib:text\",\"network\":\"Sealed\",\"outputs\":[],",
                "\"role\":\"Build\"}},",
                "\"provenance\":{\"directory\":\"lib\",\"file\":\"lib/build.lua\"}}\n",
            )
        );
    }

    #[test]
    fn an_evaluation_that_declared_nothing_there_says_so() {
        let error = Error::Nondeterministic {
            provenance: build_file(""),
            first_run: Some(Box::new(setting("", "mode"))),
            second_run: None,
        };
        assert_eq!(
            error_text(&error, Paint::Plain),
            concat!(
                "error: build.lua: declarations differ between two evaluations\n",
                "    first evaluation:  {\"item\":{\"Setting\":{\"default\":\"debug\",",
                "\"name\":\"mode\"}},\"provenance\":{\"directory\":\"\",\"file\":\"build.lua\"}}\n",
                "    second evaluation: nothing\n",
            )
        );
    }
}
