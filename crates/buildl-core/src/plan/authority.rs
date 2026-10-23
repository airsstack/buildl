//! Authority: what a graph's targets want, and whether the workspace's ceiling and the plugins
//! that made them allow it.
//!
//! Its own file because these are the checks that run before anything else is asked of the
//! machine: a build that wants more than its workspace or its plugins allow is refused here,
//! with nothing hashed, looked up or read.
//!
//! Responsibilities: [`check`] and [`plugin_check`].
//!
//! Non-responsibilities: what a ceiling or a plugin's cap is, which is the manifest's to say;
//! collecting what the targets want, which is Grants'; and finding the programs a build may run.

use std::collections::BTreeMap;

use crate::error::{DeclarationSite, Error, Result};
use crate::grants::plugins::plugin_violations;
use crate::grants::wanted::{Sites, wanted};
use crate::placement::layout::Layout;
use crate::types::{
    Directory, Node, SettingName, SettingValue, SourcePath, TargetGraph, Want, WantedSet, Workspace,
};

/// Checks every want of a plugin's targets against the plugins that hold them.
///
/// Runs before [`check`], so a plugin's target is refused by its plugin before the ceiling is
/// asked.
///
/// # Errors
///
/// Returns [`Error::PluginRefused`] for the first violation, in label order, then chain order.
pub(crate) fn plugin_check(
    graph: &TargetGraph,
    settings: &BTreeMap<SettingName, SettingValue>,
    layout: &Layout,
    workspace: &Workspace,
) -> Result<()> {
    plugin_violations(graph, settings, layout, workspace)
        .into_iter()
        .next()
        .map_or(Ok(()), |violation| {
            Err(Error::PluginRefused {
                violation: Box::new(violation),
            })
        })
}

/// Checks every want against `workspace`'s ceiling and returns what the build wants.
///
/// The directories a build writes into are the workspace's own two, `out` and `state`. A target
/// declared to reach the network is listed in the result and compared with nothing: a ceiling
/// has no entry for the network.
///
/// # Errors
///
/// Returns [`Error::Refused`] for the first want outside the ceiling. Programs are checked
/// first, then environment variables, then inputs, then the directories written into; within
/// each kind the lowest want first. The refusal names the lowest-labelled target wanting it,
/// except for a directory, which no target declares.
pub(crate) fn check(
    graph: &TargetGraph,
    sites: &Sites<'_>,
    workspace: &Workspace,
) -> Result<WantedSet> {
    let ceiling = &workspace.ceiling;
    let refused = |want: Want, node: Option<&Node>| Error::Refused {
        want,
        site: node.map(|node| Box::new(DeclarationSite::from(node))),
        ceiling: Box::new(ceiling.clone()),
    };

    let outside = sites
        .programs
        .iter()
        .find(|(program, _)| !ceiling.run.contains(*program));
    if let Some((program, node)) = outside {
        return Err(refused(Want::Program(program.clone()), Some(node)));
    }

    let outside = sites
        .env
        .iter()
        .find(|(name, _)| !ceiling.env.contains(**name));
    if let Some((name, node)) = outside {
        return Err(refused(Want::Env((*name).clone()), Some(node)));
    }

    let outside = sites
        .inputs
        .iter()
        .find(|(path, _)| !ceiling.read.iter().any(|allowed| holds(allowed, path)));
    if let Some((path, node)) = outside {
        return Err(refused(Want::Read((*path).clone()), Some(node)));
    }

    let wanted = wanted(graph, sites, workspace);
    let outside = wanted.write.iter().find(|directory| {
        !ceiling
            .write
            .iter()
            .any(|allowed| allowed.contains(directory))
    });
    if let Some(directory) = outside {
        return Err(refused(Want::Write(directory.clone()), None));
    }

    Ok(wanted)
}

