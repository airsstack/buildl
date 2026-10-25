//! The action a remote cache is asked about, as a plain value.
//!
//! Its own file because the remote cache is keyed by a description of the work, not by buildl's
//! own key: the command, its environment, the files it reads and where it runs. The core builds
//! this value and an adapter turns it into the wire form, so nothing about that form is known
//! here.
//!
//! Responsibilities: [`RemoteFile`], [`RemotePlatform`] and [`RemoteAction`].
//!
//! Non-responsibilities: building one from a ready action, and encoding it, which the phase that
//! executes and the remote adapter do.

use std::collections::BTreeMap;

use crate::types::{Command, Digest, EnvName, EnvValue, FileMode, OutputPath, RootPath, Timeout};

/// One file of the input root: where it is, the digest of its bytes, and whether it may be run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteFile {
    /// Where the file is, from the workspace root.
    pub path: RootPath,
    /// The digest of the file's bytes.
    pub digest: Digest,
    /// Whether the file may be run; `None` for a declared input, whose bit the adapter reads
    /// from the file on disk.
    pub mode: Option<FileMode>,
}

/// The machine an action runs on, in the two properties a remote cache tells machines apart by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemotePlatform {
    /// The operating system family, such as `linux` or `macos`.
    pub os_family: String,
    /// The instruction set, such as `x86-64` or `arm-a64`.
    pub isa: String,
}

/// An action as a remote cache is asked about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteAction {
    /// The target's action key. It sets the action apart from any other with the same fields, so
    /// everything the key folds in is part of the action's identity.
    pub salt: Digest,
    /// The command that runs.
    pub command: Command,
    /// The environment the command runs with.
    pub env: BTreeMap<EnvName, EnvValue>,
    /// The paths the command is to leave.
    pub outputs: Vec<OutputPath>,
    /// Every file of the input root.
    pub files: Vec<RemoteFile>,
    /// Where the command runs.
    pub platform: RemotePlatform,
    /// How long the command may run.
    pub timeout: Timeout,
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeMap;

    use super::{RemoteAction, RemoteFile, RemotePlatform};
    use crate::types::{
        Argument, Command, Digest, EnvName, EnvValue, FileMode, RootPath, SourcePath, Timeout,
    };

    fn action(env: &[(&str, &str)]) -> RemoteAction {
        RemoteAction {
            salt: Digest::of(b"key"),
            command: Command::new(vec![Argument::parse("cc").unwrap()]).unwrap(),
            env: env
                .iter()
                .map(|(name, value)| {
                    (
                        EnvName::parse(*name).unwrap(),
                        EnvValue::parse(*value).unwrap(),
                    )
                })
                .collect::<BTreeMap<_, _>>(),
            outputs: Vec::new(),
            files: vec![RemoteFile {
                path: RootPath::from(&SourcePath::parse("src/main.c").unwrap()),
                digest: Digest::of(b"int main(void);"),
                mode: None,
            }],
            platform: RemotePlatform {
                os_family: "linux".to_owned(),
                isa: "x86-64".to_owned(),
            },
            timeout: Timeout::default(),
        }
    }

    #[test]
    fn the_order_the_environment_was_given_in_does_not_matter() {
        assert_eq!(
            action(&[("A", "1"), ("B", "2")]),
            action(&[("B", "2"), ("A", "1")])
        );
    }

    #[test]
    fn the_salt_sets_two_otherwise_equal_actions_apart() {
        let mut other = action(&[]);
        other.salt = Digest::of(b"another key");
        assert_ne!(action(&[]), other);
    }

    #[test]
    fn a_file_of_the_same_bytes_and_another_mode_is_another_file() {
        let plain = RemoteFile {
            mode: Some(FileMode::Plain),
            ..action(&[]).files.remove(0)
        };
        let executable = RemoteFile {
            mode: Some(FileMode::Executable),
            ..plain.clone()
        };
        assert_ne!(plain, executable);
    }
}
