//! The limits of the two gates around remote jobs and local runs, over the real thread pool.
//!
//! `buildl-core` starts no thread, so what a gate does when several workers meet at it is
//! proven here. The ports are the ones `Wiring::ports` makes, moved into a `PortSet` whose
//! remote cache is a fake that holds each `execute` until the test releases it, and whose
//! strategy counts the runs in flight and holds each until the test releases it; the build goes
//! through `Pipeline`. The fixture `gates`
//! declares eight targets with nothing between them.

use core::num::NonZeroUsize;
use std::collections::BTreeMap;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::Duration;

use buildl::approver::NoTerminal;
use buildl::buildl_core::ContentStore;
use buildl::buildl_core::{
    ActionOutcome, Blob, BlobSource, DecodedTree, Diagnostic, Digest, Dispatcher, Error,
    ExecStrategy, Executed, FileMode, Found, InfraKind, LiveOutput, LiveSink, OnFailure,
    OutcomeClass, Pipeline, PortName, PortSet, Ports, RawOutcome, ReadyAction, RemoteAction,
    RemoteCache, RemoteExecution, RemoteFailure, RemoteHit, RemoteJobs, RemoteOutputFile,
    RemotePolicy, RemoteUpload, RemoteUse, Result, Scope, Turn, Uploaded, Workers,
};
use buildl::dispatch::ThreadPool;
use buildl::report::Silent;
use buildl::{LocalPorts, Wiring};

use crate::common::Fixture;

/// The ports `Wiring::ports` makes.
type Real = LocalPorts<Silent, NoTerminal>;

/// How long a test waits for something it expects before it gives up; reached only when a gate
/// admits fewer than it should, or the build hangs.
const PATIENT: Duration = Duration::from_secs(20);

/// How long a test lets the workers settle after the gate is full, so that one a gate wrongly
/// admits has the time to arrive. It decides only how surely a gate that admits too many is
/// caught, never what a correct build ends as.
const SETTLE: Duration = Duration::from_millis(300);

/// A flag that opens once and is waited on by any number of threads.
#[derive(Debug, Default)]
struct Latch {
    open: Mutex<bool>,
    opened: Condvar,
}

impl Latch {
    fn open(&self) {
        *self.open.lock().unwrap_or_else(PoisonError::into_inner) = true;
        self.opened.notify_all();
    }

    /// Waits until the latch is open, or for [`PATIENT`].
    fn wait(&self) {
        drop(
            self.opened
                .wait_timeout_while(
                    self.open.lock().unwrap_or_else(PoisonError::into_inner),
                    PATIENT,
                    |open| !*open,
                )
                .unwrap_or_else(PoisonError::into_inner),
        );
    }
}

/// What the remote cache was asked, behind its lock.
#[derive(Debug, Default)]
struct Counts {
    /// The `execute` calls begun.
    calls: usize,
    /// The `execute` calls waiting to be released.
    in_flight: usize,
    /// The most `execute` calls that were ever waiting at once.
    peak: usize,
    /// The calls that may go on without waiting.
    permits: usize,
    /// The `lookup` calls begun.
    lookups: usize,
    /// The bytes it serves, by digest.
    served: BTreeMap<Digest, Vec<u8>>,
}

/// What a test sees of the remote cache while a build is under way.
#[derive(Debug, Default)]
struct Observed {
    counts: Mutex<Counts>,
    changed: Condvar,
}

impl Observed {
    fn counts(&self) -> MutexGuard<'_, Counts> {
        self.counts.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Waits until `n` calls are held at once, or for [`PATIENT`]; whether they are.
    fn wait_for_in_flight(&self, n: usize) -> bool {
        let (counts, _) = self
            .changed
            .wait_timeout_while(self.counts(), PATIENT, |counts| counts.in_flight < n)
            .unwrap_or_else(PoisonError::into_inner);
        counts.in_flight >= n
    }

