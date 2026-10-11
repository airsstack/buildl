//! A target's own key components, built from what was gathered for the whole graph.
//!
//! Its own file because this is where the answers of five ports meet one target: by the time a
//! node's components are built, every value they need has been fetched once for the graph, and
//! building them asks nothing more.
//!
//! Responsibilities: [`environment`], [`Gathered`] and [`own`].
//!
//! Non-responsibilities: fetching the other values, and a target's dependencies, which join its
//! key only once it is known what they produced.

use std::collections::BTreeMap;

use crate::error::Result;
use crate::plan::settings::referenced;
use crate::plan::tools::program_of;
use crate::ports::Host;
use crate::types::{
    Digest, EnvName, EnvValue, InputDigest, Node, OwnComponents, Platform, Program, SettingName,
    SettingValue, SourcePath, ToolIdentity,
};

/// The value of every variable in `names`, asked for in the order given.
///
/// # Errors
///
/// Returns whatever `host` returns for the first variable it cannot report.
pub(crate) fn environment<'g, H: Host>(
    names: impl IntoIterator<Item = &'g EnvName>,
    host: &H,
) -> Result<BTreeMap<&'g EnvName, Option<EnvValue>>> {
    names
        .into_iter()
        .map(|name| Ok((name, host.env(name)?)))
        .collect()
}

/// Everything gathered for a graph before any target's components are built.
#[derive(Debug)]
pub(crate) struct Gathered<'g> {
    /// The value every declared setting has for this run.
    pub(crate) settings: BTreeMap<SettingName, SettingValue>,
    /// The fingerprint of every program a command names.
    pub(crate) fingerprints: BTreeMap<Program, Digest>,
    /// The content digest of every input.
    pub(crate) digests: BTreeMap<&'g SourcePath, Digest>,
    /// The value of every environment variable a target reads; `None` for one that is unset.
    pub(crate) env: BTreeMap<&'g EnvName, Option<EnvValue>>,
    /// The platform actions run on.
    pub(crate) platform: Platform,
}

/// The components of `node`'s key that do not wait on another target.
///
/// Inputs and outputs keep the order they were written in. The environment is keyed by name, so
/// the order `node` lists its variables in does not reach the key. Only the settings the command
/// names are kept. An input or a program `gathered` holds nothing for is left out, and a variable
/// it holds nothing for is taken as unset; neither happens when `gathered` was built for the
/// graph `node` belongs to.
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
        .map(|name| (name.clone(), gathered.env.get(name).cloned().flatten()))
        .collect();
    let tool = program_of(&node.run).and_then(|program| {
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
        tool,
        settings: referenced(&node.run, &gathered.settings),
        network: node.network,
        platform: gathered.platform.clone(),
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeMap;

    use super::{Gathered, own};
    use crate::plan::fixtures::{command, env_name, node, path, program, settings};
    use crate::types::{
        Digest, EnvName, EnvValue, InputDigest, NetworkAccess, OutputName, Platform, SourcePath,
        ToolIdentity,
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
            node.env = vec![env_name("PATH"), env_name("HOME")];
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
        assert!(components.inputs.is_empty());
        assert!(components.env.is_empty());
        assert!(components.settings.is_empty());
        assert_eq!(components.network, NetworkAccess::Sealed);
    }

    #[test]
    fn the_order_a_node_lists_its_variables_in_does_not_reach_its_components() {
        let names = [env_name("HOME"), env_name("PATH")];
        let gathered = gathered(&[], &names, &[Some("/home/u"), Some("/bin")]);
        let forward = node("//:app", |node| {
            node.env = vec![env_name("HOME"), env_name("PATH")];
        });
        let backward = node("//:app", |node| {
            node.env = vec![env_name("PATH"), env_name("HOME")];
        });
        assert_eq!(own(&forward, &gathered), own(&backward, &gathered));
    }
}
