//! What an action left at one output path, said exactly: one file, or a directory of files.
//!
//! Its own file because this is what the digest of an output is taken of. An output is not only
//! bytes: it is a file that may or may not be run, or a whole directory. The manifest names all
//! of that, its canonical JSON is stored like any other content, and the digest of those bytes
//! is the one digest a cache row, a log event and every dependent's key hold for the output. So
//! two outputs share a digest exactly when they are the same files, with the same bytes and
//! modes, at the same paths.
//!
//! Responsibilities: [`OutputManifest`], its bytes, and reading it back from them.
//!
//! Non-responsibilities: storing the bytes and the files they name, which is a port's; and
//! deciding what an action may leave, which the phase that executes does.

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::json::canonical;
use crate::types::{StoredFile, Tree};

/// What one output is: a regular file, or a directory tree of regular files.
///
/// Serializes as an object of one key, `file` or `tree`. Reading one back goes through
/// [`Tree::new`], so a manifest read from bytes holds the same guarantees as one that was built.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputManifest {
    /// One regular file.
    File(StoredFile),
    /// A directory, and every file below it.
    Tree(Tree),
}

impl OutputManifest {
    /// The canonical JSON of the manifest: the bytes that are stored, and whose digest stands
    /// for the output.
    ///
    /// # Errors
    ///
    /// Returns [`Error::CanonicalJson`](crate::Error::CanonicalJson) when the manifest cannot be
    /// serialized.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        canonical::to_vec(self)
    }

    /// The manifest `bytes` hold; `None` when they are not exactly what
    /// [`to_bytes`](Self::to_bytes) writes for one.
    ///
    /// Read through [`canonical::from_slice`], so the manifest read back has the digest the
    /// bytes were stored under: a manifest written any other way is not one.
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        canonical::from_slice(bytes).ok()
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use super::OutputManifest;
    use crate::types::{Digest, FileMode, StoredFile, Tree, TreeEntry, TreePath};

    fn file(content: &str, mode: FileMode) -> StoredFile {
        StoredFile {
            digest: Digest::of(content.as_bytes()),
            mode,
        }
    }

    /// A tree of `(path, content, mode)` files, given in that order.
    fn tree(files: &[(&str, &str, FileMode)]) -> OutputManifest {
        let entries = files
            .iter()
            .map(|(path, content, mode)| TreeEntry {
                path: TreePath::parse(*path).unwrap(),
                digest: Digest::of(content.as_bytes()),
                mode: *mode,
            })
            .collect();
        OutputManifest::Tree(Tree::new(entries).unwrap())
    }

    fn digest(manifest: &OutputManifest) -> Digest {
        Digest::of(&manifest.to_bytes().unwrap())
    }

    const PLAIN: FileMode = FileMode::Plain;
    const RUNS: FileMode = FileMode::Executable;

    #[test]
    fn a_file_is_one_key_holding_its_digest_and_mode() {
        let manifest = OutputManifest::File(file("app", RUNS));
        assert_eq!(
            String::from_utf8(manifest.to_bytes().unwrap()).unwrap(),
            format!(
                r#"{{"file":{{"digest":"{}","mode":"executable"}}}}"#,
                Digest::of(b"app")
            )
        );
    }

    #[test]
    fn a_tree_is_one_key_holding_its_files_in_path_order() {
        let manifest = tree(&[("index.html", "page", PLAIN), ("bin/go", "go", RUNS)]);
        assert_eq!(
            String::from_utf8(manifest.to_bytes().unwrap()).unwrap(),
            format!(
                concat!(
                    r#"{{"tree":[{{"digest":"{go}","mode":"executable","path":"bin/go"}},"#,
                    r#"{{"digest":"{page}","mode":"plain","path":"index.html"}}]}}"#
                ),
                go = Digest::of(b"go"),
                page = Digest::of(b"page")
            )
        );
    }

    #[test]
    fn the_same_tree_has_the_same_digest_whatever_order_its_files_were_found_in() {
        let one = tree(&[
            ("index.html", "page", PLAIN),
            ("assets/app.js", "script", PLAIN),
            ("bin/go", "go", RUNS),
        ]);
        let other = tree(&[
            ("bin/go", "go", RUNS),
            ("index.html", "page", PLAIN),
            ("assets/app.js", "script", PLAIN),
        ]);
        assert_eq!(digest(&one), digest(&other));
        assert_eq!(digest(&one), digest(&one.clone()), "and every time");
    }

    #[test]
    fn anything_that_makes_two_outputs_differ_changes_the_digest() {
        let base = tree(&[("index.html", "page", PLAIN), ("bin/go", "go", RUNS)]);
        let changed = [
            (
                tree(&[("index.html", "page v2", PLAIN), ("bin/go", "go", RUNS)]),
                "the bytes of one file",
            ),
            (
                tree(&[("index.html", "page", PLAIN), ("bin/go", "go", PLAIN)]),
                "whether one file may be run",
            ),
            (
                tree(&[("index.htm", "page", PLAIN), ("bin/go", "go", RUNS)]),
                "the path of one file",
            ),
            (tree(&[("index.html", "page", PLAIN)]), "a file fewer"),
        ];
        for (other, what) in changed {
            assert_ne!(digest(&base), digest(&other), "{what}");
        }
    }

    #[test]
    fn a_file_that_may_be_run_is_not_the_same_output_as_one_that_may_not() {
        let plain = OutputManifest::File(file("app", PLAIN));
        let executable = OutputManifest::File(file("app", RUNS));
        assert_ne!(digest(&plain), digest(&executable));
    }

    #[test]
    fn a_file_is_not_the_same_output_as_a_tree_holding_only_that_file() {
        let alone = OutputManifest::File(file("app", PLAIN));
        let inside = tree(&[("app", "app", PLAIN)]);
        assert_ne!(digest(&alone), digest(&inside));
    }

    #[test]
    fn a_manifest_reads_back_from_its_bytes() {
        for manifest in [
            OutputManifest::File(file("app", PLAIN)),
            OutputManifest::File(file("app", RUNS)),
            tree(&[("index.html", "page", PLAIN), ("bin/go", "go", RUNS)]),
        ] {
            let bytes = manifest.to_bytes().unwrap();
            assert_eq!(OutputManifest::from_bytes(&bytes), Some(manifest));
        }
    }

    #[test]
    fn bytes_that_are_not_a_manifest_read_back_as_nothing() {
        let digest = Digest::of(b"a");
        let entry = format!(r#"{{"digest":"{digest}","mode":"plain","path":"a"}}"#);
        let escaping = format!(r#"{{"digest":"{digest}","mode":"plain","path":"../a"}}"#);
        for bytes in [
            String::new(),
            "compiling\n".to_owned(),
            "{}".to_owned(),
            r#"{"tree":[]}"#.to_owned(),
            format!(r#"{{"tree":[{entry},{entry}]}}"#),
            format!(r#"{{"tree":[{escaping}]}}"#),
            format!(r#"{{"link":{entry}}}"#),
            format!(r#"{{"file":{{"digest":"{digest}","mode":"setuid"}}}}"#),
            format!(r#"{{"file":{{"digest":"{digest}"}}}}"#),
        ] {
            assert_eq!(
                OutputManifest::from_bytes(bytes.as_bytes()),
                None,
                "{bytes}"
            );
        }
    }

    #[test]
    fn a_manifest_written_any_other_way_than_its_own_bytes_reads_back_as_nothing() {
        let a = Digest::of(b"a");
        let b = Digest::of(b"b");
        let entry = |path: &str, digest: Digest| {
            format!(r#"{{"digest":"{digest}","mode":"plain","path":"{path}"}}"#)
        };
        let sorted = format!(r#"{{"tree":[{},{}]}}"#, entry("a", a), entry("b", b));
        assert!(OutputManifest::from_bytes(sorted.as_bytes()).is_some());
        for bytes in [
            format!(r#"{{"tree":[{},{}]}}"#, entry("b", b), entry("a", a)),
            format!(r#"{{"file":{{"mode":"plain","digest":"{a}"}}}}"#),
            format!("{sorted}\n"),
            format!(r#"{{ "file":{{"digest":"{a}","mode":"plain"}}}}"#),
        ] {
            assert_eq!(
                OutputManifest::from_bytes(bytes.as_bytes()),
                None,
                "{bytes}"
            );
        }
    }
}