    /// Counts an `execute` call in, holds it until it is released or for [`PATIENT`], and counts
    /// it out; the number of the call, from zero.
    fn hold(&self) -> usize {
        let call = {
            let mut counts = self.counts();
            counts.calls += 1;
            counts.in_flight += 1;
            counts.peak = counts.peak.max(counts.in_flight);
            counts.calls - 1
        };
        self.changed.notify_all();
        let (mut counts, _) = self
            .changed
            .wait_timeout_while(self.counts(), PATIENT, |counts| counts.permits == 0)
            .unwrap_or_else(PoisonError::into_inner);
        counts.permits = counts.permits.saturating_sub(1);
        counts.in_flight -= 1;
        call
    }

    /// Lets `n` held calls, and later ones, go on.
    fn release(&self, n: usize) {
        self.counts().permits += n;
        self.changed.notify_all();
    }

    /// The most calls ever held at once.
    fn peak(&self) -> usize {
        self.counts().peak
    }

    /// The calls begun.
    fn calls(&self) -> usize {
        self.counts().calls
    }
}

/// What the remote cache answers to the `n`th `execute` call, counting from zero.
#[derive(Debug, Clone, Copy)]
enum Reply {
    /// It ran the action, and left every output.
    Ran,
    /// It ran the action, and the action exited with status 1.
    Exited,
    /// The call fails.
    Fails,
}

/// A remote cache that misses every lookup and holds every `execute` until released.
#[derive(Debug)]
struct HeldRemote {
    seen: Arc<Observed>,
    reply: fn(usize) -> Reply,
    /// When set, every lookup but the first waits for it to open.
    lookups_wait_for: Option<Arc<Latch>>,
}

fn adapter(what: &str) -> Error {
    Error::Adapter {
        port: PortName::RemoteCache,
        diagnostic: Diagnostic::new(what),
    }
}

impl RemoteCache for HeldRemote {
    fn lookup(&self, _action: &RemoteAction) -> Result<Option<RemoteHit>> {
        let index = {
            let mut counts = self.seen.counts();
            counts.lookups += 1;
            counts.lookups - 1
        };
        if index > 0
            && let Some(latch) = &self.lookups_wait_for
        {
            latch.wait();
        }
        Ok(None)
    }

    fn download(&self, blobs: &[Blob]) -> Result<Vec<Vec<u8>>> {
        let counts = self.seen.counts();
        blobs
            .iter()
            .map(|blob| {
                counts
                    .served
                    .get(&blob.digest)
                    .cloned()
                    .ok_or_else(|| adapter("the blob is not held"))
            })
            .collect()
    }

    fn decode_tree(&self, _bytes: &[u8]) -> Result<DecodedTree> {
        Ok(DecodedTree::Files(Vec::new()))
    }

    fn upload(
        &self,
        _action: &RemoteAction,
        _produced: &RemoteUpload,
        _blobs: &dyn BlobSource,
    ) -> Result<Uploaded> {
        Err(adapter("nothing is uploaded here"))
    }

    fn execute(&self, action: &RemoteAction) -> Result<Executed> {
        let call = self.seen.hold();
        match (self.reply)(call) {
            Reply::Fails => Err(adapter("the server cannot run it")),
            Reply::Exited => Ok(Executed::Ran(RemoteHit {
                exit_code: 1,
                files: Vec::new(),
                directories: Vec::new(),
                symlinks: Vec::new(),
                stdout: None,
                stderr: None,
            })),
            Reply::Ran => {
                let mut counts = self.seen.counts();
                let files = action
                    .outputs
                    .iter()
                    .map(|path| {
                        let bytes = format!("ran remotely: {path}").into_bytes();
                        let blob = Blob::of(&bytes);
                        counts.served.insert(blob.digest, bytes);
                        RemoteOutputFile {
                            path: path.to_string(),
                            blob,
                            mode: FileMode::Plain,
                        }
                    })
                    .collect();
                Ok(Executed::Ran(RemoteHit {
                    exit_code: 0,
                    files,
                    directories: Vec::new(),
                    symlinks: Vec::new(),
                    stdout: None,
                    stderr: None,
                }))
            }
        }
    }
}

