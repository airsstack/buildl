//! In-memory implementations of the ports, and builders for the staged values they return.
//!
//! Everything here uses only `buildl_core`'s public API, exactly as an adapter crate would.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

use buildl_core::{
    BuildFile, DeclarationOrder, DeclarationSource, Diagnostic, Directory, Error, Evaluated,
    EvaluationFailure, Freshness, NetworkAccess, Ports, Result, StagedAlias, StagedDeclaration,
    StagedFile, StagedItem, StagedRule, StagedSetting, StagedSubdir, StagedTarget, TargetRole,
    Written,
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

/// The bundle choosing [`FakeSource`].
#[derive(Debug)]
pub struct FakePorts;

impl Ports for FakePorts {
    type Source = FakeSource;
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
