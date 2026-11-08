//! A target's own key components, built from what was gathered for the whole graph.
//!
//! Its own file because this is where the answers of five ports meet one target: by the time a
//! node's components are built, every value they need has been fetched once for the graph, and
//! building them asks nothing more.
//!
//! Responsibilities: [`environment`], [`check_settings`], [`Gathered`] and [`own`].
//!
//! Non-responsibilities: fetching the other values, and a target's dependencies, which join its
//! key only once it is known what they produced.

use std::collections::{BTreeMap, BTreeSet};

use crate::error::{DeclarationSite, Error, Result};
use crate::plan::settings::referenced;
use crate::plan::tools::program_of;
use crate::ports::Host;
use crate::types::{
    Digest, EnvMark, EnvName, EnvValue, InputDigest, Label, MachineFolders, Node, OwnComponents,
    Platform, Program, SettingName, SettingValue, SourcePath, ToolIdentity,
};

/// The value of every variable some node of `nodes` marks `key`, asked for in name order.
///
/// A variable no node marks `key` is not asked for: an `unkeyed` variable's value takes no part
/// in a key, so the plan never reads it.
///
/// A value that holds one of the machine's own folders is refused: it would give the target
/// another key on every machine.
///
/// # Errors
///
/// Returns whatever `host` returns for the first variable it cannot report, and
/// [`Error::MachinePathInVariable`] for the first variable, in name order, whose value holds one
/// of the folders `host` reports, naming the first node that marks it `key`.
pub(crate) fn environment<'g, H: Host>(
    nodes: &'g [Node],
    host: &H,
) -> Result<BTreeMap<&'g EnvName, Option<EnvValue>>> {
    let mut first_reader: BTreeMap<&EnvName, &Node> = BTreeMap::new();
    for node in nodes {
        for (name, mark) in &node.env {
            if *mark == EnvMark::Key {
                first_reader.entry(name).or_insert(node);
            }
        }
    }
    let folders = host.machine_folders();
    first_reader
        .into_iter()
        .map(|(name, node)| {
            let value = host.env(name)?;
            let held = value
                .as_ref()
                .and_then(|value| folders.find_in(value.as_str()));
            if let Some(folder) = held {
                return Err(Error::MachinePathInVariable {
                    site: Box::new(DeclarationSite::from(node)),
                    name: name.clone(),
                    folder: folder.to_owned(),
                });
            }
            Ok((name, value))
        })
        .collect()
}

/// Refuses a setting a command names whose value holds one of `folders`.
///
/// Only the settings a node's command names are searched, as only those reach its key; the value
/// is the `--set` override or else the default, whichever `values` holds.
///
/// # Errors
///
/// Returns [`Error::MachinePathInSetting`] for the first hit, searching the nodes in order and
/// each node's settings in name order.
pub(crate) fn check_settings(
    nodes: &[Node],
    values: &BTreeMap<SettingName, SettingValue>,
    folders: &MachineFolders,
) -> Result<()> {
    for node in nodes {
        for (name, value) in referenced(&node.run, values) {
            if let Some(folder) = folders.find_in(value.as_str()) {
                return Err(Error::MachinePathInSetting {
                    site: Box::new(DeclarationSite::from(node)),
                    name,
                    folder: folder.to_owned(),
                });
            }
        }
    }
    Ok(())
}

/// Everything gathered for a graph before any target's components are built.
#[derive(Debug)]
pub(crate) struct Gathered<'g> {
    /// The value every declared setting has for this run.
    pub(crate) settings: BTreeMap<SettingName, SettingValue>,
    /// The fingerprint of every program a command names that is not built.
    pub(crate) fingerprints: BTreeMap<Program, Digest>,
    /// The targets whose program a dependency builds, which run no tool of the machine.
    pub(crate) built: BTreeSet<&'g Label>,
    /// The content digest of every input.
    pub(crate) digests: BTreeMap<&'g SourcePath, Digest>,
    /// The value of every environment variable a target reads marked `key`; `None` for one that
    /// is unset.
    pub(crate) env: BTreeMap<&'g EnvName, Option<EnvValue>>,
    /// The platform actions run on.
    pub(crate) platform: Platform,
}

