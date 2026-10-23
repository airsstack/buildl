//! Workspaces to evaluate: checked-in fixtures, or a temporary one holding a single build file.

use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

use buildl_core::{
    BuildFile, DeclarationSource, Diagnostic, Directory, Discovered, Dispatcher, Error, Evaluated,
    EvaluationFailure, Provenance, Result, Turn,
};
use buildl_lua::{DeclarationLimits, LuaSource};

/// The canonical path of the checked-in fixture workspace `name`.
pub(crate) fn fixture(name: &str) -> PathBuf {
    fs::canonicalize(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name),
    )
    .unwrap()
}

/// A temporary workspace whose root build file holds `source`.
pub(crate) fn workspace(source: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    fs::write(root.join("build.lua"), source).unwrap();
    (dir, root)
}

/// The workspace root's build file.
pub(crate) fn root_file() -> BuildFile {
    BuildFile::new(Provenance::new(
        PathBuf::from("build.lua"),
        Directory::root(),
    ))
}

/// Evaluates the root build file of the workspace at `root` under the default limits.
pub(crate) fn evaluate(root: &Path) -> Result<Evaluated> {
    LuaSource::new(root, DeclarationLimits::default(), [], BTreeMap::new())
        .evaluate(&root_file(), &Discovered::empty())
}

/// The failure and diagnostic of evaluating a root build file holding `source`.
pub(crate) fn failure_of(source: &str) -> (EvaluationFailure, Diagnostic) {
    let (_dir, root) = workspace(source);
    match evaluate(&root) {
        Err(Error::Evaluation {
            failure,
            diagnostic,
            ..
        }) => (failure, diagnostic),
        other => panic!("expected an evaluation failure, got {other:?}"),
    }
}

/// A test double for the dispatcher: runs each task on the calling thread, oldest first, and
/// drops what a task tells while it works.
#[derive(Debug, Default)]
pub(crate) struct InlineDispatcher;

impl Dispatcher for InlineDispatcher {
    fn dispatch<T, M, R, W, S>(&mut self, work: W, mut step: S) -> Result<()>
    where
        T: Send,
        M: Send,
        R: Send,
        W: Fn(T, &(dyn Fn(M) + Sync)) -> R + Sync,
        S: FnMut(Turn<M, R>) -> Vec<T>,
    {
        let mut waiting: VecDeque<T> = step(Turn::Begin).into();
        while let Some(task) = waiting.pop_front() {
            let result = work(task, &|_message| {});
            waiting.extend(step(Turn::Done(result)));
        }
        Ok(())
    }
}
