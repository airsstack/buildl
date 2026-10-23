//! The bundle of real adapters, and the wiring that makes one instance of each.
//!
//! Its own module because the adapters have to meet somewhere: every other module of this crate
//! serves one port and names no other, so which adapter serves which port, what each is made
//! from and in which order is decided here, in one place.
//!
//! Responsibilities: [`LocalPorts`], the choice of an adapter for every port;
//! [`LocalPipeline`], the pipeline over that choice; and [`Wiring`], a workspace whose manifest
//! has been read, which makes the adapters from it.
//!
//! Non-responsibilities: running a command, which is the caller's; and anything an adapter does
//! once it is made.

use core::marker::PhantomData;
use std::path::{Path, PathBuf};

use buildl_core::{
    ApproverMode, Diagnostic, Directory, EntryName, Error, Isolation, Manifest, Pipeline, PortName,
    PortSet, Ports, Reporter, Result, Want,
};
use buildl_lua::{DeclarationLimits, LuaSource};

use crate::approver::{
    Approvals, Console, DataDir, DenyApprover, InteractiveApprover, LocalApprover, ManifestApprover,
};
use crate::cache::LocalCache;
use crate::cas::LocalCas;
use crate::clock::SystemClock;
use crate::digest::FileDigester;
use crate::dispatch::ThreadPool;
use crate::exec::{Bare, InputSandbox, Local, OutPaths};
use crate::graph::GraphFile;
use crate::host::LocalHost;
use crate::log::LocalLog;
use crate::manifest::{TomlManifest, WorkspaceRoot};
use crate::plugins::{Lock, check};
use crate::search_path::SearchPath;
use crate::statcache::LocalStatCache;
use crate::state::{LockError, Scratch, StateLayout, StateLock};
use crate::tools::PathTools;

/// The bundle of real adapters. `R` is the reporter a build is lent, and `C` the console the
/// approver may ask on.
///
/// A bundle is a choice of types and is never made as a value: it is named as the parameter of
/// a [`PortSet`] or of a [`Pipeline`].
#[derive(Debug)]
pub struct LocalPorts<R, C>(PhantomData<fn() -> (R, C)>);

impl<R: Reporter, C: Console> Ports for LocalPorts<R, C> {
    type Source = LuaSource;
    type Manifest = TomlManifest;
    type Host = LocalHost;
    type Digester = FileDigester;
    type StatCache = LocalStatCache;
    type Tools = PathTools;
    type Cache = LocalCache;
    type Cas = LocalCas;
    type Log = LocalLog;
    type Strategy = Local;
    type Dispatcher = ThreadPool;
    type Clock = SystemClock;
    type Graph = GraphFile;
    type Reporter = R;
    type Approver = LocalApprover<C>;
}

/// The pipeline over the real adapters.
pub type LocalPipeline<R, C> = Pipeline<LocalPorts<R, C>>;

/// A workspace whose manifest has been read: everything a port is made from. It holds no lock
/// and has written nothing.
#[derive(Debug)]
pub struct Wiring {
    root: WorkspaceRoot,
    manifest: TomlManifest,
    workspace: buildl_core::Workspace,
    search: SearchPath,
    scratch: Scratch,
    data: DataDir,
}

impl Wiring {
    /// Finds the workspace at or above `start` and reads its manifest.
    ///
    /// The manifest is the first thing read and the only one: no build file is opened, and no
    /// directory is made.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Adapter`](buildl_core::Error::Adapter), naming
    /// [`PortName::Manifest`](buildl_core::PortName::Manifest), when no directory at or above
    /// `start` holds a manifest, and when the manifest cannot be read or is refused.
    pub fn read(start: &Path) -> Result<Self> {
        let root = WorkspaceRoot::find(start)?;
        let manifest = TomlManifest::read(&root)?;
        let workspace = manifest.workspace()?;
        let scratch = Scratch::new(&std::env::temp_dir(), root.as_path());
        Ok(Self {
            root,
            manifest,
            workspace,
            search: SearchPath::from_process(),
            scratch,
            data: DataDir::User,
        })
    }

