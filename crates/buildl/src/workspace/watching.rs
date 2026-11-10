//! Watching a workspace: the cycles of a watch, each on a workspace opened again.
//!
//! An open [`Workspace`] reads `buildl.toml` once, so a watch keeps none open between cycles:
//! [`Reopening`] holds the first wiring only as the recipe for the next one, and every cycle
//! reads the manifest again.
//!
//! Responsibilities: [`Reopening`], which runs a cycle's build and its graph check on a
//! workspace opened for that cycle alone, hands the loop the ignore rules of the last workspace
//! it opened for a build or a graph check, and forwards the loop's lines to the caller's
//! reporter; and [`Workspace::watch`].
//!
//! Non-responsibilities: when a cycle starts and what a change means, which is the loop's in
//! `buildl_core::watch`; seeing file events, which is the `Changes` adapter's; and the build
//! itself, which is [`Workspace::build`]'s.

use std::collections::BTreeMap;

use buildl_core::{
    Changes, Cycle, Cycles, Error, Reporter, Result, Scope, SettingName, SettingValue, TargetGraph,
    Trigger, WatchReport, Watched,
};

use crate::approver::Console;
use crate::ignore_rules::BuildlIgnore;
use crate::local_ports::Wiring;
use crate::workspace::{RunOptions, Workspace};

/// The cycles of one watch: each opens the workspace again and runs on what it reads.
///
/// It also keeps the `.buildlignore` rules of the last workspace it opened, for a build or for a
/// graph check, for the loop to ask about each changed path.
///
/// The reporter is both the build's `Reporter` and the loop's `WatchReport` and is borrowed
/// once, here, so one value writes to the terminal and the two kinds of line never interleave.
pub(super) struct Reopening<'a, R, C> {
    wiring: &'a Wiring,
    scope: &'a Scope,
    overrides: &'a BTreeMap<SettingName, SettingValue>,
    options: RunOptions,
    reporter: &'a mut R,
    console: &'a mut C,
    rules: Option<BuildlIgnore>,
}

impl<'a, R, C> Reopening<'a, R, C> {
    /// Cycles that reopen from `wiring` and build `scope` with `overrides` and `options`.
    pub(super) const fn new(
        wiring: &'a Wiring,
        scope: &'a Scope,
        overrides: &'a BTreeMap<SettingName, SettingValue>,
        options: RunOptions,
        reporter: &'a mut R,
        console: &'a mut C,
    ) -> Self {
        Self {
            wiring,
            scope,
            overrides,
            options,
            reporter,
            console,
            rules: None,
        }
    }
}

impl<R: Reporter, C: Console> Cycles for Reopening<'_, R, C> {
    type Ignore = BuildlIgnore;

    fn rules(&self) -> Option<&BuildlIgnore> {
        self.rules.as_ref()
    }

    fn build(&mut self) -> Cycle {
        let opened = self.wiring.reopened().and_then(|wiring| {
            self.rules = Some(wiring.ignore().clone());
            let layout = wiring.watch_layout()?;
            Ok((Workspace { wiring }, layout))
        });
        match opened {
            Ok((workspace, layout)) => Cycle {
                layout: Some(layout),
                watched: workspace.build_watched(
                    self.scope,
                    self.overrides,
                    self.options,
                    &mut *self.reporter,
                    &mut *self.console,
                ),
            },
            Err(error) => Cycle {
                layout: None,
                watched: Watched {
                    built: None,
                    ended: Err(error),
                },
            },
        }
    }

    fn graph(&mut self) -> Result<TargetGraph> {
        let wiring = self.wiring.reopened()?;
        self.rules = Some(wiring.ignore().clone());
        let workspace = Workspace { wiring };
        workspace.graph().map(|(graph, _)| graph)
    }
}

impl<R: WatchReport, C> WatchReport for Reopening<'_, R, C> {
    fn building(&mut self, trigger: &Trigger) {
        self.reporter.building(trigger);
    }

    fn failed(&mut self, error: &Error) {
        self.reporter.failed(error);
    }

    fn waiting(&mut self) {
        self.reporter.waiting();
    }
}

