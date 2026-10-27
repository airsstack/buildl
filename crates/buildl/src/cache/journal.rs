//! The action cache's journal: one line for each row recorded, or forgotten, since the snapshot
//! was written.
//!
//! Its own file because a row must be kept from the moment it is recorded, and writing the whole
//! cache again for each row costs time that grows with the square of the number of rows. One
//! appended line costs the same however many rows there are. A line holds a row, or a `null`
//! where the row of its target was forgotten.
//!
//! Responsibilities: [`Journal`], and [`Entry`], the line it holds.
//!
//! Non-responsibilities: the rows in memory and the snapshot, which the cache keeps.

use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use buildl_core::json::canonical;
use buildl_core::{CacheRow, Diagnostic, Error, Label, PortName, Result};
use serde::{Deserialize, Serialize};

/// One line of the journal: a target, and the row recorded for it or the forgetting of its row.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Entry {
    /// The target.
    pub(crate) label: Label,
    /// The row recorded for it; `None` when its row was forgotten.
    pub(crate) row: Option<CacheRow>,
}

/// The file entries are appended to, one canonical line each: a row recorded, or a row forgotten.
#[derive(Debug)]
pub(crate) struct Journal {
    path: PathBuf,
    file: Option<File>,
    broken: bool,
}

/// The journal's failure doing `what` to `path`.
fn failed(what: &str, path: &Path, error: &io::Error) -> Error {
    refused(format!("{what} {}: {error}", path.display()))
}

/// A failure of the journal, worded by `message`.
fn refused(message: String) -> Error {
    Error::Adapter {
        port: PortName::ActionCache,
        diagnostic: Diagnostic::new(message),
    }
}

impl Journal {
    /// A journal at `path`. Touches nothing on disk: the file is made by the first entry.
    pub(crate) const fn new(path: PathBuf) -> Self {
        Self {
            path,
            file: None,
            broken: false,
        }
    }

