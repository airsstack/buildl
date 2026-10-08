//! Workspaces to evaluate: checked-in fixtures, or a temporary one holding a single build file.

use std::fs;
use std::path::{Path, PathBuf};

use buildl_core::{
    BuildFile, DeclarationSource, Diagnostic, Directory, Error, Evaluated, EvaluationFailure,
    Provenance, Result,
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
    LuaSource::new(root, DeclarationLimits::default()).evaluate(&root_file())
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
