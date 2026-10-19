//! [`LocalCache`], the action cache of a local workspace.
//!
//! Its own file because it holds the two ways rows are kept and the rule that joins them:
//! `cache.json` is the canonical snapshot of every row, the journal holds what was recorded
//! since the snapshot was written (rows recorded, and rows forgotten), and a fold writes the
//! snapshot again and removes the journal.
//!
//! Responsibilities: [`LocalCache`], its [`ActionCache`] implementation, and the fold.
//!
//! Non-responsibilities: the journal's file, which `Journal` keeps; and deciding when to fold,
//! which whoever ran the build does.

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use buildl_core::json::canonical;
use buildl_core::{ActionCache, CacheRow, Diagnostic, Error, Label, PortName, Result};

use crate::cache::journal::{Entry, Journal};
use crate::state::{Access, StateLayout, write_whole};

/// Holds the row each target's last successful build recorded, across runs, until it is
/// forgotten.
///
/// The rows are held in memory, read once from the snapshot `cache.json`. A row that is
/// recorded is appended to the journal before it is held, and a row that is forgotten is
/// appended to the journal as a forgetting before it is dropped, so each is kept from then on
/// whatever becomes of the process. [`LocalCache::fold`] writes every row to the snapshot and
/// removes the journal.
///
/// The snapshot is written from a sorted map by the canonical serializer, so the order rows
/// were recorded in never reaches it: the same rows always give the same bytes.
#[derive(Debug)]
pub struct LocalCache {
    rows: BTreeMap<Label, CacheRow>,
    snapshot: PathBuf,
    staging: PathBuf,
    journal: Journal,
    unfolded: bool,
}

/// The cache's failure doing `what` to `path`.
fn failed(what: &str, path: &Path, error: &io::Error) -> Error {
    Error::Adapter {
        port: PortName::ActionCache,
        diagnostic: Diagnostic::new(format!("{what} {}: {error}", path.display())),
    }
}

impl LocalCache {
    /// Reads `cache.json`, then folds in a journal an earlier run left. A workspace that has
    /// neither has no rows.
    ///
    /// A journal is there when the run that wrote it ended before its fold, or its fold failed.
    /// Each whole line is applied in order, as it was recorded or forgotten, and the cache is
    /// folded at once, so the snapshot is whole and the journal gone before anything is planned.
    /// A last line with no newline is left out.
    ///
    /// # Errors
    ///
    /// Returns [`Error::CorruptState`], naming the file, when `cache.json` or a whole line of
    /// the journal does not hold what the canonical serializer writes there; both files are
    /// left as they are. Returns [`Error::Adapter`], naming [`PortName::ActionCache`], when a
    /// file that is there cannot be read, or the fold fails.
    pub fn open(layout: &StateLayout) -> Result<Self> {
        let snapshot = layout.cache();
        let mut rows: BTreeMap<Label, CacheRow> = match fs::read(&snapshot) {
            Ok(bytes) => canonical::from_slice(&bytes).map_err(|source| Error::CorruptState {
                file: snapshot.clone(),
                source: Box::new(source),
            })?,
            Err(error) if error.kind() == ErrorKind::NotFound => BTreeMap::new(),
            Err(error) => return Err(failed("reading", &snapshot, &error)),
        };
        let journal = layout.journal();
        for entry in Journal::read(&journal)? {
            match entry.row {
                Some(row) => rows.insert(entry.label, row),
                None => rows.remove(&entry.label),
            };
        }
        // A journal that holds no whole line is folded too: a cut line left in it would join
        // the next line appended.
        let unfolded = journal
            .try_exists()
            .map_err(|error| failed("examining", &journal, &error))?;
        let mut cache = Self {
            rows,
            snapshot,
            staging: layout.dir().to_path_buf(),
            journal: Journal::new(journal),
            unfolded,
        };
        cache.fold()?;
        Ok(cache)
    }