/// The local runs in flight, and the most there ever were at once.
#[derive(Debug, Default)]
struct RunCounts {
    active: usize,
    peak: usize,
}

/// What a test sees of the local runs while a build is under way, and the latch that holds each
/// run until the test lets it go.
#[derive(Debug, Default)]
struct LocalRuns {
    counts: Mutex<RunCounts>,
    changed: Condvar,
    release: Latch,
}

impl LocalRuns {
    fn counts(&self) -> MutexGuard<'_, RunCounts> {
        self.counts.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Counts a run in, and holds it until the release latch is open or for [`PATIENT`]. The run
    /// stays counted until [`LocalRuns::leave`].
    fn enter_and_hold(&self) {
        {
            let mut counts = self.counts();
            counts.active += 1;
            counts.peak = counts.peak.max(counts.active);
        }
        self.changed.notify_all();
        self.release.wait();
    }

    fn leave(&self) {
        self.counts().active -= 1;
    }

    /// Waits until `n` runs are held at once, or for [`PATIENT`]; whether they are.
    fn wait_for_active(&self, n: usize) -> bool {
        let (counts, _) = self
            .changed
            .wait_timeout_while(self.counts(), PATIENT, |counts| counts.active < n)
            .unwrap_or_else(PoisonError::into_inner);
        counts.active >= n
    }

    /// The most runs ever in flight at once.
    fn peak(&self) -> usize {
        self.counts().peak
    }
}

/// A strategy that counts the runs under way, and holds each until the test releases the runs.
#[derive(Debug)]
struct Counted<S> {
    inner: S,
    runs: Arc<LocalRuns>,
}

impl<S: ExecStrategy> ExecStrategy for Counted<S> {
    type Dir = S::Dir;

    fn prepare(&self, action: &ReadyAction) -> Result<Self::Dir> {
        self.inner.prepare(action)
    }

    fn run(
        &self,
        dir: &Self::Dir,
        action: &ReadyAction,
        live: Option<LiveSink<'_>>,
    ) -> Result<RawOutcome> {
        self.runs.enter_and_hold();
        let ended = self.inner.run(dir, action, live);
        self.runs.leave();
        ended
    }

    fn collect<C: ContentStore>(
        &self,
        dir: &Self::Dir,
        action: &ReadyAction,
        cas: &C,
    ) -> Result<Vec<Found>> {
        self.inner.collect(dir, action, cas)
    }
}

/// A dispatcher that opens a latch once the build has taken a result after the remote cache was
/// first asked to run something, which is after the build has decided from that result whether
/// to stop. Loading the build files goes through the dispatcher too, and its results do not
/// count.
#[derive(Debug)]
struct Opening<D> {
    inner: D,
    latch: Arc<Latch>,
    seen: Arc<Observed>,
}

impl<D: Dispatcher> Dispatcher for Opening<D> {
    fn dispatch<T, M, R, W, S>(&mut self, work: W, mut step: S) -> Result<()>
    where
        T: Send,
        M: Send,
        R: Send,
        W: Fn(T, &(dyn Fn(M) + Sync)) -> R + Sync,
        S: FnMut(Turn<M, R>) -> Vec<T>,
    {
        let (latch, seen) = (Arc::clone(&self.latch), Arc::clone(&self.seen));
        self.inner.dispatch(work, move |turn| {
            let done = matches!(turn, Turn::Done(_));
            let next = step(turn);
            if done && seen.calls() > 0 {
                latch.open();
            }
            next
        })
    }
}

/// The ports of `Real`, except for the three this file replaces.
#[derive(Debug)]
struct Gated;

