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
use std::path::Path;

use buildl_core::{
    Diagnostic, Directory, EntryName, Error, Isolation, Manifest, Pipeline, PortName, PortSet,
    Ports, Reporter, Result, Want,
};
use buildl_lua::{DeclarationLimits, LuaSource};

use crate::cache::LocalCache;
use crate::cas::LocalCas;
use crate::clock::SystemClock;
use crate::digest::FileDigester;
use crate::dispatch::ThreadPool;
use crate::exec::Bare;
use crate::graph::GraphFile;
use crate::host::LocalHost;
use crate::log::LocalLog;
use crate::manifest::{TomlManifest, WorkspaceRoot};
use crate::search_path::SearchPath;
use crate::statcache::LocalStatCache;
use crate::state::{StateLayout, StateLock};
use crate::tools::PathTools;

/// The file a workspace's manifest is read from, as a refusal names it.
const MANIFEST_FILE: &str = "buildl.toml";

/// The bundle of real adapters. `R` is the reporter a build is lent.
///
/// A bundle is a choice of types and is never made as a value: it is named as the parameter of
/// a [`PortSet`] or of a [`Pipeline`].
#[derive(Debug)]
pub struct LocalPorts<R>(PhantomData<fn() -> R>);

impl<R: Reporter> Ports for LocalPorts<R> {
    type Source = LuaSource;
    type Manifest = TomlManifest;
    type Host = LocalHost;
    type Digester = FileDigester;
    type StatCache = LocalStatCache;
    type Tools = PathTools;
    type Cache = LocalCache;
    type Cas = LocalCas;
    type Log = LocalLog;
    type Strategy = Bare;
    type Dispatcher = ThreadPool;
    type Clock = SystemClock;
    type Graph = GraphFile;
    type Reporter = R;
}

/// The pipeline over the real adapters.
pub type LocalPipeline<R> = Pipeline<LocalPorts<R>>;

/// A workspace whose manifest has been read: everything a port is made from. It holds no lock
/// and has written nothing.
#[derive(Debug)]
pub struct Wiring {
    root: WorkspaceRoot,
    manifest: TomlManifest,
    workspace: buildl_core::Workspace,
    search: SearchPath,
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
        Ok(Self {
            root,
            manifest,
            workspace,
            search: SearchPath::from_process(),
        })
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

    /// The source of build files: all that checking them and resolving their graph need.
    ///
    /// Build files are evaluated under the ceilings the manifest sets, each one it leaves out
    /// taking its default, and their source listings stay out of the output and the state
    /// directory.
    #[must_use]
    pub fn source(&self) -> LuaSource {
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
        LuaSource::new(
            self.root.as_path(),
            limits,
            [self.workspace.out.clone(), self.workspace.state.clone()],
        )
    }

    /// Whether this version of buildl can run actions under the workspace's strategy.
    ///
    /// It runs `bare` and nothing else. A workspace that asks for more isolation is refused
    /// instead of being built under less, so a build never drops a guarantee its manifest
    /// asked for.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Adapter`], naming [`PortName::Manifest`], for every strategy but
    /// [`Isolation::Bare`].
    pub fn runs(&self) -> Result<()> {
        let strategy = self.workspace.execution.strategy;
        if strategy == Isolation::Bare {
            return Ok(());
        }
        let unwritten = if strategy == Isolation::default() {
            " (the default when it is not written)"
        } else {
            ""
        };
        Err(Error::Adapter {
            port: PortName::Manifest,
            diagnostic: Diagnostic::new(format!(
                "{MANIFEST_FILE}: [execution] strategy is \"{strategy}\"{unwritten}, and this \
                 version of buildl runs \"{bare}\" only: write strategy = \"{bare}\" under \
                 [execution]",
                bare = Isolation::Bare,
            )),
        })
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
    /// directory or its lock file cannot be made.
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
        StateLock::acquire(self.layout()).map_err(|error| Error::Adapter {
            port: PortName::ActionCache,
            diagnostic: Diagnostic::new(error.to_string()),
        })
    }

    /// The dispatcher that runs several tasks at once: as many workers as the manifest says.
    ///
    /// A command that loads build files without taking the lock runs them through one of these.
    #[must_use]
    pub fn dispatcher(&self) -> ThreadPool {
        ThreadPool::new(self.workspace.execution.workers)
    }