    /// Writes every row to `cache.json` and removes the journal. Does nothing when no row was
    /// recorded or forgotten since the last fold, so a build that changes no row leaves the
    /// snapshot as it was, bytes and modification time.
    ///
    /// The snapshot is written whole before the journal is removed. A run that ends between the
    /// two leaves both, and both hold the same rows.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Adapter`], naming [`PortName::ActionCache`], when the snapshot cannot be
    /// written or the journal cannot be removed. Every row recorded and every row forgotten
    /// is then still in the journal.
    pub fn fold(&mut self) -> Result<()> {
        if !self.unfolded {
            return Ok(());
        }
        let bytes = canonical::to_vec(&self.rows)?;
        write_whole(&self.staging, &self.snapshot, &bytes, Access::Rewritable)
            .map_err(|error| failed("writing", &self.snapshot, &error))?;
        self.journal
            .remove()
            .map_err(|error| failed("removing", self.journal.path(), &error))?;
        self.unfolded = false;
        Ok(())
    }
}

impl ActionCache for LocalCache {
    fn row(&self, label: &Label) -> Option<&CacheRow> {
        self.rows.get(label)
    }

    fn labels(&self) -> Vec<Label> {
        self.rows.keys().cloned().collect()
    }

    /// Appends the row to the journal as one line, then holds it.
    ///
    /// The row is kept once this returns `Ok`: its line has been handed to the system. After an
    /// append that failed, no later row is taken or forgotten by this cache.
    fn record(&mut self, label: Label, row: CacheRow) -> Result<()> {
        self.journal.append(&Entry {
            label: label.clone(),
            row: Some(row.clone()),
        })?;
        self.rows.insert(label, row);
        self.unfolded = true;
        Ok(())
    }

    /// Appends the forgetting of the row to the journal as one line, then drops the row.
    ///
    /// The removal is kept once this returns `Ok`: its line has been handed to the system. A
    /// target with no row appends nothing. After an append that failed, no later entry is taken
    /// by this cache.
    fn forget(&mut self, label: &Label) -> Result<()> {
        if !self.rows.contains_key(label) {
            return Ok(());
        }
        let entry = Entry {
            label: label.clone(),
            row: None,
        };
        self.journal.append(&entry)?;
        self.rows.remove(label);
        self.unfolded = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeMap;
    use std::fs::{self, File, OpenOptions};
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    use std::path::Path;
    use std::time::{Duration, UNIX_EPOCH};

    use buildl_core::{
        ActionCache, ActionKey, Argument, CacheRow, Command, Digest, Directory, Error,
        KeyComponents, Label, NetworkAccess, OutputName, OwnComponents, Platform, PortName,
    };

    use super::LocalCache;
    use crate::cache::journal::Journal;
    use crate::state::StateLayout;

    /// The layout of a new workspace whose state directory exists and is empty.
    fn workspace() -> (tempfile::TempDir, StateLayout) {
        let dir = tempfile::tempdir().unwrap();
        let layout = StateLayout::new(dir.path(), &Directory::parse(".buildl").unwrap());
        fs::create_dir_all(layout.dir()).unwrap();
        (dir, layout)
    }

    /// The label `//:<name>`.
    fn label(name: &str) -> Label {
        Label::parse(&format!("//:{name}")).unwrap()
    }

    /// The row of a target named `name` at the workspace root, which ran `cc` and produced one
    /// output of that name.
    fn row(name: &str) -> CacheRow {
        let key = ActionKey::new(KeyComponents {
            own: OwnComponents {
                command: Command::new(vec![Argument::parse("cc").unwrap()]).unwrap(),
                inputs: Vec::new(),
                outputs: vec![OutputName::parse(name).unwrap()],
                env: BTreeMap::new(),
                tool: None,
                settings: BTreeMap::new(),
                network: NetworkAccess::Sealed,
                platform: Platform::parse("linux/x86_64").unwrap(),
            },
            deps: Vec::new(),
        })
        .unwrap();
        let nothing = Digest::of(b"");
        CacheRow::new(key, vec![Digest::of(name.as_bytes())], nothing, nothing).unwrap()
    }

    /// The bytes the row of the target `name` is written as. `key` is the digest of its key and
    /// `output` the digest of its output, both as a run of this crate printed them.
    fn written(name: &str, key: &str, output: &str) -> String {
        [
            r#"{"key":{"components":{"deps":[],"own":{"command":["cc"],"env":{},"inputs":[],"#,
            r#""network":"Sealed","outputs":[""#,
            name,
            r#""],"platform":"linux/x86_64","settings":{},"tool":null}},"digest":""#,
            key,
            r#""},"outputs":[""#,
            output,
            r#""],"stderr":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855","#,
            r#""stdout":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"}"#,
        ]
        .concat()
    }

    /// The row of the target `a`, as it is written.
    fn written_a() -> String {
        written(
            "a",
            "c39a0161a4194c4ef0cedffc45bb82fdea72693ef4b6cc1f36177bed109be4d7",
            "ca978112ca1bbdcafac231b39a23dc4da786eff8147c4e72b9807785afee48bb",
        )
    }

    /// The row of the target `b`, as it is written.
    fn written_b() -> String {
        written(
            "b",
            "74b4485d5a6d40dc17ae98fd2b5dc4979cdf3bbd33288c24dbad18927a330ed8",
            "3e23e8160039594a33894f6564e1b1348bbd7a0088d42c4acb73eeaed59c009d",
        )
    }

    /// The row of the target `c`, as it is written.
    fn written_c() -> String {
        written(
            "c",
            "d110aa5671e75762b1cc66260c40a2309b8c6fb40944b8368a6e5cf7d351e2e9",
            "2e7d2c03a9507ae265ecf5b5356885a53393a2029d241394997265a1a25aefc6",
        )
    }

    /// The permission bits the umask lets through: those of a file made asking for all of them.
    fn allowed(dir: &Path) -> u32 {
        let path = dir.join("mask-probe");
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o777)
            .open(&path)
            .unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        fs::remove_file(&path).unwrap();
        mode
    }

