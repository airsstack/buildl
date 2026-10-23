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

use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::mem;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use airsl::Script;
use buildl_core::{
    BuildFile, DeclarationSource, Diagnostic, Directory, Discovered, Error, Evaluated,
    EvaluationFailure, PluginName, Result,
};

use crate::classify::classify;
use crate::engine;
use crate::limits::DeclarationLimits;
use crate::module::BuildlModule;
use crate::results::Results;
use crate::sources::Sources;
use crate::staging::Staging;

/// Evaluates build files on the airsl embedded runtime, one fresh engine per file.
///
/// A build file runs on airsl's minimal Lua surface with no grants, under `limits`. It sees
/// the `buildl` module table as both `buildl` and `airsstack.buildl`, and the curated modules
/// `json`, `path`, `regex`, `hash` and `glob` under `airsstack`.
///
/// `buildl.use(name)` loads the plugin the source was built with a directory for under that name,
/// and refuses any other name. A plugin runs under its own environment, narrowed from the build
/// file's, and every declaration it makes carries the plugin as its origin.
///
/// `buildl.discover` declares a discovery target. `buildl.discovered(label)` returns, as a Lua
/// string, the bytes the map handed to [`evaluate`](DeclarationSource::evaluate) holds for that
/// target, or `nil`; a relative label resolves against the build file's directory.
///
/// `buildl.sources` never lists a file under a directory the source was built to skip. A source
/// built with the workspace's output and state directories therefore keeps a build file at the
/// workspace root from seeing what the build wrote.
///
/// # Examples
///
/// ```
/// use std::collections::BTreeMap;
/// use std::fs;
/// use std::path::PathBuf;
///
/// use buildl_core::{BuildFile, DeclarationSource, Directory, Discovered, Evaluated, Provenance};
/// use buildl_lua::{DeclarationLimits, LuaSource};
///
/// let workspace = tempfile::tempdir()?;
/// let root = fs::canonicalize(workspace.path())?;
/// fs::write(root.join("build.lua"), "buildl.alias('default', 'app')")?;
///
/// let skipped = [Directory::parse("out")?, Directory::parse(".buildl")?];
/// let source = LuaSource::new(&root, DeclarationLimits::default(), skipped, BTreeMap::new());
/// let file = BuildFile::new(Provenance::new(PathBuf::from("build.lua"), Directory::root()));
/// assert!(matches!(
///     source.evaluate(&file, &Discovered::empty())?,
///     Evaluated::Staged(staged) if staged.declarations.len() == 1
/// ));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LuaSource {
    root: PathBuf,
    limits: DeclarationLimits,
    skipped: Vec<Directory>,
    plugins: BTreeMap<PluginName, PathBuf>,
}

impl LuaSource {
    /// A source for the workspace at `root`, evaluating under `limits`, whose `buildl.sources`
    /// stays out of `skipped` and whose `buildl.use` loads the plugins in `plugins`.
    ///
    /// `root` must be the workspace root's canonical, absolute path. `buildl.sources` walks
    /// beneath it and refuses any directory that resolves outside it.
    ///
    /// `skipped` names workspace-relative directories. The walk neither lists, enters nor counts
    /// anything inside one. Each is matched by its whole path from the workspace root, so `out`
    /// skips `<root>/out` and not `<root>/lib/out`. A build file whose own directory is a skipped
    /// directory, or lies inside one, gets an empty list from `buildl.sources`. An empty `skipped`
    /// skips nothing: the walk then lists the workspace's own output and state directories too.
    ///
    /// `plugins` maps each plugin name a build file may use to the directory holding its
    /// `extension.toml`. The caller has already checked the directories, so this source reads
    /// them as given. A plugin's `require` stays inside its own directory, however that directory
    /// is reached.
    #[must_use]
    pub fn new(
        root: impl Into<PathBuf>,
        limits: DeclarationLimits,
        skipped: impl IntoIterator<Item = Directory>,
        plugins: BTreeMap<PluginName, PathBuf>,
    ) -> Self {
        Self {
            root: root.into(),
            limits,
            skipped: skipped.into_iter().collect(),
            plugins,
        }
    }
}

