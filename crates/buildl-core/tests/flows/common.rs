//! In-memory implementations of the ports, and builders for the staged values they return.
//!
//! Everything here uses only `buildl_core`'s public API, exactly as an adapter crate would.

use std::cell::RefCell;
use std::collections::BTreeMap;

use buildl_core::{
    BuildFile, DeclarationOrder, DeclarationSource, Diagnostic, Directory, Error, Evaluated,
    EvaluationFailure, Freshness, NetworkAccess, Result, StagedDeclaration, StagedFile, StagedItem,
    StagedSetting, StagedSubdir, StagedTarget, TargetRole, Written,
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
/// A directory it holds no file for evaluates to [`Evaluated::Absent`].
#[derive(Debug)]
pub struct FakeSource {
    files: BTreeMap<Directory, FakeFile>,
    evaluated: RefCell<Vec<Directory>>,
}

impl FakeSource {
    /// A source returning the same files on every evaluation.
    #[must_use]
    pub const fn new(files: BTreeMap<Directory, FakeFile>) -> Self {
        Self {
            files,
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
        self.evaluated.borrow_mut().push(directory.clone());
        match self.files.get(directory) {
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
    StagedItem::Target(StagedTarget {
        name: Written::new(name),
        role: TargetRole::Build,
        rule: None,
        run: Some(vec![Written::new("cc")]),
        inputs: Vec::new(),
        deps: deps.iter().map(|dep| Written::new(*dep)).collect(),
        outputs: None,
        env: Vec::new(),
        network: NetworkAccess::Sealed,
        freshness: Freshness::Cached,
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
