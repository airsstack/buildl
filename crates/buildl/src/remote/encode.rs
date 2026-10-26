//! REAPI messages from buildl's remote values, and back.
//!
//! [`command`], [`platform`] and [`action`] turn a [`RemoteAction`] into the messages whose
//! digests name it to a cache; [`action_result`] turns what a run produced into the
//! `ActionResult` stored for it, with the `Tree` of each output directory; [`hit`] and
//! [`tree_files`] read a cache's answer back into the values the core checks.
//!
//! A digest is the SHA-256 of a message's encoding, and prost writes fields in tag order, so
//! the same value gives the same digest every time. Every list a digest covers is put in the
//! order the protocol asks for before it is encoded.
//!
//! Responsibilities: encoding and decoding messages, and the digests of what is encoded.
//!
//! Non-responsibilities: the input root's `Directory` messages, which `input_tree` builds;
//! calling the cache; deciding whether an answer is usable, which is the core's.

use std::collections::BTreeMap;

use buildl_core::{
    Blob, DecodedTree, Digest, FileMode, RemoteAction, RemoteContent, RemoteHit,
    RemoteOutputDirectory, RemoteOutputFile, RemotePlatform, RemoteTreeFile, RemoteUpload, Timeout,
    TreePath,
};
use prost::Message;

use super::error::RemoteError;
use super::proto::build::bazel::remote::execution::v2 as wire;
use super::vocabulary::Method;

/// A message in its wire form, with the blob its bytes make.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Encoded {
    /// The digest and size of `bytes`.
    pub(crate) blob: Blob,
    /// The message as prost encodes it.
    pub(crate) bytes: Vec<u8>,
}

impl Encoded {
    /// `message` encoded, with the blob its encoding makes.
    pub(crate) fn of<M: Message>(message: &M) -> Self {
        let bytes = message.encode_to_vec();
        Self {
            blob: Blob::of(&bytes),
            bytes,
        }
    }

    /// The protocol's digest of the encoding.
    pub(crate) fn digest(&self) -> wire::Digest {
        wire::Digest {
            hash: self.blob.digest.to_string(),
            // A `Vec` holds at most `isize::MAX` bytes, so the length of `bytes` is below `i64::MAX`.
            size_bytes: self.blob.size.cast_signed(),
        }
    }
}

/// What an upload sends as the result of an action: the `ActionResult`, and the `Tree` of each
/// directory output, in the order the outputs were given.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ResultMessages {
    /// The result to store under the action's digest.
    pub(crate) result: wire::ActionResult,
    /// One `Tree` for each output directory, which the cache must also hold.
    pub(crate) trees: Vec<Encoded>,
}

/// `blob` as the protocol's digest.
///
/// # Errors
///
/// Returns [`RemoteError::TooLarge`] when the blob is larger than the protocol's signed size can
/// state.
pub(crate) fn wire_digest(blob: &Blob) -> Result<wire::Digest, RemoteError> {
    let size_bytes =
        i64::try_from(blob.size).map_err(|_| RemoteError::TooLarge { size: blob.size })?;
    Ok(wire::Digest {
        hash: blob.digest.to_string(),
        size_bytes,
    })
}

/// The answer broke the protocol in the way `detail` says.
const fn protocol(detail: String) -> RemoteError {
    RemoteError::Protocol {
        call: Method::GetActionResult,
        detail,
    }
}

/// The blob a digest the cache sent names; `what` says what the digest is of.
fn blob_of(digest: Option<&wire::Digest>, what: &str) -> Result<Blob, RemoteError> {
    let digest = digest.ok_or_else(|| protocol(format!("{what} has no digest")))?;
    let hash = Digest::from_hex(&digest.hash).map_err(|_| {
        protocol(format!(
            "{what} has a digest that is not SHA-256: {}",
            digest.hash
        ))
    })?;
    let size = u64::try_from(digest.size_bytes)
        .map_err(|_| protocol(format!("{what} has a negative size: {}", digest.size_bytes)))?;
    Ok(Blob { digest: hash, size })
}