impl Ports for Gated {
    type Source = <Real as Ports>::Source;
    type Manifest = <Real as Ports>::Manifest;
    type Host = <Real as Ports>::Host;
    type Digester = <Real as Ports>::Digester;
    type StatCache = <Real as Ports>::StatCache;
    type Tools = <Real as Ports>::Tools;
    type Cache = <Real as Ports>::Cache;
    type Cas = <Real as Ports>::Cas;
    type Log = <Real as Ports>::Log;
    type Strategy = Counted<<Real as Ports>::Strategy>;
    type Dispatcher = Opening<ThreadPool>;
    type Clock = <Real as Ports>::Clock;
    type Graph = <Real as Ports>::Graph;
    type Reporter = <Real as Ports>::Reporter;
    type Approver = <Real as Ports>::Approver;
    type Remote = HeldRemote;
}

/// A workspace to build, and what is watching the build.
struct Rig {
    fixture: Fixture,
    /// How many tasks the pool works on at once.
    width: usize,
    seen: Arc<Observed>,
    runs: Arc<LocalRuns>,
    latch: Arc<Latch>,
    reply: fn(usize) -> Reply,
    holds_lookups: bool,
}

impl Rig {
    /// A rig over a copy of `gates` whose pool is `width` wide and whose remote answers `reply`.
    fn new(width: usize, reply: fn(usize) -> Reply) -> Self {
        Self {
            fixture: Fixture::copy_of("gates", &[]),
            width,
            seen: Arc::default(),
            runs: Arc::default(),
            latch: Arc::default(),
            reply,
            holds_lookups: false,
        }
    }

    /// Makes every lookup but the first wait until the build has taken its first result.
    const fn holding_lookups(mut self) -> Self {
        self.holds_lookups = true;
        self
    }

    /// Builds the whole workspace under `policy`.
    fn build(&self, policy: RemotePolicy, on_failure: OnFailure) -> Result<Vec<ActionOutcome>> {
        let wiring = Wiring::read(self.fixture.root())?;
        let lock = wiring.lock()?;
        let PortSet {
            source,
            manifest,
            host,
            digester,
            stat_cache,
            tools,
            cache,
            cas,
            log,
            strategy,
            dispatcher: _,
            clock,
            graph,
            reporter,
            approver,
            remote: _,
        } = wiring.ports(&lock, Silent, NoTerminal, RemotePolicy::OFF)?;
        let ports = PortSet::<Gated> {
            source,
            manifest,
            host,
            digester,
            stat_cache,
            tools,
            cache,
            cas,
            log,
            strategy: Counted {
                inner: strategy,
                runs: Arc::clone(&self.runs),
            },
            dispatcher: Opening {
                inner: ThreadPool::new(Workers::of(self.width)),
                latch: Arc::clone(&self.latch),
                seen: Arc::clone(&self.seen),
            },
            clock,
            graph,
            reporter,
            approver,
            remote: HeldRemote {
                seen: Arc::clone(&self.seen),
                reply: self.reply,
                lookups_wait_for: self.holds_lookups.then(|| Arc::clone(&self.latch)),
            },
        };
        let mut pipeline = Pipeline::new(ports);
        pipeline.set_remote(policy);
        pipeline.build(
            wiring.entry(),
            &Scope::Workspace,
            &BTreeMap::new(),
            on_failure,
            LiveOutput::Off,
        )
    }

    /// Builds under `policy` while `watch` runs on another thread, and returns the outcomes.
    fn build_watched(
        &self,
        policy: RemotePolicy,
        on_failure: OnFailure,
        watch: impl FnOnce(&Self) + Send,
    ) -> Vec<ActionOutcome> {
        thread::scope(|scope| {
            scope.spawn(|| watch(self));
            self.build(policy, on_failure).unwrap()
        })
    }
}

/// A policy that reads from the remote, sends what misses to be run there, at most `jobs` at a
/// time, and runs at most `local` actions here at a time.
fn executing(jobs: i64, local: usize) -> RemotePolicy {
    RemotePolicy {
        use_: RemoteUse::Read,
        failure: RemoteFailure::Lenient,
        execution: RemoteExecution::On {
            jobs: RemoteJobs::from_integer(jobs).unwrap(),
            local: NonZeroUsize::new(local).unwrap(),
        },
    }
}

