//! The answer to an ignore question, and the path a refusal names.
//!
//! Its own file because both values say what an ignore file decided: [`Ignoring`] which file and
//! which pattern did, [`IgnoredPath`] which declared path it covered.
//!
//! Responsibilities: [`Ignoring`], [`IgnoredPath`] and the rendering of the path.
//!
//! Non-responsibilities: reading ignore files and matching patterns, which an ignore-rules
//! adapter does behind a port.

use core::fmt;

use crate::types::{Directory, SourcePath};

/// The ignore file whose pattern decided that a path is ignored, and that pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ignoring {
    file: String,
    pattern: String,
}

impl Ignoring {
    /// Wraps the workspace-relative, `/`-separated path of the ignore file and the pattern as it
    /// is written there.
    ///
    /// The file is text, not a [`SourcePath`], because the folder holding it may have a name the
    /// source-path grammar refuses.
    pub fn new(file: impl Into<String>, pattern: impl Into<String>) -> Self {
        Self {
            file: file.into(),
            pattern: pattern.into(),
        }
    }

    /// The workspace-relative, `/`-separated path of the ignore file.
    #[must_use]
    pub fn file(&self) -> &str {
        &self.file
    }

    /// The pattern, as written in the ignore file.
    #[must_use]
    pub fn pattern(&self) -> &str {
        &self.pattern
    }
}

/// A path a declaration named that an ignore pattern covers.
///
/// One value rather than a field and a path, so the kind of declaration and the path cannot
/// disagree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IgnoredPath {
    /// A file a target reads.
    Input(SourcePath),
    /// A directory a build file hands to the loader.
    Subdir(Directory),
}

impl fmt::Display for IgnoredPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Input(path) => path.fmt(f),
            Self::Subdir(directory) => directory.fmt(f),
        }
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use super::{IgnoredPath, Ignoring};
    use crate::types::{Directory, SourcePath};

    #[test]
    fn an_ignoring_keeps_its_file_and_pattern() {
        let ignoring = Ignoring::new("notes/.buildlignore", "drafts/");
        assert_eq!(ignoring.file(), "notes/.buildlignore");
        assert_eq!(ignoring.pattern(), "drafts/");
    }

    #[test]
    fn an_ignored_path_displays_as_its_path() {
        let input = IgnoredPath::Input(SourcePath::parse("notes/a.md").unwrap());
        let subdir = IgnoredPath::Subdir(Directory::parse("examples").unwrap());
        assert_eq!(input.to_string(), "notes/a.md");
        assert_eq!(subdir.to_string(), "examples");
    }
}
