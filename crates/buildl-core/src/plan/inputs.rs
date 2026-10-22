//! Input digests: which recorded digests still stand, and which files must be hashed again.
//!
//! Its own file because hashing is what a warm plan must avoid: a file whose metadata has not
//! changed since it was last hashed keeps its recorded digest, and only the others are read.
//!
//! Responsibilities: [`digests`], and the decisions it is made of: [`Read`], [`reusable`],
//! [`split`], [`Split`] and [`hashed`].
//!
//! Non-responsibilities: reading metadata, hashing, and storing what was hashed. All three
//! arrive through ports; the decisions here are over the values those ports returned.

use std::collections::BTreeMap;

use crate::error::{DeclarationSite, Error, PortName, Result};
use crate::ports::{Digester, StatCache};
use crate::types::{Diagnostic, Digest, FileStat, InputStat, Node, SourcePath, StatEntry};

/// The content digest of every input in `inputs`.
///
/// Each input's metadata is read first, in path order. An input whose metadata equals what was
/// recorded for it keeps the recorded digest; the others are hashed in one call, in path order,
/// and recorded. The stat cache is committed once when anything was recorded and not at all
/// otherwise, so a plan that hashes nothing writes nothing.
///
/// # Errors
///
/// Returns, for the first input in path order that is not a regular file, naming the
/// lowest-labelled target declaring it, [`Error::MissingInput`] when nothing is there and
/// [`Error::InputNotAFile`] when something else is; [`Error::Adapter`] when the digester's reply does not
/// hold one digest per file; and whatever either port returns.
pub(crate) fn digests<'g, D: Digester, S: StatCache>(
    inputs: &BTreeMap<&'g SourcePath, &'g Node>,
    digester: &D,
    stat_cache: &mut S,
) -> Result<BTreeMap<&'g SourcePath, Digest>> {
    let mut read = Vec::with_capacity(inputs.len());
    for (path, node) in inputs {
        let stat = match digester.stat(path)? {
            InputStat::File(stat) => stat,
            InputStat::Missing => {
                return Err(Error::MissingInput {
                    site: Box::new(DeclarationSite::from(*node)),
                    path: (*path).clone(),
                });
            }
            InputStat::Other(found) => {
                return Err(Error::InputNotAFile {
                    site: Box::new(DeclarationSite::from(*node)),
                    path: (*path).clone(),
                    found,
                });
            }
        };
        read.push(Read {
            path,
            stat,
            recorded: stat_cache.entry(path),
        });
    }

    let Split { reused, pending } = split(&read);
    let mut found: BTreeMap<&SourcePath, Digest> = reused.into_iter().collect();
    if pending.is_empty() {
        return Ok(found);
    }
    let paths: Vec<SourcePath> = pending.iter().map(|(path, _)| (*path).clone()).collect();
    let reply = digester.digest(&paths)?;
    for (path, entry) in hashed(&pending, &reply)? {
        stat_cache.record(path.clone(), entry);
        found.insert(path, entry.digest);
    }
    stat_cache.commit()?;
    Ok(found)
}

/// One input once its metadata has been read.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Read<'g> {
    /// The file.
    pub(crate) path: &'g SourcePath,
    /// Its metadata now.
    pub(crate) stat: FileStat,
    /// What was recorded for it when it was last hashed, if anything.
    pub(crate) recorded: Option<StatEntry>,
}

/// Inputs sorted by whether they must be hashed, each list in the order the inputs were given.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Split<'g> {
    /// The inputs whose recorded digest stands, with that digest.
    pub(crate) reused: Vec<(&'g SourcePath, Digest)>,
    /// The inputs to hash, with the metadata each will be recorded under.
    pub(crate) pending: Vec<(&'g SourcePath, FileStat)>,
}

/// The digest recorded for a file, when its metadata is still what it was recorded under.
///
/// The two are compared for equality: a modification time earlier than the recorded one is a
/// change like any other.
pub(crate) fn reusable(recorded: Option<StatEntry>, stat: FileStat) -> Option<Digest> {
    recorded
        .filter(|entry| entry.stat == stat)
        .map(|entry| entry.digest)
}

/// Sorts `read` into the inputs that keep their recorded digest and the inputs to hash.
pub(crate) fn split<'g>(read: &[Read<'g>]) -> Split<'g> {
    let mut reused = Vec::new();
    let mut pending = Vec::new();
    for input in read {
        match reusable(input.recorded, input.stat) {
            Some(digest) => reused.push((input.path, digest)),
            None => pending.push((input.path, input.stat)),
        }
    }
    Split { reused, pending }
}