    /// Every port, for one plan or one build. `lock` proves the state directory is this
    /// process's own.
    ///
    /// The worker count and the timeout of the manifest reach the dispatcher and the strategy
    /// through here and nowhere else, and the program lookup and the strategy are handed one search
    /// path, so the file that is run is the file that was fingerprinted.
    ///
    /// # Errors
    ///
    /// Returns whatever an adapter returns when it is made: the host's, the stat cache's and the
    /// action cache's own errors, unchanged.
    pub fn ports<R: Reporter>(
        &self,
        lock: &StateLock,
        reporter: R,
    ) -> Result<PortSet<LocalPorts<R>>> {
        let layout = lock.layout();
        let root = self.root.as_path();
        Ok(PortSet {
            source: self.source(),
            manifest: self.manifest.clone(),
            host: LocalHost::new()?,
            digester: FileDigester::new(root),
            stat_cache: LocalStatCache::open(layout, lock.acquired())?,
            tools: PathTools::new(root, self.search.clone()),
            cache: LocalCache::open(layout)?,
            cas: LocalCas::new(root, layout),
            log: LocalLog::new(layout),
            strategy: Bare::new(
                root.to_path_buf(),
                self.workspace.execution.timeout,
                self.search.clone(),
            ),
            dispatcher: self.dispatcher(),
            clock: SystemClock,
            graph: GraphFile::new(layout),
            reporter,
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
        Directory, Error, Pipeline, PortName, Scope, Timeout, Verdict, Want, Workers,
    };
    use buildl_lua::{DeclarationLimits, LuaSource};

    use crate::cas::LocalCas;
    use crate::digest::FileDigester;
    use crate::dispatch::ThreadPool;
    use crate::exec::Bare;
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

    /// A manifest that allows a build to write its outputs and its state, and to run `cp`.
    const BARE: &str = "[ceiling]\nproc.run = [\"cp\"]\nfs.read = [\".\"]\nfs.write = [\"out\", \".buildl\"]\n\n[execution]\nstrategy = \"bare\"\n";

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
            every.source(),
            LuaSource::new(
                &root,
                DeclarationLimits::new(
                    NonZeroU64::new(2 * 1024 * 1024).unwrap(),
                    NonZeroU64::new(5000).unwrap()
                )
                .with_walk_entries(NonZeroU64::new(70).unwrap()),
                skipped.clone(),
            )
        );

        let (_dir, root) =
            workspace("[workspace]\nout = \"dist\"\n\n[declaration]\ninstructions = 5000\n");
        let one = Wiring::read(&root).unwrap();
        assert_eq!(
            one.source(),
            LuaSource::new(
                &root,
                DeclarationLimits::new(defaults.memory_bytes(), NonZeroU64::new(5000).unwrap())
                    .with_walk_entries(defaults.walk_entries()),
                skipped.clone(),
            )
        );

        let (_dir, root) = workspace("[workspace]\nout = \"dist\"\n");
        let none = Wiring::read(&root).unwrap();
        assert_eq!(none.source(), LuaSource::new(&root, defaults, skipped));
    }

    #[test]
    fn only_bare_can_be_run() {
        let (_dir, root) = workspace("[execution]\nstrategy = \"bare\"\n");
        Wiring::read(&root).unwrap().runs().unwrap();

        for manifest in ["", "[execution]\nstrategy = \"sandbox\"\n"] {
            let (_dir, root) = workspace(manifest);
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
                "manifest: buildl.toml: [execution] strategy is \"sandbox\" (the default when it \
                 is not written), and this version of buildl runs \"bare\" only: write strategy \
                 = \"bare\" under [execution]",
                "{manifest:?}"
            );
        }

        for strategy in ["os-sandbox", "container"] {
            let (_dir, root) = workspace(&format!("[execution]\nstrategy = \"{strategy}\"\n"));
            let error = Wiring::read(&root).unwrap().runs().unwrap_err();
            assert_eq!(
                error.to_string(),
                format!(
                    "manifest: buildl.toml: [execution] strategy is \"{strategy}\", and this \
                     version of buildl runs \"bare\" only: write strategy = \"bare\" under \
                     [execution]"
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
        let (_dir, root) = workspace(BARE);
        let wiring = Wiring::read(&root).unwrap();

        let layout = wiring.layout();

        assert_eq!(layout.dir(), root.join(".buildl"));
        assert!(!root.join(".buildl").exists());
    }

    #[test]
    fn a_busy_lock_is_the_action_caches_adapter_error() {
        let (_dir, root) = workspace(BARE);
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
    fn a_state_directory_that_cannot_be_made_is_the_action_caches_adapter_error() {
        let (_dir, root) = workspace(BARE);
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
    fn every_port_is_made_from_the_manifest_the_root_and_the_lock() {
        let (_dir, root) = workspace(&format!("{BARE}workers = 3\ntimeout = \"7s\"\n"));
        let wiring = Wiring::read(&root).unwrap();
        let lock = wiring.lock().unwrap();
        let layout = lock.layout();
        let search = wiring.search.clone();

        let ports = wiring.ports(&lock, Silent).unwrap();

        assert_eq!(ports.source, wiring.source());
        assert_eq!(ports.manifest, TomlManifest::read(wiring.root()).unwrap());
        assert_eq!(ports.host, LocalHost::new().unwrap());
        assert_eq!(ports.digester, FileDigester::new(&root));
        assert_eq!(ports.tools, PathTools::new(&root, search.clone()));
        // The adapters below cannot be compared, so each is held to the rendering of one made
        // here from the values it must have been handed.
        let timeout = Timeout::parse("7s").unwrap();
        assert_eq!(
            format!("{:?}", ports.strategy),
            format!("{:?}", Bare::new(root.clone(), timeout, search))
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
    }

    #[test]
    fn a_cache_that_is_not_canonical_stops_the_wiring_with_its_own_error() {
        let (_dir, root) = workspace(BARE);
        let wiring = Wiring::read(&root).unwrap();
        let lock = wiring.lock().unwrap();
        fs::write(lock.layout().cache(), "{} ").unwrap();

        let error = wiring.ports(&lock, Silent).unwrap_err();

        assert!(
            matches!(&error, Error::CorruptState { file, .. } if *file == root.join(".buildl/cache.json")),
            "{error:?}"
        );
    }

    #[test]
    fn a_pipeline_over_the_ports_plans_a_workspace() {
        let (_dir, root) = workspace(BARE);
        fs::write(root.join("a.txt"), "a\n").unwrap();
        fs::write(
            root.join("build.lua"),
            "buildl.target(\"copy\", { inputs = { \"a.txt\" }, run = { \"cp\", \"$in\", \"$out\" } })\n",
        )
        .unwrap();
        let wiring = Wiring::read(&root).unwrap();
        let lock = wiring.lock().unwrap();

        let mut pipeline: LocalPipeline<Silent> =
            Pipeline::new(wiring.ports(&lock, Silent).unwrap());
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
