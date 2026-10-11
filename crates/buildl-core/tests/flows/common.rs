//! In-memory implementations of the ports, and builders for the staged values they return.
//!
//! Everything here uses only `buildl_core`'s public API, exactly as an adapter crate would.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};

use buildl_core::{
    ActionCache, BuildFile, CacheRow, Ceiling, DeclarationOrder, DeclarationSource, Diagnostic,
    Digest, Digester, Directory, EnvName, EnvValue, Error, Evaluated, EvaluationFailure, FileStat,
    Freshness, Host, Label, Manifest, NetworkAccess, Platform, PortName, PortSet, Ports, Program,
    Result, SourcePath, StagedAlias, StagedDeclaration, StagedFile, StagedItem, StagedRule,
    StagedSetting, StagedSubdir, StagedTarget, StatCache, StatEntry, TargetRole, ToolResolver,
    Workspace, Written,
};

/// What the fake holds for one directory.
#[derive(Debug, Clone)]
pub enum FakeFile {
    /// A build file that evaluates to these staged values.
    Staged(StagedFile),
    /// A build file whose evaluation fails this way.
    Fails(EvaluationFailure),
}

/// A build-file source holding fixed files per directory, recording every evaluation.
///
/// Built with [`FakeSource::new`] it holds one set of files. Built with
/// [`FakeSource::with_second_run`] it holds two, and switches from the first to the second when
/// the root directory is evaluated a second time, so every directory evaluated after that point
/// reads from the second set. A directory it holds no file for evaluates to
/// [`Evaluated::Absent`].
#[derive(Debug)]
pub struct FakeSource {
    first: BTreeMap<Directory, FakeFile>,
    second: Option<BTreeMap<Directory, FakeFile>>,
    root_evaluations: Cell<u32>,
    evaluated: RefCell<Vec<Directory>>,
}

impl FakeSource {
    /// A source returning the same files on every evaluation.
    #[must_use]
    pub const fn new(files: BTreeMap<Directory, FakeFile>) -> Self {
        Self {
            first: files,
            second: None,
            root_evaluations: Cell::new(0),
            evaluated: RefCell::new(Vec::new()),
        }
    }

    /// A source returning `first` until the root is evaluated a second time, and `second` from
    /// then on — a workspace that declares differently on its second load.
    #[must_use]
    pub const fn with_second_run(
        first: BTreeMap<Directory, FakeFile>,
        second: BTreeMap<Directory, FakeFile>,
    ) -> Self {
        Self {
            first,
            second: Some(second),
            root_evaluations: Cell::new(0),
            evaluated: RefCell::new(Vec::new()),
        }
    }

    /// Every directory evaluated so far, in evaluation order.
    #[must_use]
    pub fn evaluated(&self) -> Vec<Directory> {
        self.evaluated.borrow().clone()
    }
}

impl DeclarationSource for FakeSource {
    fn evaluate(&self, file: &BuildFile) -> Result<Evaluated> {
        let directory = file.directory();
        if directory.is_root() {
            self.root_evaluations.set(self.root_evaluations.get() + 1);
        }
        self.evaluated.borrow_mut().push(directory.clone());
        let files = match &self.second {
            Some(second) if self.root_evaluations.get() > 1 => second,
            _ => &self.first,
        };
        match files.get(directory) {
            None => Ok(Evaluated::Absent),
            Some(FakeFile::Staged(staged)) => Ok(Evaluated::Staged(staged.clone())),
            Some(FakeFile::Fails(failure)) => Err(Error::Evaluation {
                provenance: file.provenance().clone(),
                failure: failure.clone(),
                diagnostic: Diagnostic::new("fake evaluation failure"),
            }),
        }
    }
}

/// A manifest holding one workspace.
#[derive(Debug, Clone)]
pub struct FakeManifest {
    /// What the manifest says.
    pub workspace: Workspace,
}

impl Manifest for FakeManifest {
    fn workspace(&self) -> Result<Workspace> {
        Ok(self.workspace.clone())
    }
}

/// A machine with a fixed platform and a fixed environment.
#[derive(Debug, Clone)]
pub struct FakeHost {
    /// The platform it reports.
    pub platform: Platform,
    /// The variables that are set.
    pub variables: BTreeMap<EnvName, EnvValue>,
    /// The variables whose value it fails to report.
    pub unreportable: BTreeSet<EnvName>,
}

impl Host for FakeHost {
    fn platform(&self) -> Platform {
        self.platform.clone()
    }

