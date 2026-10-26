//! The input set of a watched build: every workspace file whose change could change its result.
//!
//! Its own file because it is a different decision from reacting to a batch or running the loop:
//! it reads a graph and says which files a watcher must see.
//!
//! Responsibilities: [`watched_inputs`].
//!
//! Non-responsibilities: noticing a change to those files, which a [`Changes`](crate::Changes)
//! adapter does; and the build files, which are no target's declared input.

use std::collections::BTreeSet;

use crate::error::Result;
use crate::resolve::narrow;
use crate::types::{Label, Purpose, Scope, SourcePath, TargetGraph, TargetRole};

/// The workspace files a build of `scoped` reads, taken from the whole graph `whole` it was cut
/// from.
///
/// That is the declared inputs of every target of `scoped`, and of every discovery target of
/// `whole` with what each depends on: a build runs the discovery targets first whatever it was
/// asked for, so a change to what they read changes the build.
///
/// # Errors
///
/// Returns whatever [`narrow`](fn@crate::narrow) returns for the discovery targets; their labels
/// come from `whole`, so a graph that holds them gives none.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the watched build that gathers the input set is the only caller"
    )
)]
pub(crate) fn watched_inputs(
    whole: &TargetGraph,
    scoped: &TargetGraph,
) -> Result<BTreeSet<SourcePath>> {
    let mut inputs = inputs_of(scoped);
    let discovery: BTreeSet<Label> = whole
        .nodes()
        .iter()
        .filter(|node| node.role == TargetRole::Discovery)
        .map(|node| node.label.clone())
        .collect();
    if !discovery.is_empty() {
        let discovered = narrow(whole, &Scope::Targets(discovery), Purpose::Build)?;
        inputs.extend(inputs_of(&discovered));
    }
    Ok(inputs)
}

/// The declared inputs of every target of `graph`.
fn inputs_of(graph: &TargetGraph) -> BTreeSet<SourcePath> {
    graph
        .nodes()
        .iter()
        .flat_map(|node| node.inputs.iter().cloned())
        .collect()
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeSet;

    use super::watched_inputs;
    use crate::grants::fixtures::{graph, label, node, path};
    use crate::resolve::narrow;
    use crate::types::{Node, Purpose, Scope, SourcePath, TargetGraph, TargetRole};

    /// A change that gives a node the inputs `files`.
    fn reads(files: &[&str]) -> impl FnOnce(&mut Node) {
        let files: Vec<SourcePath> = files.iter().map(|file| path(file)).collect();
        move |node| node.inputs = files
    }

    fn paths(files: &[&str]) -> BTreeSet<SourcePath> {
        files.iter().map(|file| path(file)).collect()
    }

    /// `//app:app` needs `//lib:lib`; `//other:other` stands alone; each reads its own file.
    fn workspace() -> TargetGraph {
        graph(
            vec![
                node("//app:app", reads(&["app/main.c"])),
                node("//lib:lib", reads(&["lib/lib.c"])),
                node("//other:other", reads(&["other/other.c"])),
            ],
            &[&[1], &[], &[]],
        )
    }

    /// The part of `whole` a build of `target` works on.
    fn scoped(whole: &TargetGraph, target: &str) -> TargetGraph {
        let scope = Scope::Targets(BTreeSet::from([label(target)]));
        narrow(whole, &scope, Purpose::Build).unwrap().into_owned()
    }

    #[test]
    fn the_scoped_targets_and_what_they_need_give_their_inputs() {
        let whole = workspace();
        let scoped = scoped(&whole, "//app:app");
        assert_eq!(
            watched_inputs(&whole, &scoped).unwrap(),
            paths(&["app/main.c", "lib/lib.c"])
        );
    }

    #[test]
    fn a_target_outside_the_scope_gives_nothing() {
        let whole = workspace();
        let scoped = scoped(&whole, "//app:app");
        let inputs = watched_inputs(&whole, &scoped).unwrap();
        assert!(!inputs.is_empty());
        assert!(!inputs.contains(&path("other/other.c")));
    }

    #[test]
    fn every_discovery_target_gives_its_inputs_whatever_the_scope() {
        let whole = graph(
            vec![
                node("//app:app", reads(&["app/main.c"])),
                node("//find:find", |node| {
                    node.role = TargetRole::Discovery;
                    node.inputs = vec![path("find/list.txt")];
                }),
                node("//lib:lib", reads(&["lib/lib.c"])),
                node("//other:other", reads(&["other/other.c"])),
            ],
            &[&[], &[2], &[], &[]],
        );
        let scoped = scoped(&whole, "//app:app");
        assert_eq!(
            watched_inputs(&whole, &scoped).unwrap(),
            paths(&["app/main.c", "find/list.txt", "lib/lib.c"])
        );
    }

    #[test]
    fn no_discovery_target_adds_nothing() {
        let whole = workspace();
        let scoped = scoped(&whole, "//lib:lib");
        assert_eq!(
            watched_inputs(&whole, &scoped).unwrap(),
            paths(&["lib/lib.c"])
        );
    }
}
