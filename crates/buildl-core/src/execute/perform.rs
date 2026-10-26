//! One action on a worker: what is handed over, what comes back, and how the action ended.
//!
//! Its own file because an action is run away from the thread that schedules the build: a
//! task goes out and a result comes back, and both must be whole values that borrow nothing.
//! How the action ended is decided here from what the worker saw, in a fixed order, so the
//! same exit and the same collected outputs always end the same way.
//!
//! Responsibilities: [`Task`], [`Told`], [`Shared`], [`RemoteRun`], [`Worked`], [`Performed`],
//! [`Span`], [`Printed`], [`Kept`] and [`Ended`]; the decisions [`exited`], [`collected`] with
//! [`Unkept`], and [`failed_to`]; what a failed call to the remote cache does to the worker that
//! met it; and [`perform`], which takes one task through the ports in that fixed order, sends
//! what it produced to the remote cache when the build writes there, and says what it is doing
//! while it does.
//!
//! Non-responsibilities: how an action is run and where what it left is stored, which are the
//! ports'; checking what the remote cache answers, which the fetch step does; describing an
//! upload and reading the answer to it, which the upload step does; and recording the result,
//! which happens on the thread that schedules the build.

use core::sync::atomic::{AtomicBool, Ordering};

use crate::error::{Error, PortName, Result};
use crate::execute::fetch::fetch;
use crate::execute::remote_action::remote_action;
use crate::execute::upload::{Sent, upload, upload_of};
use crate::ports::{Clock, ContentStore, ExecStrategy, RemoteCache};
use crate::types::{
    ActionKey, Channel, Diagnostic, Digest, Execution, Exit, Found, FoundEntry, FoundKind,
    InfraKind, LiveSink, NodeId, OutcomeClass, OutputManifest, OutputName, OutputRefusal,
    PlatformProperties, RawOutcome, ReadyAction, RemoteCall, RemoteFailure, RemoteNotice,
    RemotePolicy, RemoteSwitch, RemoteUse, TargetRole, Timestamp, Tree, TreeEntry, TreePath,
};

/// What a worker says about an action while it is still running it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Told {
    /// It began the action of this target.
    Began(NodeId),
    /// The action of this target wrote these bytes. Said only for the action shown live.
    Printed {
        /// The target.
        node: NodeId,
        /// Which output it wrote to.
        channel: Channel,
        /// What it wrote.
        bytes: Vec<u8>,
    },
    /// Something about the remote cache the build should say, with the worker going on. A failed
    /// call is said once per command, by the one worker that turned the remote off. An input
    /// that changed after its digest was taken, so that the target was not uploaded, is said for
    /// each target it happens to, and does not turn the remote off.
    Remote(RemoteNotice),
}

/// What the thread that schedules a build shares with its workers.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Shared<'a> {
    /// Set once nothing more may begin.
    pub(crate) stop: &'a AtomicBool,
    /// The one place of the action shown live, taken while set; `None` when no action is shown
    /// live. A worker takes it as it begins an action, and only the scheduling thread gives it
    /// back, once that action's outcome has been reported.
    pub(crate) live: Option<&'a AtomicBool>,
}

/// What a worker needs to consult the remote cache for the actions it takes.
#[derive(Debug)]
pub(crate) struct RemoteRun<'a, R> {
    /// The remote cache.
    pub(crate) port: &'a R,
    /// How much of it the build uses, and what a failed call does.
    pub(crate) policy: RemotePolicy,
    /// The settings the build's actions run under, which a remote result is looked up by.
    pub(crate) execution: &'a Execution,
    /// The platform properties the workspace asks for, which every action is described with.
    pub(crate) properties: &'a PlatformProperties,
    /// Whether a failed call has turned the remote off for the rest of the command.
    pub(crate) down: &'a RemoteSwitch,
}

impl<R> RemoteRun<'_, R> {
    /// Whether a worker asks the remote for a result before it runs an action: the build reads
    /// from the remote and no failed call has turned it off.
    fn consulted(&self) -> bool {
        matches!(self.policy.use_, RemoteUse::Read | RemoteUse::ReadWrite) && !self.down.is_off()
    }

    /// Whether a worker sends what an action produced to the remote: the build writes to the
    /// remote and no failed call has turned it off.
    fn uploading(&self) -> bool {
        self.policy.use_ == RemoteUse::ReadWrite && !self.down.is_off()
    }
}

/// One action to run, as it is handed to a worker.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Task {
    /// The target.
    pub(crate) node: NodeId,
    /// How the target is declared, which decides what its outputs may be.
    pub(crate) role: TargetRole,
    /// The target's finished key.
    pub(crate) key: ActionKey,
    /// What to run, and where its inputs and outputs are.
    pub(crate) action: ReadyAction,
}

/// What a worker hands back for one task.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Worked {
    /// The build had stopped before the worker began the action: nothing was run, and nothing
    /// is known but the target.
    NotBegun(NodeId),
    /// The worker began the action. Boxed, so that a task that was not begun is not as large
    /// as one that ran.
    Performed(Box<Performed>),
}

/// One action a worker began, with how it ended.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Performed {
    /// The target.
    pub(crate) node: NodeId,
    /// The key the task was handed over with.
    pub(crate) key: ActionKey,
    /// When the worker began and when it was done.
    pub(crate) span: Span,
    /// Whether the action held the place of the action shown live.
    pub(crate) live: bool,
    /// How the action ended.
    pub(crate) ended: Ended,
}

/// When a worker began an action, and when it was done with it.
///
/// One value and not two optional instants: an action a worker began has both, and a target no
/// worker began has neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Span {
    /// Read before the worker did anything for the action.
    pub(crate) started: Timestamp,
    /// Read after the last thing the worker did for the action.
    pub(crate) finished: Timestamp,
}