    fn env(&self, name: &EnvName) -> Result<Option<EnvValue>> {
        if self.unreportable.contains(name) {
            return Err(Error::Adapter {
                port: PortName::Host,
                diagnostic: Diagnostic::new(format!("{name} does not hold valid text")),
            });
        }
        Ok(self.variables.get(name).cloned())
    }
}

/// One file a [`FakeDigester`] holds.
#[derive(Debug, Clone)]
pub struct FakeContent {
    /// The file's bytes.
    pub bytes: Vec<u8>,
    /// The metadata reported for it.
    pub stat: FileStat,
}

/// Workspace files held in memory, counting every call made to it.
#[derive(Debug, Default)]
pub struct FakeDigester {
    /// The files that exist.
    pub files: BTreeMap<SourcePath, FakeContent>,
    stat_calls: Cell<u32>,
    digest_calls: Cell<u32>,
    digested: RefCell<Vec<SourcePath>>,
}

impl FakeDigester {
    /// How many times metadata was asked for.
    #[must_use]
    pub const fn stat_calls(&self) -> u32 {
        self.stat_calls.get()
    }

    /// How many times digests were asked for.
    #[must_use]
    pub const fn digest_calls(&self) -> u32 {
        self.digest_calls.get()
    }

    /// Every path a digest was asked for, in the order asked.
    #[must_use]
    pub fn digested(&self) -> Vec<SourcePath> {
        self.digested.borrow().clone()
    }
}

impl Digester for FakeDigester {
    fn stat(&self, path: &SourcePath) -> Result<Option<FileStat>> {
        self.stat_calls.set(self.stat_calls.get() + 1);
        Ok(self.files.get(path).map(|file| file.stat))
    }

    fn digest(&self, paths: &[SourcePath]) -> Result<Vec<Digest>> {
        self.digest_calls.set(self.digest_calls.get() + 1);
        self.digested.borrow_mut().extend_from_slice(paths);
        Ok(paths
            .iter()
            .map(|path| {
                self.files
                    .get(path)
                    .map_or_else(|| Digest::of(b""), |file| Digest::of(&file.bytes))
            })
            .collect())
    }
}

/// A stat cache held in memory, counting how often it was committed.
#[derive(Debug, Default)]
pub struct FakeStatCache {
    /// What is recorded.
    pub entries: BTreeMap<SourcePath, StatEntry>,
    commits: u32,
}

impl FakeStatCache {
    /// How many times the entries were committed.
    #[must_use]
    pub const fn commits(&self) -> u32 {
        self.commits
    }
}

impl StatCache for FakeStatCache {
    fn entry(&self, path: &SourcePath) -> Option<StatEntry> {
        self.entries.get(path).copied()
    }

    fn record(&mut self, path: SourcePath, entry: StatEntry) {
        self.entries.insert(path, entry);
    }

    fn commit(&mut self) -> Result<()> {
        self.commits += 1;
        Ok(())
    }
}

/// Programs with fixed fingerprints, recording every lookup.
#[derive(Debug, Default)]
pub struct FakeTools {
    /// The programs that can be found.
    pub fingerprints: BTreeMap<Program, Digest>,
    lookups: RefCell<Vec<Program>>,
}

impl FakeTools {
    /// Every program looked up, in the order asked.
    #[must_use]
    pub fn lookups(&self) -> Vec<Program> {
        self.lookups.borrow().clone()
    }
}

impl ToolResolver for FakeTools {
    fn fingerprint(&self, program: &Program) -> Result<Option<Digest>> {
        self.lookups.borrow_mut().push(program.clone());
        Ok(self.fingerprints.get(program).copied())
    }
}

/// The rows of a previous build, held in memory.
#[derive(Debug, Default)]
pub struct FakeCache {
    /// The rows.
    pub rows: BTreeMap<Label, CacheRow>,
}

impl ActionCache for FakeCache {
    fn row(&self, label: &Label) -> Option<&CacheRow> {
        self.rows.get(label)
    }
}

/// The bundle choosing the fakes of this file.
#[derive(Debug)]
pub struct FakePorts;

impl Ports for FakePorts {
    type Source = FakeSource;
    type Manifest = FakeManifest;
    type Host = FakeHost;
    type Digester = FakeDigester;
    type StatCache = FakeStatCache;
    type Tools = FakeTools;
    type Cache = FakeCache;
}

/// A workspace that writes to `out` and `.buildl`, under a ceiling allowing `cc` to run, any
/// workspace file to be read, and those two directories to be written.
#[must_use]
pub fn open_workspace() -> Workspace {
    Workspace {
        out: dir("out"),
        state: dir(".buildl"),
        ceiling: Ceiling {
            run: BTreeSet::from([program("cc")]),
            env: BTreeSet::new(),
            read: BTreeSet::from([Directory::root()]),
            write: BTreeSet::from([dir("out"), dir(".buildl")]),
        },
    }
}

