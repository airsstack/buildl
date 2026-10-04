//! Where a declaration came from, carried rather than reconstructed.
//!
//! Its own file because it is attached once, when a build file is evaluated, and then threaded
//! through every later value: no phase ever asks "which file declared this?", because it always
//! already knows. That is what lets an error name the declaring site.
//!
//! Responsibilities: [`Provenance`] and its two accessors.
//!
//! Non-responsibilities: the filesystem. The path is a value; nothing here reads it or asks
//! whether it exists.

use core::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::types::Directory;

/// The build file a declaration came from, and the directory it was evaluated in.
///
/// Construction is infallible: the directory is already validated and the path is a plain value.
/// A path that is not valid UTF-8 fails at serialization with `path contains invalid UTF-8
/// characters` rather than being silently mangled.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Provenance {
    file: PathBuf,
    directory: Directory,
}

impl Provenance {
    /// Records the file and the directory it was evaluated in.
    #[must_use]
    pub const fn new(file: PathBuf, directory: Directory) -> Self {
        Self { file, directory }
    }

    /// The build file that made the declaration.
    #[must_use]
    pub fn file(&self) -> &Path {
        &self.file
    }

    /// The directory the build file was evaluated in.
    #[must_use]
    pub const fn directory(&self) -> &Directory {
        &self.directory
    }
}

impl fmt::Display for Provenance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.file.display())
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use super::Provenance;
    use crate::types::Directory;
    use std::path::{Path, PathBuf};

    fn fixture() -> Provenance {
        Provenance::new(
            PathBuf::from("lib/text/build.lua"),
            Directory::parse("lib/text").unwrap(),
        )
    }

    #[test]
    fn exposes_the_file_and_the_declaring_directory() {
        let provenance = fixture();
        assert_eq!(provenance.file(), Path::new("lib/text/build.lua"));
        assert_eq!(provenance.directory().as_str(), "lib/text");
    }

    #[test]
    fn renders_as_the_declaring_file() {
        assert_eq!(fixture().to_string(), "lib/text/build.lua");
    }

    #[test]
    fn round_trips_through_json() {
        let provenance = fixture();
        let json = serde_json::to_string(&provenance).unwrap();
        assert_eq!(
            json,
            r#"{"file":"lib/text/build.lua","directory":"lib/text"}"#
        );
        assert_eq!(
            serde_json::from_str::<Provenance>(&json).unwrap(),
            provenance
        );
    }
}
