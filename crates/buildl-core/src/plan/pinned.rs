//! Pinned variables: the environment a strategy sets itself, which a target cannot also declare.
//!
//! Its own file because it is a check of its own, run once the ceiling has passed and before the
//! machine is asked anything: a target that declares a variable the sandbox fixes would be asking
//! for a value it can never receive.
//!
//! Responsibilities: [`pinned`].
//!
//! Non-responsibilities: which variables a strategy pins, which is [`Isolation::pinned`]'s, and
//! setting them, which the strategy does when it runs an action.
//!
//! [`Isolation::pinned`]: crate::types::Isolation::pinned

use crate::error::{DeclarationSite, Error, Result};
use crate::grants::wanted::Sites;
use crate::types::Workspace;

/// Refuses the first variable `workspace`'s strategy pins that a target declares.
///
/// The variables are tried in name order, and the target named is the lowest-labelled one
/// declaring the variable. Under a strategy that pins nothing, every declaration passes.
///
/// # Errors
///
/// Returns [`Error::PinnedEnv`] for a pinned variable a target declares.
pub(super) fn pinned(sites: &Sites<'_>, workspace: &Workspace) -> Result<()> {
    for (name, value) in workspace.execution.strategy.pinned() {
        if let Some(node) = sites.env.get(&name) {
            return Err(Error::PinnedEnv {
                name,
                value,
                site: Box::new(DeclarationSite::from(*node)),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::{BTreeMap, BTreeSet};

    use super::pinned;
    use crate::error::{Error, Result};
    use crate::grants::fixtures::{dir, env_name, graph, node};
    use crate::grants::wanted::sites;
    use crate::placement::layout::Layout;
    use crate::types::{ApproverMode, Ceiling, Execution, Isolation, TargetGraph, Workspace};

    fn workspace(strategy: Isolation) -> Workspace {
        Workspace {
            out: dir("out"),
            state: dir(".buildl"),
            ceiling: Ceiling {
                run: BTreeSet::new(),
                env: BTreeSet::new(),
                read: BTreeSet::new(),
                write: BTreeSet::new(),
            },
            execution: Execution {
                strategy,
                ..Execution::default()
            },
            approver: ApproverMode::Manifest,
            plugins: std::collections::BTreeMap::new(),
            remote: None,
        }
    }

    /// A graph whose targets declare the variables given, as `(label, names)`.
    fn declaring(targets: &[(&str, &[&str])]) -> TargetGraph {
        let nodes = targets
            .iter()
            .map(|(label, names)| {
                node(label, |node| {
                    node.env = names.iter().map(|name| env_name(name)).collect();
                })
            })
            .collect::<Vec<_>>();
        let edges = vec![&[][..]; nodes.len()];
        graph(nodes, &edges)
    }

    fn pinned_in(graph: &TargetGraph, strategy: Isolation) -> Result<()> {
        let layout = Layout::of(graph, &dir("out")).unwrap();
        pinned(
            &sites(graph, &BTreeMap::new(), &layout),
            &workspace(strategy),
        )
    }

    #[test]
    fn a_pinned_name_is_refused_with_the_lowest_labelled_site() {
        let graph = declaring(&[("//a:y", &["TZ"]), ("//b:x", &["TZ"])]);
        match pinned_in(&graph, Isolation::Sandbox).unwrap_err() {
            Error::PinnedEnv { name, value, site } => {
                assert_eq!(name.as_str(), "TZ");
                assert_eq!(value.as_str(), "UTC");
                assert_eq!(site.label.to_string(), "//a:y");
            }
            other => unreachable!("expected PinnedEnv, got {other:?}"),
        }
    }

    #[test]
    fn the_first_pinned_name_in_name_order_is_reported() {
        let graph = declaring(&[("//a:y", &["TZ", "LC_ALL"])]);
        match pinned_in(&graph, Isolation::Sandbox).unwrap_err() {
            Error::PinnedEnv { name, .. } => assert_eq!(name.as_str(), "LC_ALL"),
            other => unreachable!("expected PinnedEnv, got {other:?}"),
        }
    }

    #[test]
    fn under_bare_a_pinned_name_is_planned() {
        let graph = declaring(&[("//a:y", &["TZ", "LC_ALL"])]);
        assert!(pinned_in(&graph, Isolation::Bare).is_ok());
    }

    #[test]
    fn no_pinned_name_is_planned() {
        let graph = declaring(&[("//a:y", &["HOME"])]);
        assert!(pinned_in(&graph, Isolation::Sandbox).is_ok());
    }
}