    /// The names of the entries of the state directory, in order.
    fn entries(layout: &StateLayout) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(layout.dir())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn labels_lists_rows_after_open_record_and_forget() {
        let (_dir, layout) = workspace();
        let b = Label::parse("//b:x").unwrap();
        let a = Label::parse("//a:y").unwrap();
        let mut cache = LocalCache::open(&layout).unwrap();
        assert_eq!(cache.labels(), Vec::<Label>::new());

        cache.record(b.clone(), row("x")).unwrap();
        cache.record(a.clone(), row("y")).unwrap();
        assert_eq!(cache.labels(), [a.clone(), b.clone()]);

        cache.forget(&a).unwrap();
        assert_eq!(cache.labels(), std::slice::from_ref(&b));

        drop(cache);
        let reopened = LocalCache::open(&layout).unwrap();
        assert_eq!(reopened.labels(), [b]);
    }

    #[test]
    fn no_file_is_no_rows() {
        let (_dir, layout) = workspace();

        let cache = LocalCache::open(&layout).unwrap();

        assert_eq!(cache.row(&label("a")), None);
        assert_eq!(entries(&layout), [] as [&str; 0]);
    }

    #[test]
    fn a_recorded_row_is_returned() {
        let (_dir, layout) = workspace();
        let mut cache = LocalCache::open(&layout).unwrap();

        cache.record(label("a"), row("a")).unwrap();

        assert_eq!(cache.row(&label("a")), Some(&row("a")));
        assert_eq!(cache.row(&label("b")), None);

        // A second row for the same target replaces the first.
        cache.record(label("a"), row("b")).unwrap();
        assert_eq!(cache.row(&label("a")), Some(&row("b")));
    }

    #[test]
    fn fold_writes_canonical_bytes_and_removes_the_journal() {
        let (dir, layout) = workspace();
        let mut cache = LocalCache::open(&layout).unwrap();
        cache.record(label("a"), row("a")).unwrap();
        assert_eq!(entries(&layout), ["cache.journal"]);

        cache.fold().unwrap();

        assert_eq!(entries(&layout), ["cache.json"]);
        assert_eq!(
            fs::read_to_string(layout.cache()).unwrap(),
            format!(r#"{{"//:a":{}}}"#, written_a())
        );
        let mode = fs::metadata(layout.cache()).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o644 & allowed(dir.path()));

        // The snapshot is what the next run reads.
        let reopened = LocalCache::open(&layout).unwrap();
        assert_eq!(reopened.row(&label("a")), Some(&row("a")));
    }