impl DeclarationSource for LuaSource {
    /// Evaluates `file`; `buildl.discovered` reads `discovered`.
    ///
    /// A file that does not exist is [`Evaluated::Absent`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::Evaluation`] when the file cannot be read, is not UTF-8, does not parse,
    /// raises, is refused an operation, reaches a ceiling, calls a `buildl` primitive with
    /// arguments it does not accept, or uses a plugin that is unknown, cyclic, malformed or
    /// raises. A refusal from a primitive is reported even when the build
    /// file caught it with `pcall` and carried on.
    fn evaluate(&self, file: &BuildFile, discovered: &Discovered) -> Result<Evaluated> {
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
            Sources::new(
                &self.root,
                file.directory(),
                self.limits.walk_entries(),
                &self.skipped,
            ),
            Results::new(discovered, file.directory()),
            &self.root,
            self.plugins.clone(),
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

    use std::collections::BTreeMap;
    use std::fs;
    use std::path::{Path, PathBuf};

    use buildl_core::{
        BuildFile, DeclarationSource, Directory, Discovered, Error, Evaluated, EvaluationFailure,
        Provenance, StagedItem,
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
        match LuaSource::new(root, DeclarationLimits::default(), [], BTreeMap::new())
            .evaluate(&root_file(), &Discovered::empty())
        {
            Err(Error::Evaluation { failure, .. }) => failure,
            other => panic!("expected an evaluation failure, got {other:?}"),
        }
    }

    #[test]
    fn a_missing_file_is_absent() {
        let dir = tempfile::tempdir().unwrap();
        let source = LuaSource::new(
            dir.path(),
            DeclarationLimits::default(),
            [],
            BTreeMap::new(),
        );
        assert_eq!(
            source.evaluate(&root_file(), &Discovered::empty()).unwrap(),
            Evaluated::Absent
        );
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
        let error = LuaSource::new(&root, DeclarationLimits::default(), [], BTreeMap::new())
            .evaluate(&root_file(), &Discovered::empty())
            .unwrap_err();
        assert!(
            error.to_string().starts_with("build.lua: syntax error: "),
            "{error}"
        );
    }

    #[test]
    fn a_source_keeps_the_directories_it_was_built_with() {
        let skipped = [
            Directory::parse("out").unwrap(),
            Directory::parse(".buildl").unwrap(),
        ];
        let source = LuaSource::new(
            "/ws",
            DeclarationLimits::default(),
            skipped.clone(),
            BTreeMap::new(),
        );
        assert_eq!(source.skipped, skipped);
        assert_eq!(
            LuaSource::new("/ws", DeclarationLimits::default(), [], BTreeMap::new()).skipped,
            []
        );
    }

    #[test]
    fn a_skipped_directory_is_kept_out_of_the_sources_a_file_lists() {
        let (_dir, root) = workspace(
            b"buildl.target('app', { run = { 'cc' }, inputs = buildl.sources('**/*.c') })",
        );
        fs::create_dir_all(root.join("out")).unwrap();
        fs::write(root.join("out/o.c"), "").unwrap();
        fs::write(root.join("a.c"), "").unwrap();
        let Evaluated::Staged(staged) = LuaSource::new(
            &root,
            DeclarationLimits::default(),
            [Directory::parse("out").unwrap()],
            BTreeMap::new(),
        )
        .evaluate(&root_file(), &Discovered::empty())
        .unwrap() else {
            panic!("the build file exists");
        };
        let StagedItem::Target(target) = &staged.declarations[0].item else {
            panic!("the build file declares a target");
        };
        assert_eq!(target.inputs.len(), 1);
        assert_eq!(target.inputs[0].as_written(), "a.c");
    }

    #[test]
    fn a_staged_file_is_returned_with_its_declarations() {
        let (_dir, root) = workspace(b"buildl.subdir('lib')\nbuildl.alias('default', 'app')");
        let Evaluated::Staged(staged) =
            LuaSource::new(&root, DeclarationLimits::default(), [], BTreeMap::new())
                .evaluate(&root_file(), &Discovered::empty())
                .unwrap()
        else {
            panic!("the build file exists");
        };
        assert_eq!(staged.declarations.len(), 1);
        assert_eq!(staged.subdirs.len(), 1);
    }
}
