//! The `Directory` tree of a remote action's input root.
//!
//! The files of an action are laid out as the canonical REAPI tree: one `Directory` per
//! directory of the input root, its files and subdirectories each sorted by name bytes, and a
//! subdirectory named by the digest of its child's encoding. The digest of the root `Directory`
//! is the action's `input_root_digest`.
//!
//! Responsibilities: [`input_tree`] and the [`InputTree`] it returns.
//!
//! Non-responsibilities: uploading the `Directory` messages, and checking that the bytes of a
//! file still match the digest it was given.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use buildl_core::{Diagnostic, Error, FileMode, PortName, RemoteFile};
use prost::Message;

use super::proto::build::bazel::remote::execution::v2::{
    Digest, Directory, DirectoryNode, FileNode,
};

/// The input root of an action as REAPI `Directory` messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InputTree {
    /// The digest of the root `Directory`.
    pub root: Digest,
    /// Every distinct `Directory` of the tree, each with its encoding, children before parents.
    pub directories: Vec<(Digest, Vec<u8>)>,
}

/// One directory of the input root before it is encoded.
#[derive(Default)]
struct Node {
    files: BTreeMap<String, FileNode>,
    directories: BTreeMap<String, Self>,
}

impl Node {
    fn insert(&mut self, names: &[&str], file: FileNode) {
        match names {
            [] => {}
            [name] => {
                self.files.insert((*name).to_owned(), file);
            }
            [first, rest @ ..] => {
                self.directories
                    .entry((*first).to_owned())
                    .or_default()
                    .insert(rest, file);
            }
        }
    }

    /// Encodes the children, then this directory, pushing each new one onto `out`, and returns
    /// this directory's digest.
    fn encode(self, out: &mut Vec<(Digest, Vec<u8>)>, seen: &mut BTreeSet<String>) -> Digest {
        let directories = self
            .directories
            .into_iter()
            .map(|(name, child)| DirectoryNode {
                name,
                digest: Some(child.encode(out, seen)),
            })
            .collect();
        let directory = Directory {
            files: self.files.into_values().collect(),
            directories,
            symlinks: Vec::new(),
            node_properties: None,
        };
        let bytes = directory.encode_to_vec();
        let digest = digest_of(&bytes);
        if seen.insert(digest.hash.clone()) {
            out.push((digest.clone(), bytes));
        }
        digest
    }
}

/// Builds the `Directory` tree of `files`, which sit under `root` on disk.
///
/// A file's size is read from disk. Its executable bit is the file's [`FileMode`] when it has
/// one, and otherwise comes from the file's permissions on disk: executable when any execute
/// bit is set.
///
/// # Errors
///
/// [`Error::Adapter`] naming the remote cache and the path when a file cannot be read from disk
/// under `root`.
pub(crate) fn input_tree(root: &Path, files: &[RemoteFile]) -> Result<InputTree, Error> {
    let mut top = Node::default();
    for file in files {
        let names: Vec<&str> = file.path.as_str().split('/').collect();
        top.insert(&names, file_node(root, file, names.last().copied())?);
    }
    let mut directories = Vec::new();
    let root = top.encode(&mut directories, &mut BTreeSet::new());
    Ok(InputTree { root, directories })
}

fn file_node(root: &Path, file: &RemoteFile, name: Option<&str>) -> Result<FileNode, Error> {
    let path = file.path.as_str();
    let metadata = fs::metadata(root.join(path))
        .map_err(|error| failure(&format!("cannot read input file {path}: {error}")))?;
    let size_bytes = i64::try_from(metadata.len())
        .map_err(|_| failure(&format!("input file {path} is too large")))?;
    let mode = file
        .mode
        .unwrap_or_else(|| FileMode::of_unix_bits(metadata.permissions().mode()));
    Ok(FileNode {
        name: name.unwrap_or_default().to_owned(),
        digest: Some(Digest {
            hash: file.digest.to_string(),
            size_bytes,
        }),
        is_executable: mode == FileMode::Executable,
        node_properties: None,
    })
}

fn digest_of(bytes: &[u8]) -> Digest {
    Digest {
        hash: buildl_core::Digest::of(bytes).to_string(),
        size_bytes: i64::try_from(bytes.len()).unwrap_or(i64::MAX),
    }
}