    #[test]
    fn rows_recorded_in_any_order_fold_to_the_same_bytes() {
        let expected = format!(
            r#"{{"//:a":{},"//:b":{},"//:c":{}}}"#,
            written_a(),
            written_b(),
            written_c()
        );
        for order in [["a", "b", "c"], ["c", "a", "b"], ["b", "c", "a"]] {
            let (_dir, layout) = workspace();
            let mut cache = LocalCache::open(&layout).unwrap();
            for name in order {
                cache.record(label(name), row(name)).unwrap();
            }

            cache.fold().unwrap();

            assert_eq!(
                fs::read_to_string(layout.cache()).unwrap(),
                expected,
                "{order:?}"
            );
        }
    }

    #[test]
    fn a_fold_with_nothing_recorded_touches_nothing() {
        let (_dir, layout) = workspace();

        // With no snapshot, none is made.
        let mut cache = LocalCache::open(&layout).unwrap();
        cache.fold().unwrap();
        assert_eq!(entries(&layout), [] as [&str; 0]);

        // With a snapshot, it keeps its bytes and its modification time.
        cache.record(label("a"), row("a")).unwrap();
        cache.fold().unwrap();
        let long_ago = UNIX_EPOCH + Duration::from_secs(1_600_000_000);
        File::options()
            .write(true)
            .open(layout.cache())
            .unwrap()
            .set_modified(long_ago)
            .unwrap();
        let before = fs::read(layout.cache()).unwrap();

        cache.fold().unwrap();
        let mut reopened = LocalCache::open(&layout).unwrap();
        reopened.fold().unwrap();

        assert_eq!(fs::read(layout.cache()).unwrap(), before);
        assert_eq!(
            fs::metadata(layout.cache()).unwrap().modified().unwrap(),
            long_ago
        );
        assert_eq!(entries(&layout), ["cache.json"]);
    }

