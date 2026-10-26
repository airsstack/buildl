//! The `Changes` port over the operating system's file events.
//!
//! Its own file because it holds the watcher, the channel its events arrive on, and the rule
//! that turns an event's absolute path into a workspace-relative one.
//!
//! Responsibilities: [`NotifyChanges`], the settling of a burst of events into one batch,
//! telling a read, which is dropped, from a change, and dropping a path the watch only heard
//! of late.
//!
//! On macOS the event stream can deliver, after the watch began, events for files written just
//! before it. The watch is given the instant it began, and an event whose path still exists and
//! whose status-change time is earlier than that is dropped: the build that runs before the
//! first wait has already read it. A path that cannot be examined is kept. A batch left empty
//! by the dropping keeps waiting. The adapter never reads the clock; the caller passes the
//! instant in.
//!
//! Non-responsibilities: which paths matter to a target, and any timer that runs while nothing
//! changes.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use buildl_core::{
    ChangeBatch, ChangedPath, Changes, Diagnostic, Error, PortName, Result, Timestamp,
};
use notify::event::{AccessKind, AccessMode};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

/// How long the events must be quiet before a burst counts as settled.
const WINDOW: Duration = Duration::from_millis(50);

/// What the watcher sends: an event, or the error it hit.
type Delivery = notify::Result<Event>;

/// Watches a workspace root and reports its changes in debounced batches.
///
/// The watch lasts as long as the value does. Every path under the root is reported, whatever
/// the kind of change; a read is not a change, so an access event is dropped unless it closes a
/// file written. A path outside the root, the root itself, and a path that is not a valid
/// changed path are dropped. A report that the operating system dropped events is an error,
/// never a silent gap.
///
/// On macOS an event for a path whose status-change time is earlier than the start of the watch
/// is dropped, because the file was written before the watch and the build that runs first has
/// read it. The status-change time is used, not the modification time, which can be set back;
/// a path that no longer exists is kept. The rule takes the file system's clock and the start
/// instant to agree, which they do on a local volume; on a network volume or one with coarse
/// timestamps, an edit made just after the watch began can be dropped.
#[derive(Debug)]
pub struct NotifyChanges {
    root: PathBuf,
    events: Receiver<Delivery>,
    /// When the watch was about to be registered, as the caller read it.
    started: Timestamp,
    /// Held so the watch lasts; dropping it stops the events.
    _watcher: RecommendedWatcher,
}

/// An adapter error for the file watcher with `text` as its message.
fn failure(text: String) -> Error {
    Error::Adapter {
        port: PortName::Changes,
        diagnostic: Diagnostic::new(text),
    }
}

/// The adapter error for a watcher that reported `source` while watching `root`.
fn watch_failure(root: &Path, source: &notify::Error) -> Error {
    failure(format!("cannot watch {}: {source}", root.display()))
}

/// `path` relative to `root` as a [`ChangedPath`], or `None` when it lies outside `root`, is
/// `root` itself, or is not a valid workspace-relative path.
fn relative(root: &Path, path: &Path) -> Option<ChangedPath> {
    let below = path.strip_prefix(root).ok()?;
    let mut segments = Vec::new();
    for component in below.components() {
        match component {
            Component::Normal(name) => segments.push(name.to_string_lossy()),
            _ => return None,
        }
    }
    ChangedPath::parse(segments.join("/")).ok()
}

impl NotifyChanges {
    /// Starts watching `root` and everything below it.
    ///
    /// The root is canonicalized first, because the operating system reports canonical paths.
    /// `started` is the instant, read from the clock just before this call, from which a change
    /// counts; on macOS an event for a path last changed before it is dropped.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Adapter`] naming [`PortName::Changes`] and the root when the root cannot
    /// be resolved or the watch cannot be set up.
    pub fn start(root: &Path, started: Timestamp) -> Result<Self> {
        let canonical = root
            .canonicalize()
            .map_err(|source| failure(format!("cannot resolve {}: {source}", root.display())))?;
        let (sender, events) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(sender)
            .map_err(|source| watch_failure(&canonical, &source))?;
        watcher
            .watch(&canonical, RecursiveMode::Recursive)
            .map_err(|source| watch_failure(&canonical, &source))?;
        Ok(Self {
            root: canonical,
            events,
            started,
            _watcher: watcher,
        })
    }
}