/// The `Command` of `action`: its words, its environment sorted by name, its output paths as
/// given, and no working directory, since the input root is the workspace root.
pub(crate) fn command(action: &RemoteAction) -> wire::Command {
    let mut environment_variables: Vec<wire::command::EnvironmentVariable> = action
        .env
        .iter()
        .map(|(name, value)| wire::command::EnvironmentVariable {
            name: name.as_str().to_owned(),
            value: value.as_str().to_owned(),
        })
        .collect();
    environment_variables.sort_by(|left, right| left.name.cmp(&right.name));
    wire::Command {
        arguments: action
            .command
            .arguments()
            .iter()
            .map(|argument| argument.as_str().to_owned())
            .collect(),
        environment_variables,
        output_paths: action
            .outputs
            .iter()
            .map(|path| path.as_str().to_owned())
            .collect(),
        working_directory: String::new(),
        ..wire::Command::default()
    }
}

/// The `Platform` of `action`: its `ISA` and `OSFamily` and the workspace's own properties,
/// sorted by name and then value, as the protocol requires.
///
/// Rust compares `str` by UTF-8 bytes, the order the protocol names, so upper-case names such as
/// `ISA` come before lower-case ones.
pub(crate) fn platform(action: &RemoteAction) -> wire::Platform {
    let property = |name: &str, value: &str| wire::platform::Property {
        name: name.to_owned(),
        value: value.to_owned(),
    };
    let mut properties = vec![
        property(RemotePlatform::ISA_PROPERTY, action.platform.isa()),
        property(
            RemotePlatform::OS_FAMILY_PROPERTY,
            action.platform.os_family(),
        ),
    ];
    properties.extend(
        action
            .properties
            .iter()
            .map(|(name, value)| property(name.as_str(), value)),
    );
    properties.sort_by(|left, right| (&left.name, &left.value).cmp(&(&right.name, &right.value)));
    wire::Platform { properties }
}

/// `timeout` as the protocol's duration.
fn duration(timeout: Timeout) -> prost_types::Duration {
    let length = timeout.as_duration();
    prost_types::Duration {
        seconds: i64::try_from(length.as_secs()).unwrap_or(i64::MAX),
        nanos: length.subsec_nanos().cast_signed(),
    }
}

/// The `Action` for `remote`, whose `Command` and input root have the given digests.
///
/// The salt is the 32 bytes of the target's action key, so two targets with different keys
/// never share an `Action` whatever else they hold.
pub(crate) fn action(
    command_digest: &wire::Digest,
    input_root_digest: &wire::Digest,
    remote: &RemoteAction,
) -> wire::Action {
    wire::Action {
        command_digest: Some(command_digest.clone()),
        input_root_digest: Some(input_root_digest.clone()),
        timeout: Some(duration(remote.timeout)),
        salt: remote.salt.as_bytes().to_vec(),
        platform: Some(platform(remote)),
        ..wire::Action::default()
    }
}

/// The files and directories of an output directory, before they become messages.
#[derive(Default)]
struct Node {
    files: BTreeMap<String, (Blob, FileMode)>,
    directories: BTreeMap<String, Self>,
}

/// `node` as a `Directory`, with every directory below it appended to `below` parents first, in
/// name order.
fn directory(
    node: &Node,
    below: &mut Vec<wire::Directory>,
) -> Result<wire::Directory, RemoteError> {
    let files = node
        .files
        .iter()
        .map(|(name, (blob, mode))| {
            Ok(wire::FileNode {
                name: name.clone(),
                digest: Some(wire_digest(blob)?),
                is_executable: *mode == FileMode::Executable,
                ..wire::FileNode::default()
            })
        })
        .collect::<Result<_, RemoteError>>()?;
    let mut directories = Vec::new();
    for (name, child) in &node.directories {
        let mut deeper = Vec::new();
        let message = directory(child, &mut deeper)?;
        directories.push(wire::DirectoryNode {
            name: name.clone(),
            digest: Some(Encoded::of(&message).digest()),
        });
        below.push(message);
        below.append(&mut deeper);
    }
    Ok(wire::Directory {
        files,
        directories,
        ..wire::Directory::default()
    })
}

