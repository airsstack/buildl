//! One build file's evaluation, as a value.
//!
//! Its own file because the walk over many build files and the work done on one are separate
//! concerns: this is the second, and it is the same whether files are visited one at a time or
//! several at once.
//!
//! Responsibilities: [`FileResult`] and [`evaluate_file`].
//!
//! Non-responsibilities: choosing which files to evaluate or in what order, which `loading`
//! does; and evaluating the file itself, which the
//! [`DeclarationSource`] port does.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::Error;
use crate::ports::DeclarationSource;
use crate::types::{Declaration, Directory, Discovered, EntryName, Evaluated};

use super::traversal::{build_file, convert, resolve_subdirs};

/// What evaluating one build file came to.
#[derive(Debug)]
pub(crate) enum FileResult {
    /// The stop flag was already set, so the source was not called.
    Stopped,
    /// The directory holds no build file.
    Absent,
    /// The source failed, a `subdir` request left the workspace, or a declaration failed its
    /// grammar: the first of these in the order the file is processed.
    Failed(Error),
    /// The file was evaluated and every declaration in it is valid.
    Loaded {
        /// The directories it requests, sorted and without repeats.
        subdirs: Vec<Directory>,
        /// Its declarations, in call order.
        declarations: Vec<Declaration>,
    },
}