/// The adapter error for a watcher on `root` that has stopped delivering events.
fn closed(root: &Path) -> Error {
    failure(format!(
        "the watch on {} stopped delivering events",
        root.display()
    ))
}

/// Whether `kind` reports a change. An access event is a read, which inotify raises on every
/// open, so only a file closed after a write counts.
const fn is_change(kind: EventKind) -> bool {
    match kind {
        EventKind::Access(AccessKind::Close(AccessMode::Write)) => true,
        EventKind::Access(_) => false,
        _ => true,
    }
}

/// Whether `path` was last changed before `started`, by its status-change time.
///
/// The change time, unlike the modification time, cannot be set back by `cp -p` or `tar`. It is
/// compared at full precision, and it is taken to be on the same clock as `started`, which holds
/// on a local volume. A path that cannot be examined, because it was removed or renamed away, is
/// kept; a change time before 1970 is before `started`, so its path is dropped.
#[cfg(target_os = "macos")]
fn changed_before(path: &Path, started: Timestamp) -> bool {
    use std::os::unix::fs::MetadataExt;

    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return false;
    };
    let Ok(secs) = u64::try_from(metadata.ctime()) else {
        return true;
    };
    let nanos = u64::try_from(metadata.ctime_nsec()).unwrap_or(0);
    let Some(changed) = secs
        .checked_mul(1_000_000_000)
        .and_then(|whole| whole.checked_add(nanos))
    else {
        return false;
    };
    Timestamp::from_unix_nanos(changed) < started
}

/// Whether `path` was last changed before `started`; always `false` where the file events are
/// not replayed from before the watch began.
#[cfg(not(target_os = "macos"))]
const fn changed_before(_path: &Path, _started: Timestamp) -> bool {
    false
}

/// Adds the kept paths of `delivery` to `kept`, or fails when the watcher reported an error or
/// said the operating system dropped events.
fn take(
    root: &Path,
    started: Timestamp,
    delivery: Delivery,
    kept: &mut BTreeSet<ChangedPath>,
) -> Result<()> {
    let event = delivery.map_err(|source| watch_failure(root, &source))?;
    if event.need_rescan() {
        return Err(failure(format!(
            "the operating system dropped file events under {}; changes may have been missed",
            root.display()
        )));
    }
    if !is_change(event.kind) {
        return Ok(());
    }
    kept.extend(
        event
            .paths
            .iter()
            .filter(|path| !changed_before(path, started))
            .filter_map(|path| relative(root, path)),
    );
    Ok(())
}

/// Waits on `events` for a first kept path, then collects what arrives until the events have
/// been quiet for [`WINDOW`], and returns it as one batch.
fn gather(root: &Path, events: &Receiver<Delivery>, started: Timestamp) -> Result<ChangeBatch> {
    let mut kept = BTreeSet::new();
    while kept.is_empty() {
        let delivery = events.recv().map_err(|_| closed(root))?;
        take(root, started, delivery, &mut kept)?;
    }
    while let Ok(delivery) = events.recv_timeout(WINDOW) {
        take(root, started, delivery, &mut kept)?;
    }
    ChangeBatch::new(kept).ok_or_else(|| closed(root))
}

