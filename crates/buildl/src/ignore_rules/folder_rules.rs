//! One folder's `.buildlignore`, read and parsed.
//!
//! Its own file because reading a file, refusing one that cannot be used and turning its lines
//! into matchers is one job, whose answer for a path relative to the folder the caller combines
//! with the answers of other folders.
//!
//! Responsibilities: [`FolderRules`], [`Decision`], and the escaping of braces before a line is
//! handed to the matcher.
//!
//! Non-responsibilities: which folders bear on a path, and the order they are asked in.

use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use buildl_core::{Error, Ignoring, Result};
use ignore::Match;
use ignore::gitignore::{Gitignore, GitignoreBuilder};

/// The name of an ignore file.
const FILE_NAME: &str = ".buildlignore";

/// What one folder's file says about a path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Decision {
    /// A pattern ignores the path.
    Ignored(Ignoring),
    /// A `!` line re-includes the path.
    Included,
    /// No line matches.
    NoMatch,
}

/// The parsed `.buildlignore` of one folder.
pub(super) struct FolderRules {
    file: String,
    gitignore: Gitignore,
    /// Each line as the matcher was given it, mapped to the line as written. Different written
    /// lines that share an escaped form match the same paths, so the last one stands for them.
    written: HashMap<String, String>,
}

impl FolderRules {
    /// Reads `<folder>/.buildlignore`, where `folder` is workspace-relative and `/`-separated
    /// (empty for the root) and `root` is the workspace root.
    ///
    /// Returns `None` when the folder has no such file.
    ///
    /// # Errors
    ///
    /// Returns [`Error::IgnoreFile`] when the file is a symlink, cannot be read, is not UTF-8, or
    /// holds a line that does not parse as a pattern.
    pub(super) fn read(root: &Path, folder: &str) -> Result<Option<Self>> {
        let file = if folder.is_empty() {
            FILE_NAME.to_owned()
        } else {
            format!("{folder}/{FILE_NAME}")
        };
        let on_disk = root.join(&file);
        match fs::symlink_metadata(&on_disk) {
            Ok(meta) if meta.is_symlink() => {
                return Err(refuse(&file, None, "a symlink, not a file".to_owned()));
            }
            Ok(_) => {}
            Err(err) if matches!(err.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) => {
                return Ok(None);
            }
            Err(err) => return Err(refuse(&file, None, err.to_string())),
        }
        let bytes = fs::read(&on_disk).map_err(|err| refuse(&file, None, err.to_string()))?;
        let text = std::str::from_utf8(&bytes).map_err(|_| {
            let line = bytes
                .split(|byte| *byte == b'\n')
                .position(|line| std::str::from_utf8(line).is_err())
                .map(line_number);
            refuse(&file, line, "not UTF-8".to_owned())
        })?;
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        Self::parse(&root.join(folder), file, text).map(Some)
    }

    /// Whether this file ignores, re-includes or says nothing about `relative`, a path relative
    /// to the file's folder. `is_folder` is true when the path is a folder.
    #[must_use]
    pub(super) fn decide(&self, relative: &str, is_folder: bool) -> Decision {
        match self.gitignore.matched(relative, is_folder) {
            Match::None => Decision::NoMatch,
            Match::Whitelist(_) => Decision::Included,
            Match::Ignore(glob) => {
                let pattern = self
                    .written
                    .get(glob.original())
                    .map_or_else(|| glob.original().to_owned(), Clone::clone);
                Decision::Ignored(Ignoring::new(self.file.clone(), pattern))
            }
        }
    }

    fn parse(folder: &Path, file: String, text: &str) -> Result<Self> {
        let mut builder = GitignoreBuilder::new(folder);
        let mut written = HashMap::new();
        for (index, line) in text.lines().enumerate() {
            let escaped = escape_braces(line);
            if let Err(err) = builder.add_line(Some(PathBuf::from(&file)), &escaped) {
                let as_written = trimmed(line);
                let reason = match err {
                    ignore::Error::Glob { err, .. } => {
                        format!("error parsing glob '{as_written}': {err}")
                    }
                    other => other.to_string(),
                };
                return Err(refuse(&file, Some(line_number(index)), reason));
            }
            written.insert(trimmed(&escaped).to_owned(), trimmed(line).to_owned());
        }
        let gitignore = builder
            .build()
            .map_err(|err| refuse(&file, None, err.to_string()))?;
        Ok(Self {
            file,
            gitignore,
            written,
        })
    }
}

/// The error of an ignore file that cannot be used.
fn refuse(file: &str, line: Option<u32>, reason: String) -> Error {
    Error::IgnoreFile {
        file: file.to_owned(),
        line,
        reason,
    }
}

/// The one-based number of the line at a zero-based index.
fn line_number(index: usize) -> u32 {
    u32::try_from(index + 1).unwrap_or(u32::MAX)
}

/// The line as the matcher trims it: trailing whitespace goes unless the line ends in an escaped
/// space.
fn trimmed(line: &str) -> &str {
    if line.ends_with("\\ ") {
        line
    } else {
        line.trim_end()
    }
}

/// Escapes every `{` and `}` that is not already escaped, so braces match themselves.
///
/// Read left to right, a backslash escapes the character after it and the pair is left alone.
fn escape_braces(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                out.push(c);
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            }
            '{' | '}' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use buildl_core::Error;

    use super::{Decision, FolderRules, escape_braces, trimmed};

    fn parsed(text: &str) -> FolderRules {
        match FolderRules::parse(Path::new("/ws"), ".buildlignore".to_owned(), text) {
            Ok(rules) => rules,
            Err(err) => unreachable!("fixture does not parse: {err}"),
        }
    }

    #[test]
    fn a_bare_brace_is_escaped() {
        assert_eq!(escape_braces("{a,b}"), "\\{a,b\\}");
    }

    #[test]
    fn an_escaped_brace_is_left_alone() {
        assert_eq!(escape_braces("\\{a"), "\\{a");
    }

    #[test]
    fn a_brace_after_an_escaped_backslash_is_escaped() {
        assert_eq!(escape_braces("\\\\{a"), "\\\\\\{a");
    }

    #[test]
    fn a_trailing_backslash_is_kept() {
        assert_eq!(escape_braces("a\\"), "a\\");
    }

    #[test]
    fn trailing_spaces_go_unless_escaped() {
        assert_eq!(trimmed("a  "), "a");
        assert_eq!(trimmed("a\\ "), "a\\ ");
    }

    #[test]
    fn a_bad_line_with_braces_is_reported_as_written() {
        let refused = FolderRules::parse(Path::new("/ws"), ".buildlignore".to_owned(), "{[z-a]\n");
        let Err(err @ Error::IgnoreFile { line: Some(1), .. }) = refused else {
            unreachable!("expected an IgnoreFile error on line 1");
        };
        assert!(
            err.to_string()
                .starts_with(".buildlignore:1: error parsing glob '{[z-a]': "),
            "{err}"
        );
    }

    #[test]
    fn a_pattern_is_reported_without_its_trailing_spaces() {
        let rules = parsed("{a  \n");
        let Decision::Ignored(ignoring) = rules.decide("{a", false) else {
            unreachable!("expected {{a to be ignored");
        };
        assert_eq!(ignoring.pattern(), "{a");
    }
}