/// The components of `node`'s key that do not wait on another target.
///
/// Inputs and outputs keep the order they were written in. The environment holds each variable
/// `node` marks `key` with its value, and the names of those it marks `unkeyed` apart, with no
/// value; both are keyed by name, so the order `node` lists its variables in does not reach the
/// key. Only the settings the command names are kept. The tool is the program the command names
/// under the gathered settings, so
/// one named through a setting is keyed like one written out, and a program a dependency builds
/// is no tool: its target's key holds the dependency's output instead. An input or a program
/// `gathered` holds nothing for is left out, and a variable it holds nothing for is taken as
/// unset; neither happens when `gathered` was built for the graph `node` belongs to.
pub(crate) fn own(node: &Node, gathered: &Gathered<'_>) -> OwnComponents {
    let inputs = node
        .inputs
        .iter()
        .filter_map(|path| {
            gathered.digests.get(path).map(|digest| InputDigest {
                path: path.clone(),
                digest: *digest,
            })
        })
        .collect();
    let env = node
        .env
        .iter()
        .filter(|(_, mark)| **mark == EnvMark::Key)
        .map(|(name, _)| (name.clone(), gathered.env.get(name).cloned().flatten()))
        .collect();
    let unkeyed = node
        .env
        .iter()
        .filter(|(_, mark)| **mark == EnvMark::Unkeyed)
        .map(|(name, _)| name.clone())
        .collect();
    let tool = program_of(&node.run, &gathered.settings)
        .filter(|_| !gathered.built.contains(&node.label))
        .and_then(|program| {
            let fingerprint = *gathered.fingerprints.get(&program)?;
            Some(ToolIdentity {
                program,
                fingerprint,
            })
        });
    OwnComponents {
        command: node.run.clone(),
        inputs,
        outputs: node.outputs.clone(),
        env,
        unkeyed,
        tool,
        settings: referenced(&node.run, &gathered.settings),
        network: node.network,
        output_source: node.output_source,
        platform: gathered.platform.clone(),
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::{BTreeMap, BTreeSet};

    use super::{Gathered, environment, own};
    use crate::grants::fixtures::{
        command, env_name, keyed, marked, node, path, program, settings,
    };
    use crate::ports::two_machines::TwoMachines;
    use crate::types::{
        ActionKey, Digest, EnvMark, EnvName, EnvValue, InputDigest, KeyComponents, MachineFolders,
        NetworkAccess, Node, OutputName, Platform, SourcePath, ToolIdentity,
    };

    /// What was gathered for a graph reading `src/a.c` and `src/z.c`, `HOME` (unset) and `PATH`,
    /// running `cc`, with the settings `mode` and `test_filter`.
    fn gathered<'g>(
        paths: &'g [SourcePath],
        names: &'g [EnvName],
        values: &[Option<&str>],
    ) -> Gathered<'g> {
        Gathered {
            settings: settings(&[("mode", "debug"), ("test_filter", "TestLogin")]),
            fingerprints: BTreeMap::from([(program("cc"), Digest::of(b"cc 14.2"))]),
            built: BTreeSet::new(),
            digests: paths
                .iter()
                .map(|path| (path, Digest::of(path.as_str().as_bytes())))
                .collect(),
            env: names
                .iter()
                .zip(values)
                .map(|(name, value)| (name, value.map(|text| EnvValue::parse(text).unwrap())))
                .collect(),
            platform: Platform::parse("linux/x86_64").unwrap(),
        }
    }

    #[test]
    fn a_nodes_components_are_built_from_the_node_and_what_was_gathered() {
        let paths = [path("src/a.c"), path("src/z.c")];
        let names = [env_name("HOME"), env_name("PATH")];
        let gathered = gathered(&paths, &names, &[None, Some("/bin")]);
        let node = node("//:app", |node| {
            node.run = command(&["cc", "$in", "--filter=$opt:test_filter"]);
            node.inputs = vec![path("src/z.c"), path("src/a.c")];
            node.outputs = vec![
                OutputName::parse("app").unwrap(),
                OutputName::parse("app.map").unwrap(),
            ];
            node.env = keyed(&["PATH", "HOME"]);
            node.network = NetworkAccess::Declared;
        });

        let components = own(&node, &gathered);

        assert_eq!(components.command, node.run);
        assert_eq!(
            components.inputs,
            [
                InputDigest {
                    path: path("src/z.c"),
                    digest: Digest::of(b"src/z.c"),
                },
                InputDigest {
                    path: path("src/a.c"),
                    digest: Digest::of(b"src/a.c"),
                },
            ],
            "inputs keep their written order"
        );
        assert_eq!(components.outputs, node.outputs);
        assert_eq!(
            components.env,
            BTreeMap::from([
                (env_name("HOME"), None),
                (env_name("PATH"), Some(EnvValue::parse("/bin").unwrap())),
            ])
        );
        assert_eq!(
            components.tool,
            Some(ToolIdentity {
                program: program("cc"),
                fingerprint: Digest::of(b"cc 14.2"),
            })
        );
        assert_eq!(
            components.settings,
            settings(&[("test_filter", "TestLogin")]),
            "only the setting the command names"
        );
        assert_eq!(components.network, NetworkAccess::Declared);
        assert_eq!(components.platform, gathered.platform);
    }

    #[test]
    fn a_program_named_through_a_setting_is_the_tool_and_the_setting_is_kept_too() {
        let mut gathered = gathered(&[], &[], &[]);
        gathered.settings = settings(&[("compiler", "cc"), ("mode", "debug")]);
        let node = node("//:app", |node| {
            node.run = command(&["$opt:compiler", "-c", "$in"]);
        });

        let components = own(&node, &gathered);

        assert_eq!(
            components.tool,
            Some(ToolIdentity {
                program: program("cc"),
                fingerprint: Digest::of(b"cc 14.2"),
            })
        );
        assert_eq!(components.settings, settings(&[("compiler", "cc")]));
        assert_eq!(components.command, node.run, "the command stays as written");
    }

    #[test]
    fn a_command_naming_no_program_has_no_tool() {
        let gathered = gathered(&[], &[], &[]);
        let node = node("//:app_test", |node| node.run = command(&["$out/app"]));
        assert_eq!(own(&node, &gathered).tool, None);
    }

    #[test]
    fn a_target_reading_nothing_has_empty_inputs_environment_and_settings() {
        let paths = [path("src/a.c")];
        let names = [env_name("PATH")];
        let gathered = gathered(&paths, &names, &[Some("/bin")]);
        let components = own(&node("//:app", |_| {}), &gathered);
        assert_eq!(components.inputs, []);
        assert!(components.env.is_empty());
        assert!(components.settings.is_empty());
        assert_eq!(components.network, NetworkAccess::Sealed);
    }

    #[test]
    fn the_order_a_node_lists_its_variables_in_does_not_reach_its_components() {
        let names = [env_name("HOME"), env_name("PATH")];
        let gathered = gathered(&[], &names, &[Some("/home/u"), Some("/bin")]);
        let forward = node("//:app", |node| {
            node.env = keyed(&["HOME", "PATH"]);
        });
        let backward = node("//:app", |node| {
            node.env = keyed(&["PATH", "HOME"]);
        });
        assert_eq!(own(&forward, &gathered), own(&backward, &gathered));
    }

    #[test]
    fn a_target_whose_program_a_dependency_builds_has_no_tool_though_another_runs_it() {
        let mut gathered = gathered(&[], &[], &[]);
        let runs = node("//:runs", |node| node.run = command(&["cc", "$in"]));
        let tool = node("//:tool", |node| node.run = command(&["cc", "$in"]));
        gathered.built = BTreeSet::from([&runs.label]);
        assert_eq!(own(&runs, &gathered).tool, None);
        assert_eq!(
            own(&tool, &gathered).tool,
            Some(ToolIdentity {
                program: program("cc"),
                fingerprint: Digest::of(b"cc 14.2"),
            })
        );
    }

    /// The target the two-machine tests key: `HOME` and `PATH` unkeyed, `RUSTFLAGS` keyed.
    fn cargo_build() -> Node {
        node("//:app", |node| {
            node.run = command(&["cc", "$in"]);
            node.inputs = vec![path("src/a.c")];
            node.env = marked(&[
                ("HOME", EnvMark::Unkeyed),
                ("PATH", EnvMark::Unkeyed),
                ("RUSTFLAGS", EnvMark::Key),
            ]);
        })
    }

    /// The digest of `node`'s key on `machine`, whose tool has `fingerprint` and whose input
    /// has the digest of `content`.
    fn key_on(machine: &TwoMachines, node: &Node, fingerprint: &[u8], content: &[u8]) -> Digest {
        let mut gathered = gathered(&[], &[], &[]);
        gathered.env = environment(std::slice::from_ref(node), machine).unwrap();
        gathered.fingerprints = BTreeMap::from([(program("cc"), Digest::of(fingerprint))]);
        let input = path("src/a.c");
        gathered.digests = BTreeMap::from([(&input, Digest::of(content))]);
        let components = KeyComponents {
            own: own(node, &gathered),
            deps: Vec::new(),
        };
        ActionKey::new(components).unwrap().digest()
    }

    fn key_of(machine: &TwoMachines, node: &Node) -> Digest {
        key_on(machine, node, b"cc 14.2", b"a")
    }

    #[test]
    fn two_machines_share_a_key_when_home_and_path_are_unkeyed() {
        let node = cargo_build();
        assert_eq!(
            key_of(&TwoMachines::first(), &node),
            key_of(&TwoMachines::second(), &node)
        );
    }

    #[test]
    fn a_key_variable_value_still_splits_the_key() {
        let node = cargo_build();
        let other = TwoMachines::second().with("RUSTFLAGS", "-Copt-level=3");
        assert_ne!(key_of(&TwoMachines::first(), &node), key_of(&other, &node));
    }

    #[test]
    fn a_tool_or_input_change_still_splits_the_key() {
        let node = cargo_build();
        let machine = TwoMachines::first();
        let original = key_of(&machine, &node);
        assert_ne!(original, key_on(&machine, &node, b"cc 15.1", b"a"));
        assert_ne!(original, key_on(&machine, &node, b"cc 14.2", b"a2"));
    }

    #[test]
    fn an_unset_unkeyed_variable_gives_the_same_key() {
        let node = cargo_build();
        let unset = TwoMachines::first().without("HOME");
        assert_eq!(key_of(&TwoMachines::first(), &node), key_of(&unset, &node));
    }

    #[test]
    fn components_hold_key_values_and_unkeyed_names_apart() {
        let node = cargo_build();
        let mut gathered = gathered(&[], &[], &[]);
        gathered.env = environment(std::slice::from_ref(&node), &TwoMachines::first()).unwrap();

        let components = own(&node, &gathered);

        assert_eq!(
            components.env,
            BTreeMap::from([(
                env_name("RUSTFLAGS"),
                Some(EnvValue::parse("-Cdebuginfo=0").unwrap())
            )])
        );
        assert_eq!(
            components.unkeyed,
            BTreeSet::from([env_name("HOME"), env_name("PATH")])
        );
    }

    #[test]
    fn a_setting_holding_a_folder_is_refused_for_the_first_node_naming_it() {
        let folders = MachineFolders::new(&["/home/al"], &[], &[]);
        let values = settings(&[("a", "/home/al/x"), ("b", "/home/al/y")]);
        let nodes = [
            node("//:one", |node| {
                node.run = command(&["cc", "$opt:b"]);
            }),
            node("//:two", |node| {
                node.run = command(&["cc", "$opt:a", "$opt:b"]);
            }),
        ];

        let error = super::check_settings(&nodes, &values, &folders).unwrap_err();

        assert_eq!(
            error.to_string(),
            "build.lua: //:one: the setting b holds this machine's path /home/al; it would give \
             the target another key on every machine"
        );
    }

    #[test]
    fn a_setting_no_command_names_or_without_a_folder_is_accepted() {
        let folders = MachineFolders::new(&["/home/al"], &[], &[]);
        let values = settings(&[("a", "/home/al/x"), ("b", "8")]);
        let nodes = [node("//:one", |node| {
            node.run = command(&["cc", "$opt:b"]);
        })];

        super::check_settings(&nodes, &values, &folders).unwrap();
    }

    #[test]
    fn only_key_variables_are_asked_of_the_host() {
        let node = cargo_build();
        let asked = environment(std::slice::from_ref(&node), &TwoMachines::first()).unwrap();
        let names: Vec<&EnvName> = asked.into_keys().collect();
        assert_eq!(names, [&env_name("RUSTFLAGS")]);
    }
}