/// The `Tree` holding `files`: the root directory, and every directory below it parents first
/// and in name order, so the same files always give the same bytes.
fn tree(files: &[RemoteTreeFile]) -> Result<wire::Tree, RemoteError> {
    let mut root = Node::default();
    for file in files {
        let mut node = &mut root;
        let mut segments = file.path.as_str().split('/').peekable();
        while let Some(segment) = segments.next() {
            if segments.peek().is_some() {
                node = node.directories.entry(segment.to_owned()).or_default();
            } else {
                node.files
                    .insert(segment.to_owned(), (file.blob, file.mode));
            }
        }
    }
    let mut children = Vec::new();
    let root = directory(&root, &mut children)?;
    Ok(wire::Tree {
        root: Some(root),
        children,
    })
}

/// The `ActionResult` that stores what `upload` holds, with the `Tree` of each directory output.
///
/// The exit code is 0: only an action that succeeded is uploaded.
///
/// # Errors
///
/// Returns [`RemoteError::TooLarge`] when a blob of the upload is larger than a digest can name.
pub(crate) fn action_result(upload: &RemoteUpload) -> Result<ResultMessages, RemoteError> {
    let mut output_files = Vec::new();
    let mut output_directories = Vec::new();
    let mut trees = Vec::new();
    for output in &upload.outputs {
        let path = output.path.as_str().to_owned();
        match &output.content {
            RemoteContent::File { blob, mode } => output_files.push(wire::OutputFile {
                path,
                digest: Some(wire_digest(blob)?),
                is_executable: *mode == FileMode::Executable,
                ..wire::OutputFile::default()
            }),
            RemoteContent::Tree(files) => {
                let encoded = Encoded::of(&tree(files.files())?);
                output_directories.push(wire::OutputDirectory {
                    path,
                    tree_digest: Some(encoded.digest()),
                    ..wire::OutputDirectory::default()
                });
                trees.push(encoded);
            }
        }
    }
    Ok(ResultMessages {
        result: wire::ActionResult {
            output_files,
            output_directories,
            exit_code: 0,
            stdout_digest: Some(wire_digest(&upload.stdout)?),
            stderr_digest: Some(wire_digest(&upload.stderr)?),
            ..wire::ActionResult::default()
        },
        trees,
    })
}

/// The paths `result` names as output symlinks, from any of the three lists that can hold one,
/// each once and in path order.
#[expect(
    deprecated,
    reason = "a v2.0 server reports symlinks only in these fields"
)]
fn symlink_paths(result: &wire::ActionResult) -> Vec<String> {
    let mut paths: Vec<String> = result
        .output_symlinks
        .iter()
        .chain(&result.output_file_symlinks)
        .chain(&result.output_directory_symlinks)
        .map(|symlink| symlink.path.clone())
        .collect();
    paths.sort_unstable();
    paths.dedup();
    paths
}

/// The mode a file node's executable bit gives.
const fn mode_of(is_executable: bool) -> FileMode {
    if is_executable {
        FileMode::Executable
    } else {
        FileMode::Plain
    }
}

/// The answer a cache gave, as the values the core checks: every output file and directory as
/// the server wrote it, and the paths it names as output symlinks.
///
/// # Errors
///
/// Returns [`RemoteError::Protocol`] when a digest the result holds is missing, is not
/// SHA-256 or has a negative size.
pub(crate) fn hit(result: &wire::ActionResult) -> Result<RemoteHit, RemoteError> {
    let files = result
        .output_files
        .iter()
        .map(|file| {
            Ok(RemoteOutputFile {
                path: file.path.clone(),
                blob: blob_of(file.digest.as_ref(), &format!("output file {}", file.path))?,
                mode: mode_of(file.is_executable),
            })
        })
        .collect::<Result<Vec<_>, RemoteError>>()?;
    let directories = result
        .output_directories
        .iter()
        .map(|directory| {
            Ok(RemoteOutputDirectory {
                path: directory.path.clone(),
                tree: blob_of(
                    directory.tree_digest.as_ref(),
                    &format!("output directory {}", directory.path),
                )?,
            })
        })
        .collect::<Result<Vec<_>, RemoteError>>()?;
    let log = |digest: Option<&wire::Digest>, what: &str| {
        digest.map(|digest| blob_of(Some(digest), what)).transpose()
    };
    Ok(RemoteHit {
        exit_code: result.exit_code,
        files,
        directories,
        symlinks: symlink_paths(result),
        stdout: log(result.stdout_digest.as_ref(), "stdout")?,
        stderr: log(result.stderr_digest.as_ref(), "stderr")?,
    })
}