/// How many outcomes ended as `class`.
fn ended_as(outcomes: &[ActionOutcome], class: &OutcomeClass) -> usize {
    outcomes
        .iter()
        .filter(|outcome| &outcome.class == class)
        .count()
}

#[test]
fn remote_jobs_never_exceed_jobs_below_workers() {
    let rig = Rig::new(8, |_| Reply::Ran);

    let outcomes = rig.build_watched(executing(2, 4), OnFailure::Stop, |rig| {
        assert!(
            rig.seen.wait_for_in_flight(2),
            "the gate admitted fewer than 2"
        );
        thread::sleep(SETTLE);
        rig.seen.release(8);
    });

    assert_eq!(outcomes.len(), 8);
    assert_eq!(
        ended_as(&outcomes, &OutcomeClass::Remote),
        8,
        "{outcomes:?}"
    );
    assert_eq!(rig.seen.peak(), 2);
    assert_eq!(rig.seen.calls(), 8);
}

#[test]
fn remote_jobs_reach_jobs_above_workers() {
    let rig = Rig::new(6, |_| Reply::Ran);

    let outcomes = rig.build_watched(executing(6, 2), OnFailure::Stop, |rig| {
        assert!(
            rig.seen.wait_for_in_flight(6),
            "the gate admitted fewer than 6"
        );
        thread::sleep(SETTLE);
        rig.seen.release(8);
    });

    assert_eq!(
        ended_as(&outcomes, &OutcomeClass::Remote),
        8,
        "{outcomes:?}"
    );
    assert_eq!(rig.seen.peak(), 6);
}

#[test]
fn local_runs_never_exceed_workers_while_executing() {
    let rig = Rig::new(6, |_| Reply::Fails);

    let outcomes = rig.build_watched(executing(6, 2), OnFailure::Stop, |rig| {
        assert!(
            rig.seen.wait_for_in_flight(6),
            "the gate admitted fewer than 6"
        );
        rig.seen.release(8);
        // Every remote call fails, so the actions run here. The two the local gate admits are
        // held until both are running, and a third that is admitted wrongly has the time to
        // arrive before they are let go.
        assert!(
            rig.runs.wait_for_active(2),
            "the gate admitted fewer than 2"
        );
        thread::sleep(SETTLE);
        rig.runs.release.open();
    });

    assert_eq!(ended_as(&outcomes, &OutcomeClass::Built), 8, "{outcomes:?}");
    assert_eq!(rig.runs.peak(), 2);
}

#[test]
fn a_task_waiting_at_a_gate_when_the_build_stops_is_not_begun() {
    // Every lookup but the first waits until the build has taken the first result, which is when
    // the build stops: what then reaches the gate finds the build stopped.
    let rig = Rig::new(4, |_| Reply::Exited).holding_lookups();

    let outcomes = rig.build_watched(executing(1, 4), OnFailure::Stop, |rig| {
        assert!(rig.seen.wait_for_in_flight(1));
        rig.seen.release(8);
    });

    assert_eq!(rig.seen.calls(), 1, "nothing else was sent");
    assert_eq!(
        ended_as(&outcomes, &OutcomeClass::ActionFailure { status: 1 }),
        1,
        "{outcomes:?}"
    );
    let not_begun = outcomes
        .iter()
        .filter(|outcome| matches!(outcome.class, OutcomeClass::Skipped { .. }))
        .count();
    assert_eq!(not_begun, outcomes.len() - 1, "{outcomes:?}");
    assert!(
        outcomes.iter().all(|outcome| !matches!(
            outcome.class,
            OutcomeClass::InfraFailure {
                kind: InfraKind::TimedOut
            }
        )),
        "{outcomes:?}"
    );
    assert_eq!(rig.runs.peak(), 0, "nothing ran here");
}