    #[test]
    fn a_cut_or_reordered_cache_json_is_refused_naming_the_file() {
        let whole = format!(r#"{{"//:a":{},"//:b":{}}}"#, written_a(), written_b());
        let cut = &whole[..whole.len() / 2];
        let reordered = format!(r#"{{"//:b":{},"//:a":{}}}"#, written_b(), written_a());
        let with_a_newline = format!("{whole}\n");

        for (damaged, unreadable) in [
            (cut, true),
            (reordered.as_str(), false),
            (with_a_newline.as_str(), false),
        ] {
            let (_dir, layout) = workspace();
            fs::write(layout.cache(), damaged).unwrap();

            let error = LocalCache::open(&layout).unwrap_err();

            assert!(
                error.to_string().starts_with(&format!(
                    "{} is not a file buildl wrote: ",
                    layout.cache().display()
                )),
                "{error}"
            );
            let Error::CorruptState { file, source } = error else {
                unreachable!("a corrupt state error, got {error:?}")
            };
            assert_eq!(file, layout.cache());
            if unreadable {
                assert!(
                    matches!(*source, Error::UnreadableJson { .. }),
                    "{source:?}"
                );
            } else {
                assert!(
                    matches!(*source, Error::NonCanonicalJson { .. }),
                    "{source:?}"
                );
            }
            assert_eq!(fs::read_to_string(layout.cache()).unwrap(), damaged);
        }

        // The bytes the damage was done to are themselves accepted.
        let (_dir, layout) = workspace();
        fs::write(layout.cache(), &whole).unwrap();
        let cache = LocalCache::open(&layout).unwrap();
        assert_eq!(cache.row(&label("b")), Some(&row("b")));
    }

    #[test]
    fn a_cache_json_that_cannot_be_read_is_an_adapter_error() {
        let (_dir, layout) = workspace();
        fs::create_dir(layout.cache()).unwrap();

        let error = LocalCache::open(&layout).unwrap_err();

        let Error::Adapter { port, diagnostic } = error else {
            unreachable!("an adapter error, got {error:?}")
        };
        assert_eq!(port, PortName::ActionCache);
        assert!(
            diagnostic
                .as_str()
                .starts_with(&format!("reading {}: ", layout.cache().display())),
            "{diagnostic}"
        );
    }

    /// The journal line of the target `name`, whose row is written as `row`, without its
    /// newline.
    fn line(name: &str, row: &str) -> String {
        format!(r#"{{"label":"//:{name}","row":{row}}}"#)
    }

    #[test]
    fn a_row_survives_a_cache_that_was_never_folded() {
        let (_dir, layout) = workspace();
        let mut interrupted = LocalCache::open(&layout).unwrap();
        interrupted.record(label("b"), row("a")).unwrap();
        interrupted.record(label("a"), row("a")).unwrap();
        interrupted.record(label("b"), row("b")).unwrap();
        drop(interrupted);
        assert_eq!(entries(&layout), ["cache.journal"]);

        let cache = LocalCache::open(&layout).unwrap();

        // Each line was applied in order, so the later row of `b` is the one held.
        assert_eq!(cache.row(&label("a")), Some(&row("a")));
        assert_eq!(cache.row(&label("b")), Some(&row("b")));
        assert_eq!(entries(&layout), ["cache.json"]);
        assert_eq!(
            fs::read_to_string(layout.cache()).unwrap(),
            format!(r#"{{"//:a":{},"//:b":{}}}"#, written_a(), written_b())
        );
    }

    #[test]
    fn a_last_line_with_no_newline_is_ignored() {
        let (_dir, layout) = workspace();
        // A whole line, then one that is whole but for the newline that ends it.
        let journal = format!("{}\n{}", line("a", &written_a()), line("b", &written_b()));
        fs::write(layout.journal(), journal).unwrap();

        let cache = LocalCache::open(&layout).unwrap();

        assert_eq!(cache.row(&label("a")), Some(&row("a")));
        assert_eq!(cache.row(&label("b")), None);
        assert_eq!(entries(&layout), ["cache.json"]);
        assert_eq!(
            fs::read_to_string(layout.cache()).unwrap(),
            format!(r#"{{"//:a":{}}}"#, written_a())
        );

        // A journal that holds nothing but a cut line is removed all the same, and the snapshot
        // of no rows is written.
        let (_dir, layout) = workspace();
        fs::write(layout.journal(), r#"{"label":"//:b","row":{"key""#).unwrap();
        let cache = LocalCache::open(&layout).unwrap();
        assert_eq!(cache.row(&label("b")), None);
        assert_eq!(entries(&layout), ["cache.json"]);
        assert_eq!(fs::read_to_string(layout.cache()).unwrap(), "{}");
    }

    #[test]
    fn a_damaged_journal_line_is_refused_naming_the_journal() {
        let (_dir, layout) = workspace();
        // The first line's key no longer has the digest written beside it.
        let damaged = line("a", &written_a()).replacen("c39a0161", "c39a0162", 1);
        let journal = format!("{damaged}\n{}\n", line("b", &written_b()));
        fs::write(layout.journal(), &journal).unwrap();

        let error = LocalCache::open(&layout).unwrap_err();

        assert!(
            error.to_string().starts_with(&format!(
                "{} is not a file buildl wrote: ",
                layout.journal().display()
            )),
            "{error}"
        );
        let Error::CorruptState { file, source } = error else {
            unreachable!("a corrupt state error, got {error:?}")
        };
        assert_eq!(file, layout.journal());
        assert!(
            matches!(*source, Error::UnreadableJson { .. }),
            "{source:?}"
        );
        assert_eq!(fs::read_to_string(layout.journal()).unwrap(), journal);
        assert_eq!(entries(&layout), ["cache.journal"]);
    }

    #[test]
    fn a_forgotten_row_is_gone_after_fold() {
        let (_dir, layout) = workspace();
        let mut cache = LocalCache::open(&layout).unwrap();
        cache.record(label("a"), row("a")).unwrap();
        cache.record(label("b"), row("b")).unwrap();

        cache.forget(&label("a")).unwrap();
        assert_eq!(cache.row(&label("a")), None);
        cache.fold().unwrap();

        assert_eq!(cache.row(&label("a")), None);
        assert_eq!(
            fs::read_to_string(layout.cache()).unwrap(),
            format!(r#"{{"//:b":{}}}"#, written_b())
        );
    }

    #[test]
    fn a_forgotten_row_is_gone_after_reopen_from_the_journal() {
        let (_dir, layout) = workspace();
        let mut first = LocalCache::open(&layout).unwrap();
        first.record(label("a"), row("a")).unwrap();
        first.fold().unwrap();
        first.record(label("b"), row("b")).unwrap();
        first.forget(&label("a")).unwrap();
        drop(first);
        assert_eq!(entries(&layout), ["cache.journal", "cache.json"]);

        let cache = LocalCache::open(&layout).unwrap();

        assert_eq!(cache.row(&label("a")), None);
        assert_eq!(cache.row(&label("b")), Some(&row("b")));
    }

    #[test]
    fn forgetting_a_label_with_no_row_writes_nothing() {
        let (_dir, layout) = workspace();
        let mut cache = LocalCache::open(&layout).unwrap();

        cache.forget(&label("a")).unwrap();
        cache.fold().unwrap();

        assert_eq!(entries(&layout), [] as [&str; 0]);
    }

    #[test]
    fn after_a_failed_append_no_row_is_taken_and_none_is_lost() {
        let (_dir, layout) = workspace();
        let mut cache = LocalCache::open(&layout).unwrap();
        cache.record(label("a"), row("a")).unwrap();

        // From here on the journal cannot be appended to: its path holds a directory.
        let blocked = layout.dir().join("blocked");
        fs::create_dir(&blocked).unwrap();
        cache.journal = Journal::new(blocked.clone());
        let error = cache.record(label("b"), row("b")).unwrap_err();
        assert!(
            matches!(
                error,
                Error::Adapter {
                    port: PortName::ActionCache,
                    ..
                }
            ),
            "{error:?}"
        );
        assert_eq!(cache.row(&label("b")), None);
        assert_eq!(cache.row(&label("a")), Some(&row("a")));

        // The path is free again, and still no row is taken.
        fs::remove_dir(&blocked).unwrap();
        cache.record(label("c"), row("c")).unwrap_err();
        assert_eq!(cache.row(&label("c")), None);
        assert!(!blocked.exists());
        drop(cache);

        // The row recorded before the failure is the one the next run finds.
        let reopened = LocalCache::open(&layout).unwrap();
        assert_eq!(reopened.row(&label("a")), Some(&row("a")));
        assert_eq!(reopened.row(&label("b")), None);
        assert_eq!(reopened.row(&label("c")), None);
        assert_eq!(
            fs::read_to_string(layout.cache()).unwrap(),
            format!(r#"{{"//:a":{}}}"#, written_a())
        );
    }

    #[test]
    fn after_a_failed_append_no_row_is_forgotten() {
        let (_dir, layout) = workspace();
        let mut cache = LocalCache::open(&layout).unwrap();
        cache.record(label("a"), row("a")).unwrap();

        // From here on the journal cannot be appended to: its path holds a directory.
        let blocked = layout.dir().join("blocked");
        fs::create_dir(&blocked).unwrap();
        cache.journal = Journal::new(blocked);
        let error = cache.forget(&label("a")).unwrap_err();
        assert!(
            matches!(
                error,
                Error::Adapter {
                    port: PortName::ActionCache,
                    ..
                }
            ),
            "{error:?}"
        );
        assert_eq!(cache.row(&label("a")), Some(&row("a")));
        drop(cache);

        // The row recorded before the failure is the one the next run finds.
        let reopened = LocalCache::open(&layout).unwrap();
        assert_eq!(reopened.row(&label("a")), Some(&row("a")));
    }
}