/// The most files and directories a `Tree` may expand to.
///
/// A `Tree` names its children by digest, so several directories may point at one child, and a
/// small tree can expand to far more paths than it holds messages. Past this many the tree is
/// not used.
const MAX_TREE_ENTRIES: usize = 1 << 18;

/// What the `Tree` that `bytes` encode holds.
///
/// [`DecodedTree::Files`] holds its regular files in path order. A symlink, or a directory that
/// holds nothing (the root included), is named by the first such path in path order. The bytes
/// are [`DecodedTree::Unreadable`] when they are not a `Tree`, when it refers to a directory it
/// does not hold, when a name in it is not one `TreePath` takes, or when it expands to more than
/// [`MAX_TREE_ENTRIES`] files and directories.
pub(crate) fn tree_files(bytes: &[u8]) -> DecodedTree {
    let Ok(tree) = wire::Tree::decode(bytes) else {
        return DecodedTree::Unreadable;
    };
    let Some(root) = tree.root.as_ref() else {
        return DecodedTree::Unreadable;
    };
    read_tree(&tree, root).unwrap_or(DecodedTree::Unreadable)
}

/// Something in a tree a build cannot place.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Unplaceable {
    Symlink,
    EmptyDirectory,
}

/// The contents of `tree`, below `root`; `None` when it cannot be read as a directory.
fn read_tree(tree: &wire::Tree, root: &wire::Directory) -> Option<DecodedTree> {
    let by_blob: BTreeMap<Blob, &wire::Directory> = tree
        .children
        .iter()
        .map(|child| (Encoded::of(child).blob, child))
        .collect();
    let mut files = Vec::new();
    let mut unplaceable: Vec<(String, Unplaceable)> = Vec::new();
    let mut pending = vec![(String::new(), root)];
    // The root, and then every file and directory as it is queued.
    let mut expanded = 1_usize;
    while let Some((prefix, directory)) = pending.pop() {
        if directory.files.is_empty()
            && directory.directories.is_empty()
            && directory.symlinks.is_empty()
        {
            let at = prefix.trim_end_matches('/').to_owned();
            unplaceable.push((at, Unplaceable::EmptyDirectory));
        }
        for link in &directory.symlinks {
            unplaceable.push((format!("{prefix}{}", link.name), Unplaceable::Symlink));
        }
        for file in &directory.files {
            expanded += 1;
            if file.name.contains('/') || expanded > MAX_TREE_ENTRIES {
                return None;
            }
            files.push(RemoteTreeFile {
                path: TreePath::parse(format!("{prefix}{}", file.name)).ok()?,
                blob: blob_of(file.digest.as_ref(), &file.name).ok()?,
                mode: mode_of(file.is_executable),
            });
        }
        for node in &directory.directories {
            expanded += 1;
            if node.name.contains('/') || expanded > MAX_TREE_ENTRIES {
                return None;
            }
            let child = by_blob.get(&blob_of(node.digest.as_ref(), &node.name).ok()?)?;
            pending.push((format!("{prefix}{}/", node.name), child));
        }
    }
    unplaceable.sort();
    if let Some((path, kind)) = unplaceable.into_iter().next() {
        return Some(match kind {
            Unplaceable::Symlink => DecodedTree::Symlink(path),
            Unplaceable::EmptyDirectory => DecodedTree::EmptyDirectory(path),
        });
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Some(DecodedTree::Files(files))
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeMap;

    use buildl_core::{
        Argument, Blob, Command, DecodedTree, Digest, Directory, EnvName, EnvValue, FileMode,
        Label, OutputName, OutputPath, Platform, PlatformProperties, PlatformPropertyName,
        RemoteAction, RemoteContent, RemoteOutput, RemotePlatform, RemoteTree, RemoteTreeFile,
        RemoteUpload, Timeout, TreePath,
    };
    use prost::Message;

    use super::{
        Encoded, RemoteError, action, action_result, command, hit, platform, tree_files,
        wire_digest,
    };
    use crate::remote::proto::build::bazel::remote::execution::v2 as wire;

    fn output_path(name: &str) -> OutputPath {
        OutputPath::of(
            &Directory::parse("out").unwrap(),
            &Label::parse("//:gen").unwrap(),
            &OutputName::parse(name).unwrap(),
        )
    }

    fn remote_action() -> RemoteAction {
        let env = [("TZ", "UTC"), ("LC_ALL", "C")]
            .into_iter()
            .map(|(name, value)| {
                (
                    EnvName::parse(name).unwrap(),
                    EnvValue::parse(value).unwrap(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        RemoteAction {
            salt: Digest::of(b"key"),
            command: Command::new(vec![
                Argument::parse("cc").unwrap(),
                Argument::parse("-c").unwrap(),
            ])
            .unwrap(),
            env,
            outputs: vec![output_path("a"), output_path("b")],
            files: Vec::new(),
            platform: RemotePlatform::of(&Platform::parse("linux/x86_64").unwrap()),
            timeout: Timeout::parse("90s").unwrap(),
            properties: PlatformProperties::default(),
        }
    }

    fn digest_of(bytes: &[u8]) -> wire::Digest {
        wire_digest(&Blob::of(bytes)).unwrap()
    }

    fn tree_file(path: &str, bytes: &[u8], mode: FileMode) -> RemoteTreeFile {
        RemoteTreeFile {
            path: TreePath::parse(path).unwrap(),
            blob: Blob::of(bytes),
            mode,
        }
    }

    fn upload_of(outputs: Vec<RemoteOutput>) -> RemoteUpload {
        RemoteUpload {
            outputs,
            stdout: Blob::of(b"out text"),
            stderr: Blob::of(b""),
        }
    }

    #[test]
    fn the_command_matches_spec_section_2() {
        let expected = wire::Command {
            arguments: vec!["cc".to_owned(), "-c".to_owned()],
            environment_variables: vec![
                wire::command::EnvironmentVariable {
                    name: "LC_ALL".to_owned(),
                    value: "C".to_owned(),
                },
                wire::command::EnvironmentVariable {
                    name: "TZ".to_owned(),
                    value: "UTC".to_owned(),
                },
            ],
            output_paths: vec!["out/a".to_owned(), "out/b".to_owned()],
            working_directory: String::new(),
            ..wire::Command::default()
        };
        let built = command(&remote_action());
        assert_eq!(built, expected);
        assert_eq!(built.encode_to_vec(), expected.encode_to_vec());
        assert_eq!(built.working_directory, "");
    }

    #[test]
    fn the_platform_is_two_sorted_properties() {
        let property = |name: &str, value: &str| wire::platform::Property {
            name: name.to_owned(),
            value: value.to_owned(),
        };
        assert_eq!(
            platform(&remote_action()).properties,
            [property("ISA", "x86-64"), property("OSFamily", "linux")]
        );
    }

    fn with_properties(pairs: &[(&str, &str)]) -> RemoteAction {
        let mut remote = remote_action();
        remote.properties = PlatformProperties::new(
            pairs
                .iter()
                .map(|(name, value)| {
                    (
                        PlatformPropertyName::parse(*name).unwrap(),
                        (*value).to_owned(),
                    )
                })
                .collect(),
        );
        remote
    }

    #[test]
    fn the_platform_lists_every_property_sorted_by_name() {
        let remote = with_properties(&[
            ("pool", "large"),
            ("arch-hint", "x"),
            ("Pool", "z"),
            ("Arch", "y"),
        ]);

        let listed: Vec<(String, String)> = platform(&remote)
            .properties
            .into_iter()
            .map(|property| (property.name, property.value))
            .collect();

        let expected = [
            ("Arch", "y"),
            ("ISA", "x86-64"),
            ("OSFamily", "linux"),
            ("Pool", "z"),
            ("arch-hint", "x"),
            ("pool", "large"),
        ]
        .map(|(name, value)| (name.to_owned(), value.to_owned()));
        assert_eq!(listed, expected);
    }

    #[test]
    fn properties_change_the_action_digest() {
        let command_digest = digest_of(b"command");
        let root = digest_of(b"root");
        let digest_of_action = |remote: &RemoteAction| {
            Encoded::of(&action(&command_digest, &root, remote))
                .blob
                .digest
        };
        assert_ne!(
            digest_of_action(&remote_action()),
            digest_of_action(&with_properties(&[("pool", "large")]))
        );
    }

    #[test]
    fn the_salt_changes_the_action_digest() {
        let command_digest = digest_of(b"command");
        let root = digest_of(b"root");
        let mut other = remote_action();
        other.salt = Digest::of(b"another key");
        let digest_of_action = |remote: &RemoteAction| {
            Encoded::of(&action(&command_digest, &root, remote))
                .blob
                .digest
        };
        assert_ne!(digest_of_action(&remote_action()), digest_of_action(&other));
    }

    #[test]
    fn an_action_digest_is_stable() {
        let command_digest = digest_of(b"command");
        let root = digest_of(b"root");
        let first = Encoded::of(&action(&command_digest, &root, &remote_action()));
        let second = Encoded::of(&action(&command_digest, &root, &remote_action()));
        assert_eq!(first.bytes, second.bytes);
        assert_eq!(first.blob, second.blob);
        let message = action(&command_digest, &root, &remote_action());
        assert_eq!(message.salt, Digest::of(b"key").as_bytes().to_vec());
        assert_eq!(message.command_digest, Some(command_digest));
        assert_eq!(message.input_root_digest, Some(root));
        assert_eq!(
            message.timeout,
            Some(prost_types::Duration {
                seconds: 90,
                nanos: 0
            })
        );
        assert_eq!(message.platform, Some(platform(&remote_action())));
    }

    #[test]
    fn an_action_result_round_trips_to_a_hit() {
        let files = vec![
            tree_file("a.txt", b"a", FileMode::Plain),
            tree_file("sub/b.sh", b"b", FileMode::Executable),
        ];
        let upload = upload_of(vec![
            RemoteOutput {
                path: output_path("bin"),
                content: RemoteContent::File {
                    blob: Blob::of(b"elf"),
                    mode: FileMode::Executable,
                },
            },
            RemoteOutput {
                path: output_path("dir"),
                content: RemoteContent::Tree(RemoteTree::new(files)),
            },
        ]);
        let built = action_result(&upload).unwrap();
        assert_eq!(built.trees.len(), 1);
        let result = hit(&built.result).unwrap();
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.files.len(), 1);
        assert_eq!(result.files[0].path, "out/bin");
        assert_eq!(result.files[0].blob, Blob::of(b"elf"));
        assert_eq!(result.files[0].mode, FileMode::Executable);
        assert_eq!(result.directories.len(), 1);
        assert_eq!(result.directories[0].path, "out/dir");
        assert_eq!(result.directories[0].tree, built.trees[0].blob);
        assert_eq!(result.stdout, Some(Blob::of(b"out text")));
        assert_eq!(result.stderr, Some(Blob::of(b"")));
        assert_eq!(result.symlinks, Vec::<String>::new());
    }

    #[test]
    fn a_tree_round_trips_to_its_files() {
        let files = vec![
            tree_file("z.txt", b"z", FileMode::Plain),
            tree_file("sub/deep/c.txt", b"c", FileMode::Plain),
            tree_file("sub/b.sh", b"b", FileMode::Executable),
            tree_file("a.txt", b"a", FileMode::Plain),
        ];
        let tree = RemoteTree::new(files);
        let upload = upload_of(vec![RemoteOutput {
            path: output_path("dir"),
            content: RemoteContent::Tree(tree.clone()),
        }]);
        let built = action_result(&upload).unwrap();
        assert_eq!(
            tree_files(&built.trees[0].bytes),
            DecodedTree::Files(tree.files().to_vec())
        );
        // the same files encode to the same tree every time
        assert_eq!(
            action_result(&upload).unwrap().trees[0].bytes,
            built.trees[0].bytes
        );
    }

    #[test]
    fn a_tree_with_a_symlink_or_an_empty_directory_names_it() {
        let file = |name: &str| wire::FileNode {
            name: name.to_owned(),
            digest: Some(digest_of(name.as_bytes())),
            ..wire::FileNode::default()
        };
        let with_symlink = wire::Tree {
            root: Some(wire::Directory {
                files: vec![file("a")],
                symlinks: vec![wire::SymlinkNode {
                    name: "link".to_owned(),
                    target: "a".to_owned(),
                    node_properties: None,
                }],
                ..wire::Directory::default()
            }),
            children: Vec::new(),
        };
        assert_eq!(
            tree_files(&with_symlink.encode_to_vec()),
            DecodedTree::Symlink("link".to_owned())
        );

        let empty_child = wire::Directory::default();
        let with_empty_directory = wire::Tree {
            root: Some(wire::Directory {
                files: vec![file("a")],
                directories: vec![wire::DirectoryNode {
                    name: "empty".to_owned(),
                    digest: Some(digest_of(&empty_child.encode_to_vec())),
                }],
                ..wire::Directory::default()
            }),
            children: vec![empty_child],
        };
        assert_eq!(
            tree_files(&with_empty_directory.encode_to_vec()),
            DecodedTree::EmptyDirectory("empty".to_owned())
        );

        let bad_name = wire::Tree {
            root: Some(wire::Directory {
                files: vec![file("a/b")],
                ..wire::Directory::default()
            }),
            children: Vec::new(),
        };
        assert_eq!(
            tree_files(&bad_name.encode_to_vec()),
            DecodedTree::Unreadable
        );
    }

    #[test]
    fn a_tree_whose_directories_share_children_is_not_expanded_without_bound() {
        // Each level holds two directories that name the one level below, so the tree has 40
        // messages and 2^40 paths.
        let file = wire::FileNode {
            name: "leaf".to_owned(),
            digest: Some(digest_of(b"leaf")),
            ..wire::FileNode::default()
        };
        let mut children = vec![wire::Directory {
            files: vec![file],
            ..wire::Directory::default()
        }];
        for _ in 0..40 {
            let below = Encoded::of(children.last().unwrap()).digest();
            let node = |name: &str| wire::DirectoryNode {
                name: name.to_owned(),
                digest: Some(below.clone()),
            };
            children.push(wire::Directory {
                directories: vec![node("a"), node("b")],
                ..wire::Directory::default()
            });
        }
        let root = children.pop();
        let tree = wire::Tree { root, children };

        assert_eq!(tree_files(&tree.encode_to_vec()), DecodedTree::Unreadable);
    }

    #[test]
    fn a_blob_larger_than_a_digest_can_name_is_refused() {
        let huge = Blob {
            digest: Digest::of(b"x"),
            size: u64::MAX,
        };

        let error = wire_digest(&huge).unwrap_err();

        assert!(matches!(error, RemoteError::TooLarge { size: u64::MAX }));
        let upload = RemoteUpload {
            outputs: Vec::new(),
            stdout: huge,
            stderr: Blob::of(b""),
        };
        assert!(action_result(&upload).is_err());
    }

    #[test]
    #[expect(
        deprecated,
        reason = "the test builds a result the way a v2.0 server does"
    )]
    fn output_symlinks_are_reported() {
        let symlink = wire::OutputSymlink {
            path: "out/link".to_owned(),
            target: "x".to_owned(),
            node_properties: None,
        };
        let plain = hit(&wire::ActionResult::default()).unwrap();
        assert_eq!(plain.symlinks, Vec::<String>::new());
        for result in [
            wire::ActionResult {
                output_symlinks: vec![symlink.clone()],
                ..wire::ActionResult::default()
            },
            wire::ActionResult {
                output_file_symlinks: vec![symlink.clone()],
                ..wire::ActionResult::default()
            },
            wire::ActionResult {
                output_directory_symlinks: vec![symlink],
                ..wire::ActionResult::default()
            },
        ] {
            assert_eq!(hit(&result).unwrap().symlinks, ["out/link"]);
        }
    }

    #[test]
    #[expect(
        deprecated,
        reason = "the test builds a result the way a v2.0 server does"
    )]
    fn a_hit_names_its_symlinked_outputs() {
        let link = |path: &str| wire::OutputSymlink {
            path: path.to_owned(),
            target: "x".to_owned(),
            node_properties: None,
        };
        let one = wire::ActionResult {
            output_symlinks: vec![link("out/link")],
            ..wire::ActionResult::default()
        };
        assert_eq!(hit(&one).unwrap().symlinks, ["out/link"]);

        // A v2.1 server may repeat a link in the older field as well; it is named once.
        let repeated = wire::ActionResult {
            output_symlinks: vec![link("out/link")],
            output_file_symlinks: vec![link("out/link")],
            output_directory_symlinks: vec![link("out/dir-link")],
            ..wire::ActionResult::default()
        };
        assert_eq!(
            hit(&repeated).unwrap().symlinks,
            ["out/dir-link", "out/link"]
        );
    }
}
