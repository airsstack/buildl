//! [`LuaSource`], the [`DeclarationSource`] that evaluates build files with airsl.
//!
//! Its own file because it is the adapter's entry point: it turns one [`BuildFile`] into either
//! the declarations the file staged or the one failure that stopped it. Every evaluation gets a
//! fresh engine and a fresh staging buffer, so nothing one file does can reach the next.
//!
//! Responsibilities: [`LuaSource`] and its [`DeclarationSource`] implementation.
//!
//! Non-responsibilities: deciding which build files to evaluate, and what their declarations
//! mean. Both are the caller's.

use std::fs;
use std::io::ErrorKind;
use std::mem;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use airsl::Script;
use buildl_core::{
    BuildFile, DeclarationSource, Diagnostic, Error, Evaluated, EvaluationFailure, Result,
};

use crate::classify::classify;
use crate::engine;
use crate::limits::DeclarationLimits;
use crate::module::BuildlModule;
use crate::sources::Sources;
use crate::staging::Staging;

/// Evaluates build files on the airsl embedded runtime, one fresh engine per file.
///
/// A build file runs on airsl's minimal Lua surface with no grants, under `limits`. It sees
/// the `buildl` module table as both `buildl` and `airsstack.buildl`, and the curated modules
/// `json`, `path`, `regex`, `hash` and `glob` under `airsstack`.
///
/// # Examples
///
/// ```
/// use std::fs;
/// use std::path::PathBuf;
///
/// use buildl_core::{BuildFile, DeclarationSource, Directory, Evaluated, Provenance};
/// use buildl_lua::{DeclarationLimits, LuaSource};
///
/// let workspace = tempfile::tempdir()?;
/// let root = fs::canonicalize(workspace.path())?;
/// fs::write(root.join("build.lua"), "buildl.alias('default', 'app')")?;
///
/// let source = LuaSource::new(&root, DeclarationLimits::default());
/// let file = BuildFile::new(Provenance::new(PathBuf::from("build.lua"), Directory::root()));
/// assert!(matches!(
///     source.evaluate(&file)?,
///     Evaluated::Staged(staged) if staged.declarations.len() == 1
/// ));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LuaSource {
    root: PathBuf,
    limits: DeclarationLimits,
}

impl LuaSource {
    /// A source for the workspace at `root`, evaluating under `limits`.
    ///
    /// `root` must be the workspace root's canonical, absolute path. `buildl.sources` walks
    /// beneath it and refuses any directory that resolves outside it.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>, limits: DeclarationLimits) -> Self {
        Self {
            root: root.into(),
            limits,
        }
    }
}

impl DeclarationSource for LuaSource {
    /// Evaluates `file`.
    ///
    /// A file that does not exist is [`Evaluated::Absent`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::Evaluation`] when the file cannot be read, is not UTF-8, does not parse,
    /// raises, is refused an operation, reaches a ceiling, or calls a `buildl` primitive with
    /// arguments it does not accept. A refusal from a primitive is reported even when the build
    /// file caught it with `pcall` and carried on.
    fn evaluate(&self, file: &BuildFile) -> Result<Evaluated> {
        let path = self.root.join(file.file());
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Evaluated::Absent),
            Err(error) => {
                return Err(failed(
                    file,
                    EvaluationFailure::Runtime,
                    format!("{}: {error}", path.display()),
                ));
            }
        };
        let Ok(text) = String::from_utf8(bytes) else {
            return Err(failed(file, EvaluationFailure::Syntax, "not UTF-8 text"));
        };

        let staging = Arc::new(Mutex::new(Staging::new(self.limits.memory_bytes().get())));
        let outcome = BuildlModule::new(
            Arc::clone(&staging),
            Sources::new(&self.root, file.directory()),
        )
        .and_then(|module| engine::build(&self.limits, module))
        .and_then(|engine| {
            let chunk = file.file().to_string_lossy().into_owned();
            engine.eval(&Script::from_source(text, chunk)?)
        });

        let staged = mem::replace(
            &mut *staging.lock().unwrap_or_else(PoisonError::into_inner),
            Staging::new(0),
        );
        let staged = staged
            .finish()
            .map_err(|(failure, diagnostic)| Error::Evaluation {
                provenance: file.provenance().clone(),
                failure,
                diagnostic,
            })?;
        outcome.map_err(|error| failed(file, classify(&error), error.to_string()))?;
        Ok(Evaluated::Staged(staged))
    }
}

