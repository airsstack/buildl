//! A task described as a remote cache is asked about it.
//!
//! Its own file because the remote cache is keyed by a description of the work, not by buildl's
//! own key, and building that description reads the content store: every file a dependency
//! produced is part of the input root, and the key holds only the digest of each output's
//! manifest. The fold from a task and the execution settings into that description is pure
//! apart from those reads, so it is kept apart from running anything.
//!
//! Responsibilities: [`remote_action`].
//!
//! Non-responsibilities: asking the remote, encoding the description for the wire, and reading
//! the bytes of a declared input, which are the remote adapter's.

use crate::error::{Error, PortName, Result};
use crate::execute::perform::Task;
use crate::ports::ContentStore;
use crate::types::{
    ActionKey, Diagnostic, Digest, Execution, OutputManifest, OutputPath, PlatformProperties,
    RemoteAction, RemoteFile, RemotePlatform, RootPath,
};

/// The `RemoteAction` of `task`, run under `execution`.
///
/// The environment is the action's own with the variables the strategy pins laid over it, as
/// the strategy sets them where the command runs. The outputs are sorted and hold no path
/// twice. The files are every declared input, with no mode, and every file of every output of
/// every dependency, with the mode its manifest records, all sorted by path. Each dependency
/// output's manifest is read from `cas` by the digest the key holds for it. The `properties` are
/// the workspace's own, carried as they are; they take no part in the action key.
///
/// # Errors
///
/// Returns [`Error::Adapter`], naming [`PortName::ContentStore`], when the task's action and
/// its key do not name the same number of dependency outputs, or when a dependency output's
/// manifest is not held, cannot be read, or is not a manifest; and whatever `cas` returns.
pub(crate) fn remote_action<C: ContentStore>(
    task: &Task,
    execution: &Execution,
    properties: &PlatformProperties,
    cas: &C,
) -> Result<RemoteAction> {
    let (ready, key) = (&task.action, &task.key);
    let mut env = ready.env.clone();
    env.extend(execution.strategy.pinned());

    let mut outputs = ready.outputs.clone();
    outputs.sort();
    outputs.dedup();

    let components = key.components();
    let mut files: Vec<RemoteFile> = components
        .own
        .inputs
        .iter()
        .map(|input| RemoteFile {
            path: RootPath::from(&input.path),
            digest: input.digest,
            mode: None,
        })
        .collect();
    let produced: Vec<_> = components
        .deps
        .iter()
        .flat_map(|dep| &dep.outputs)
        .collect();
    if produced.len() != ready.deps.len() {
        return Err(Error::Adapter {
            port: PortName::ContentStore,
            diagnostic: Diagnostic::new(format!(
                "the action of {} names {} dependency outputs but its key holds {}",
                ready.label,
                ready.deps.len(),
                produced.len()
            )),
        });
    }
    for (at, produced) in ready.deps.iter().zip(produced) {
        files.extend(output_files(at, &produced.digest, cas)?);
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));

    Ok(RemoteAction {
        salt: key.digest(),
        command: ready.command.clone(),
        env,
        outputs,
        files,
        platform: platform_of(key),
        timeout: execution.timeout,
        properties: properties.clone(),
    })
}

/// The files of the dependency output at `at`, whose manifest `cas` holds under `manifest`.
fn output_files<C: ContentStore>(
    at: &OutputPath,
    manifest: &Digest,
    cas: &C,
) -> Result<Vec<RemoteFile>> {
    let unreadable = |what: &str| Error::Adapter {
        port: PortName::ContentStore,
        diagnostic: Diagnostic::new(format!("the manifest of {at} {what}: {manifest}")),
    };
    let bytes = cas
        .get(manifest)?
        .ok_or_else(|| unreadable("is not held"))?;
    let parsed =
        OutputManifest::from_bytes(&bytes).ok_or_else(|| unreadable("is not a manifest"))?;
    Ok(match parsed {
        OutputManifest::File(file) => vec![RemoteFile {
            path: RootPath::from(at),
            digest: file.digest,
            mode: Some(file.mode),
        }],
        OutputManifest::Tree(tree) => tree
            .entries()
            .iter()
            .map(|entry| RemoteFile {
                path: RootPath::from(&at.entry(&entry.path)),
                digest: entry.digest,
                mode: Some(entry.mode),
            })
            .collect(),
    })
}