/// Pairs each input that was sent to be hashed with its digest from `reply`, as the entry to
/// record for it.
///
/// # Errors
///
/// Returns [`Error::Adapter`], naming [`PortName::Digester`], when `reply` does not hold exactly
/// one digest per input.
pub(crate) fn hashed<'g>(
    pending: &[(&'g SourcePath, FileStat)],
    reply: &[Digest],
) -> Result<Vec<(&'g SourcePath, StatEntry)>> {
    if reply.len() != pending.len() {
        return Err(Error::Adapter {
            port: PortName::Digester,
            diagnostic: Diagnostic::new(format!(
                "{} digests were returned for {} files",
                reply.len(),
                pending.len()
            )),
        });
    }
    Ok(pending
        .iter()
        .zip(reply)
        .map(|((path, stat), digest)| {
            (
                *path,
                StatEntry {
                    stat: *stat,
                    digest: *digest,
                },
            )
        })
        .collect())
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use super::{Read, Split, hashed, reusable, split};
    use crate::error::{Error, PortName};
    use crate::grants::fixtures::path;
    use crate::types::{Digest, FileStat, StatEntry, Timestamp};

    fn stat(modified: u64, size: u64) -> FileStat {
        FileStat {
            modified: Timestamp::from_unix_nanos(modified),
            size,
        }
    }

    fn entry(modified: u64, size: u64, content: &str) -> StatEntry {
        StatEntry {
            stat: stat(modified, size),
            digest: Digest::of(content.as_bytes()),
        }
    }

    #[test]
    fn a_recorded_digest_stands_while_the_metadata_is_unchanged() {
        assert_eq!(
            reusable(Some(entry(100, 7, "main")), stat(100, 7)),
            Some(Digest::of(b"main"))
        );
    }

    #[test]
    fn a_file_never_hashed_has_no_digest_to_reuse() {
        assert_eq!(reusable(None, stat(100, 7)), None);
    }

    #[test]
    fn any_change_of_metadata_discards_the_recorded_digest() {
        let recorded = Some(entry(100, 7, "main"));
        assert_eq!(reusable(recorded, stat(101, 7)), None, "a later time");
        assert_eq!(reusable(recorded, stat(99, 7)), None, "an earlier time");
        assert_eq!(reusable(recorded, stat(100, 8)), None, "another size");
    }

    #[test]
    fn inputs_are_sorted_without_changing_their_order() {
        let a = path("src/a.c");
        let b = path("src/b.c");
        let c = path("src/c.c");
        let d = path("src/d.c");
        let read = [
            Read {
                path: &a,
                stat: stat(1, 10),
                recorded: None,
            },
            Read {
                path: &b,
                stat: stat(2, 20),
                recorded: Some(entry(2, 20, "b")),
            },
            Read {
                path: &c,
                stat: stat(3, 30),
                recorded: Some(entry(9, 30, "c")),
            },
            Read {
                path: &d,
                stat: stat(4, 40),
                recorded: Some(entry(4, 40, "d")),
            },
        ];
        assert_eq!(
            split(&read),
            Split {
                reused: vec![(&b, Digest::of(b"b")), (&d, Digest::of(b"d"))],
                pending: vec![(&a, stat(1, 10)), (&c, stat(3, 30))],
            }
        );
    }

    #[test]
    fn each_hashed_input_is_recorded_under_the_metadata_it_was_read_with() {
        let a = path("src/a.c");
        let c = path("src/c.c");
        let pending = [(&a, stat(1, 10)), (&c, stat(3, 30))];
        let reply = [Digest::of(b"a"), Digest::of(b"c")];
        assert_eq!(
            hashed(&pending, &reply).unwrap(),
            [(&a, entry(1, 10, "a")), (&c, entry(3, 30, "c"))]
        );
        assert_eq!(hashed(&[], &[]).unwrap(), []);
    }

    #[test]
    fn a_reply_of_the_wrong_length_is_the_digesters_failure() {
        let a = path("src/a.c");
        let c = path("src/c.c");
        let pending = [(&a, stat(1, 10)), (&c, stat(3, 30))];
        let short = [Digest::of(b"a")];
        let long = [Digest::of(b"a"), Digest::of(b"c"), Digest::of(b"x")];
        for (reply, text) in [
            (&short[..], "digester: 1 digests were returned for 2 files"),
            (&long[..], "digester: 3 digests were returned for 2 files"),
        ] {
            let error = hashed(&pending, reply).unwrap_err();
            assert_eq!(error.to_string(), text);
            match error {
                Error::Adapter { port, .. } => assert_eq!(port, PortName::Digester),
                other => unreachable!("expected Adapter, got {other:?}"),
            }
        }
    }
}