/// The digests of what an action printed, once both are stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Printed {
    /// What it wrote to standard output.
    pub(crate) stdout: Digest,
    /// What it wrote to standard error.
    pub(crate) stderr: Digest,
}

/// One output an action left, once its manifest is stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Kept {
    /// The digest of the manifest: the output's digest.
    pub(crate) digest: Digest,
    /// What the output is, which says what to put in place for it.
    pub(crate) manifest: OutputManifest,
}

/// How an action a worker began ended, before anything about it is recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Ended {
    /// It exited with status zero and left every output it declares, and all of it is stored.
    Produced {
        /// Each declared output, in declared order.
        outputs: Vec<Kept>,
        /// What it printed.
        printed: Printed,
    },
    /// It did not get that far.
    Failed {
        /// Why.
        class: OutcomeClass,
        /// A message for display, when the builder itself failed and said how.
        diagnostic: Option<Diagnostic>,
        /// What it printed, when the action ran and that was stored.
        printed: Option<Printed>,
    },
    /// The remote cache held a usable result for it, and all of that is stored. The action was
    /// not run.
    Fetched {
        /// Each declared output, in declared order.
        outputs: Vec<Kept>,
        /// What the original run printed.
        printed: Printed,
    },
    /// A call to the remote cache failed in a build that fails when one does.
    RemoteFailed(RemoteNotice),
}

/// How an action ended because a port failed with `error` while the builder did `kind` for it.
///
/// The error's text becomes the message. `printed` is what the action printed, when it had
/// already run and that was stored.
pub(crate) fn failed_to(kind: InfraKind, error: &Error, printed: Option<Printed>) -> Ended {
    Ended::Failed {
        class: OutcomeClass::InfraFailure { kind },
        diagnostic: Some(Diagnostic::new(error.to_string())),
        printed,
    }
}

/// How an action that ended with `exit` ended, when the exit alone decides it.
///
/// `None` for an exit with status zero: what the action left then decides. An action that ran
/// out of time is the builder's failure; any other status, and a signal that ended the action,
/// is the action's own.
pub(crate) const fn exited(exit: Exit, printed: Printed) -> Option<Ended> {
    let class = match exit {
        Exit::Exited(0) => return None,
        Exit::Exited(status) => OutcomeClass::ActionFailure { status },
        Exit::Signaled(signal) => OutcomeClass::Killed { signal },
        Exit::TimedOut => OutcomeClass::InfraFailure {
            kind: InfraKind::TimedOut,
        },
    };
    Some(Ended::Failed {
        class,
        diagnostic: None,
        printed: Some(printed),
    })
}

/// Why what an action left is not kept: the class its build ends in, and the message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Unkept {
    /// How the build of the target ends.
    pub(crate) class: OutcomeClass,
    /// A message for display; `None` for an output that is missing.
    pub(crate) diagnostic: Option<Diagnostic>,
}

/// The class of a target the builder itself failed: its outputs, or its row, could not be
/// stored or put in place.
pub(crate) const NOT_STORED: OutcomeClass = OutcomeClass::InfraFailure {
    kind: InfraKind::Store,
};

/// What an action that exited with status zero left, from what was found at each of its
/// outputs: one manifest per output, in declared order.
///
/// `role` is the role of the target: the output of a [`TargetRole::Discovery`] target must be a
/// file. `names` are the outputs the target declares and `found` what a strategy reported for them,
/// in the same order. An answer that does not hold one entry per output is the builder's
/// failure. Otherwise the outputs are examined in declared order, and the first that cannot be
/// kept decides how the build ends:
///
/// - nothing there is [`OutcomeClass::OutputMissing`];
/// - a symbolic link, or something that is neither a file nor a directory, is
///   [`OutcomeClass::OutputRefused`];
/// - a directory is [`OutputRefusal::NotAFile`] for a discovery target, whatever it holds;
/// - otherwise a directory is examined entry by entry in path order, and is refused for the
///   first entry that is not a regular file — an empty directory, a symbolic link, something
///   else, or a name that is not text — as it is when it holds nothing at all;
/// - a directory whose files do not make a tree — a path that is not below the directory, one
///   listed twice, a file where another needs a directory — was misreported, which is the
///   builder's failure.
pub(crate) fn collected(
    role: TargetRole,
    names: &[OutputName],
    found: Vec<Found>,
) -> core::result::Result<Vec<OutputManifest>, Unkept> {
    if found.len() != names.len() {
        return Err(Unkept {
            class: NOT_STORED,
            diagnostic: Some(Diagnostic::new(format!(
                "the strategy answered for {} outputs of {}",
                found.len(),
                names.len()
            ))),
        });
    }
    names
        .iter()
        .zip(found)
        .map(|(name, found)| examined(role, name, found))
        .collect()
}

/// The manifest of the output `name` of a target with `role`, from what was found at its path.
fn examined(
    role: TargetRole,
    name: &OutputName,
    found: Found,
) -> core::result::Result<OutputManifest, Unkept> {
    match found {
        Found::Nothing => Err(Unkept {
            class: OutcomeClass::OutputMissing { name: name.clone() },
            diagnostic: None,
        }),
        Found::File(file) => Ok(OutputManifest::File(file)),
        Found::Tree(_) if role == TargetRole::Discovery => {
            Err(refused(name, None, OutputRefusal::NotAFile))
        }
        Found::Tree(entries) => tree(name, entries).map(OutputManifest::Tree),
        Found::Symlink => Err(refused(name, None, OutputRefusal::Symlink)),
        Found::Special => Err(refused(name, None, OutputRefusal::Special)),
    }
}