/// Every port around `source`: an [`open_workspace`], a `linux/x86_64` machine with no variable
/// set, no workspace file, nothing recorded, no program and no row.
#[must_use]
pub fn ports(source: FakeSource) -> PortSet<FakePorts> {
    PortSet {
        source,
        manifest: FakeManifest {
            workspace: open_workspace(),
        },
        host: FakeHost {
            platform: Platform::parse("linux/x86_64")
                .unwrap_or_else(|error| unreachable!("fixture platform: {error}")),
            variables: BTreeMap::new(),
            unreportable: BTreeSet::new(),
        },
        digester: FakeDigester::default(),
        stat_cache: FakeStatCache::default(),
        tools: FakeTools::default(),
        cache: FakeCache::default(),
    }
}

/// A program from known-valid text.
///
/// # Panics
///
/// Panics when `raw` is not a valid program name; a test fixture is wrong then.
#[must_use]
pub fn program(raw: &str) -> Program {
    Program::parse(raw).unwrap_or_else(|error| unreachable!("fixture program: {error}"))
}

/// A directory from known-valid text.
///
/// # Panics
///
/// Panics when `raw` is not a valid directory; a test fixture is wrong then.
#[must_use]
pub fn dir(raw: &str) -> Directory {
    Directory::parse(raw).unwrap_or_else(|error| unreachable!("fixture directory: {error}"))
}

/// A build file making `items` as its declaration calls, then `subdirs` as its `subdir` calls.
///
/// Call order counts declarations first, then subdirs.
#[must_use]
pub fn file(items: Vec<StagedItem>, subdirs: &[&str]) -> FakeFile {
    let declarations: Vec<StagedDeclaration> = items
        .into_iter()
        .zip(0_u32..)
        .map(|(item, index)| StagedDeclaration {
            order: DeclarationOrder::new(index),
            item,
        })
        .collect();
    let first_subdir = u32::try_from(declarations.len()).unwrap_or(u32::MAX);
    let subdirs = subdirs
        .iter()
        .zip(first_subdir..)
        .map(|(path, index)| StagedSubdir {
            path: Written::new(*path),
            order: DeclarationOrder::new(index),
        })
        .collect();
    FakeFile::Staged(StagedFile {
        declarations,
        subdirs,
    })
}

/// A `b.target(name, { run = { "cc" }, deps = deps })` call.
#[must_use]
pub fn target(name: &str, deps: &[&str]) -> StagedItem {
    target_with(name, |target| {
        target.deps = deps.iter().map(|dep| Written::new(*dep)).collect();
    })
}

/// A `b.target(name, { run = { "cc" } })` call, after `change` is applied to it.
#[must_use]
pub fn target_with(name: &str, change: impl FnOnce(&mut StagedTarget)) -> StagedItem {
    let mut target = StagedTarget {
        name: Written::new(name),
        role: TargetRole::Build,
        rule: None,
        run: Some(vec![Written::new("cc")]),
        inputs: Vec::new(),
        deps: Vec::new(),
        outputs: None,
        env: Vec::new(),
        network: NetworkAccess::Sealed,
        freshness: Freshness::Cached,
    };
    change(&mut target);
    StagedItem::Target(target)
}

/// A `b.rule(name, { run = run, desc = description })` call.
#[must_use]
pub fn rule(name: &str, run: &[&str], description: Option<&str>) -> StagedItem {
    StagedItem::Rule(StagedRule {
        name: Written::new(name),
        run: run.iter().map(|argument| Written::new(*argument)).collect(),
        description: description.map(Written::new),
    })
}

/// A `b.alias(name, target)` call.
#[must_use]
pub fn alias(name: &str, target: &str) -> StagedItem {
    StagedItem::Alias(StagedAlias {
        name: Written::new(name),
        target: Written::new(target),
    })
}

/// A `b.option(name, { default = "" })` call.
#[must_use]
pub fn setting(name: &str) -> StagedItem {
    StagedItem::Setting(StagedSetting {
        name: Written::new(name),
        default: Written::new(""),
    })
}

/// A workspace from `(directory, file)` pairs.
#[must_use]
pub fn workspace(files: Vec<(&str, FakeFile)>) -> BTreeMap<Directory, FakeFile> {
    files
        .into_iter()
        .map(|(directory, file)| (dir(directory), file))
        .collect()
}
