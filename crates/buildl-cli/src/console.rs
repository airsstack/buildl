//! The person at the terminal, as the approver sees them.
//!
//! Its own file because asking is the one place the binary reads standard input: a build asks
//! on standard error, so the prompt never mixes into standard output, and reads the answer from
//! standard input.
//!
//! Responsibilities: [`StdConsole`].
//!
//! Non-responsibilities: deciding what to ask or what an answer means, which the approver does.
//!
//! The rule for when asking is possible is tested; the rest only forwards to the process's real
//! streams and holds no logic of its own.

use std::io::{self, IsTerminal, Write};

use buildl::approver::Console;

/// Whether a person can be asked: the question goes to standard error and the answer comes from
/// standard input, so a terminal on one alone is not enough.
const fn both_terminals(stdin: bool, stderr: bool) -> bool {
    stdin && stderr
}

/// Standard error to ask on, standard input to read the answer from.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct StdConsole;

impl Console for StdConsole {
    /// Whether both streams are terminals: a question written where nobody reads it, or an
    /// answer read from a pipe, is not a conversation.
    fn is_terminal(&self) -> bool {
        both_terminals(io::stdin().is_terminal(), io::stderr().is_terminal())
    }

    /// Writes `prompt` to standard error, then reads one line from standard input, returned as
    /// read. Input that ends at once gives an empty line.
    fn ask(&mut self, prompt: &str) -> io::Result<String> {
        let mut stderr = io::stderr().lock();
        stderr.write_all(prompt.as_bytes())?;
        stderr.flush()?;
        let mut line = String::new();
        io::stdin().read_line(&mut line)?;
        Ok(line)
    }
}

#[cfg(test)]
mod tests {
    use super::both_terminals;

    #[test]
    fn one_terminal_alone_is_not_enough_to_ask() {
        assert!(both_terminals(true, true));
        assert!(!both_terminals(true, false));
        assert!(!both_terminals(false, true));
        assert!(!both_terminals(false, false));
    }
}