/// The tree of the output `name`, from everything found below its path.
///
/// The entries are taken in path order, so which one a refusal names does not depend on the
/// order a filesystem listed them in.
fn tree(name: &OutputName, mut entries: Vec<FoundEntry>) -> core::result::Result<Tree, Unkept> {
    if entries.is_empty() {
        return Err(refused(name, None, OutputRefusal::EmptyDirectory));
    }
    entries.sort();
    let files = entries
        .into_iter()
        .map(|entry| {
            let refusal = match entry.kind {
                FoundKind::File(file) => {
                    return TreePath::parse(entry.path)
                        .map(|path| TreeEntry {
                            path,
                            digest: file.digest,
                            mode: file.mode,
                        })
                        .map_err(|error| misreported(name, &error));
                }
                FoundKind::EmptyDirectory => OutputRefusal::EmptyDirectory,
                FoundKind::Symlink => OutputRefusal::Symlink,
                FoundKind::Special => OutputRefusal::Special,
                FoundKind::NotText => OutputRefusal::NotText,
            };
            Err(refused(name, Some(&entry.path), refusal))
        })
        .collect::<core::result::Result<Vec<TreeEntry>, Unkept>>()?;
    Tree::new(files).map_err(|error| misreported(name, &error))
}

/// Why the output `name` is refused: for what is at `path` below it, or for the output itself
/// when `path` is `None`.
fn refused(name: &OutputName, path: Option<&str>, refusal: OutputRefusal) -> Unkept {
    let what = path.map_or_else(|| name.to_string(), |path| format!("{name}/{path}"));
    let why = match refusal {
        OutputRefusal::Symlink => "is a symbolic link",
        OutputRefusal::Special => "is neither a file nor a directory",
        OutputRefusal::EmptyDirectory => "is an empty directory",
        OutputRefusal::NotText => "has a name that is not valid Unicode",
        OutputRefusal::NotAFile => "is a directory, and a discovery output must be a file",
    };
    Unkept {
        class: OutcomeClass::OutputRefused {
            name: name.clone(),
            refusal,
        },
        diagnostic: Some(Diagnostic::new(format!("{what} {why}"))),
    }
}

/// The builder's failure for an output `name` whose report is not a tree, as `error` says.
fn misreported(name: &OutputName, error: &Error) -> Unkept {
    Unkept {
        class: NOT_STORED,
        diagnostic: Some(Diagnostic::new(format!(
            "the strategy reported {name}: {error}"
        ))),
    }
}

/// Runs one task on the thread it is called from, and returns what came of it.
///
/// A task handed over once `shared` says nothing more may begin is not begun: the clock is not
/// read and no port is asked anything. Otherwise the clock is read, the action is taken through
/// `strategy` and `cas`, and the clock is read again, however the action ended: an action a
/// worker began always carries both instants.
///
/// With `remote` given and reading, and not turned off by an earlier failure, the worker first
/// asks the remote cache for the task's result. One it holds ends the task [`Ended::Fetched`]:
/// the clock is read before the request and after the result is stored, the action is not run,
/// and the build is not told the task began. A miss, or a failed call under
/// [`RemoteFailure::Lenient`], goes on to run the action. Without `remote`, or with a policy
/// that does not read, nothing is asked of any remote and the task goes as it always has.
///
/// With `remote` writing, and not turned off, an action that ran and left every output is then
/// sent to the remote cache, once it is all in `cas`. A failed call is handled as a failed lookup
/// is: under [`RemoteFailure::Lenient`] the remote is turned off and the task still ends
/// [`Ended::Produced`]; under [`RemoteFailure::Strict`] it ends [`Ended::RemoteFailed`]. An input
/// that changed since its digest was taken is told to the build, and leaves the remote on. An
/// action that failed, and one that was fetched, are not sent. The second clock reading is taken
/// after the upload.
///
/// A worker that begins an action says so through `tell`. When an action may be shown live and
/// no other is, this one takes that place: everything it writes is said through `tell` as it is
/// written, and the result says the place was taken. The place is not given back here.
pub(crate) fn perform<S, C, K, R>(
    task: Task,
    strategy: &S,
    cas: &C,
    clock: &K,
    remote: Option<&RemoteRun<'_, R>>,
    shared: Shared<'_>,
    tell: &(dyn Fn(Told) + Sync),
) -> Worked
where
    S: ExecStrategy,
    C: ContentStore,
    K: Clock,
    R: RemoteCache,
{
    if shared.stop.load(Ordering::SeqCst) {
        return Worked::NotBegun(task.node);
    }
    let node = task.node;
    let asked = remote
        .filter(|remote| remote.consulted())
        .map(|remote| (remote, clock.now()));
    if let Some((remote, started)) = asked
        && let Some(ended) = consult(&task, remote, cas, tell)
    {
        let finished = clock.now();
        return Worked::Performed(Box::new(Performed {
            node,
            key: task.key,
            span: Span { started, finished },
            live: false,
            ended,
        }));
    }
    let live = shared.live.is_some_and(|place| {
        place
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    });
    tell(Told::Began(node));
    let printed: LiveSink<'_> = &|channel, bytes| {
        tell(Told::Printed {
            node,
            channel,
            bytes: bytes.to_vec(),
        });
    };
    let started = asked.map_or_else(|| clock.now(), |(_, started)| started);
    let ended = run(&task, strategy, cas, live.then_some(printed));
    let ended = match remote.filter(|remote| remote.uploading()) {
        Some(remote) => sent(&task, ended, remote, cas, tell),
        None => ended,
    };
    let finished = clock.now();
    Worked::Performed(Box::new(Performed {
        node,
        key: task.key,
        span: Span { started, finished },
        live,
        ended,
    }))
}

