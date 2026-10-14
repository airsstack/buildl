//! Whether what a build shows is coloured.
//!
//! Its own file because colour is decided once, by whoever knows where the text goes, and then
//! applied by several renderings: the header of a block, the word that marks a failed target,
//! the word that opens an error. One value carries the decision to all of them, so none of them
//! writes an escape sequence of its own.
//!
//! Responsibilities: [`Paint`] and the two styles it applies.
//!
//! Non-responsibilities: deciding whether to colour. That takes knowing whether the text goes
//! to a terminal and what the person asked for, which only the caller knows.

/// The sequence that makes what follows bold.
const BOLD: &str = "\x1b[1m";
/// The sequence that makes what follows bold and red.
const BOLD_RED: &str = "\x1b[1;31m";
/// The sequence that ends either style.
const RESET: &str = "\x1b[0m";

/// Whether text is styled with terminal colour sequences.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Paint {
    /// Text is written as it is: no escape sequence ever.
    Plain,
    /// A header is bold, and a word that marks a failure is bold red.
    Ansi,
}

impl Paint {
    /// `text` as the header of a block: bold under [`Paint::Ansi`], unchanged under
    /// [`Paint::Plain`].
    #[must_use]
    pub fn header(self, text: &str) -> String {
        self.styled(BOLD, text)
    }

    /// `text` as a word that marks a failure: bold red under [`Paint::Ansi`], unchanged under
    /// [`Paint::Plain`].
    #[must_use]
    pub fn failure(self, text: &str) -> String {
        self.styled(BOLD_RED, text)
    }

    /// `text` between `style` and the sequence that ends it, or alone when nothing is styled.
    fn styled(self, style: &str, text: &str) -> String {
        match self {
            Self::Plain => text.to_owned(),
            Self::Ansi => format!("{style}{text}{RESET}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Paint;

    #[test]
    fn plain_leaves_text_as_it_is() {
        assert_eq!(Paint::Plain.header("--- //:app"), "--- //:app");
        assert_eq!(Paint::Plain.failure("FAILED"), "FAILED");
    }

    #[test]
    fn a_header_is_bold() {
        assert_eq!(Paint::Ansi.header("--- //:app"), "\x1b[1m--- //:app\x1b[0m");
    }

    #[test]
    fn a_failure_is_bold_red() {
        assert_eq!(Paint::Ansi.failure("FAILED"), "\x1b[1;31mFAILED\x1b[0m");
    }

    #[test]
    fn styled_text_ends_its_style_even_when_empty() {
        assert_eq!(Paint::Ansi.header(""), "\x1b[1m\x1b[0m");
        assert_eq!(Paint::Plain.header(""), "");
    }
}
