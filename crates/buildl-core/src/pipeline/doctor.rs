//! `doctor`: what a workspace allows and what its build wants, reported without refusing.
//!
//! Its own file because it is a command of its own that needs only some of the ports: it reads
//! the build files and the manifest, and asks nothing of the machine.
//!
//! Responsibilities: [`doctor`](fn@doctor).
//!
//! Non-responsibilities: collecting the wants, which Grants does; asking an approver, which a
//! build does; and showing the report, which the command line does.

use crate::error::Result;
use crate::grants::attribution::attribution;
use crate::grants::outside::outside;
use crate::grants::plugins::plugin_violations;
use crate::grants::wanted::{sites, wanted};
use crate::load::load;
use crate::placement::layout::Layout;
use crate::ports::{DeclarationSource, Dispatcher, Manifest};
use crate::resolve::resolve;
use crate::types::{Checkup, EntryName};

/// Loads the workspace once through `source`, resolves it, and reports what its build wants
/// against the ceiling `manifest` describes.
///
/// The wants are those of every target in the workspace, test targets included, with every
/// build setting at its default: what a build of the whole workspace with no override would
/// want. Unlike a plan, a want outside the ceiling, or outside a plugin that holds it, is
/// reported, not refused: the checkup lists every plugin violation, not the first. Nothing is
/// locked, written, hashed or looked up.
///
/// # Errors
///
/// Returns whatever the load returns, whatever [`resolve`] returns, whatever the manifest port
/// returns, and [`Error::OutputConflict`](crate::Error::OutputConflict) for two outputs that
/// would be written to one place.
pub fn doctor<S: DeclarationSource, D: Dispatcher, M: Manifest>(
    source: &S,
    dispatcher: &mut D,
    manifest: &M,
    entry: &EntryName,
) -> Result<Checkup> {
    let graph = resolve(load(source, dispatcher, entry)?)?;
    let workspace = manifest.workspace()?;
    let layout = Layout::of(&graph, &workspace.out)?;
    let sites = sites(&graph, graph.settings(), &layout);
    let wanted = wanted(&graph, &sites, &workspace);
    let outside = outside(&wanted, &workspace);
    let attribution = attribution(&graph, graph.settings(), &layout);
    let violations = plugin_violations(&graph, graph.settings(), &layout, &workspace);
    Ok(Checkup::new(
        workspace,
        wanted,
        outside,
        attribution,
        violations,
    ))
}
