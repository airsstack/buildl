//! An adapter's rendered message for a failure, carried for display only.
//!
//! Its own file because the message is the one piece of an evaluation failure that is not
//! structured: the structured part is the failure's kind, and this text only explains it to a
//! person. Nothing branches on it.
//!
//! Responsibilities: [`Diagnostic`] and its rendering.
//!
//! Non-responsibilities: classification. What went wrong is the failure's kind, never something
//! parsed out of this text.

use core::fmt;

/// A human-readable message an adapter attaches to a failure, such as a Lua error's text.
///
/// Any text is accepted: it is shown, never parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic(String);

impl Diagnostic {
    /// Wraps `text`.
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    /// The message as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::Diagnostic;

    #[test]
    fn keeps_and_renders_the_text_verbatim() {
        let diagnostic = Diagnostic::new("build.lua:3: unexpected symbol near '}'");
        assert_eq!(
            diagnostic.as_str(),
            "build.lua:3: unexpected symbol near '}'"
        );
        assert_eq!(
            diagnostic.to_string(),
            "build.lua:3: unexpected symbol near '}'"
        );
    }
}