impl Changes for NotifyChanges {
    fn next(&mut self) -> Result<ChangeBatch> {
        gather(&self.root, &self.events, self.started)
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::fs;
    use std::path::Path;
    use std::sync::mpsc::{self, Receiver};
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use buildl_core::{ChangeBatch, Changes, Error, PortName, Result, Timestamp};
    use notify::event::{AccessKind, AccessMode, Flag};
    use notify::{Event, EventKind};

    use super::{Delivery, NotifyChanges, gather, relative};

    /// How long a test waits for a batch before it calls the event lost.
    const BOUND: Duration = Duration::from_secs(10);

    /// What a helper thread running `next` hands back: the watcher and the result.
    type Pending = Receiver<(NotifyChanges, Result<ChangeBatch>)>;

    /// Starts `next` on a helper thread and returns where its result will arrive.
    fn start_next(mut changes: NotifyChanges) -> Pending {
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let result = changes.next();
            let _ = sender.send((changes, result));
        });
        receiver
    }

    /// Waits for `pending`, so a lost event fails the test after `BOUND` instead of hanging it.
    fn finish(pending: &Pending) -> (NotifyChanges, Result<ChangeBatch>) {
        pending.recv_timeout(BOUND).unwrap()
    }

    /// Runs `next` on a helper thread and hands the watcher back with its result.
    fn next_within(changes: NotifyChanges) -> (NotifyChanges, Result<ChangeBatch>) {
        finish(&start_next(changes))
    }

    /// An event for `path`.
    fn event_at(path: &str) -> Event {
        Event::new(EventKind::Any).add_path(path.into())
    }

    /// An event of `kind` for `path`.
    fn event_of(kind: EventKind, path: &str) -> Event {
        Event::new(kind).add_path(path.into())
    }

    /// Whether `error` is the Changes adapter error with `needle` in its text.
    fn is_changes_error(error: &Error, needle: &str) -> bool {
        matches!(
            error,
            Error::Adapter {
                port: PortName::Changes,
                diagnostic,
            } if diagnostic.to_string().contains(needle)
        )
    }

    /// The paths of `batch` as text.
    fn names(batch: &ChangeBatch) -> Vec<&str> {
        batch.iter().map(buildl_core::ChangedPath::as_str).collect()
    }

    #[test]
    fn relative_strips_the_root() {
        let path = relative(Path::new("/r"), Path::new("/r/lib/a.c")).unwrap();
        assert_eq!(path.as_str(), "lib/a.c");
    }

    #[test]
    fn relative_drops_a_path_outside_the_root() {
        assert!(relative(Path::new("/r"), Path::new("/elsewhere/a.c")).is_none());
    }

    #[test]
    fn relative_drops_the_root_itself() {
        assert!(relative(Path::new("/r"), Path::new("/r")).is_none());
    }

    #[test]
    fn a_write_yields_its_relative_path() {
        let dir = tempfile::tempdir().unwrap();
        let changes = NotifyChanges::start(dir.path(), now()).unwrap();
        fs::write(dir.path().join("a.c"), "x").unwrap();
        let (_changes, batch) = next_within(changes);
        assert!(names(&batch.unwrap()).contains(&"a.c"));
    }

    #[test]
    fn a_nested_write_is_slash_separated() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("lib/words")).unwrap();
        let changes = NotifyChanges::start(dir.path(), now()).unwrap();
        fs::write(dir.path().join("lib/words/a.txt"), "x").unwrap();
        let (_changes, batch) = next_within(changes);
        assert!(names(&batch.unwrap()).contains(&"lib/words/a.txt"));
    }

    #[test]
    fn a_burst_is_one_batch() {
        let dir = tempfile::tempdir().unwrap();
        let changes = NotifyChanges::start(dir.path(), now()).unwrap();
        let pending = start_next(changes);
        let wanted = ["a.c", "b.c", "c.c", "d.c", "e.c"];
        for name in wanted {
            fs::write(dir.path().join(name), "x").unwrap();
        }
        let (_changes, batch) = finish(&pending);
        let batch = batch.unwrap();
        let seen = names(&batch);
        for name in wanted {
            assert!(seen.contains(&name), "{name} missing from {seen:?}");
        }
    }

    #[test]
    fn writes_while_nobody_asks_drain_into_one_batch() {
        let dir = tempfile::tempdir().unwrap();
        let changes = NotifyChanges::start(dir.path(), now()).unwrap();
        let wanted = ["x.c", "y.c", "z.c"];
        for name in wanted {
            fs::write(dir.path().join(name), "x").unwrap();
        }
        let (_changes, batch) = next_within(changes);
        let batch = batch.unwrap();
        let seen = names(&batch);
        for name in wanted {
            assert!(seen.contains(&name), "{name} missing from {seen:?}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_root_given_through_a_link_still_relativizes() {
        let dir = tempfile::tempdir().unwrap();
        let links = tempfile::tempdir().unwrap();
        let link = links.path().join("root");
        std::os::unix::fs::symlink(dir.path(), &link).unwrap();
        let changes = NotifyChanges::start(&link, now()).unwrap();
        fs::write(link.join("a.c"), "x").unwrap();
        let (_changes, batch) = next_within(changes);
        assert!(names(&batch.unwrap()).contains(&"a.c"));
    }

    #[test]
    fn out_and_state_writes_are_still_reported() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("out")).unwrap();
        fs::create_dir_all(dir.path().join(".buildl")).unwrap();
        let changes = NotifyChanges::start(dir.path(), now()).unwrap();
        fs::write(dir.path().join("out/t"), "x").unwrap();
        fs::write(dir.path().join(".buildl/s"), "x").unwrap();
        let (_changes, batch) = next_within(changes);
        let batch = batch.unwrap();
        let seen = names(&batch);
        assert!(seen.contains(&"out/t"), "{seen:?}");
        assert!(seen.contains(&".buildl/s"), "{seen:?}");
    }

    #[test]
    fn gather_keeps_waiting_after_a_batch_that_drops_to_nothing() {
        let (sender, events) = mpsc::channel();
        sender.send(Ok(event_at("/elsewhere/a.c"))).unwrap();
        sender.send(Ok(event_at("/r/kept.c"))).unwrap();
        let batch = gather(Path::new("/r"), &events, Timestamp::UNIX_EPOCH).unwrap();
        assert_eq!(names(&batch), ["kept.c"]);
    }

    /// Long enough for a status-change time to differ from the instants taken around it.
    #[cfg(target_os = "macos")]
    const TICK: Duration = Duration::from_millis(20);

    /// The instant a caller would hand to `NotifyChanges::start`.
    fn now() -> Timestamp {
        let since = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        Timestamp::from_unix_nanos(u64::try_from(since.as_nanos()).unwrap())
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn gather_drops_a_path_changed_before_the_watch_began() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("old.c"), "x").unwrap();
        thread::sleep(TICK);
        let started = now();
        thread::sleep(TICK);
        fs::write(dir.path().join("new.c"), "x").unwrap();
        let (sender, events) = mpsc::channel();
        sender
            .send(Ok(event_at(dir.path().join("old.c").to_str().unwrap())))
            .unwrap();
        sender
            .send(Ok(event_at(dir.path().join("new.c").to_str().unwrap())))
            .unwrap();
        let batch = gather(dir.path(), &events, started).unwrap();
        assert_eq!(names(&batch), ["new.c"]);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn gather_keeps_a_path_changed_after_the_watch_began() {
        let dir = tempfile::tempdir().unwrap();
        let started = now();
        thread::sleep(TICK);
        fs::write(dir.path().join("a.c"), "x").unwrap();
        let (sender, events) = mpsc::channel();
        sender
            .send(Ok(event_at(dir.path().join("a.c").to_str().unwrap())))
            .unwrap();
        let batch = gather(dir.path(), &events, started).unwrap();
        assert_eq!(names(&batch), ["a.c"]);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn gather_keeps_a_removed_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gone.c");
        fs::write(&path, "x").unwrap();
        fs::remove_file(&path).unwrap();
        thread::sleep(TICK);
        let started = now();
        let (sender, events) = mpsc::channel();
        sender.send(Ok(event_at(path.to_str().unwrap()))).unwrap();
        let batch = gather(dir.path(), &events, started).unwrap();
        assert_eq!(names(&batch), ["gone.c"]);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn gather_keeps_a_path_written_after_the_watch_whose_mtime_is_set_back() {
        let dir = tempfile::tempdir().unwrap();
        let started = now();
        thread::sleep(TICK);
        let path = dir.path().join("a.c");
        fs::write(&path, "x").unwrap();
        let file = fs::File::options().write(true).open(&path).unwrap();
        file.set_modified(UNIX_EPOCH + Duration::from_secs(3600))
            .unwrap();
        let (sender, events) = mpsc::channel();
        sender.send(Ok(event_at(path.to_str().unwrap()))).unwrap();
        let batch = gather(dir.path(), &events, started).unwrap();
        assert_eq!(names(&batch), ["a.c"]);
    }

    #[test]
    fn gather_collects_every_event_of_a_burst() {
        let (sender, events) = mpsc::channel();
        sender.send(Ok(event_at("/r/a.c"))).unwrap();
        sender.send(Ok(event_at("/r/b.c"))).unwrap();
        let batch = gather(Path::new("/r"), &events, Timestamp::UNIX_EPOCH).unwrap();
        assert_eq!(names(&batch), ["a.c", "b.c"]);
    }

    #[test]
    fn gather_drops_an_open() {
        let (sender, events) = mpsc::channel();
        let open = EventKind::Access(AccessKind::Open(AccessMode::Any));
        sender.send(Ok(event_of(open, "/r/build.lua"))).unwrap();
        drop(sender);
        let error = gather(Path::new("/r"), &events, Timestamp::UNIX_EPOCH).unwrap_err();
        assert!(is_changes_error(&error, "stopped delivering"), "{error:?}");

        let (sender, events) = mpsc::channel();
        sender.send(Ok(event_of(open, "/r/build.lua"))).unwrap();
        sender.send(Ok(event_at("/r/kept.c"))).unwrap();
        let batch = gather(Path::new("/r"), &events, Timestamp::UNIX_EPOCH).unwrap();
        assert_eq!(names(&batch), ["kept.c"]);
    }

    #[test]
    fn gather_keeps_a_closed_write() {
        let (sender, events) = mpsc::channel();
        let closed = EventKind::Access(AccessKind::Close(AccessMode::Write));
        sender.send(Ok(event_of(closed, "/r/a.c"))).unwrap();
        let batch = gather(Path::new("/r"), &events, Timestamp::UNIX_EPOCH).unwrap();
        assert_eq!(names(&batch), ["a.c"]);
    }

    #[test]
    fn gather_fails_on_a_watcher_error() {
        let (sender, events) = mpsc::channel();
        sender.send(Err(notify::Error::generic("boom"))).unwrap();
        let error = gather(Path::new("/r"), &events, Timestamp::UNIX_EPOCH).unwrap_err();
        assert!(is_changes_error(&error, "boom"), "{error:?}");
    }

    #[test]
    fn gather_fails_when_the_channel_is_closed() {
        let (sender, events) = mpsc::channel::<Delivery>();
        drop(sender);
        let error = gather(Path::new("/r"), &events, Timestamp::UNIX_EPOCH).unwrap_err();
        assert!(is_changes_error(&error, "/r"), "{error:?}");
    }

    #[test]
    fn gather_fails_when_the_system_dropped_events() {
        let (sender, events) = mpsc::channel();
        sender
            .send(Ok(Event::new(EventKind::Other).set_flag(Flag::Rescan)))
            .unwrap();
        drop(sender);
        let error = gather(Path::new("/r"), &events, Timestamp::UNIX_EPOCH).unwrap_err();
        assert!(is_changes_error(&error, "dropped file events"), "{error:?}");
        assert!(is_changes_error(&error, "/r"), "{error:?}");
    }

    #[test]
    fn start_on_a_missing_root_is_a_changes_error() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("nope");
        let error = NotifyChanges::start(&missing, now()).unwrap_err();
        assert!(
            matches!(
                &error,
                Error::Adapter {
                    port: PortName::Changes,
                    diagnostic,
                } if diagnostic.to_string().contains("nope")
            ),
            "{error:?}"
        );
    }
}
