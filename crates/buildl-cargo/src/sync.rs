//! `cargo buildl sync`: bring the generated section of `build.lua` in line with `Cargo.toml`.
//!
//! [`sync`] reads the Cargo project, generates the section, and either rewrites the lines
//! between the markers of the `build.lua` at the workspace root or, with `check`, only says
//! whether they are current. Nothing but that one file is ever written.

use std::ffi::OsStr;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

use buildl::cli::Exit;

use crate::{generate, metadata, section};

/// What `sync` found in `build.lua` and what it does about it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Decision {
    /// The exit status of the command.
    exit: Exit,
    /// What is printed to standard error.
    message: String,
    /// The new text of `build.lua`, when it is to be written.
    write: Option<String>,
}

/// Decides what to do about `text`, the current `build.lua`, given the `generated` section.
///
/// Without `check`, a differing section is written and a current one is left alone. With
/// `check`, nothing is written and a differing section is [`Exit::Definition`]. Broken markers
/// are [`Exit::Definition`] either way.
fn decide(text: &str, generated: &str, check: bool) -> Decision {
    let replacement = match section::replace(text, generated) {
        Ok(replacement) => replacement,
        Err(error) => {
            return Decision {
                exit: error.exit(),
                message: error.to_string(),
                write: None,
            };
        }
    };
    match (replacement.changed, check) {
        (false, _) => Decision {
            exit: Exit::Success,
            message: "sync: build.lua is up to date".to_owned(),
            write: None,
        },
        (true, true) => Decision {
            exit: Exit::Definition,
            message: "sync: build.lua is out of date with Cargo.toml; run cargo buildl sync"
                .to_owned(),
            write: None,
        },
        (true, false) => Decision {
            exit: Exit::Success,
            message: "sync: build.lua updated".to_owned(),
            write: Some(replacement.text),
        },
    }
}

/// Writes `message` as a line of `stderr`. A failed write cannot be reported anywhere else.
fn say<E: Write>(stderr: &mut E, message: &str) {
    let _ = writeln!(stderr, "{message}");
}

/// Runs `cargo buildl sync`, with `cargo` as the program that describes the project and `start`
/// as the folder it describes it from.
///
/// The `build.lua` read and written is the one in the workspace root, wherever `start` lies in
/// the workspace. Every message goes to `stderr`.
///
/// | Outcome | Exit |
/// |---|---|
/// | up to date, or updated | [`Exit::Success`] |
/// | out of date with `check`; no `build.lua`; broken markers; cargo refused the project | [`Exit::Definition`] |
/// | cargo cannot be run or read; `build.lua` cannot be read or written | [`Exit::Infrastructure`] |
pub(crate) fn sync<E: Write>(cargo: &OsStr, start: &Path, check: bool, stderr: &mut E) -> Exit {
    let project = match metadata::read(cargo, start) {
        Ok(project) => project,
        Err(error) => {
            say(stderr, &error.to_string());
            return error.exit();
        }
    };
    let generated = match generate::section(&project) {
        Ok(generated) => generated,
        Err(refusal) => {
            say(stderr, &refusal.to_string());
            return generate::Refusal::EXIT;
        }
    };
    let path = project.workspace_root.join("build.lua");
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            say(stderr, "sync: no build.lua; run cargo buildl init");
            return Exit::Definition;
        }
        Err(error) => {
            say(stderr, &format!("error: cannot read build.lua: {error}"));
            return Exit::Infrastructure;
        }
    };
    let decision = decide(&text, &generated, check);
    if let Some(new_text) = &decision.write
        && let Err(error) = fs::write(&path, new_text)
    {
        say(stderr, &format!("error: cannot write build.lua: {error}"));
        return Exit::Infrastructure;
    }
    say(stderr, &decision.message);
    decision.exit
}

#[cfg(test)]
mod tests {
    use super::*;

    const BEGIN: &str = section::BEGIN_LINE;
    const END: &str = section::END_LINE;

    fn file(inside: &str) -> String {
        format!("-- mine\n{BEGIN}\n{inside}{END}\n-- also mine\n")
    }

    #[test]
    fn a_current_section_is_left_alone() {
        for check in [false, true] {
            let decision = decide(&file("local a = 1\n"), "local a = 1\n", check);
            assert_eq!(decision.exit, Exit::Success);
            assert_eq!(decision.message, "sync: build.lua is up to date");
            assert_eq!(decision.write, None);
        }
    }

    #[test]
    fn a_stale_section_is_written_without_check() {
        let decision = decide(&file("old\n"), "new\n", false);
        assert_eq!(decision.exit, Exit::Success);
        assert_eq!(decision.message, "sync: build.lua updated");
        assert_eq!(decision.write, Some(file("new\n")));
    }

    #[test]
    fn a_stale_section_is_5_and_unwritten_with_check() {
        let decision = decide(&file("old\n"), "new\n", true);
        assert_eq!(decision.exit, Exit::Definition);
        assert_eq!(
            decision.message,
            "sync: build.lua is out of date with Cargo.toml; run cargo buildl sync"
        );
        assert_eq!(decision.write, None);
    }

    #[test]
    fn broken_markers_are_5_with_or_without_check() {
        for check in [false, true] {
            let decision = decide("-- no markers\n", "new\n", check);
            assert_eq!(decision.exit, Exit::Definition);
            assert!(
                decision
                    .message
                    .starts_with("sync: build.lua has no generated section")
            );
            assert_eq!(decision.write, None);

            let decision = decide(&format!("{END}\n{BEGIN}\n"), "new\n", check);
            assert_eq!(decision.exit, Exit::Definition);
            assert_eq!(
                decision.message,
                "sync: build.lua must hold one begin line and, after it, one end line"
            );
        }
    }
}