/// Asks the remote cache for the result of `task`, and says how the task ended when that decides
/// it: `None` when the action is to run.
///
/// A result the remote holds is checked and stored, and ends the task [`Ended::Fetched`]. No
/// result runs the action. A failed call runs it too, once the one worker that turned the remote
/// off has told the build, or ends the task [`Ended::RemoteFailed`] when the build fails on a
/// failed call. A task whose request cannot be described, because the content store does not
/// hold what its dependencies produced, ends as a failure to store, after the build is told it
/// began, as any action the builder fails to take is. So does a task whose fetched result the
/// content store cannot keep: that is the store's failure, not the remote's.
fn consult<C: ContentStore, R: RemoteCache>(
    task: &Task,
    remote: &RemoteRun<'_, R>,
    cas: &C,
    tell: &(dyn Fn(Told) + Sync),
) -> Option<Ended> {
    let described = match remote_action(task, remote.execution, remote.properties, cas) {
        Ok(described) => described,
        Err(error) => {
            tell(Told::Began(task.node));
            return Some(failed_to(InfraKind::Store, &error, None));
        }
    };
    let hit = match remote.port.lookup(&described) {
        Ok(Some(hit)) => hit,
        Ok(None) => return None,
        Err(error) => return failed_call(remote, RemoteCall::Lookup, &error, tell),
    };
    let checked = match fetch(&hit, task, remote.port) {
        Ok(checked) => checked,
        Err(error) => return failed_call(remote, RemoteCall::Download, &error, tell),
    };
    Some(match checked.keep(cas) {
        Ok((outputs, printed)) => Ended::Fetched { outputs, printed },
        Err(error) => failed_to(InfraKind::Store, &error, None),
    })
}

/// What a worker does when `call` to the remote failed with `error`: `None` to go on and run the
/// action, or the end of the task when the build fails on a failed call.
///
/// Otherwise the remote is turned off, and the worker that [`RemoteSwitch::turn_off`] reports as
/// the one that turned it off tells the build; a worker that fails at the same moment says
/// nothing.
fn failed_call<R>(
    remote: &RemoteRun<'_, R>,
    call: RemoteCall,
    error: &Error,
    tell: &(dyn Fn(Told) + Sync),
) -> Option<Ended> {
    let diagnostic = match error {
        Error::Adapter { diagnostic, .. } => diagnostic.clone(),
        other => Diagnostic::new(other.to_string()),
    };
    failed_with(remote, RemoteNotice::Failed { call, diagnostic }, tell)
}

/// What a worker does when a call to the remote failed as `notice` says: as [`failed_call`].
fn failed_with<R>(
    remote: &RemoteRun<'_, R>,
    notice: RemoteNotice,
    tell: &(dyn Fn(Told) + Sync),
) -> Option<Ended> {
    match remote.policy.failure {
        RemoteFailure::Strict => Some(Ended::RemoteFailed(notice)),
        RemoteFailure::Lenient => {
            if remote.down.turn_off() {
                tell(Told::Remote(notice));
            }
            None
        }
    }
}

/// Sends what `task` produced to the remote cache, and says how the task ends: as `ended` says
/// for anything but an action that left every output, which is all that is sent.
///
/// What cannot be described, because the content store does not hold what the upload names,
/// ends the task as a failure to store; that is not the remote's fault.
fn sent<C: ContentStore, R: RemoteCache>(
    task: &Task,
    ended: Ended,
    remote: &RemoteRun<'_, R>,
    cas: &C,
    tell: &(dyn Fn(Told) + Sync),
) -> Ended {
    let Ended::Produced { outputs, printed } = &ended else {
        return ended;
    };
    let printed = *printed;
    let described = remote_action(task, remote.execution, remote.properties, cas)
        .and_then(|action| Ok((action, upload_of(task, outputs, &printed, cas)?)));
    let (action, produced) = match described {
        Ok(described) => described,
        Err(error) => return failed_to(InfraKind::Store, &error, Some(printed)),
    };
    match upload(remote.port, &task.action.label, &action, &produced, cas) {
        Sent::Stored => ended,
        Sent::Declined(notice) => {
            tell(Told::Remote(notice));
            ended
        }
        Sent::Failed(notice) => failed_with(remote, notice, tell).unwrap_or(ended),
    }
}

/// Takes the action of `task` through its steps, and says how it ended at the first step that
/// decides it: a directory to run in, the run, storing what it printed, its exit, storing the
/// files it left, what those are, and storing the manifest of each output.
fn run<S: ExecStrategy, C: ContentStore>(
    task: &Task,
    strategy: &S,
    cas: &C,
    live: Option<LiveSink<'_>>,
) -> Ended {
    let action = &task.action;
    let dir = match strategy.prepare(action) {
        Ok(dir) => dir,
        Err(error) => return failed_to(InfraKind::Prepare, &error, None),
    };
    let raw = match strategy.run(&dir, action, live) {
        Ok(raw) => raw,
        Err(error) => return failed_to(InfraKind::Run, &error, None),
    };
    let printed = match stored(&raw, cas) {
        Ok(printed) => printed,
        Err(error) => return failed_to(InfraKind::Store, &error, None),
    };
    if let Some(ended) = exited(raw.exit, printed) {
        return ended;
    }
    let found = match strategy.collect(&dir, action, cas) {
        Ok(found) => found,
        Err(error) => return failed_to(InfraKind::Store, &error, Some(printed)),
    };
    let manifests = match collected(task.role, &task.key.components().own.outputs, found) {
        Ok(manifests) => manifests,
        Err(unkept) => {
            return Ended::Failed {
                class: unkept.class,
                diagnostic: unkept.diagnostic,
                printed: Some(printed),
            };
        }
    };
    match kept(manifests, cas) {
        Ok(outputs) => Ended::Produced { outputs, printed },
        Err(error) => failed_to(InfraKind::Store, &error, Some(printed)),
    }
}