    /// Where the journal's file is.
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// Every whole line of the file at `path`, in order. A file that is not there holds none.
    ///
    /// A last line with no newline is left out: the append that wrote it did not finish, so
    /// the row was never acknowledged.
    ///
    /// # Errors
    ///
    /// Returns [`Error::CorruptState`], naming the file, when a whole line is not what the
    /// canonical serializer writes for an entry, and [`Error::Adapter`], naming
    /// [`PortName::ActionCache`], when the file is there and cannot be read.
    pub(crate) fn read(path: &Path) -> Result<Vec<Entry>> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(failed("reading", path, &error)),
        };
        bytes
            .split_inclusive(|byte| *byte == b'\n')
            .filter_map(|line| line.strip_suffix(b"\n"))
            .map(|line| {
                canonical::from_slice(line).map_err(|source| Error::CorruptState {
                    file: path.to_path_buf(),
                    source: Box::new(source),
                })
            })
            .collect()
    }

    /// The file, opened for appending on first use.
    fn opened(&mut self) -> io::Result<&mut File> {
        let file = match self.file.take() {
            Some(file) => file,
            None => OpenOptions::new()
                .append(true)
                .create(true)
                .mode(0o644)
                .open(&self.path)?,
        };
        Ok(self.file.insert(file))
    }

    /// Appends `entry` as one line, handed to the system in one write.
    ///
    /// After an append that failed the journal takes no more entries: part of a line may be in
    /// the file, and a line written after it would join it.
    pub(crate) fn append(&mut self, entry: &Entry) -> Result<()> {
        if self.broken {
            return Err(refused(format!(
                "{} took part of a line, and takes no more",
                self.path.display()
            )));
        }
        let mut line = canonical::to_vec(entry)?;
        line.push(b'\n');
        let appended = self.opened().and_then(|file| file.write_all(&line));
        appended.map_err(|error| {
            self.broken = true;
            failed("appending to", &self.path, &error)
        })
    }

    /// Removes the file. One that is not there is not an error.
    pub(crate) fn remove(&mut self) -> io::Result<()> {
        self.file = None;
        match fs::remove_file(&self.path) {
            Err(error) if error.kind() != ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeMap;
    use std::fs::{self, File};

    use buildl_core::{
        ActionKey, Argument, CacheRow, Command, Digest, Error, KeyComponents, Label, NetworkAccess,
        OutputName, OutputSource, OwnComponents, Platform, PortName,
    };

    use super::{Entry, Journal};

    /// The line the journal holds for the entry of the target `a`.
    const LINE_A: &str = concat!(
        r#"{"label":"//:a","row":{"key":{"components":{"deps":[],"own":{"command":["cc"],"#,
        r#""env":{},"inputs":[],"network":"Sealed","output_source":"Command","outputs":["a"],"platform":"linux/x86_64","#,
        r#""settings":{},"tool":null}},"#,
        r#""digest":"06ff22740d6f5f789d0a8f80e0ff1cc7a5177a3747ab772ac1cb4a409b6db82f"},"#,
        r#""outputs":["ca978112ca1bbdcafac231b39a23dc4da786eff8147c4e72b9807785afee48bb"],"#,
        r#""stderr":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855","#,
        r#""stdout":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"}}"#,
        "\n"
    );

    /// The entry of a target named `name` at the workspace root, which ran `cc` and produced
    /// one output of that name.
    fn entry(name: &str) -> Entry {
        let key = ActionKey::new(KeyComponents {
            own: OwnComponents {
                command: Command::new(vec![Argument::parse("cc").unwrap()]).unwrap(),
                inputs: Vec::new(),
                outputs: vec![OutputName::parse(name).unwrap()],
                env: BTreeMap::new(),
                tool: None,
                settings: BTreeMap::new(),
                network: NetworkAccess::Sealed,
                output_source: OutputSource::Command,
                platform: Platform::parse("linux/x86_64").unwrap(),
            },
            deps: Vec::new(),
        })
        .unwrap();
        let nothing = Digest::of(b"");
        Entry {
            label: Label::parse(&format!("//:{name}")).unwrap(),
            row: Some(
                CacheRow::new(key, vec![Digest::of(name.as_bytes())], nothing, nothing).unwrap(),
            ),
        }
    }

    #[test]
    fn nothing_is_on_disk_before_the_first_entry() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.journal");

        let journal = Journal::new(path.clone());

        assert_eq!(journal.path(), path);
        assert!(!path.exists());
    }

    #[test]
    fn an_entry_is_appended_as_one_canonical_line() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.journal");
        let mut journal = Journal::new(path.clone());

        journal.append(&entry("a")).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), LINE_A);

        journal.append(&entry("a")).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), LINE_A.repeat(2));
    }

    #[test]
    fn a_write_that_fails_closes_the_journal_for_good() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.journal");
        let mut journal = Journal::new(path.clone());
        journal.append(&entry("a")).unwrap();

        // A handle that cannot be written to stands in for a disk that cannot.
        journal.file = Some(File::open(&path).unwrap());
        let error = journal.append(&entry("b")).unwrap_err();
        let Error::Adapter { port, diagnostic } = error else {
            unreachable!("an adapter error, got {error:?}")
        };
        assert_eq!(port, PortName::ActionCache);
        assert!(
            diagnostic
                .as_str()
                .starts_with(&format!("appending to {}: ", path.display())),
            "{diagnostic}"
        );

        // The next append would open the file afresh and could write; it is refused all the same.
        journal.file = None;
        let error = journal.append(&entry("c")).unwrap_err();
        assert_eq!(
            error.to_string(),
            format!(
                "action cache: {} took part of a line, and takes no more",
                path.display()
            )
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), LINE_A);
    }

    #[test]
    fn remove_deletes_the_file_and_one_not_there_is_no_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.journal");
        let mut journal = Journal::new(path.clone());

        journal.remove().unwrap();

        journal.append(&entry("b")).unwrap();
        journal.remove().unwrap();
        assert!(!path.exists());

        // An entry appended afterwards starts a new file.
        journal.append(&entry("a")).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), LINE_A);
    }

    /// The label of each entry, in order.
    fn labels(entries: &[Entry]) -> Vec<String> {
        entries
            .iter()
            .map(|entry| entry.label.to_string())
            .collect()
    }

    #[test]
    fn a_file_that_is_not_there_holds_no_line() {
        let dir = tempfile::tempdir().unwrap();

        let entries = Journal::read(&dir.path().join("cache.journal")).unwrap();

        assert_eq!(labels(&entries), [] as [&str; 0]);
    }

    #[test]
    fn every_whole_line_is_read_in_order_and_a_cut_last_one_is_left_out() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.journal");
        let mut journal = Journal::new(path.clone());
        for name in ["b", "a", "b"] {
            journal.append(&entry(name)).unwrap();
        }

        let entries = Journal::read(&path).unwrap();
        assert_eq!(labels(&entries), ["//:b", "//:a", "//:b"]);
        assert_eq!(entries[1].row, entry("a").row);

        // A line with no newline was never acknowledged: whether it is cut short or whole.
        let whole = fs::read_to_string(&path).unwrap();
        for cut in [&LINE_A[..40], LINE_A.trim_end()] {
            fs::write(&path, format!("{whole}{cut}")).unwrap();
            assert_eq!(
                labels(&Journal::read(&path).unwrap()),
                ["//:b", "//:a", "//:b"]
            );
        }
        fs::write(&path, LINE_A.trim_end()).unwrap();
        assert_eq!(labels(&Journal::read(&path).unwrap()), [] as [&str; 0]);
    }

    #[test]
    fn a_whole_line_that_is_not_canonical_is_refused_naming_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.journal");
        let line = LINE_A.trim_end();

        // A space before the newline, a line that is empty, and a key nothing declares.
        for damaged in [
            format!("{LINE_A}{line} \n"),
            format!("{LINE_A}\n"),
            format!(
                "{}\n",
                line.replacen(r#"{"label""#, r#"{"extra":1,"label""#, 1)
            ),
        ] {
            fs::write(&path, &damaged).unwrap();

            let error = Journal::read(&path).unwrap_err();

            let Error::CorruptState { file, source } = error else {
                unreachable!("a corrupt state error, got {error:?}")
            };
            assert_eq!(file, path);
            assert!(
                matches!(
                    *source,
                    Error::UnreadableJson { .. } | Error::NonCanonicalJson { .. }
                ),
                "{source:?}"
            );
            assert_eq!(fs::read_to_string(&path).unwrap(), damaged);
        }
    }

    #[test]
    fn a_forget_line_is_canonical() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.journal");
        let mut journal = Journal::new(path.clone());

        journal
            .append(&Entry {
                label: Label::parse("//:a").unwrap(),
                row: None,
            })
            .unwrap();

        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "{\"label\":\"//:a\",\"row\":null}\n"
        );
        let entries = Journal::read(&path).unwrap();
        assert_eq!(labels(&entries), ["//:a"]);
        assert_eq!(entries[0].row, None);
    }

    #[test]
    fn a_forget_line_out_of_form_is_corrupt() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.journal");

        // A line with no `row` at all, and one whose keys are out of order.
        for damaged in [
            "{\"label\":\"//:a\"}\n",
            "{\"row\":null,\"label\":\"//:a\"}\n",
        ] {
            fs::write(&path, damaged).unwrap();

            let error = Journal::read(&path).unwrap_err();

            let Error::CorruptState { file, .. } = error else {
                unreachable!("a corrupt state error, got {error:?}")
            };
            assert_eq!(file, path);
        }
    }

    #[test]
    fn a_journal_that_cannot_be_read_is_an_adapter_error() {
        let dir = tempfile::tempdir().unwrap();

        let error = Journal::read(dir.path()).unwrap_err();

        let Error::Adapter { port, diagnostic } = error else {
            unreachable!("an adapter error, got {error:?}")
        };
        assert_eq!(port, PortName::ActionCache);
        assert!(
            diagnostic
                .as_str()
                .starts_with(&format!("reading {}: ", dir.path().display())),
            "{diagnostic}"
        );
    }
}