fn failure(text: &str) -> Error {
    Error::Adapter {
        port: PortName::RemoteCache,
        diagnostic: Diagnostic::new(text),
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    use buildl_core::{FileMode, RemoteFile, RootPath, SourcePath};
    use prost::Message;

    use super::{InputTree, input_tree};
    use crate::remote::proto::build::bazel::remote::execution::v2::{Digest, Directory};

    fn write(root: &Path, path: &str, bytes: &[u8], mode: u32) -> RemoteFile {
        let full = root.join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(&full, bytes).unwrap();
        fs::set_permissions(&full, fs::Permissions::from_mode(mode)).unwrap();
        RemoteFile {
            path: RootPath::from(&SourcePath::parse(path).unwrap()),
            digest: buildl_core::Digest::of(bytes),
            mode: None,
        }
    }

    fn decode(tree: &InputTree, digest: &Digest) -> Directory {
        let (_, bytes) = tree
            .directories
            .iter()
            .find(|(held, _)| held == digest)
            .unwrap();
        Directory::decode(bytes.as_slice()).unwrap()
    }

    fn root_of(tree: &InputTree) -> Directory {
        decode(tree, &tree.root)
    }

    #[test]
    fn files_in_one_directory_are_sorted_by_name() {
        let dir = tempfile::tempdir().unwrap();
        let files = vec![
            write(dir.path(), "b.c", b"bb", 0o644),
            write(dir.path(), "a.c", b"a", 0o644),
            write(dir.path(), "Z.c", b"ZZZ", 0o644),
        ];
        let tree = input_tree(dir.path(), &files).unwrap();
        let root = root_of(&tree);
        let names: Vec<_> = root.files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["Z.c", "a.c", "b.c"]);
        let a = &root.files[1];
        let digest = a.digest.as_ref().unwrap();
        assert_eq!(digest.hash, buildl_core::Digest::of(b"a").to_string());
        assert_eq!(digest.size_bytes, 1);
        assert_eq!(root.files[2].digest.as_ref().unwrap().size_bytes, 2);
    }

    #[test]
    fn nested_paths_become_nested_directories_whose_digests_chain() {
        let dir = tempfile::tempdir().unwrap();
        let files = vec![
            write(dir.path(), "top.c", b"t", 0o644),
            write(dir.path(), "src/lib/b.c", b"b", 0o644),
            write(dir.path(), "src/a.c", b"a", 0o644),
        ];
        let tree = input_tree(dir.path(), &files).unwrap();

        let root = root_of(&tree);
        assert_eq!(root.files.len(), 1);
        assert_eq!(root.directories.len(), 1);
        let src_node = &root.directories[0];
        assert_eq!(src_node.name, "src");
        let src_digest = src_node.digest.as_ref().unwrap();
        let src = decode(&tree, src_digest);
        assert_eq!(src.files[0].name, "a.c");
        let lib_node = &src.directories[0];
        assert_eq!(lib_node.name, "lib");
        let lib = decode(&tree, lib_node.digest.as_ref().unwrap());
        assert_eq!(lib.files[0].name, "b.c");

        for (digest, bytes) in &tree.directories {
            assert_eq!(digest.hash, buildl_core::Digest::of(bytes).to_string());
            assert_eq!(usize::try_from(digest.size_bytes).unwrap(), bytes.len());
        }
        assert_eq!(tree.directories.len(), 3);
    }

    #[test]
    fn an_input_executable_on_disk_is_marked_executable() {
        let dir = tempfile::tempdir().unwrap();
        let files = vec![
            write(dir.path(), "run.sh", b"#!/bin/sh", 0o755),
            write(dir.path(), "data.txt", b"x", 0o644),
        ];
        let root = root_of(&input_tree(dir.path(), &files).unwrap());
        let flags: Vec<_> = root
            .files
            .iter()
            .map(|f| (f.name.as_str(), f.is_executable))
            .collect();
        assert_eq!(flags, [("data.txt", false), ("run.sh", true)]);
    }

    #[test]
    fn a_dependency_file_keeps_its_manifest_mode() {
        let dir = tempfile::tempdir().unwrap();
        let mut run = write(dir.path(), "run", b"r", 0o644);
        run.mode = Some(FileMode::Executable);
        let mut data = write(dir.path(), "data", b"d", 0o755);
        data.mode = Some(FileMode::Plain);
        let root = root_of(&input_tree(dir.path(), &[run, data]).unwrap());
        let flags: Vec<_> = root
            .files
            .iter()
            .map(|f| (f.name.as_str(), f.is_executable))
            .collect();
        assert_eq!(flags, [("data", false), ("run", true)]);
    }

    #[test]
    fn the_same_files_give_the_same_root_digest() {
        let dir = tempfile::tempdir().unwrap();
        let files = vec![
            write(dir.path(), "src/a.c", b"a", 0o644),
            write(dir.path(), "b.c", b"b", 0o755),
        ];
        let first = input_tree(dir.path(), &files).unwrap();
        let second = input_tree(dir.path(), &files).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn a_missing_file_names_its_path() {
        let dir = tempfile::tempdir().unwrap();
        let mut files = vec![write(dir.path(), "here.c", b"h", 0o644)];
        files.push(RemoteFile {
            path: RootPath::from(&SourcePath::parse("src/gone.c").unwrap()),
            digest: buildl_core::Digest::of(b"gone"),
            mode: None,
        });
        let error = input_tree(dir.path(), &files).unwrap_err();
        let text = error.to_string();
        assert!(text.starts_with("remote cache:"), "{text}");
        assert!(text.contains("src/gone.c"), "{text}");
    }
}
