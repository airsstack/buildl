//! Whether a build keeps a status line, and whether what is printed is coloured.
//!
//! Its own file because both answers come from the same few facts about the process, and they
//! are decided once, here, for everything the binary prints. The facts are handed in as values,
//! so each answer is a pure function a test can ask without a terminal.
//!
//! Responsibilities: [`Terminal`], with [`Terminal::mode`] and [`Terminal::paint`].
//!
//! Non-responsibilities: reading the facts, which `main` does; and applying a colour or drawing
//! a status line, which `buildl::report` does once it is told the answers.

use std::ffi::{OsStr, OsString};

use buildl::report::{Mode, Paint};

use crate::cli::Color;

/// What the process knows about where its standard error goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Terminal {
    /// Whether standard error is a terminal.
    pub interactive: bool,
    /// The value of `TERM`, when it is set.
    pub term: Option<OsString>,
    /// The value of `NO_COLOR`, when it is set.
    pub no_color: Option<OsString>,
}

impl Terminal {
    /// Whether a status line is kept: only on a terminal that can redraw a row.
    pub(crate) fn mode(&self) -> Mode {
        if self.redraws() {
            Mode::Live
        } else {
            Mode::Plain
        }
    }

    /// Whether what is printed is coloured, given what the command line asked for.
    ///
    /// Asked for or refused outright, the command line decides. Left to this function, colour
    /// goes to a terminal that can redraw a row, unless `NO_COLOR` holds any value but the empty
    /// one.
    pub(crate) fn paint(&self, color: Color) -> Paint {
        let coloured = match color {
            Color::Always => true,
            Color::Never => false,
            Color::Auto => self.redraws() && !set(self.no_color.as_deref()),
        };
        if coloured { Paint::Ansi } else { Paint::Plain }
    }

    /// Whether standard error is a terminal that understands the sequences a status line and a
    /// colour are made of: `TERM` is set, and is neither empty nor `dumb`.
    fn redraws(&self) -> bool {
        self.interactive
            && set(self.term.as_deref())
            && self.term.as_deref() != Some(OsStr::new("dumb"))
    }
}

/// Whether a variable holds a value that is not the empty one.
fn set(value: Option<&OsStr>) -> bool {
    value.is_some_and(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use buildl::report::{Mode, Paint};

    use super::Terminal;
    use crate::cli::Color;

    /// A process whose standard error is a terminal, with `TERM` and `NO_COLOR` as given.
    fn on_a_terminal(term: Option<&str>, no_color: Option<&str>) -> Terminal {
        Terminal {
            interactive: true,
            term: term.map(OsString::from),
            no_color: no_color.map(OsString::from),
        }
    }

    /// A process whose standard error is not a terminal, with `TERM` and `NO_COLOR` as given.
    fn into_a_pipe(term: Option<&str>, no_color: Option<&str>) -> Terminal {
        Terminal {
            interactive: false,
            term: term.map(OsString::from),
            no_color: no_color.map(OsString::from),
        }
    }

    #[test]
    fn a_terminal_gets_a_status_line_and_colour() {
        let terminal = on_a_terminal(Some("xterm-256color"), None);
        assert_eq!(terminal.mode(), Mode::Live);
        assert_eq!(terminal.paint(Color::Auto), Paint::Ansi);
    }

    #[test]
    fn a_pipe_is_plain() {
        let pipe = into_a_pipe(Some("xterm-256color"), None);
        assert_eq!(pipe.mode(), Mode::Plain);
        assert_eq!(pipe.paint(Color::Auto), Paint::Plain);
    }

    #[test]
    fn a_dumb_terminal_is_plain() {
        for term in [Some("dumb"), Some(""), None] {
            let terminal = on_a_terminal(term, None);
            assert_eq!(terminal.mode(), Mode::Plain, "TERM is {term:?}");
            assert_eq!(
                terminal.paint(Color::Auto),
                Paint::Plain,
                "TERM is {term:?}"
            );
        }
    }

    #[test]
    fn no_color_turns_colour_off_and_keeps_the_status_line() {
        let terminal = on_a_terminal(Some("xterm"), Some("1"));
        assert_eq!(terminal.mode(), Mode::Live);
        assert_eq!(terminal.paint(Color::Auto), Paint::Plain);

        let empty = on_a_terminal(Some("xterm"), Some(""));
        assert_eq!(empty.paint(Color::Auto), Paint::Ansi);
    }

    #[test]
    fn always_colours_a_pipe() {
        let pipe = into_a_pipe(None, Some("1"));
        assert_eq!(pipe.paint(Color::Always), Paint::Ansi);
        assert_eq!(pipe.mode(), Mode::Plain);
    }

    #[test]
    fn never_leaves_a_terminal_uncoloured() {
        let terminal = on_a_terminal(Some("xterm"), None);
        assert_eq!(terminal.paint(Color::Never), Paint::Plain);
        assert_eq!(terminal.mode(), Mode::Live);
    }
}