impl Workspace {
    /// Builds `scope`, then again each time `changes` names a path the build reads, until
    /// `changes` can no longer be watched, and returns the error that ended it.
    ///
    /// Every cycle opens the workspace again, so an edit to `buildl.toml` or to a plugin's
    /// `extension.toml` takes effect on the next cycle, and a cycle that fails is reported and
    /// followed by the next change. The caller starts `changes` before calling, so an edit made
    /// during the first cycle is seen. `reporter` shows both what each build reports and the
    /// watch's own lines.
    ///
    /// The error is, at once, what [`Wiring::watch_layout`] returns for this workspace: it
    /// opened, so that is a manifest whose watched files it cannot name. Otherwise it is the
    /// error `changes` ended with.
    pub fn watch<Ch: Changes, R: Reporter + WatchReport, C: Console>(
        &self,
        changes: &mut Ch,
        scope: &Scope,
        overrides: &BTreeMap<SettingName, SettingValue>,
        options: RunOptions,
        reporter: &mut R,
        console: &mut C,
    ) -> Error {
        let layout = match self.wiring.watch_layout() {
            Ok(layout) => layout,
            Err(error) => return error,
        };
        buildl_core::watch(
            changes,
            &mut Reopening::new(&self.wiring, scope, overrides, options, reporter, console),
            layout,
        )
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeMap;
    use std::fs;
    use std::path::PathBuf;

    use buildl_core::{
        Cycles, EntryKind, Error, IgnoreRules, LiveOutput, OnFailure, PortName, Scope,
    };

    use crate::approver::NoTerminal;
    use crate::local_ports::Wiring;
    use crate::report::Silent;
    use crate::workspace::RunOptions;
    use crate::workspace::watching::Reopening;

    /// A temporary workspace whose `buildl.toml` holds `manifest`, and its canonical root.
    fn workspace(manifest: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        fs::write(root.join("buildl.toml"), manifest).unwrap();
        (dir, root)
    }

    /// A manifest whose ceiling lets a build take the state directory.
    const CEILING: &str = "[ceiling]\nfs.read = [\".\"]\nfs.write = [\"out\", \".buildl\"]\n";

    /// Runs `check` on cycles that reopen from `wiring`, building the whole workspace for
    /// nobody to see.
    fn with_cycles<T>(
        wiring: &Wiring,
        check: impl FnOnce(&mut Reopening<'_, Silent, NoTerminal>) -> T,
    ) -> T {
        let overrides = BTreeMap::new();
        let (mut reporter, mut console) = (Silent, NoTerminal);
        let mut cycles = Reopening::new(
            wiring,
            &Scope::Workspace,
            &overrides,
            RunOptions::new(OnFailure::Stop, LiveOutput::Off),
            &mut reporter,
            &mut console,
        );
        check(&mut cycles)
    }

    #[test]
    fn a_cycle_on_a_broken_manifest_has_no_layout_and_an_error() {
        let (_dir, root) = workspace("");
        let wiring = Wiring::read(&root).unwrap();
        fs::write(root.join("buildl.toml"), "[nonsense]\nx = 1\n").unwrap();

        let cycle = with_cycles(&wiring, |cycles| cycles.build());

        assert!(cycle.layout.is_none());
        assert!(cycle.watched.built.is_none());
        let error = cycle.watched.ended.unwrap_err();
        assert!(
            matches!(
                &error,
                Error::Adapter {
                    port: PortName::Manifest,
                    ..
                }
            ),
            "{error:?}"
        );
    }

    #[test]
    fn a_cycle_reads_the_manifest_again() {
        let (_dir, root) = workspace("");
        let wiring = Wiring::read(&root).unwrap();

        let first = with_cycles(&wiring, |cycles| cycles.build());
        fs::write(root.join("buildl.toml"), "[workspace]\nout = \"build\"\n").unwrap();
        let second = with_cycles(&wiring, |cycles| cycles.build());

        assert_eq!(first.layout.unwrap().out.as_str(), "out");
        assert_eq!(second.layout.unwrap().out.as_str(), "build");
    }

    #[test]
    fn graph_takes_no_lock() {
        let (_dir, root) = workspace(CEILING);
        fs::write(root.join("build.lua"), "").unwrap();
        let wiring = Wiring::read(&root).unwrap();
        let _held = wiring.lock().unwrap();

        let graph = with_cycles(&wiring, |cycles| cycles.graph());

        assert!(graph.is_ok(), "{graph:?}");
    }

    #[test]
    fn no_rules_before_a_cycle_opens() {
        let (_dir, root) = workspace("");
        let wiring = Wiring::read(&root).unwrap();

        let rules = with_cycles(&wiring, |cycles| cycles.rules().is_some());

        assert!(!rules);
    }

    /// Whether the rules `cycles` hands back ignore `notes/a.md`, or `None` while it hands none.
    fn ignores_notes(cycles: &Reopening<'_, Silent, NoTerminal>) -> Option<bool> {
        cycles.rules().map(|rules| {
            rules
                .ignored("notes/a.md", EntryKind::Unknown)
                .unwrap()
                .is_some()
        })
    }

    /// Asks the rules of `wiring` about `notes/a.md` while no `.buildlignore` exists, so they
    /// hold "no rules" from then on; rules handed back from `wiring` itself, not from the
    /// workspace a cycle reopens, would then miss a file written afterwards.
    fn ask_before_the_file_exists(wiring: &Wiring) {
        let asked = wiring.ignore().ignored("notes/a.md", EntryKind::Unknown);
        assert!(asked.unwrap().is_none());
    }

    #[test]
    fn a_cycle_hands_back_the_rules_it_read() {
        let (_dir, root) = workspace("");
        let wiring = Wiring::read(&root).unwrap();
        ask_before_the_file_exists(&wiring);
        fs::write(root.join(".buildlignore"), "notes/\n").unwrap();

        let ignored = with_cycles(&wiring, |cycles| {
            let _ = cycles.build();
            ignores_notes(cycles)
        });

        assert_eq!(ignored, Some(true));
    }

    #[test]
    fn a_graph_check_hands_back_the_rules_it_read() {
        let (_dir, root) = workspace(CEILING);
        fs::write(root.join("build.lua"), "").unwrap();
        let wiring = Wiring::read(&root).unwrap();
        ask_before_the_file_exists(&wiring);
        fs::write(root.join(".buildlignore"), "notes/\n").unwrap();

        let ignored = with_cycles(&wiring, |cycles| {
            cycles.graph().unwrap();
            ignores_notes(cycles)
        });

        assert_eq!(ignored, Some(true));
    }

    #[test]
    fn a_cycle_that_could_not_open_keeps_the_last_rules() {
        let (_dir, root) = workspace("");
        let wiring = Wiring::read(&root).unwrap();
        fs::write(root.join(".buildlignore"), "notes/\n").unwrap();

        let ignored = with_cycles(&wiring, |cycles| {
            let _ = cycles.build();
            fs::write(root.join("buildl.toml"), "[nonsense]\nx = 1\n").unwrap();
            let failed = cycles.build();
            assert!(failed.layout.is_none());
            ignores_notes(cycles)
        });

        assert_eq!(ignored, Some(true));
    }
}