/// Evaluates the build file of `directory` and validates what it staged.
///
/// Within the file the steps run in one fixed order, so a file that fails in more than one way
/// always fails the same way: evaluate, then resolve the `subdir` requests, then convert each
/// declaration in call order. `discovered` is handed to `source` as it is. Returns
/// [`FileResult::Stopped`] without calling `source` when `stop` is already set.
pub(crate) fn evaluate_file<S: DeclarationSource>(
    source: &S,
    entry: &EntryName,
    directory: &Directory,
    discovered: &Discovered,
    stop: &AtomicBool,
) -> FileResult {
    if stop.load(Ordering::Acquire) {
        return FileResult::Stopped;
    }
    let file = build_file(directory, entry);
    let staged = match source.evaluate(&file, discovered) {
        Ok(Evaluated::Staged(staged)) => staged,
        Ok(Evaluated::Absent) => return FileResult::Absent,
        Err(error) => return FileResult::Failed(error),
    };
    let provenance = file.provenance();
    let subdirs = match resolve_subdirs(&staged.subdirs, provenance) {
        Ok(subdirs) => subdirs,
        Err(error) => return FileResult::Failed(error),
    };
    let mut declarations = Vec::with_capacity(staged.declarations.len());
    for declaration in staged.declarations {
        match convert(declaration, provenance) {
            Ok(declaration) => declarations.push(declaration),
            Err(error) => return FileResult::Failed(error),
        }
    }
    FileResult::Loaded {
        subdirs,
        declarations,
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeMap;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use super::{FileResult, evaluate_file};
    use crate::error::{DeclarationField, Error, EvaluationFailure, Result};
    use crate::ports::DeclarationSource;
    use crate::types::{
        BuildFile, DeclarationOrder, Diagnostic, Directory, Discovered, EntryName, Evaluated,
        Freshness, Label, NetworkAccess, Origin, OutputSource, StagedDeclaration, StagedFile,
        StagedItem, StagedSubdir, StagedTarget, TargetRole, Written,
    };

    /// A source that answers every call with one canned reply and counts the calls.
    struct Counting {
        calls: AtomicUsize,
        reply: Mutex<Option<Result<Evaluated>>>,
    }

    impl Counting {
        fn replying(reply: Result<Evaluated>) -> Self {
            Self {
                calls: AtomicUsize::new(0),
                reply: Mutex::new(Some(reply)),
            }
        }

        fn staged(file: StagedFile) -> Self {
            Self::replying(Ok(Evaluated::Staged(file)))
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    impl DeclarationSource for Counting {
        fn evaluate(&self, _file: &BuildFile, _discovered: &Discovered) -> Result<Evaluated> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.reply.lock().unwrap().take().unwrap()
        }
    }

    fn entry() -> EntryName {
        EntryName::parse("build.lua").unwrap()
    }

    fn run(source: &Counting, directory: &str) -> FileResult {
        evaluate_file(
            source,
            &entry(),
            &Directory::parse(directory).unwrap(),
            &Discovered::empty(),
            &AtomicBool::new(false),
        )
    }

    fn subdir(path: &str, order: u32) -> StagedSubdir {
        StagedSubdir {
            path: Written::new(path),
            order: DeclarationOrder::new(order),
        }
    }

    fn target(name: &str, order: u32) -> StagedDeclaration {
        StagedDeclaration {
            order: DeclarationOrder::new(order),
            origin: Origin::BuildFile,
            item: StagedItem::Target(StagedTarget {
                name: Written::new(name),
                role: TargetRole::Build,
                rule: None,
                run: Some(vec![Written::new("cc")]),
                inputs: Vec::new(),
                deps: Vec::new(),
                outputs: None,
                env: Vec::new(),
                network: NetworkAccess::Sealed,
                output_source: OutputSource::Command,
                freshness: Freshness::Cached,
            }),
        }
    }

    fn failed(result: FileResult) -> Error {
        match result {
            FileResult::Failed(error) => error,
            other => unreachable!("expected Failed, got {other:?}"),
        }
    }

    fn invalid_field(error: &Error) -> DeclarationField {
        match error {
            Error::InvalidDeclaration { field, .. } => *field,
            other => unreachable!("expected InvalidDeclaration, got {other:?}"),
        }
    }

    #[test]
    fn a_set_stop_flag_evaluates_nothing() {
        let source = Counting::staged(StagedFile {
            declarations: Vec::new(),
            subdirs: Vec::new(),
        });
        let stop = AtomicBool::new(true);
        let result = evaluate_file(
            &source,
            &entry(),
            &Directory::root(),
            &Discovered::empty(),
            &stop,
        );
        assert!(matches!(result, FileResult::Stopped), "{result:?}");
        assert_eq!(source.calls(), 0);
    }

    #[test]
    fn a_missing_file_is_absent() {
        let source = Counting::replying(Ok(Evaluated::Absent));
        let result = run(&source, "lib");
        assert!(matches!(result, FileResult::Absent), "{result:?}");
        assert_eq!(source.calls(), 1);
    }

    #[test]
    fn an_evaluation_error_is_failed() {
        let provenance = super::build_file(&Directory::root(), &entry())
            .provenance()
            .clone();
        let source = Counting::replying(Err(Error::Evaluation {
            provenance,
            failure: EvaluationFailure::Syntax,
            diagnostic: Diagnostic::new("unexpected symbol near '}'"),
        }));
        let error = failed(run(&source, ""));
        assert_eq!(
            error.to_string(),
            "build.lua: syntax error: unexpected symbol near '}'"
        );
    }

    #[test]
    fn a_bad_subdir_is_failed() {
        let source = Counting::staged(StagedFile {
            declarations: Vec::new(),
            subdirs: vec![subdir("../out", 3)],
        });
        let error = failed(run(&source, "lib"));
        assert_eq!(invalid_field(&error), DeclarationField::Subdir);
    }

    #[test]
    fn a_bad_declaration_is_failed() {
        let source = Counting::staged(StagedFile {
            declarations: vec![target("//other:x", 0)],
            subdirs: Vec::new(),
        });
        let error = failed(run(&source, ""));
        assert_eq!(invalid_field(&error), DeclarationField::Name);
    }

    #[test]
    fn a_file_with_both_a_bad_subdir_and_a_bad_declaration_fails_on_the_subdir() {
        let source = Counting::staged(StagedFile {
            declarations: vec![target("//other:x", 0)],
            subdirs: vec![subdir("../out", 1)],
        });
        let error = failed(run(&source, ""));
        assert_eq!(invalid_field(&error), DeclarationField::Subdir);
    }

    #[test]
    fn a_good_file_is_loaded_with_sorted_subdirs_and_its_declarations() {
        let source = Counting::staged(StagedFile {
            declarations: vec![target("app", 0), target("lib", 1)],
            subdirs: vec![subdir("zeta", 2), subdir("alpha", 3), subdir("zeta", 4)],
        });
        let FileResult::Loaded {
            subdirs,
            declarations,
        } = run(&source, "src")
        else {
            unreachable!("expected Loaded");
        };
        let subdirs: Vec<&str> = subdirs.iter().map(Directory::as_str).collect();
        assert_eq!(subdirs, ["src/alpha", "src/zeta"]);
        let names: Vec<&str> = declarations.iter().map(|d| d.item().name()).collect();
        assert_eq!(names, ["app", "lib"]);
        assert_eq!(source.calls(), 1);
    }

    /// A source that records the bytes the map holds for `//svc:pkgs` each time it is called.
    struct Reading {
        seen: Mutex<Vec<Option<Vec<u8>>>>,
    }

    impl DeclarationSource for Reading {
        fn evaluate(&self, _file: &BuildFile, discovered: &Discovered) -> Result<Evaluated> {
            let pkgs = Label::parse("//svc:pkgs").unwrap();
            self.seen
                .lock()
                .unwrap()
                .push(discovered.get(&pkgs).map(<[u8]>::to_vec));
            Ok(Evaluated::Absent)
        }
    }

    #[test]
    fn the_discovered_map_reaches_the_source() {
        let source = Reading {
            seen: Mutex::new(Vec::new()),
        };
        let held = Discovered::new(BTreeMap::from([(
            Label::parse("//svc:pkgs").unwrap(),
            b"[]".to_vec(),
        )]));
        let directory = Directory::parse("svc").unwrap();

        evaluate_file(
            &source,
            &entry(),
            &directory,
            &held,
            &AtomicBool::new(false),
        );
        evaluate_file(
            &source,
            &entry(),
            &directory,
            &Discovered::empty(),
            &AtomicBool::new(false),
        );

        assert_eq!(*source.seen.lock().unwrap(), [Some(b"[]".to_vec()), None]);
    }
}