/// Whether the file `path` lies in `directory` or below it.
fn holds(directory: &Directory, path: &SourcePath) -> bool {
    directory.is_root()
        || path
            .as_str()
            .strip_prefix(directory.as_str())
            .is_some_and(|rest| rest.starts_with('/'))
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::{BTreeMap, BTreeSet};

    use super::{check, holds, plugin_check};
    use crate::error::{DeclarationSite, Error};
    use crate::grants::fixtures::{
        command, dir, env_name, graph, label, node, path, program, settings,
    };
    use crate::grants::wanted::sites;
    use crate::placement::layout::Layout;
    use crate::types::ApproverMode;
    use crate::types::{
        Ceiling, Directory, Execution, NetworkAccess, Node, Origin, PluginName, PluginRef,
        PluginVersion, SettingName, SettingValue, TargetGraph, Want, WantedSet, Workspace,
    };

    /// A workspace writing to `out` and `.buildl`, whose ceiling allows `cc` and `ld`, `HOME`
    /// and `PATH`, reads under `src` and `lib`, and writes to its own two directories.
    fn workspace() -> Workspace {
        Workspace {
            out: dir("out"),
            state: dir(".buildl"),
            ceiling: Ceiling {
                run: BTreeSet::from([program("cc"), program("ld")]),
                env: BTreeSet::from([env_name("HOME"), env_name("PATH")]),
                read: BTreeSet::from([dir("src"), dir("lib")]),
                write: BTreeSet::from([dir("out"), dir(".buildl")]),
            },
            execution: Execution::default(),
            approver: ApproverMode::Manifest,
            plugins: std::collections::BTreeMap::new(),
        }
    }

    /// `//:app` linking with `ld`, reading `PATH` and `src/main.c`.
    fn app() -> Node {
        node("//:app", |node| {
            node.run = command(&["ld", "$in"]);
            node.env = vec![env_name("PATH")];
            node.inputs = vec![path("src/main.c")];
        })
    }

    /// `//lib:text` compiling with `cc`, reading `HOME`, `PATH` and `lib/text.c`, on the network.
    fn text() -> Node {
        node("//lib:text", |node| {
            node.env = vec![env_name("PATH"), env_name("HOME")];
            node.inputs = vec![path("lib/text.c")];
            node.network = NetworkAccess::Declared;
        })
    }

    fn checked(graph: &TargetGraph, workspace: &Workspace) -> crate::error::Result<WantedSet> {
        checked_under(graph, &BTreeMap::new(), workspace)
    }

    fn checked_under(
        graph: &TargetGraph,
        settings: &BTreeMap<SettingName, SettingValue>,
        workspace: &Workspace,
    ) -> crate::error::Result<WantedSet> {
        let layout = Layout::of(graph, &workspace.out).unwrap();
        check(graph, &sites(graph, settings, &layout), workspace)
    }

    /// What `graph` is refused under `workspace`: the want, and the label and build file named.
    fn refusal(graph: &TargetGraph, workspace: &Workspace) -> (Want, Option<(String, String)>) {
        refusal_of(checked(graph, workspace), workspace)
    }

    /// What `checked` refused under `workspace`: the want, and the label and build file named.
    fn refusal_of(
        checked: crate::error::Result<WantedSet>,
        workspace: &Workspace,
    ) -> (Want, Option<(String, String)>) {
        match checked.unwrap_err() {
            Error::Refused {
                want,
                site,
                ceiling,
            } => {
                assert_eq!(*ceiling, workspace.ceiling);
                let named = site.map(|site| {
                    let DeclarationSite {
                        label, provenance, ..
                    } = *site;
                    (label.to_string(), provenance.to_string())
                });
                (want, named)
            }
            other => unreachable!("expected Refused, got {other:?}"),
        }
    }

    fn named(label: &str, file: &str) -> (String, String) {
        (label.to_owned(), file.to_owned())
    }

    #[test]
    fn a_build_inside_the_ceiling_reports_what_it_wants() {
        let graph = graph(vec![app(), text()], &[&[], &[]]);
        assert_eq!(
            checked(&graph, &workspace()).unwrap(),
            WantedSet {
                programs: BTreeSet::from([program("cc"), program("ld")]),
                env: BTreeSet::from([env_name("HOME"), env_name("PATH")]),
                write: BTreeSet::from([dir("out"), dir(".buildl")]),
                network: BTreeSet::from([label("//lib:text")]),
            }
        );
    }

    #[test]
    fn a_program_outside_the_ceiling_is_refused_with_its_site() {
        let mut workspace = workspace();
        workspace.ceiling.run = BTreeSet::from([program("ld")]);
        let graph = graph(vec![app(), text()], &[&[], &[]]);
        assert_eq!(
            refusal(&graph, &workspace),
            (
                Want::Program(program("cc")),
                Some(named("//lib:text", "lib/build.lua"))
            )
        );
    }

    #[test]
    fn a_program_named_through_a_setting_is_wanted_and_held_to_the_ceiling() {
        let graph = graph(
            vec![node("//:app", |node| {
                node.run = command(&["$opt:linker", "$in"]);
            })],
            &[&[]],
        );
        let workspace = workspace();

        let allowed = checked_under(&graph, &settings(&[("linker", "ld")]), &workspace);
        assert_eq!(allowed.unwrap().programs, BTreeSet::from([program("ld")]));

        let outside = checked_under(&graph, &settings(&[("linker", "mold")]), &workspace);
        assert_eq!(
            refusal_of(outside, &workspace),
            (
                Want::Program(program("mold")),
                Some(named("//:app", "build.lua"))
            )
        );
    }

    #[test]
    fn a_variable_outside_the_ceiling_is_refused_with_its_site() {
        let mut workspace = workspace();
        workspace.ceiling.env = BTreeSet::from([env_name("PATH")]);
        let graph = graph(vec![app(), text()], &[&[], &[]]);
        assert_eq!(
            refusal(&graph, &workspace),
            (
                Want::Env(env_name("HOME")),
                Some(named("//lib:text", "lib/build.lua"))
            )
        );
    }

    #[test]
    fn an_input_outside_the_ceiling_is_refused_with_its_site() {
        let mut workspace = workspace();
        workspace.ceiling.read = BTreeSet::from([dir("lib")]);
        let graph = graph(vec![app(), text()], &[&[], &[]]);
        assert_eq!(
            refusal(&graph, &workspace),
            (
                Want::Read(path("src/main.c")),
                Some(named("//:app", "build.lua"))
            )
        );
    }

    #[test]
    fn a_directory_written_outside_the_ceiling_is_refused_without_a_site() {
        let graph = graph(vec![app()], &[&[]]);
        let mut no_out = workspace();
        no_out.ceiling.write = BTreeSet::from([dir(".buildl")]);
        assert_eq!(refusal(&graph, &no_out), (Want::Write(dir("out")), None));
        let mut no_state = workspace();
        no_state.ceiling.write = BTreeSet::from([dir("out")]);
        assert_eq!(
            refusal(&graph, &no_state),
            (Want::Write(dir(".buildl")), None)
        );
    }

    #[test]
    fn kinds_are_checked_in_a_fixed_order() {
        let graph = graph(vec![app(), text()], &[&[], &[]]);
        let mut workspace = workspace();
        workspace.ceiling = Ceiling {
            run: BTreeSet::new(),
            env: BTreeSet::new(),
            read: BTreeSet::new(),
            write: BTreeSet::new(),
        };
        assert_eq!(refusal(&graph, &workspace).0, Want::Program(program("cc")));
        workspace.ceiling.run = BTreeSet::from([program("cc"), program("ld")]);
        assert_eq!(refusal(&graph, &workspace).0, Want::Env(env_name("HOME")));
        workspace.ceiling.env = BTreeSet::from([env_name("HOME"), env_name("PATH")]);
        assert_eq!(
            refusal(&graph, &workspace).0,
            Want::Read(path("lib/text.c"))
        );
        workspace.ceiling.read = BTreeSet::from([Directory::root()]);
        assert_eq!(refusal(&graph, &workspace).0, Want::Write(dir(".buildl")));
        workspace.ceiling.write = BTreeSet::from([Directory::root()]);
        assert!(checked(&graph, &workspace).is_ok());
    }

    #[test]
    fn the_lowest_labelled_target_wanting_a_thing_is_the_one_named() {
        let graph = graph(
            vec![
                node("//:a", |node| node.run = command(&["docker", "build"])),
                node("//:b", |node| node.run = command(&["docker", "push"])),
            ],
            &[&[], &[]],
        );
        assert_eq!(
            refusal(&graph, &workspace()),
            (
                Want::Program(program("docker")),
                Some(named("//:a", "build.lua"))
            )
        );
    }

    #[test]
    fn an_empty_ceiling_refuses_even_a_build_that_asks_for_nothing_but_its_own_directories() {
        let mut workspace = workspace();
        workspace.ceiling = Ceiling {
            run: BTreeSet::new(),
            env: BTreeSet::new(),
            read: BTreeSet::new(),
            write: BTreeSet::new(),
        };
        let graph = graph(
            vec![node("//:gen", |node| node.run = command(&["$deps"]))],
            &[&[]],
        );
        assert_eq!(
            refusal(&graph, &workspace),
            (Want::Write(dir(".buildl")), None)
        );
    }

    #[test]
    fn an_input_is_held_by_its_directory_and_not_by_one_sharing_a_prefix() {
        assert!(holds(&dir("lib"), &path("lib/x")));
        assert!(holds(&dir("lib"), &path("lib/text/x")));
        assert!(!holds(&dir("lib"), &path("lib2/x")));
        assert!(!holds(&dir("lib/text"), &path("lib/x")));
        assert!(!holds(&dir("lib"), &path("lib")));
        assert!(holds(&Directory::root(), &path("anything/at/all")));
        assert!(holds(&Directory::root(), &path("top")));
    }

    #[test]
    fn an_input_under_a_directory_sharing_a_prefix_with_an_allowed_one_is_refused() {
        let graph = graph(
            vec![node("//lib2:x", |node| node.inputs = vec![path("lib2/x")])],
            &[&[]],
        );
        assert_eq!(
            refusal(&graph, &workspace()),
            (
                Want::Read(path("lib2/x")),
                Some(named("//lib2:x", "lib2/build.lua"))
            )
        );
    }

    #[test]
    fn a_directory_is_allowed_by_an_entry_at_or_above_it() {
        let graph = graph(vec![app()], &[&[]]);
        let mut workspace = workspace();
        workspace.out = dir("build/out");
        workspace.ceiling.write = BTreeSet::from([dir("build"), dir(".buildl")]);
        assert!(checked(&graph, &workspace).is_ok());
        workspace.ceiling.write = BTreeSet::from([dir("build/out/deep"), dir(".buildl")]);
        assert_eq!(
            refusal(&graph, &workspace),
            (Want::Write(dir("build/out")), None)
        );
    }

    /// `//:app` running `curl`, declared by the plugin `go`.
    fn by_go() -> Node {
        let go = PluginRef {
            name: PluginName::parse("go").unwrap(),
            version: PluginVersion::parse("1.2.0").unwrap(),
            run: BTreeSet::new(),
            env: BTreeSet::new(),
        };
        node("//:app", |node| {
            node.run = command(&["curl"]);
            node.origin = Origin::plugins(vec![go.clone()]).unwrap();
            node.narrowing = vec![go];
        })
    }

    fn plugin_checked(graph: &TargetGraph, workspace: &Workspace) -> crate::error::Result<()> {
        let layout = Layout::of(graph, &workspace.out).unwrap();
        plugin_check(graph, &BTreeMap::new(), &layout, workspace)
    }

    #[test]
    fn a_plugin_target_outside_its_plugins_caps_is_refused_with_the_first_violation() {
        let graph = graph(vec![by_go()], &[&[]]);
        match plugin_checked(&graph, &workspace()).unwrap_err() {
            Error::PluginRefused { violation } => {
                assert_eq!(violation.want, Want::Program(program("curl")));
                assert_eq!(violation.site.label, label("//:app"));
                assert_eq!(violation.plugin.name.as_str(), "go");
            }
            other => unreachable!("expected PluginRefused, got {other:?}"),
        }
    }

    #[test]
    fn a_graph_without_a_plugin_violation_passes_the_plugin_check() {
        let build_file_only = graph(vec![app()], &[&[]]);
        assert!(plugin_checked(&build_file_only, &workspace()).is_ok());
    }
}