/// The machine `key` was made for, in the words a remote cache tells machines apart by.
fn platform_of(key: &ActionKey) -> RemotePlatform {
    RemotePlatform::of(&key.components().own.platform)
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeMap;

    use super::remote_action;
    use crate::error::{Error, PortName, Result};
    use crate::execute::fixtures::{command, digest_of, file, label, output_path, own, path, tree};
    use crate::execute::perform::Task;
    use crate::ports::ContentStore;
    use crate::types::{
        ActionKey, DepProducts, Digest, EnvName, EnvValue, Execution, FileMode, InputDigest,
        Isolation, KeyComponents, NetworkAccess, NodeId, OutputManifest, OutputName, OutputPath,
        Platform, PlatformProperties, PlatformPropertyName, Produced, ReadyAction, RemoteAction,
        RemoteFile, RemotePlatform, RootPath, StoredFile, TargetRole, Timeout, TreePath,
    };

    /// A store that holds exactly the manifests it is given.
    struct Manifests(BTreeMap<Digest, Vec<u8>>);

    impl Manifests {
        fn holding(manifests: &[&OutputManifest]) -> Self {
            Self(
                manifests
                    .iter()
                    .map(|manifest| (digest_of(manifest), manifest.to_bytes().unwrap()))
                    .collect(),
            )
        }
    }

    impl ContentStore for Manifests {
        fn put(&self, bytes: &[u8]) -> Result<Digest> {
            Ok(Digest::of(bytes))
        }

        fn contains(&self, digest: &Digest) -> bool {
            self.0.contains_key(digest)
        }

        fn get(&self, digest: &Digest) -> Result<Option<Vec<u8>>> {
            Ok(self.0.get(digest).cloned())
        }

        fn clear(&self, _at: &OutputPath) -> Result<()> {
            Ok(())
        }

        fn place(&self, _digest: &Digest, _mode: FileMode, _at: &OutputPath) -> Result<()> {
            Ok(())
        }
    }

    fn env(pairs: &[(&str, &str)]) -> BTreeMap<EnvName, EnvValue> {
        pairs
            .iter()
            .map(|(name, value)| {
                (
                    EnvName::parse(*name).unwrap(),
                    EnvValue::parse(*value).unwrap(),
                )
            })
            .collect()
    }

    fn produced(name: &str, manifest: &OutputManifest) -> Produced {
        Produced {
            name: OutputName::parse(name).unwrap(),
            digest: digest_of(manifest),
        }
    }

    fn dependency(raw: &str, outputs: Vec<Produced>) -> DepProducts {
        DepProducts {
            label: label(raw),
            outputs,
        }
    }

    /// A ready action running `cc` with the environment `env`, no input and no dependency,
    /// leaving `outputs`.
    fn ready(env: BTreeMap<EnvName, EnvValue>, outputs: Vec<OutputPath>) -> ReadyAction {
        ReadyAction {
            label: label("//app:app"),
            command: command(&["cc"]),
            description: None,
            env,
            inputs: Vec::new(),
            deps: Vec::new(),
            outputs,
            network: NetworkAccess::Sealed,
        }
    }

    fn key(platform: &str, inputs: Vec<InputDigest>, deps: Vec<DepProducts>) -> ActionKey {
        let mut own = own(&["app"]);
        own.platform = Platform::parse(platform).unwrap();
        own.inputs = inputs;
        ActionKey::new(KeyComponents { own, deps }).unwrap()
    }

    fn plain_key() -> ActionKey {
        key("linux/x86_64", Vec::new(), Vec::new())
    }

    fn execution(strategy: Isolation) -> Execution {
        Execution {
            strategy,
            ..Execution::default()
        }
    }

    fn task(action: ReadyAction, key: ActionKey) -> Task {
        Task {
            node: NodeId::new(0),
            role: TargetRole::Build,
            key,
            action,
        }
    }

    fn describe(ready: ReadyAction, key: ActionKey, execution: &Execution) -> RemoteAction {
        remote_action(
            &task(ready, key),
            execution,
            &PlatformProperties::default(),
            &Manifests::holding(&[]),
        )
        .unwrap()
    }

    #[test]
    fn the_salt_is_the_buildl_key() {
        let key = plain_key();
        let action = describe(
            ready(BTreeMap::new(), Vec::new()),
            key.clone(),
            &Execution::default(),
        );
        assert_eq!(action.salt, key.digest());
        assert_eq!(action.command, command(&["cc"]));
    }

    #[test]
    fn pinned_variables_override_declared_ones_under_the_sandbox() {
        let declared = || ready(env(&[("TZ", "X"), ("KEEP", "1")]), Vec::new());

        let sandboxed = describe(declared(), plain_key(), &execution(Isolation::Sandbox));
        assert_eq!(
            sandboxed.env,
            env(&[
                ("KEEP", "1"),
                ("LC_ALL", "C"),
                ("SOURCE_DATE_EPOCH", "315532800"),
                ("TZ", "UTC"),
            ])
        );

        let bare = describe(declared(), plain_key(), &execution(Isolation::Bare));
        assert_eq!(bare.env, env(&[("KEEP", "1"), ("TZ", "X")]));
    }

    #[test]
    fn outputs_are_sorted_and_deduplicated() {
        let b = output_path("//app:app", "b");
        let a = output_path("//app:app", "a");
        let action = describe(
            ready(BTreeMap::new(), vec![b.clone(), a.clone(), b.clone()]),
            plain_key(),
            &Execution::default(),
        );
        assert_eq!(action.outputs, vec![a, b]);
    }

    #[test]
    fn inputs_and_dependency_files_form_one_sorted_list() {
        let lib = file("lib");
        let headers = tree(&[("b.h", "b"), ("a/a.h", "a")]);
        let bin = OutputManifest::File(StoredFile {
            digest: Digest::of(b"bin"),
            mode: FileMode::Executable,
        });
        let data = file("data");
        let held = Manifests::holding(&[&lib, &headers, &bin, &data]);

        let lib_at = output_path("//a:a", "lib");
        let headers_at = output_path("//a:a", "inc");
        let bin_at = output_path("//b:b", "bin");
        let data_at = output_path("//b:b", "data");
        let mut action = ready(BTreeMap::new(), Vec::new());
        action.deps = vec![
            lib_at.clone(),
            headers_at.clone(),
            bin_at.clone(),
            data_at.clone(),
        ];
        let key = key(
            "linux/x86_64",
            vec![
                InputDigest {
                    path: path("src/main.c"),
                    digest: Digest::of(b"main"),
                },
                InputDigest {
                    path: path("Makefile"),
                    digest: Digest::of(b"make"),
                },
            ],
            vec![
                dependency(
                    "//a:a",
                    vec![produced("lib", &lib), produced("inc", &headers)],
                ),
                dependency(
                    "//b:b",
                    vec![produced("bin", &bin), produced("data", &data)],
                ),
            ],
        );

        let described = remote_action(
            &task(action, key),
            &Execution::default(),
            &PlatformProperties::default(),
            &held,
        )
        .unwrap();

        let entry = |name: &str, content: &str| RemoteFile {
            path: RootPath::from(&headers_at.entry(&TreePath::parse(name).unwrap())),
            digest: Digest::of(content.as_bytes()),
            mode: Some(FileMode::Plain),
        };
        let output = |at: &OutputPath, content: &str, mode: FileMode| RemoteFile {
            path: RootPath::from(at),
            digest: Digest::of(content.as_bytes()),
            mode: Some(mode),
        };
        let input = |name: &str, content: &str| RemoteFile {
            path: RootPath::from(&path(name)),
            digest: Digest::of(content.as_bytes()),
            mode: None,
        };
        let mut expected = vec![
            input("src/main.c", "main"),
            input("Makefile", "make"),
            output(&lib_at, "lib", FileMode::Plain),
            entry("a/a.h", "a"),
            entry("b.h", "b"),
            output(&bin_at, "bin", FileMode::Executable),
            output(&data_at, "data", FileMode::Plain),
        ];
        expected.sort_by(|left, right| left.path.cmp(&right.path));
        assert_eq!(described.files, expected);
    }

    #[test]
    fn the_platform_maps_os_and_arch() {
        for (platform, os_family, isa) in [
            ("macos/aarch64", "macos", "arm-a64"),
            ("linux/x86_64", "linux", "x86-64"),
            ("linux/riscv64", "linux", "riscv64"),
        ] {
            let described = describe(
                ready(BTreeMap::new(), Vec::new()),
                key(platform, Vec::new(), Vec::new()),
                &Execution::default(),
            );
            assert_eq!(described.platform.os_family(), os_family, "{platform}");
            assert_eq!(described.platform.isa(), isa, "{platform}");
        }
    }

    /// A task with one dependency output, `lib` of `//a:a`, whose manifest is `lib`.
    fn task_with_a_dependency(lib: &OutputManifest) -> Task {
        let mut action = ready(BTreeMap::new(), Vec::new());
        action.deps = vec![output_path("//a:a", "lib")];
        let key = key(
            "linux/x86_64",
            Vec::new(),
            vec![dependency("//a:a", vec![produced("lib", lib)])],
        );
        task(action, key)
    }

    /// Asserts that `error` is a content store failure whose message holds `part`.
    fn assert_content_store_error(error: Error, part: &str) {
        match error {
            Error::Adapter { port, diagnostic } => {
                assert_eq!(port, PortName::ContentStore);
                assert!(diagnostic.to_string().contains(part), "{diagnostic}");
            }
            other => unreachable!("expected Adapter, got {other:?}"),
        }
    }

    #[test]
    fn a_dependency_manifest_not_held_is_a_content_store_error() {
        let lib = file("lib");
        let error = remote_action(
            &task_with_a_dependency(&lib),
            &Execution::default(),
            &PlatformProperties::default(),
            &Manifests::holding(&[]),
        )
        .unwrap_err();
        assert_content_store_error(error, "is not held");
    }

    #[test]
    fn a_dependency_manifest_that_is_not_a_manifest_is_a_content_store_error() {
        let lib = file("lib");
        let held = Manifests(BTreeMap::from([(
            digest_of(&lib),
            b"not a manifest".to_vec(),
        )]));
        let error = remote_action(
            &task_with_a_dependency(&lib),
            &Execution::default(),
            &PlatformProperties::default(),
            &held,
        )
        .unwrap_err();
        assert_content_store_error(error, "is not a manifest");
    }

    #[test]
    fn an_action_and_a_key_naming_different_dependency_outputs_is_an_error() {
        let lib = file("lib");
        let mut mismatched = task_with_a_dependency(&lib);
        mismatched.action.deps.clear();
        let error = remote_action(
            &mismatched,
            &Execution::default(),
            &PlatformProperties::default(),
            &Manifests::holding(&[&lib]),
        )
        .unwrap_err();
        assert_content_store_error(error, "names 0 dependency outputs but its key holds 1");
    }

    #[test]
    fn the_action_carries_the_execution_timeout() {
        let execution = Execution {
            timeout: Timeout::parse("90s").unwrap(),
            ..Execution::default()
        };
        let described = describe(ready(BTreeMap::new(), Vec::new()), plain_key(), &execution);
        assert_eq!(described.timeout, Timeout::parse("90s").unwrap());
    }

    #[test]
    fn the_workspace_properties_are_carried() {
        let properties = PlatformProperties::new(BTreeMap::from([(
            PlatformPropertyName::parse("pool").unwrap(),
            "large".to_owned(),
        )]));
        let described = remote_action(
            &task(ready(BTreeMap::new(), Vec::new()), plain_key()),
            &Execution::default(),
            &properties,
            &Manifests::holding(&[]),
        )
        .unwrap();
        assert_eq!(described.properties, properties);
    }

    #[test]
    fn no_properties_leave_the_description_as_before() {
        let key = plain_key();
        let described = describe(
            ready(BTreeMap::new(), Vec::new()),
            key.clone(),
            &execution(Isolation::Bare),
        );
        assert_eq!(
            described,
            RemoteAction {
                salt: key.digest(),
                command: command(&["cc"]),
                env: BTreeMap::new(),
                outputs: Vec::new(),
                files: Vec::new(),
                platform: RemotePlatform::of(&Platform::parse("linux/x86_64").unwrap()),
                timeout: Execution::default().timeout,
                properties: PlatformProperties::default(),
            }
        );
    }
}
