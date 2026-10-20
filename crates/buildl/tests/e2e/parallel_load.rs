//! Build files are evaluated at the same time on the real thread pool.
//!
//! The source here makes two sibling build files wait for each other, so a load can only
//! succeed when both are under way together; a pool of one worker is the control that shows the
//! wait can fail.

use std::sync::{Condvar, Mutex, PoisonError};
use std::time::Duration;

use buildl::buildl_core::{
    BuildFile, DeclarationOrder, DeclarationSource, Diagnostic, Directory, EntryName, Error,
    Evaluated, EvaluationFailure, Result, StagedFile, StagedSubdir, Workers, Written, load,
};
use buildl::dispatch::ThreadPool;

/// How long the overlap test lets one evaluation wait for the other before giving up.
const PATIENT: Duration = Duration::from_secs(10);

/// How long the control lets one evaluation wait; nothing arrives in that time.
const IMPATIENT: Duration = Duration::from_millis(200);

/// What the evaluation of `a` and `b` reports when the other one never starts.
const NO_PARTNER: &str = "the sibling build file was never evaluated at the same time";

/// A workspace whose root names the subdirs `a` and `b`, each of which waits until both have
/// been entered.
struct Rendezvous {
    arrived: Mutex<usize>,
    both: Condvar,
    patience: Duration,
}

impl Rendezvous {
    const fn new(patience: Duration) -> Self {
        Self {
            arrived: Mutex::new(0),
            both: Condvar::new(),
            patience,
        }
    }

    /// Counts the caller in, then waits until two evaluations have been counted in.
    fn meet(&self, file: &BuildFile) -> Result<()> {
        *self.arrived.lock().unwrap_or_else(PoisonError::into_inner) += 1;
        self.both.notify_all();
        let (_held, wait) = self
            .both
            .wait_timeout_while(
                self.arrived.lock().unwrap_or_else(PoisonError::into_inner),
                self.patience,
                |count| *count < 2,
            )
            .unwrap_or_else(PoisonError::into_inner);
        if wait.timed_out() {
            return Err(Error::Evaluation {
                provenance: file.provenance().clone(),
                failure: EvaluationFailure::Runtime,
                diagnostic: Diagnostic::new(NO_PARTNER),
            });
        }
        Ok(())
    }
}

impl DeclarationSource for Rendezvous {
    fn evaluate(&self, file: &BuildFile) -> Result<Evaluated> {
        if file.directory().is_root() {
            let subdir = |path: &str, index| StagedSubdir {
                path: Written::new(path),
                order: DeclarationOrder::new(index),
            };
            return Ok(Evaluated::Staged(StagedFile {
                declarations: Vec::new(),
                subdirs: vec![subdir("a", 0), subdir("b", 1)],
            }));
        }
        self.meet(file)?;
        Ok(Evaluated::Staged(StagedFile {
            declarations: Vec::new(),
            subdirs: Vec::new(),
        }))
    }
}

fn entry() -> EntryName {
    EntryName::parse("build.lua").unwrap()
}

#[test]
fn two_sibling_build_files_are_evaluated_at_once() {
    let source = Rendezvous::new(PATIENT);
    let mut pool = ThreadPool::new(Workers::of(2));
    let declarations = load(&source, &mut pool, &entry());
    assert_eq!(declarations.unwrap(), []);
}

#[test]
fn one_worker_never_overlaps() {
    let source = Rendezvous::new(IMPATIENT);
    let mut pool = ThreadPool::new(Workers::of(1));
    let error = load(&source, &mut pool, &entry()).unwrap_err();
    let Error::Evaluation {
        provenance,
        failure,
        diagnostic,
    } = error
    else {
        panic!("expected an evaluation error, got: {error}");
    };
    assert_eq!(provenance.directory(), &Directory::parse("a").unwrap());
    assert_eq!(failure, EvaluationFailure::Runtime);
    assert_eq!(diagnostic.to_string(), NO_PARTNER);
}