    /// Keeps approvals in `dir` instead of the user's data directory.
    #[must_use]
    pub fn with_data_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.data = DataDir::At(dir.into());
        self
    }

    /// The scratch directory of this workspace, outside it. Naming it makes nothing.
    #[must_use]
    pub const fn scratch(&self) -> &Scratch {
        &self.scratch
    }

    /// The workspace root, and where the search for it began.
    #[must_use]
    pub const fn root(&self) -> &WorkspaceRoot {
        &self.root
    }

    /// The file name a build file has in every directory.
    #[must_use]
    pub const fn entry(&self) -> &EntryName {
        self.manifest.entry()
    }

    /// The manifest, as it was read.
    #[must_use]
    pub const fn manifest(&self) -> &TomlManifest {
        &self.manifest
    }

    /// The approvals this workspace remembers, where the `interactive` approver keeps them.
    /// Touches nothing on disk.
    ///
    /// # Errors
    ///
    /// Returns an approver error when they are kept in the user's data directory and the
    /// environment names no absolute one.
    pub fn approvals(&self) -> Result<Approvals> {
        let data = self.data.resolve(|name| std::env::var_os(name))?;
        Ok(Approvals::new(&data, self.root.as_path()))
    }

    /// The source of build files: all that checking them and resolving their graph need.
    ///
    /// Build files are evaluated under the ceilings the manifest sets, each one it leaves out
    /// taking its default, and their source listings stay out of the output and the state
    /// directory.
    ///
    /// Every plugin `buildl.toml` names is checked first, once, against `buildl.lock` and the
    /// plugin store: a command that makes a source refuses before any build file is opened. The
    /// directories of the plugins that pass are what a build file's `buildl.use` loads from.
    ///
    /// # Errors
    ///
    /// Returns [`Error::CorruptState`] when `buildl.lock` is damaged, an adapter error when it
    /// cannot be read, and what [`check`] returns for a plugin that is not there to use.
    pub fn source(&self) -> Result<LuaSource> {
        let lock = Lock::read(self.root.as_path())?;
        let checked = check(
            self.root.as_path(),
            &self.layout(),
            self.manifest.plugins(),
            &lock,
        )?;
        let plugins = checked
            .iter()
            .map(|(name, directory)| (name.clone(), directory.to_path_buf()))
            .collect();
        let defaults = DeclarationLimits::default();
        let settings = self.manifest.declaration();
        let limits = DeclarationLimits::new(
            settings
                .memory_bytes
                .unwrap_or_else(|| defaults.memory_bytes()),
            settings
                .instructions
                .unwrap_or_else(|| defaults.instructions()),
        )
        .with_walk_entries(
            settings
                .walk_entries
                .unwrap_or_else(|| defaults.walk_entries()),
        );
        Ok(LuaSource::new(
            self.root.as_path(),
            limits,
            [self.workspace.out.clone(), self.workspace.state.clone()],
            plugins,
        ))
    }

    /// Whether this version of buildl can run actions under the workspace's strategy.
    ///
    /// It runs `bare` and `sandbox`, the default. A workspace that asks for more isolation is
    /// refused instead of being built under less, so a build never drops a guarantee its
    /// manifest asked for.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Adapter`], naming [`PortName::Manifest`], for `os-sandbox` and
    /// `container`.
    pub fn runs(&self) -> Result<()> {
        match self.workspace.execution.strategy {
            Isolation::Bare | Isolation::Sandbox => Ok(()),
            strategy @ (Isolation::OsSandbox | Isolation::Container) => {
                Err(Local::refusal(strategy))
            }
        }
    }

    /// Whether a verify can re-run actions under the workspace's strategy.
    ///
    /// A verify re-runs in the input sandbox and nowhere else: it is the one strategy that keeps
    /// a re-run from touching the workspace, so a workspace on `bare` is refused, and not
    /// re-run under a sandbox it did not ask for.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Adapter`], naming [`PortName::Manifest`], for `bare`, `os-sandbox` and
    /// `container`.
    pub fn verifies(&self) -> Result<()> {
        match self.workspace.execution.strategy {
            Isolation::Sandbox => Ok(()),
            Isolation::Bare => Err(Local::verify_refusal()),
            strategy @ (Isolation::OsSandbox | Isolation::Container) => {
                Err(Local::refusal(strategy))
            }
        }
    }

    /// Where the state directory's files are, read without taking the directory: for a command
    /// that only reads it.
    #[must_use]
    pub fn layout(&self) -> StateLayout {
        StateLayout::new(self.root.as_path(), &self.workspace.state)
    }

    /// The directory every output is placed under, as the manifest names it.
    #[must_use]
    pub const fn out(&self) -> &Directory {
        &self.workspace.out
    }

    /// The output directory, after holding it to the ceiling.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Refused`] when the output directory lies in no directory the workspace
    /// allows a build to write into.
    pub fn writable_out(&self) -> Result<&Directory> {
        let out = &self.workspace.out;
        let ceiling = &self.workspace.ceiling;
        if !ceiling.write.iter().any(|allowed| allowed.contains(out)) {
            return Err(Error::Refused {
                want: Want::Write(out.clone()),
                site: None,
                ceiling: Box::new(ceiling.clone()),
            });
        }
        Ok(out)
    }

    /// Takes the state directory for this process, after holding it to the ceiling.
    ///
    /// The directory is made when it is missing, but only once the workspace is seen to allow
    /// writing there: a state directory the ceiling leaves out is refused before anything is
    /// made.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Refused`] when the state directory lies in no directory the workspace
    /// allows a build to write into. Returns [`Error::Adapter`], naming
    /// [`PortName::ActionCache`], when another process holds the directory, and when the
    /// directory or its lock file cannot be made, and naming [`PortName::ExecStrategy`] when
    /// the scratch directory cannot be removed.
    pub fn lock(&self) -> Result<StateLock> {
        let state = &self.workspace.state;
        let ceiling = &self.workspace.ceiling;
        if !ceiling.write.iter().any(|allowed| allowed.contains(state)) {
            return Err(Error::Refused {
                want: Want::Write(state.clone()),
                site: None,
                ceiling: Box::new(ceiling.clone()),
            });
        }
        StateLock::acquire(self.layout(), self.scratch.clone()).map_err(|error| {
            let port = match error {
                LockError::Scratch { .. } => PortName::ExecStrategy,
                _ => PortName::ActionCache,
            };
            Error::Adapter {
                port,
                diagnostic: Diagnostic::new(error.to_string()),
            }
        })
    }

    /// The input sandbox, made from the root, the timeout and the search path, over the output
    /// paths as `out_paths` says.
    fn sandbox(&self, lock: &StateLock, out_paths: OutPaths) -> InputSandbox {
        InputSandbox::new(
            self.root.as_path().to_path_buf(),
            self.scratch.clone(),
            lock.owner(),
            self.workspace.execution.timeout,
            self.search.clone(),
            out_paths,
        )
    }

    /// The strategy the manifest names, made from the root, the timeout and the search path.
    fn strategy(&self, lock: &StateLock) -> Local {
        let root = self.root.as_path().to_path_buf();
        let timeout = self.workspace.execution.timeout;
        match self.workspace.execution.strategy {
            Isolation::Bare => Local::Bare(Bare::new(root, timeout, self.search.clone())),
            Isolation::Sandbox => Local::Sandbox(self.sandbox(lock, OutPaths::Cleared)),
            strategy @ (Isolation::OsSandbox | Isolation::Container) => Local::Refused(strategy),
        }
    }

    /// The dispatcher that runs several tasks at once: as many workers as the manifest says.
    ///
    /// The one place the manifest's worker count becomes a dispatcher: `ports` takes its
    /// dispatcher from here, and a command that loads build files without taking the lock runs
    /// them through one of these.
    #[must_use]
    pub fn dispatcher(&self) -> ThreadPool {
        ThreadPool::new(self.workspace.execution.workers)
    }

    /// Every port, for one plan or one build. `lock` proves the state directory is this
    /// process's own.
    ///
    /// The dispatcher comes from [`Wiring::dispatcher`], so the worker count of the manifest
    /// reaches it from that one place; the timeout reaches the strategy through here. The strategy
    /// is the one the manifest names, and a strategy this version cannot run is made too, as one
    /// that refuses to run anything, so that `plan` works under every strategy. The program
    /// lookup and the strategy are handed one search path, so the file that is run is the file that
    /// was fingerprinted. The approver is the one the manifest's `[approver]` table names; `console`
    /// is the one it asks on, and the other two never use it.
    ///
    /// # Errors
    ///
    /// Returns whatever an adapter returns when it is made: the host's, the stat cache's and the
    /// action cache's own errors, unchanged. Returns what [`Wiring::source`] returns first, when
    /// a plugin the manifest names is not there to use.
    pub fn ports<R: Reporter, C: Console>(
        &self,
        lock: &StateLock,
        reporter: R,
        console: C,
    ) -> Result<PortSet<LocalPorts<R, C>>> {
        self.assemble(lock, reporter, console, self.strategy(lock))
    }

    /// Every port, for one verify: the ports of [`Wiring::ports`], with the input sandbox as
    /// the strategy and the output paths left as the last build placed them.
    ///
    /// A verify re-runs actions to compare them with the cache, so what it re-runs must not
    /// clear or make the real output paths. These ports always use the input sandbox, whatever
    /// strategy the manifest names; [`Wiring::verifies`] is what refuses the others, so call it
    /// first.
    ///
    /// # Errors
    ///
    /// Returns whatever an adapter returns when it is made, as [`Wiring::ports`] does.
    pub fn verify_ports<R: Reporter, C: Console>(
        &self,
        lock: &StateLock,
        reporter: R,
        console: C,
    ) -> Result<PortSet<LocalPorts<R, C>>> {
        self.assemble(
            lock,
            reporter,
            console,
            Local::Sandbox(self.sandbox(lock, OutPaths::Untouched)),
        )
    }

    /// The approver the manifest's `[approver]` table names. Only the interactive one is given
    /// the console, the root and where approvals are kept.
    fn approver<C: Console>(&self, console: C) -> LocalApprover<C> {
        match self.workspace.approver {
            ApproverMode::Manifest => LocalApprover::Manifest(ManifestApprover),
            ApproverMode::Deny => LocalApprover::Deny(DenyApprover),
            ApproverMode::Interactive => LocalApprover::Interactive(InteractiveApprover::new(
                console,
                self.data.clone(),
                self.root.as_path().to_path_buf(),
            )),
        }
    }

    /// The adapters of [`Wiring::ports`] and [`Wiring::verify_ports`], which differ only in the
    /// strategy they are given.
    fn assemble<R: Reporter, C: Console>(
        &self,
        lock: &StateLock,
        reporter: R,
        console: C,
        strategy: Local,
    ) -> Result<PortSet<LocalPorts<R, C>>> {
        let layout = lock.layout();
        let root = self.root.as_path();
        Ok(PortSet {
            source: self.source()?,
            manifest: self.manifest.clone(),
            host: LocalHost::new()?,
            digester: FileDigester::new(root),
            stat_cache: LocalStatCache::open(layout, lock.acquired())?,
            tools: PathTools::new(root, self.search.clone()),
            cache: LocalCache::open(layout)?,
            cas: LocalCas::new(root, layout),
            log: LocalLog::new(layout),
            strategy,
            dispatcher: self.dispatcher(),
            clock: SystemClock,
            graph: GraphFile::new(layout),
            reporter,
            approver: self.approver(console),
        })
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use core::num::NonZeroU64;
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::{Path, PathBuf};

    use buildl_core::{
        Directory, Error, Isolation, Pipeline, PortName, Scope, Timeout, Verdict, Want, Workers,
    };
    use buildl_lua::{DeclarationLimits, LuaSource};

    use crate::approver::{DataDir, LocalApprover, NoTerminal};
    use crate::cas::LocalCas;
    use crate::digest::FileDigester;
    use crate::dispatch::ThreadPool;
    use crate::exec::{Bare, InputSandbox, Local, OutPaths};
    use crate::graph::GraphFile;
    use crate::host::LocalHost;
    use crate::local_ports::{LocalPipeline, Wiring};
    use crate::log::LocalLog;
    use crate::manifest::TomlManifest;
    use crate::report::Silent;
    use crate::tools::PathTools;

    /// A temporary workspace whose `buildl.toml` holds `manifest`, and its canonical root.
    fn workspace(manifest: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        fs::write(root.join("buildl.toml"), manifest).unwrap();
        (dir, root)
    }

    /// The names of everything in `dir`, sorted.
    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    /// The ceiling of a manifest that allows a build to write its outputs and its state, and
    /// to run `cp`.
    const CEILING: &str =
        "[ceiling]\nproc.run = [\"cp\"]\nfs.read = [\".\"]\nfs.write = [\"out\", \".buildl\"]\n\n";

    /// That manifest, under the first tier.
    fn bare() -> String {
        format!("{CEILING}[execution]\nstrategy = \"bare\"\n")
    }

    #[test]
    fn reading_finds_the_root_above_the_start_and_makes_nothing() {
        let (_dir, root) = workspace("[workspace]\nentry = \"BUILD.lua\"\n");
        fs::create_dir_all(root.join("lib/text")).unwrap();

        let wiring = Wiring::read(&root.join("lib/text")).unwrap();

        assert_eq!(wiring.root().as_path(), root);
        assert_eq!(wiring.root().within(), Path::new("lib/text"));
        assert_eq!(wiring.entry().as_str(), "BUILD.lua");
        assert_eq!(names(&root), ["buildl.toml", "lib"]);
    }

    #[test]
    fn a_manifest_that_is_refused_fails_the_reading_whatever_else_the_workspace_holds() {
        let (_dir, root) = workspace("[ceiling]\nfs.wirte = [\"out\"]\n");
        // A build file that could not be evaluated, and a state directory that could not be
        // used: neither is looked at.
        fs::write(root.join("build.lua"), "this is not Lua").unwrap();
        fs::write(root.join(".buildl"), "not a directory").unwrap();

        let error = Wiring::read(&root).unwrap_err();

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
        assert_eq!(
            error.to_string(),
            "manifest: buildl.toml:2:4: unknown field `wirte`, expected `read` or `write`"
        );
    }

    #[test]
    fn the_source_takes_each_ceiling_the_manifest_sets_and_the_default_for_the_rest() {
        let skipped = [
            Directory::parse("dist").unwrap(),
            Directory::parse(".buildl").unwrap(),
        ];
        let defaults = DeclarationLimits::default();

        let (_dir, root) = workspace(
            "[workspace]\nout = \"dist\"\n\n[declaration]\nmemory = \"2MB\"\ninstructions = 5000\nwalk_entries = 70\n",
        );
        let every = Wiring::read(&root).unwrap();
        assert_eq!(
            every.source().unwrap(),
            LuaSource::new(
                &root,
                DeclarationLimits::new(
                    NonZeroU64::new(2 * 1024 * 1024).unwrap(),
                    NonZeroU64::new(5000).unwrap()
                )
                .with_walk_entries(NonZeroU64::new(70).unwrap()),
                skipped.clone(),
                BTreeMap::new(),
            )
        );

        let (_dir, root) =
            workspace("[workspace]\nout = \"dist\"\n\n[declaration]\ninstructions = 5000\n");
        let one = Wiring::read(&root).unwrap();
        assert_eq!(
            one.source().unwrap(),
            LuaSource::new(
                &root,
                DeclarationLimits::new(defaults.memory_bytes(), NonZeroU64::new(5000).unwrap())
                    .with_walk_entries(defaults.walk_entries()),
                skipped.clone(),
                BTreeMap::new(),
            )
        );

        let (_dir, root) = workspace("[workspace]\nout = \"dist\"\n");
        let none = Wiring::read(&root).unwrap();
        assert_eq!(
            none.source().unwrap(),
            LuaSource::new(&root, defaults, skipped, BTreeMap::new())
        );
    }

    #[test]
    fn bare_and_sandbox_can_be_run() {
        for manifest in [
            "[execution]\nstrategy = \"bare\"\n",
            "[execution]\nstrategy = \"sandbox\"\n",
            "",
        ] {
            let (_dir, root) = workspace(manifest);
            Wiring::read(&root).unwrap().runs().unwrap();
        }

        for strategy in ["os-sandbox", "container"] {
            let (_dir, root) = workspace(&format!("[execution]\nstrategy = \"{strategy}\"\n"));
            let error = Wiring::read(&root).unwrap().runs().unwrap_err();
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
            assert_eq!(
                error.to_string(),
                format!(
                    "manifest: buildl.toml: [execution] strategy is \"{strategy}\", and this \
                     version of buildl runs \"bare\" and \"sandbox\" only: write strategy = \
                     \"sandbox\" under [execution]"
                )
            );
        }
    }

    #[test]
    fn verify_runs_only_under_sandbox() {
        for manifest in ["[execution]\nstrategy = \"sandbox\"\n", ""] {
            let (_dir, root) = workspace(manifest);
            Wiring::read(&root).unwrap().verifies().unwrap();
        }

        let (_dir, root) = workspace("[execution]\nstrategy = \"bare\"\n");
        let error = Wiring::read(&root).unwrap().verifies().unwrap_err();
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
        assert_eq!(
            error.to_string(),
            "manifest: buildl.toml: [execution] strategy is \"bare\", and verify re-runs \
             actions in the input sandbox only: write strategy = \"sandbox\" under [execution]"
        );

        for strategy in ["os-sandbox", "container"] {
            let (_dir, root) = workspace(&format!("[execution]\nstrategy = \"{strategy}\"\n"));
            let error = Wiring::read(&root).unwrap().verifies().unwrap_err();
            assert_eq!(
                error.to_string(),
                format!(
                    "manifest: buildl.toml: [execution] strategy is \"{strategy}\", and this \
                     version of buildl runs \"bare\" and \"sandbox\" only: write strategy = \
                     \"sandbox\" under [execution]"
                )
            );
        }
    }

    #[test]
    fn a_state_directory_outside_the_ceiling_is_refused() {
        let (_dir, root) = workspace("[ceiling]\nfs.write = [\"out\"]\n");
        let wiring = Wiring::read(&root).unwrap();

        let error = wiring.lock().unwrap_err();

        assert!(
            matches!(
                &error,
                Error::Refused { want: Want::Write(directory), site: None, ceiling }
                    if directory.as_str() == ".buildl" && **ceiling == wiring.workspace.ceiling
            ),
            "{error:?}"
        );
        assert_eq!(
            error.to_string(),
            "directory .buildl is outside the ceiling (fs.write allows: out)"
        );
        // Refused before anything was made.
        assert_eq!(names(&root), ["buildl.toml"]);
    }

    #[test]
    fn writable_out_is_refused_when_the_ceiling_does_not_allow_writing_it() {
        let (_dir, root) = workspace("[ceiling]\nfs.write = [\".buildl\"]\n");
        let wiring = Wiring::read(&root).unwrap();

        let error = wiring.writable_out().unwrap_err();

        assert!(
            matches!(
                &error,
                Error::Refused { want: Want::Write(directory), site: None, ceiling }
                    if directory.as_str() == "out" && **ceiling == wiring.workspace.ceiling
            ),
            "{error:?}"
        );
        assert_eq!(
            error.to_string(),
            "directory out is outside the ceiling (fs.write allows: .buildl)"
        );
        assert_eq!(names(&root), ["buildl.toml"]);
    }

    #[test]
    fn writable_out_is_the_output_directory_when_the_ceiling_allows_it() {
        for manifest in [
            "[ceiling]\nfs.write = [\"out\"]\n",
            "[ceiling]\nfs.write = [\".\"]\n",
        ] {
            let (_dir, root) = workspace(manifest);
            let wiring = Wiring::read(&root).unwrap();

            let out = wiring.writable_out().unwrap();

            assert_eq!(out.as_str(), "out", "{manifest:?}");
            assert_eq!(wiring.out(), out, "{manifest:?}");
        }
    }

    #[test]
    fn a_state_directory_inside_an_allowed_directory_is_locked_and_made() {
        for manifest in [
            "[ceiling]\nfs.write = [\".buildl\"]\n",
            "[ceiling]\nfs.write = [\".\"]\n",
        ] {
            let (_dir, root) = workspace(manifest);
            let wiring = Wiring::read(&root).unwrap();

            let lock = wiring.lock().unwrap();

            assert_eq!(lock.layout().dir(), root.join(".buildl"), "{manifest:?}");
            assert!(root.join(".buildl/lock").is_file(), "{manifest:?}");
        }
    }

    #[test]
    fn the_layout_is_made_without_the_lock() {
        let (_dir, root) = workspace(&bare());
        let wiring = Wiring::read(&root).unwrap();

        let layout = wiring.layout();

        assert_eq!(layout.dir(), root.join(".buildl"));
        assert!(!root.join(".buildl").exists());
    }

    #[test]
    fn a_busy_lock_is_the_action_caches_adapter_error() {
        let (_dir, root) = workspace(&bare());
        let wiring = Wiring::read(&root).unwrap();
        let held = wiring.lock().unwrap();

        let error = wiring.lock().unwrap_err();

        assert!(
            matches!(
                &error,
                Error::Adapter {
                    port: PortName::ActionCache,
                    ..
                }
            ),
            "{error:?}"
        );
        assert_eq!(
            error.to_string(),
            format!(
                "action cache: {} is in use by another buildl",
                root.join(".buildl/lock").display()
            )
        );

        // Free again as soon as the holder lets go.
        drop(held);
        wiring.lock().unwrap();
    }

    #[test]
    fn a_scratch_that_cannot_be_removed_is_the_exec_strategys_adapter_error() {
        use std::os::unix::fs::PermissionsExt;

        let (_dir, root) = workspace(&bare());
        let wiring = Wiring::read(&root).unwrap();
        let locked = wiring.scratch().dir().join("locked");
        fs::create_dir_all(&locked).unwrap();
        fs::write(locked.join("file"), b"bytes").unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o500)).unwrap();

        let error = wiring.lock();

        // Restore the mode first, so the directory can be removed whatever happens.
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o700)).unwrap();
        fs::remove_dir_all(wiring.scratch().dir()).unwrap();
        let error = error.unwrap_err();
        assert!(
            matches!(
                &error,
                Error::Adapter {
                    port: PortName::ExecStrategy,
                    ..
                }
            ),
            "{error:?}"
        );
    }

    #[test]
    fn a_state_directory_that_cannot_be_made_is_the_action_caches_adapter_error() {
        let (_dir, root) = workspace(&bare());
        fs::write(root.join(".buildl"), "not a directory").unwrap();

        let error = Wiring::read(&root).unwrap().lock().unwrap_err();

        assert!(
            matches!(
                &error,
                Error::Adapter {
                    port: PortName::ActionCache,
                    ..
                }
            ),
            "{error:?}"
        );
        assert!(
            error.to_string().starts_with(&format!(
                "action cache: {}: ",
                root.join(".buildl").display()
            )),
            "{error}"
        );
    }

    #[test]
    fn the_approver_is_the_one_the_manifest_names() {
        for (table, expected) in [
            ("", "Manifest"),
            ("[approver]\ndefault = \"manifest\"\n", "Manifest"),
            ("[approver]\ndefault = \"deny\"\n", "Deny"),
            ("[approver]\ndefault = \"interactive\"\n", "Interactive"),
        ] {
            let (_dir, root) = workspace(&format!("{}{table}", bare()));
            let wiring = Wiring::read(&root)
                .unwrap()
                .with_data_dir(root.join("data"));
            let lock = wiring.lock().unwrap();

            let ports = wiring.ports(&lock, Silent, NoTerminal).unwrap();

            let named = match ports.approver {
                LocalApprover::Manifest(_) => "Manifest",
                LocalApprover::Deny(_) => "Deny",
                LocalApprover::Interactive(_) => "Interactive",
            };
            assert_eq!(named, expected, "{table:?}");
            assert_eq!(wiring.data, DataDir::At(root.join("data")));
        }
    }

    #[test]
    fn every_port_is_made_from_the_manifest_the_root_and_the_lock() {
        let (_dir, root) = workspace(&format!("{}workers = 3\ntimeout = \"7s\"\n", bare()));
        let wiring = Wiring::read(&root).unwrap();
        let lock = wiring.lock().unwrap();
        let layout = lock.layout();
        let search = wiring.search.clone();

        let ports = wiring.ports(&lock, Silent, NoTerminal).unwrap();

        assert_eq!(ports.source, wiring.source().unwrap());
        assert_eq!(ports.manifest, TomlManifest::read(wiring.root()).unwrap());
        assert_eq!(ports.host, LocalHost::new().unwrap());
        assert_eq!(ports.digester, FileDigester::new(&root));
        assert_eq!(ports.tools, PathTools::new(&root, search.clone()));
        // The adapters below cannot be compared, so each is held to the rendering of one made
        // here from the values it must have been handed.
        let timeout = Timeout::parse("7s").unwrap();
        assert_eq!(
            format!("{:?}", ports.strategy),
            format!(
                "{:?}",
                Local::Bare(Bare::new(root.clone(), timeout, search))
            )
        );
        assert_eq!(
            format!("{:?}", ports.dispatcher),
            format!("{:?}", ThreadPool::new(Workers::of(3)))
        );
        assert_eq!(
            format!("{:?}", ports.cas),
            format!("{:?}", LocalCas::new(&root, layout))
        );
        assert_eq!(
            format!("{:?}", ports.log),
            format!("{:?}", LocalLog::new(layout))
        );
        assert_eq!(
            format!("{:?}", ports.graph),
            format!("{:?}", GraphFile::new(layout))
        );

        // With no strategy line the workspace runs the input sandbox, which is handed the
        // scratch directory and the owner of the lock.
        let (_dir, root) = workspace(&format!("{CEILING}[execution]\ntimeout = \"7s\"\n"));
        let wiring = Wiring::read(&root).unwrap();
        let lock = wiring.lock().unwrap();
        let ports = wiring.ports(&lock, Silent, NoTerminal).unwrap();
        assert_eq!(
            format!("{:?}", ports.strategy),
            format!(
                "{:?}",
                Local::Sandbox(InputSandbox::new(
                    root,
                    wiring.scratch().clone(),
                    lock.owner(),
                    timeout,
                    wiring.search.clone(),
                    OutPaths::Cleared,
                ))
            )
        );

        // A strategy this version cannot run is still made, so that `plan` has every port.
        let (_dir, root) = workspace(&format!(
            "{CEILING}[execution]\nstrategy = \"os-sandbox\"\n"
        ));
        let wiring = Wiring::read(&root).unwrap();
        let lock = wiring.lock().unwrap();
        let ports = wiring.ports(&lock, Silent, NoTerminal).unwrap();
        assert_eq!(
            format!("{:?}", ports.strategy),
            format!("{:?}", Local::Refused(Isolation::OsSandbox))
        );
    }

    #[test]
    fn a_verifys_ports_use_the_input_sandbox_and_leave_the_output_paths_alone() {
        // The manifest names `bare`; the verify's strategy is the sandbox all the same.
        let (_dir, root) = workspace(&format!("{}timeout = \"7s\"\n", bare()));
        let wiring = Wiring::read(&root).unwrap();
        let lock = wiring.lock().unwrap();

        let ports = wiring.verify_ports(&lock, Silent, NoTerminal).unwrap();

        assert_eq!(
            format!("{:?}", ports.strategy),
            format!(
                "{:?}",
                Local::Sandbox(InputSandbox::new(
                    root,
                    wiring.scratch().clone(),
                    lock.owner(),
                    Timeout::parse("7s").unwrap(),
                    wiring.search.clone(),
                    OutPaths::Untouched,
                ))
            )
        );
    }

    #[test]
    fn a_cache_that_is_not_canonical_stops_the_wiring_with_its_own_error() {
        let (_dir, root) = workspace(&bare());
        let wiring = Wiring::read(&root).unwrap();
        let lock = wiring.lock().unwrap();
        fs::write(lock.layout().cache(), "{} ").unwrap();

        let error = wiring.ports(&lock, Silent, NoTerminal).unwrap_err();

        assert!(
            matches!(&error, Error::CorruptState { file, .. } if *file == root.join(".buildl/cache.json")),
            "{error:?}"
        );
    }

    #[test]
    fn a_pipeline_over_the_ports_plans_a_workspace() {
        let (_dir, root) = workspace(&bare());
        fs::write(root.join("a.txt"), "a\n").unwrap();
        fs::write(
            root.join("build.lua"),
            "buildl.target(\"copy\", { inputs = { \"a.txt\" }, run = { \"cp\", \"$in\", \"$out\" } })\n",
        )
        .unwrap();
        let wiring = Wiring::read(&root).unwrap();
        let lock = wiring.lock().unwrap();

        let mut pipeline: LocalPipeline<Silent, NoTerminal> =
            Pipeline::new(wiring.ports(&lock, Silent, NoTerminal).unwrap());
        let plan = pipeline
            .plan(wiring.entry(), &Scope::Workspace, &BTreeMap::new())
            .unwrap();

        assert_eq!(plan.len(), 1);
        assert!(
            matches!(plan.verdicts(), [Verdict::Dirty { .. }]),
            "{:?}",
            plan.verdicts()
        );
    }
}