/// The error that fails a build over `notice`, which names the remote cache and the call.
pub(crate) fn remote_failure(notice: &RemoteNotice) -> Error {
    let diagnostic = match notice {
        RemoteNotice::Failed { call, diagnostic } => format!("{call}: {diagnostic}"),
        RemoteNotice::InputChanged { label, path } => {
            format!("{label}: {path} changed after its digest was taken")
        }
    };
    Error::Adapter {
        port: PortName::RemoteCache,
        diagnostic: Diagnostic::new(diagnostic),
    }
}

/// Stores the manifest of each output, and returns each with the digest it is held under.
fn kept<C: ContentStore>(manifests: Vec<OutputManifest>, cas: &C) -> Result<Vec<Kept>> {
    manifests
        .into_iter()
        .map(|manifest| {
            let digest = cas.put(&manifest.to_bytes()?)?;
            Ok(Kept { digest, manifest })
        })
        .collect()
}

/// Stores what an action printed, and returns the two digests.
fn stored<C: ContentStore>(raw: &RawOutcome, cas: &C) -> Result<Printed> {
    Ok(Printed {
        stdout: cas.put(&raw.stdout)?,
        stderr: cas.put(&raw.stderr)?,
    })
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeMap;
    use std::sync::Mutex;

    use super::{
        Ended, Performed, Printed, RemoteRun, Span, Task, Told, Unkept, Worked, collected, consult,
        exited, failed_call, failed_to, remote_failure,
    };
    use crate::error::{Error, PortName, Result};
    use crate::execute::fixtures::{command, file, kept, key, label, output_path, outputs, tree};
    use crate::ports::{ContentStore, NoRemote};
    use crate::types::{
        Diagnostic, Digest, Execution, Exit, FileMode, Found, FoundEntry, FoundKind, InfraKind,
        NetworkAccess, NodeId, OutcomeClass, OutputManifest, OutputRefusal, PlatformProperties,
        ReadyAction, RemoteCall, RemoteFailure, RemoteNotice, RemotePolicy, RemoteSwitch,
        RemoteUse, StoredFile, TargetRole, Timestamp,
    };

    fn printed() -> Printed {
        Printed {
            stdout: Digest::of(b"compiling\n"),
            stderr: Digest::of(b""),
        }
    }

    /// A file holding `content`, as a strategy reports one it stored.
    fn stored(content: &str, mode: FileMode) -> StoredFile {
        StoredFile {
            digest: Digest::of(content.as_bytes()),
            mode,
        }
    }

    /// Something found at `path` below a directory output.
    fn found(path: &str, kind: FoundKind) -> FoundEntry {
        FoundEntry {
            path: path.to_owned(),
            kind,
        }
    }

    /// A plain file found at `path` below a directory output, holding its own path.
    fn found_file(path: &str) -> FoundEntry {
        found(path, FoundKind::File(stored(path, FileMode::Plain)))
    }

    /// The builder's failure to store, with the message `text`.
    fn not_stored(text: &str) -> Unkept {
        Unkept {
            class: OutcomeClass::InfraFailure {
                kind: InfraKind::Store,
            },
            diagnostic: Some(Diagnostic::new(text)),
        }
    }

    /// The refusal of the output `name`, with the message `text`.
    fn refused(name: &str, refusal: OutputRefusal, text: &str) -> Unkept {
        Unkept {
            class: OutcomeClass::OutputRefused {
                name: outputs(&[name]).remove(0),
                refusal,
            },
            diagnostic: Some(Diagnostic::new(text)),
        }
    }

    fn strategy_error(text: &str) -> Error {
        Error::Adapter {
            port: PortName::ExecStrategy,
            diagnostic: Diagnostic::new(text),
        }
    }

    /// Stands for handing a value to another thread: it must be `Send` and borrow nothing.
    fn handed_over<T: Send + 'static>(value: T) -> T {
        value
    }

    /// The policy of a build that reads from the remote and treats a failed call as `failure`.
    const fn reading(failure: RemoteFailure) -> RemotePolicy {
        RemotePolicy {
            use_: RemoteUse::Read,
            failure,
        }
    }

    /// A remote run under `policy` whose remote is `down` or not.
    fn running<'a>(
        policy: RemotePolicy,
        execution: &'a Execution,
        properties: &'a PlatformProperties,
        down: &'a RemoteSwitch,
    ) -> RemoteRun<'a, NoRemote> {
        RemoteRun {
            port: &NoRemote,
            policy,
            execution,
            properties,
            down,
        }
    }

    #[test]
    fn a_remote_is_consulted_only_when_the_build_reads_and_it_is_not_down() {
        let execution = Execution::default();
        let none = PlatformProperties::default();
        let up = RemoteSwitch::new();
        let down = RemoteSwitch::new();
        down.turn_off();
        for (use_, expected) in [
            (RemoteUse::Off, false),
            (RemoteUse::Read, true),
            (RemoteUse::ReadWrite, true),
        ] {
            let policy = RemotePolicy {
                use_,
                failure: RemoteFailure::Lenient,
            };
            assert_eq!(
                running(policy, &execution, &none, &up).consulted(),
                expected
            );
            assert!(!running(policy, &execution, &none, &down).consulted());
        }
    }

    #[test]
    fn a_remote_is_written_to_only_when_the_build_writes_and_it_is_not_down() {
        let execution = Execution::default();
        let none = PlatformProperties::default();
        let up = RemoteSwitch::new();
        let down = RemoteSwitch::new();
        down.turn_off();
        for (use_, expected) in [
            (RemoteUse::Off, false),
            (RemoteUse::Read, false),
            (RemoteUse::ReadWrite, true),
        ] {
            let policy = RemotePolicy {
                use_,
                failure: RemoteFailure::Lenient,
            };
            assert_eq!(
                running(policy, &execution, &none, &up).uploading(),
                expected
            );
            assert!(!running(policy, &execution, &none, &down).uploading());
        }
    }

    #[test]
    fn a_lenient_failure_is_told_once_by_the_worker_that_turns_the_remote_off() {
        let execution = Execution::default();
        let none = PlatformProperties::default();
        let down = RemoteSwitch::new();
        let remote = running(reading(RemoteFailure::Lenient), &execution, &none, &down);
        let told = Mutex::new(Vec::new());
        let tell = |message: Told| told.lock().unwrap().push(message);
        let error = Error::Adapter {
            port: PortName::RemoteCache,
            diagnostic: Diagnostic::new("the server cannot be reached"),
        };

        assert_eq!(
            failed_call(&remote, RemoteCall::Lookup, &error, &tell),
            None
        );
        assert_eq!(
            failed_call(&remote, RemoteCall::Lookup, &error, &tell),
            None
        );

        assert!(down.is_off());
        assert_eq!(
            *told.lock().unwrap(),
            [Told::Remote(RemoteNotice::Failed {
                call: RemoteCall::Lookup,
                diagnostic: Diagnostic::new("the server cannot be reached"),
            })],
            "the adapter's own message, once"
        );
    }

    /// A content store that holds nothing and keeps nothing.
    struct EmptyStore;

    impl ContentStore for EmptyStore {
        fn put(&self, bytes: &[u8]) -> Result<Digest> {
            Ok(Digest::of(bytes))
        }

        fn contains(&self, _digest: &Digest) -> bool {
            false
        }

        fn get(&self, _digest: &Digest) -> Result<Option<Vec<u8>>> {
            Ok(None)
        }

        fn clear(&self, _at: &crate::types::OutputPath) -> Result<()> {
            Ok(())
        }

        fn place(
            &self,
            _digest: &Digest,
            _mode: FileMode,
            _at: &crate::types::OutputPath,
        ) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_task_the_remote_cannot_be_asked_about_is_begun_before_it_fails_to_store() {
        let execution = Execution::default();
        let none = PlatformProperties::default();
        let down = RemoteSwitch::new();
        let remote = running(reading(RemoteFailure::Lenient), &execution, &none, &down);
        let task = Task {
            node: NodeId::new(0),
            role: TargetRole::Build,
            key: key(&["app"]),
            action: ReadyAction {
                label: label("//:app"),
                command: command(&["cc"]),
                description: None,
                env: BTreeMap::new(),
                inputs: Vec::new(),
                deps: vec![output_path("//:lib", "lib")],
                outputs: vec![output_path("//:app", "app")],
                network: NetworkAccess::Sealed,
            },
        };
        let told = Mutex::new(Vec::new());
        let tell = |message: Told| told.lock().unwrap().push(message);

        let ended = consult(&task, &remote, &EmptyStore, &tell);

        assert!(matches!(
            ended,
            Some(Ended::Failed {
                class: OutcomeClass::InfraFailure {
                    kind: InfraKind::Store
                },
                ..
            })
        ));
        assert_eq!(
            *told.lock().unwrap(),
            [Told::Began(NodeId::new(0))],
            "a failure to take the action is told after the action began, as a run's is"
        );
        assert!(!down.is_off(), "the remote is not at fault");
    }

    #[test]
    fn a_strict_failure_ends_the_task_and_says_nothing_to_the_build() {
        let execution = Execution::default();
        let none = PlatformProperties::default();
        let down = RemoteSwitch::new();
        let remote = running(reading(RemoteFailure::Strict), &execution, &none, &down);
        let told = Mutex::new(Vec::new());
        let tell = |message: Told| told.lock().unwrap().push(message);

        let ended = failed_call(&remote, RemoteCall::Download, &Error::ForeignPlan, &tell);

        assert!(matches!(
            ended,
            Some(Ended::RemoteFailed(RemoteNotice::Failed {
                call: RemoteCall::Download,
                ..
            }))
        ));
        assert!(told.lock().unwrap().is_empty());
        assert!(!down.is_off());
    }

    #[test]
    fn a_failure_that_fails_the_build_names_the_remote_cache_and_the_call() {
        let failure = remote_failure(&RemoteNotice::Failed {
            call: RemoteCall::Lookup,
            diagnostic: Diagnostic::new("timed out"),
        });
        assert_eq!(failure.to_string(), "remote cache: lookup: timed out");
    }

    #[test]
    fn an_exit_with_status_zero_decides_nothing_yet() {
        assert_eq!(exited(Exit::Exited(0), printed()), None);
    }

    #[test]
    fn an_exit_with_another_status_is_the_actions_failure() {
        for status in [1, 2, -1, 255] {
            assert_eq!(
                exited(Exit::Exited(status), printed()),
                Some(Ended::Failed {
                    class: OutcomeClass::ActionFailure { status },
                    diagnostic: None,
                    printed: Some(printed()),
                })
            );
        }
    }

    #[test]
    fn a_signal_is_the_actions_failure_and_carries_its_number() {
        for signal in [9, 11, 15] {
            assert_eq!(
                exited(Exit::Signaled(signal), printed()),
                Some(Ended::Failed {
                    class: OutcomeClass::Killed { signal },
                    diagnostic: None,
                    printed: Some(printed()),
                })
            );
        }
    }

    #[test]
    fn a_timeout_is_the_builders_failure_and_carries_no_message() {
        assert_eq!(
            exited(Exit::TimedOut, printed()),
            Some(Ended::Failed {
                class: OutcomeClass::InfraFailure {
                    kind: InfraKind::TimedOut,
                },
                diagnostic: None,
                printed: Some(printed()),
            })
        );
    }

    #[test]
    fn a_ports_failure_is_the_builders_and_its_text_is_the_message() {
        let error = strategy_error("//:app: no directory to run in");
        for (kind, printed) in [
            (InfraKind::Prepare, None),
            (InfraKind::Run, None),
            (InfraKind::Store, None),
            (InfraKind::Store, Some(printed())),
        ] {
            assert_eq!(
                failed_to(kind, &error, printed),
                Ended::Failed {
                    class: OutcomeClass::InfraFailure { kind },
                    diagnostic: Some(Diagnostic::new(
                        "exec strategy: //:app: no directory to run in"
                    )),
                    printed,
                }
            );
        }
    }

    #[test]
    fn an_action_that_left_every_output_produced_them() {
        let app = stored("app", FileMode::Executable);
        let map = stored("map", FileMode::Plain);
        assert_eq!(
            collected(
                TargetRole::Build,
                &outputs(&["app", "app.map"]),
                vec![Found::File(app), Found::File(map)]
            ),
            Ok(vec![OutputManifest::File(app), OutputManifest::File(map)])
        );
        assert_eq!(
            collected(TargetRole::Build, &[], Vec::new()),
            Ok(Vec::new()),
            "a target that declares no output"
        );
    }

    #[test]
    fn a_directory_is_the_same_tree_whatever_order_it_was_listed_in() {
        let listed = |paths: &[&str]| {
            let entries = paths.iter().map(|path| found_file(path)).collect();
            collected(
                TargetRole::Build,
                &outputs(&["site"]),
                vec![Found::Tree(entries)],
            )
        };
        let one = listed(&["index.html", "assets/app.js", "assets/app.css"]);
        let other = listed(&["assets/app.css", "index.html", "assets/app.js"]);
        assert_eq!(one, other);
        assert_eq!(
            one,
            Ok(vec![tree(&[
                ("assets/app.css", "assets/app.css"),
                ("assets/app.js", "assets/app.js"),
                ("index.html", "index.html"),
            ])])
        );
    }

    #[test]
    fn a_file_and_a_directory_are_kept_side_by_side() {
        let app = stored("app", FileMode::Executable);
        let share = Found::Tree(vec![found_file("man/app.1")]);
        assert_eq!(
            collected(
                TargetRole::Build,
                &outputs(&["app", "share"]),
                vec![Found::File(app), share]
            ),
            Ok(vec![
                OutputManifest::File(app),
                tree(&[("man/app.1", "man/app.1")]),
            ])
        );
    }

    #[test]
    fn a_discovery_output_may_be_a_file_but_not_a_directory() {
        let pkgs = stored("{}", FileMode::Plain);
        assert_eq!(
            collected(
                TargetRole::Discovery,
                &outputs(&["pkgs.json"]),
                vec![Found::File(pkgs)]
            ),
            Ok(vec![OutputManifest::File(pkgs)])
        );
        for entries in [vec![found_file("a.json")], Vec::new()] {
            assert_eq!(
                collected(
                    TargetRole::Discovery,
                    &outputs(&["pkgs.json"]),
                    vec![Found::Tree(entries)]
                ),
                Err(refused(
                    "pkgs.json",
                    OutputRefusal::NotAFile,
                    "pkgs.json is a directory, and a discovery output must be a file"
                ))
            );
        }
    }

    #[test]
    fn an_answer_of_the_wrong_length_is_the_builders_failure() {
        let app = Found::File(stored("app", FileMode::Plain));
        for (found, text) in [
            (
                vec![app.clone()],
                "the strategy answered for 1 outputs of 2",
            ),
            (
                vec![app.clone(), Found::Nothing, app],
                "the strategy answered for 3 outputs of 2",
            ),
            (Vec::new(), "the strategy answered for 0 outputs of 2"),
        ] {
            assert_eq!(
                collected(TargetRole::Build, &outputs(&["app", "app.map"]), found),
                Err(not_stored(text))
            );
        }
    }

    #[test]
    fn an_output_the_action_did_not_leave_is_named() {
        let names = outputs(&["app", "app.map", "app.sym"]);
        let app = Found::File(stored("app", FileMode::Plain));
        let missing = |name: &str| {
            Err(Unkept {
                class: OutcomeClass::OutputMissing {
                    name: outputs(&[name]).remove(0),
                },
                diagnostic: None,
            })
        };
        assert_eq!(
            collected(
                TargetRole::Build,
                &names,
                vec![app.clone(), Found::Nothing, app.clone()]
            ),
            missing("app.map"),
            "one missing"
        );
        assert_eq!(
            collected(
                TargetRole::Build,
                &names,
                vec![app, Found::Nothing, Found::Nothing]
            ),
            missing("app.map"),
            "two missing: the first, in the order the outputs were written"
        );
        assert_eq!(
            collected(
                TargetRole::Build,
                &names,
                vec![Found::Nothing, Found::Nothing, Found::Nothing]
            ),
            missing("app"),
            "none left"
        );
    }

    #[test]
    fn an_output_that_is_neither_a_file_nor_a_directory_is_refused() {
        for (found, refusal, text) in [
            (
                Found::Symlink,
                OutputRefusal::Symlink,
                "dist is a symbolic link",
            ),
            (
                Found::Special,
                OutputRefusal::Special,
                "dist is neither a file nor a directory",
            ),
            (
                Found::Tree(Vec::new()),
                OutputRefusal::EmptyDirectory,
                "dist is an empty directory",
            ),
        ] {
            assert_eq!(
                collected(TargetRole::Build, &outputs(&["dist"]), vec![found]),
                Err(refused("dist", refusal, text))
            );
        }
    }

    #[test]
    fn a_directory_is_refused_for_anything_in_it_that_is_not_a_regular_file() {
        for (kind, refusal, text) in [
            (
                FoundKind::Symlink,
                OutputRefusal::Symlink,
                "dist/vendor/x is a symbolic link",
            ),
            (
                FoundKind::Special,
                OutputRefusal::Special,
                "dist/vendor/x is neither a file nor a directory",
            ),
            (
                FoundKind::EmptyDirectory,
                OutputRefusal::EmptyDirectory,
                "dist/vendor/x is an empty directory",
            ),
            (
                FoundKind::NotText,
                OutputRefusal::NotText,
                "dist/vendor/x has a name that is not valid Unicode",
            ),
        ] {
            let entries = vec![found_file("index.html"), found("vendor/x", kind)];
            assert_eq!(
                collected(
                    TargetRole::Build,
                    &outputs(&["dist"]),
                    vec![Found::Tree(entries)]
                ),
                Err(refused("dist", refusal, text))
            );
        }
    }

    #[test]
    fn the_entry_a_refusal_names_does_not_depend_on_the_order_of_the_listing() {
        let link = || found("m/link", FoundKind::Symlink);
        let pipe = || found("b/pipe", FoundKind::Special);
        let expected = Err(refused(
            "dist",
            OutputRefusal::Special,
            "dist/b/pipe is neither a file nor a directory",
        ));
        for entries in [
            vec![link(), found_file("a"), pipe()],
            vec![pipe(), link(), found_file("a")],
        ] {
            assert_eq!(
                collected(
                    TargetRole::Build,
                    &outputs(&["dist"]),
                    vec![Found::Tree(entries)]
                ),
                expected,
                "the lowest path that cannot be kept"
            );
        }
    }

    #[test]
    fn the_first_output_that_cannot_be_kept_decides_how_the_build_ends() {
        let names = outputs(&["app", "dist", "docs"]);
        let app = Found::File(stored("app", FileMode::Plain));
        assert_eq!(
            collected(
                TargetRole::Build,
                &names,
                vec![app.clone(), Found::Symlink, Found::Nothing]
            ),
            Err(refused(
                "dist",
                OutputRefusal::Symlink,
                "dist is a symbolic link"
            )),
            "refused before a later one is missing"
        );
        assert_eq!(
            collected(
                TargetRole::Build,
                &names,
                vec![app, Found::Nothing, Found::Symlink]
            ),
            Err(Unkept {
                class: OutcomeClass::OutputMissing {
                    name: outputs(&["dist"]).remove(0),
                },
                diagnostic: None,
            }),
            "missing before a later one is refused"
        );
    }

    #[test]
    fn a_report_that_is_not_a_tree_is_the_builders_failure() {
        for (paths, text) in [
            (
                vec!["index.html", "../escaped"],
                "the strategy reported dist: invalid tree path: \"../escaped\" — \
                 must not contain a '.' or '..' segment",
            ),
            (
                vec!["/etc/passwd"],
                "the strategy reported dist: invalid tree path: \"/etc/passwd\" — \
                 must not contain an empty path segment",
            ),
            (
                vec!["a", "b", "a"],
                "the strategy reported dist: not a tree: a is listed twice",
            ),
            (
                vec!["a/b", "a"],
                "the strategy reported dist: not a tree: a is both a file and a directory",
            ),
        ] {
            let entries = paths.iter().map(|path| found_file(path)).collect();
            assert_eq!(
                collected(
                    TargetRole::Build,
                    &outputs(&["dist"]),
                    vec![Found::Tree(entries)]
                ),
                Err(not_stored(text))
            );
        }
    }

    #[test]
    fn a_task_and_its_result_can_be_handed_to_another_thread() {
        let node = NodeId::new(3);
        let span = Span {
            started: Timestamp::from_unix_nanos(10),
            finished: Timestamp::from_unix_nanos(20),
        };
        let action = ReadyAction {
            label: label("//:app"),
            command: command(&["cc", "-o", "out/app"]),
            description: None,
            env: BTreeMap::new(),
            inputs: Vec::new(),
            deps: Vec::new(),
            outputs: vec![output_path("//:app", "app")],
            network: NetworkAccess::Sealed,
        };

        let task = handed_over(Task {
            node,
            role: TargetRole::Build,
            key: key(&["app"]),
            action: action.clone(),
        });
        assert_eq!(task.node, node);
        assert_eq!(task.key, key(&["app"]));
        assert_eq!(task.action, action);

        match handed_over(Worked::NotBegun(task.node)) {
            Worked::NotBegun(not_begun) => assert_eq!(not_begun, node),
            Worked::Performed(performed) => unreachable!("expected NotBegun, got {performed:?}"),
        }

        let ended = Ended::Produced {
            outputs: vec![kept(file("app"))],
            printed: printed(),
        };
        let worked = handed_over(Worked::Performed(Box::new(Performed {
            node: task.node,
            key: task.key,
            span,
            live: false,
            ended: ended.clone(),
        })));
        match worked {
            Worked::Performed(performed) => {
                assert_eq!(performed.node, node);
                assert_eq!(performed.key, key(&["app"]));
                assert_eq!(performed.span.started, Timestamp::from_unix_nanos(10));
                assert_eq!(performed.span.finished, Timestamp::from_unix_nanos(20));
                assert_eq!(performed.ended, ended);
            }
            Worked::NotBegun(not_begun) => unreachable!("expected Performed, got {not_begun:?}"),
        }
    }
}
