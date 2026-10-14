//! Admission: whether an expanded command names a program, and whether that program may run.
//!
//! Its own file because some commands name their program only once their paths are known — a
//! test that runs `$deps` runs what its dependency built — and a build is refused for what it
//! wants before any of it runs. A program written out, or named through a build setting, was
//! already held to the ceiling when the build was planned. One first named by `$in`, `$out` or
//! `$deps` is known only here, and so is a command that names no program at all. Every expanded
//! command goes through the one check, whichever way its program was named.
//!
//! Responsibilities: [`admit`].
//!
//! Non-responsibilities: expanding a command; and what a ceiling allows, which is the
//! manifest's to say.

use crate::error::{DeclarationSite, Error, Result};
use crate::types::{Argument, Ceiling, Command, Node, Program, Want};

/// The command `node` runs, from the texts its declared command `expanded` to.
///
/// The first argument is the program the action runs. Every other argument is kept as it is,
/// an empty one included.
///
/// # Errors
///
/// Returns [`Error::NoProgram`] when `expanded` holds no argument, or when its first argument
/// is empty or still holds a `$`, with `node` as the site. Otherwise returns
/// [`Error::Refused`], with the program, `node` as the site and `ceiling`, when `ceiling` does
/// not allow the program to run.
///
/// Returns [`Error::InvalidName`] for a text no argument can hold. Expansion puts together
/// texts that were each validated when they were declared, so it yields none.
pub(crate) fn admit(expanded: Vec<String>, node: &Node, ceiling: &Ceiling) -> Result<Command> {
    let arguments = expanded
        .into_iter()
        .map(Argument::parse)
        .collect::<Result<Vec<Argument>>>()?;
    let Some(program) = arguments.first().and_then(Program::of) else {
        return Err(Error::NoProgram {
            site: Box::new(DeclarationSite::from(node)),
        });
    };
    if !ceiling.run.contains(&program) {
        return Err(Error::Refused {
            want: Want::Program(program),
            site: Some(Box::new(DeclarationSite::from(node))),
            ceiling: Box::new(ceiling.clone()),
        });
    }
    Command::new(arguments)
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeSet;

    use super::admit;
    use crate::error::{Error, NameKind};
    use crate::execute::fixtures::{command, node, program};
    use crate::types::{Ceiling, Want};

    /// A ceiling allowing `run` and nothing else.
    fn ceiling(run: &[&str]) -> Ceiling {
        Ceiling {
            run: run.iter().map(|raw| program(raw)).collect(),
            env: BTreeSet::new(),
            read: BTreeSet::new(),
            write: BTreeSet::new(),
        }
    }

    fn texts(arguments: &[&str]) -> Vec<String> {
        arguments
            .iter()
            .map(|argument| (*argument).to_owned())
            .collect()
    }

    /// Asserts that `expanded`, as the command of `//:app`, names no program, whatever may run.
    fn names_no_program(expanded: &[&str]) {
        let app = node("//:app", |_| {});
        let error = admit(texts(expanded), &app, &ceiling(&["cc", "out/app"])).unwrap_err();
        assert_eq!(
            error.to_string(),
            "build.lua: //:app: the command names no program once expanded"
        );
        match error {
            Error::NoProgram { site } => assert_eq!(site.label, app.label),
            other => unreachable!("expected NoProgram, got {other:?}"),
        }
    }

    #[test]
    fn a_command_whose_program_may_run_is_admitted_as_it_is() {
        let text = node("//lib:text", |_| {});
        assert_eq!(
            admit(texts(&["cc", "-c", "lib/a.c"]), &text, &ceiling(&["cc"])).unwrap(),
            command(&["cc", "-c", "lib/a.c"])
        );
    }

    #[test]
    fn only_the_first_argument_has_to_name_a_program() {
        let app = node("//:app", |_| {});
        assert_eq!(
            admit(texts(&["cc", "", "$input"]), &app, &ceiling(&["cc"])).unwrap(),
            command(&["cc", "", "$input"])
        );
    }

    #[test]
    fn a_program_first_named_by_expansion_is_held_to_the_ceiling() {
        let test = node("//:app_test", |_| {});
        let expanded = texts(&["out/app", "--filter=Login"]);

        assert_eq!(
            admit(expanded.clone(), &test, &ceiling(&["cc", "out/app"])).unwrap(),
            command(&["out/app", "--filter=Login"])
        );

        let allowed = ceiling(&["cc"]);
        let error = admit(expanded, &test, &allowed).unwrap_err();
        assert_eq!(
            error.to_string(),
            "build.lua: //:app_test: program out/app is outside the ceiling (proc.run allows: cc)"
        );
        match error {
            Error::Refused {
                want,
                site,
                ceiling,
            } => {
                assert_eq!(want, Want::Program(program("out/app")));
                assert_eq!(site.unwrap().label, test.label);
                assert_eq!(*ceiling, allowed);
            }
            other => unreachable!("expected Refused, got {other:?}"),
        }
    }

    #[test]
    fn a_command_that_expanded_to_no_argument_names_no_program() {
        names_no_program(&[]);
    }

    #[test]
    fn an_empty_first_argument_names_no_program() {
        names_no_program(&["", "out/app"]);
    }

    #[test]
    fn a_first_argument_still_holding_a_dollar_names_no_program() {
        names_no_program(&["$input"]);
        names_no_program(&["$opt:missing", "cc"]);
        names_no_program(&["out/$"]);
    }

    #[test]
    fn a_command_naming_no_program_is_not_asked_of_the_ceiling() {
        let app = node("//:app", |_| {});
        assert!(matches!(
            admit(Vec::new(), &app, &ceiling(&[])).unwrap_err(),
            Error::NoProgram { .. }
        ));
    }

    #[test]
    fn text_no_argument_can_hold_is_refused_as_an_invalid_argument() {
        let app = node("//:app", |_| {});
        match admit(texts(&["cc", "a\0b"]), &app, &ceiling(&["cc"])).unwrap_err() {
            Error::InvalidName { kind, .. } => assert_eq!(kind, NameKind::Argument),
            other => unreachable!("expected InvalidName, got {other:?}"),
        }
    }
}