fn failed(file: &BuildFile, failure: EvaluationFailure, diagnostic: impl Into<String>) -> Error {
    Error::Evaluation {
        provenance: file.provenance().clone(),
        failure,
        diagnostic: Diagnostic::new(diagnostic),
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        clippy::panic,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::fs;
    use std::path::{Path, PathBuf};

    use buildl_core::{
        BuildFile, DeclarationSource, Directory, Error, Evaluated, EvaluationFailure, Provenance,
    };

    use super::LuaSource;
    use crate::limits::DeclarationLimits;

    fn root_file() -> BuildFile {
        BuildFile::new(Provenance::new(
            PathBuf::from("build.lua"),
            Directory::root(),
        ))
    }

    fn workspace(build_lua: &[u8]) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        fs::write(root.join("build.lua"), build_lua).unwrap();
        (dir, root)
    }

    fn failure(root: &Path) -> EvaluationFailure {
        match LuaSource::new(root, DeclarationLimits::default()).evaluate(&root_file()) {
            Err(Error::Evaluation { failure, .. }) => failure,
            other => panic!("expected an evaluation failure, got {other:?}"),
        }
    }

    #[test]
    fn a_missing_file_is_absent() {
        let dir = tempfile::tempdir().unwrap();
        let source = LuaSource::new(dir.path(), DeclarationLimits::default());
        assert_eq!(source.evaluate(&root_file()).unwrap(), Evaluated::Absent);
    }

    #[test]
    fn a_file_that_is_not_utf8_is_a_syntax_failure() {
        let (_dir, root) = workspace(b"return '\xff'");
        assert_eq!(failure(&root), EvaluationFailure::Syntax);
    }

    #[test]
    fn a_refusal_caught_by_pcall_is_still_reported() {
        let (_dir, root) = workspace(b"pcall(buildl.target, 'app', 1)\nreturn 'ok'");
        assert_eq!(
            failure(&root),
            EvaluationFailure::WrongFieldType {
                field: buildl_core::Written::new("options"),
            }
        );
    }

    #[test]
    fn an_uncaught_refusal_is_reported_as_the_recorded_failure() {
        let (_dir, root) = workspace(b"buildl.target('app', 1)");
        assert_eq!(
            failure(&root),
            EvaluationFailure::WrongFieldType {
                field: buildl_core::Written::new("options"),
            }
        );
    }

    #[test]
    fn a_build_file_that_is_a_directory_is_a_runtime_failure() {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        fs::create_dir(root.join("build.lua")).unwrap();
        assert_eq!(failure(&root), EvaluationFailure::Runtime);
    }

    #[test]
    fn the_source_can_be_shared_across_threads() {
        const fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<LuaSource>();
    }

    #[test]
    fn an_airsl_failure_is_classified() {
        let (_dir, root) = workspace(b"error('boom')");
        assert_eq!(failure(&root), EvaluationFailure::Runtime);
    }

    #[test]
    fn evaluation_failures_carry_the_file_s_provenance() {
        let (_dir, root) = workspace(b"return (");
        let error = LuaSource::new(&root, DeclarationLimits::default())
            .evaluate(&root_file())
            .unwrap_err();
        assert!(
            error.to_string().starts_with("build.lua: syntax error: "),
            "{error}"
        );
    }

    #[test]
    fn a_staged_file_is_returned_with_its_declarations() {
        let (_dir, root) = workspace(b"buildl.subdir('lib')\nbuildl.alias('default', 'app')");
        let Evaluated::Staged(staged) = LuaSource::new(&root, DeclarationLimits::default())
            .evaluate(&root_file())
            .unwrap()
        else {
            panic!("the build file exists");
        };
        assert_eq!(staged.declarations.len(), 1);
        assert_eq!(staged.subdirs.len(), 1);
    }
}
