//! The assembly: from a target graph, and what the ports report, to one plan.
//!
//! Its own file because it is where this phase's steps meet and where their order is fixed:
//! read the manifest, check the overrides, place every output, check every want of a plugin's
//! targets against its plugins, check every want against the ceiling, refuse a variable the
//! strategy pins, and only then ask the machine anything — find the tools, digest the inputs,
//! read the environment — before keying every target and comparing each key with the one its
//! last build recorded.
//!
//! Responsibilities: [`plan`].
//!
//! Non-responsibilities: what each step decides. Each has its own sibling file.
//!
//! This file holds no test module: it only passes values between its siblings and the ports,
//! so it is exercised by the crate's integration tests, against in-memory ports.

use std::collections::BTreeMap;

use crate::error::Result;
use crate::grants::wanted::sites;
use crate::placement::layout::Layout;
use crate::plan::authority::{check, plugin_check};
use crate::plan::components::{Gathered, environment, own};
use crate::plan::inputs::digests;
use crate::plan::order::topological;
use crate::plan::pinned::pinned;
use crate::plan::settings::effective;
use crate::plan::tools::{Named, fingerprints, naming};
use crate::plan::verdict;
use crate::ports::{ActionCache, Host, Manifest, PortSet, Ports};
use crate::types::{Plan, Rerun, SettingName, SettingValue, TargetGraph};

/// Plans a build of `graph`: one verdict per target, what the build wants, and the settings in
/// force.
///
/// `overrides` gives values to build settings for this run: the plan holds every setting `graph`
/// declares with its override, or else its default, and nothing after the plan reads an override
/// again. Every target is given an action key from its command, the digests of its inputs, its
/// environment, the tool it runs, the settings it names, the platform, and what its dependencies
/// produced; the key is compared with the one the target's last build recorded. A target is
/// clean when the two are equal, dirty when they differ, and conditional when all that could
/// still differ is the output of a dependency that is about to run. When `rerun` asks for the
/// tests to run again, a test not declared to always run is dirty with
/// [`DirtyReason::Rerun`](crate::DirtyReason::Rerun) whatever its key, and every other target is
/// judged as before.
///
/// The tool a target runs is the program its command's first argument names once each `$opt:`
/// reference in it is replaced by the setting's value for this run. So a program named through
/// a setting is looked up and keyed exactly as one written out. A first argument that still
/// holds a `$` after that, as one running what the build produced does, names no tool here.
///
/// Every program a command runs is held to the ceiling, whichever way it is named: written out,
/// through a setting, or through `$in` or `$out`. A program one of the target's dependencies
/// builds, written out, named through a setting or taken from `$deps`, is held to nothing, and
/// is neither looked up nor part of the key.
///
/// Nothing runs. The one thing a plan writes is the stat cache, and only when it had to hash a
/// file.
///
/// # Errors
///
/// Returns the first failure met, in this order:
///
/// 1. whatever the manifest port returns;
/// 2. [`Error::UnknownOverride`](crate::Error::UnknownOverride) for an override naming no
///    declared setting;
/// 3. [`Error::OutputConflict`](crate::Error::OutputConflict) for two outputs of the planned
///    graph that would be written to one place;
/// 4. [`Error::PluginRefused`](crate::Error::PluginRefused) for a want of a plugin's target that
///    one of the plugins holding it does not allow;
/// 5. [`Error::Refused`](crate::Error::Refused) for a want outside the workspace's ceiling,
///    a program named through `$in`, `$out` or `$deps` included; nothing is asked of any other
///    port before this check has passed;
/// 6. [`Error::PinnedEnv`](crate::Error::PinnedEnv) for a target that declares a variable the
///    workspace's strategy pins;
/// 7. [`Error::MissingTool`](crate::Error::MissingTool) for a program that cannot be found;
/// 8. [`Error::MissingInput`](crate::Error::MissingInput) for an input that does not exist, and
///    [`Error::InputNotAFile`](crate::Error::InputNotAFile) for one that is not a regular file;
/// 9. whatever the host port returns for an environment variable.
///
/// A port's own failure is returned from the step that met it.
pub fn plan<P: Ports>(
    graph: &TargetGraph,
    overrides: &BTreeMap<SettingName, SettingValue>,
    rerun: Rerun,
    ports: &mut PortSet<P>,
) -> Result<Plan> {
    let workspace = ports.manifest.workspace()?;
    let settings = effective(graph.settings(), overrides)?;
    let layout = Layout::of(graph, &workspace.out)?;
    plugin_check(graph, &settings, &layout, &workspace)?;
    let sites = sites(graph, &settings, &layout);
    let wanted = check(graph, &sites, &workspace)?;
    pinned(&sites, &workspace)?;

    let Named {
        programs,
        built_runners: built,
    } = naming(graph, &settings, &layout);
    let fingerprints = fingerprints(&programs, &ports.tools)?;
    let digests = digests(&sites.inputs, &ports.digester, &mut ports.stat_cache)?;
    let platform = ports.host.platform();
    let env = environment(sites.env.keys().copied(), &ports.host)?;
    let gathered = Gathered {
        settings,
        fingerprints,
        built,
        digests,
        env,
        platform,
    };

    let components = graph
        .nodes()
        .iter()
        .map(|node| own(node, &gathered))
        .collect();
    let order = topological(graph);
    let verdicts = verdict::all(graph, components, &order, rerun, |label| {
        ports.cache.row(label)
    })?;
    let labels = graph
        .nodes()
        .iter()
        .map(|node| node.label.clone())
        .collect();
    Ok(Plan::new(labels, verdicts, wanted, gathered.settings))
}
