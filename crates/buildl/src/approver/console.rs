//! The person at the other end of the approver's question.
//!
//! Its own file because the interactive approver must ask a question and read an answer without
//! knowing whether real streams stand behind them: the pipeline's tests, and any build with no
//! person present, supply a stand-in.
//!
//! Responsibilities: [`Console`] and [`NoTerminal`].
//!
//! Non-responsibilities: reading the real standard streams, which the binary does.

use std::io;

/// Somewhere to ask a question and read one line back.
pub trait Console {
    /// Whether a person can be asked: the stream a question is written to and the stream an
    /// answer is read from are both terminals.
    fn is_terminal(&self) -> bool;

    /// Writes `prompt`, then reads one line, returned as read.
    ///
    /// # Errors
    ///
    /// Returns the I/O error of writing the prompt or reading the line.
    fn ask(&mut self, prompt: &str) -> io::Result<String>;
}

impl<C: Console + ?Sized> Console for &mut C {
    fn is_terminal(&self) -> bool {
        (**self).is_terminal()
    }

    fn ask(&mut self, prompt: &str) -> io::Result<String> {
        (**self).ask(prompt)
    }
}

/// A console with nobody at it, for a command that never asks.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoTerminal;

impl Console for NoTerminal {
    fn is_terminal(&self) -> bool {
        false
    }

    fn ask(&mut self, _prompt: &str) -> io::Result<String> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "there is no terminal to ask on",
        ))
    }
}

/// A console that answers from a script and keeps what it was asked, for tests.
#[cfg(test)]
#[derive(Debug)]
pub(crate) struct Scripted {
    terminal: bool,
    answers: std::collections::VecDeque<String>,
    /// Every prompt asked, in order.
    pub(crate) prompts: Vec<String>,
}

#[cfg(test)]
impl Scripted {
    /// A console that is a terminal and gives `answers` one by one.
    pub(crate) fn answering(answers: &[&str]) -> Self {
        Self {
            terminal: true,
            answers: answers.iter().map(|&a| a.to_owned()).collect(),
            prompts: Vec::new(),
        }
    }

    /// A console that is not a terminal.
    pub(crate) fn not_a_terminal() -> Self {
        Self {
            terminal: false,
            ..Self::answering(&[])
        }
    }
}

#[cfg(test)]
impl Console for Scripted {
    fn is_terminal(&self) -> bool {
        self.terminal
    }

    fn ask(&mut self, prompt: &str) -> io::Result<String> {
        self.prompts.push(prompt.to_owned());
        self.answers
            .pop_front()
            .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "no more answers"))
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::io::ErrorKind;

    use super::{Console, NoTerminal, Scripted};

    #[test]
    fn no_terminal_is_never_a_terminal_and_cannot_ask() {
        let mut console = NoTerminal;

        assert!(!console.is_terminal());
        let error = console.ask("allow? ").unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Unsupported);
    }

    /// Uses `console` only through the trait, as the interactive approver does.
    fn through_the_trait(mut console: impl Console) -> (bool, String) {
        (console.is_terminal(), console.ask("allow? ").unwrap())
    }

    #[test]
    fn a_borrowed_console_is_the_console() {
        let mut scripted = Scripted::answering(&["yes"]);

        assert_eq!(through_the_trait(&mut scripted), (true, "yes".to_owned()));

        assert_eq!(scripted.prompts, ["allow? "]);
    }
}
