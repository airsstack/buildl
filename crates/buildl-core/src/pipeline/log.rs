//! The log command: a target's last logged run, with what it printed.
//!
//! Its own file because it is a function over two ports, the build log's history and the content
//! store, that sits beside the [`Pipeline`](super::Pipeline) driver and takes no
//! [`DeclarationSource`](crate::ports::DeclarationSource): it reads no build file.
//!
//! Responsibilities: [`Shown`] and [`log`](fn@log).
//!
//! Non-responsibilities: choosing the run, which the `history` module's `last_run` does; and
//! writing anything out, which the caller does.

use crate::error::{Error, Result};
use crate::history::{LastRun, last_run};
use crate::ports::{ContentStore, RunHistory};
use crate::types::{Digest, Label};

/// A target's last run with what it printed, read back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    /// The run, with what the log said around it.
    pub last: LastRun,
    /// What the action printed on its output channel.
    pub stdout: Vec<u8>,
    /// What the action printed on its error channel.
    pub stderr: Vec<u8>,
}

/// Reads the build log through `history` and returns the newest run of `label` that stored its
/// output, with that output read from `cas`.
///
/// # Errors
///
/// Returns the history port's failure, [`Error::NoRun`] when the log has no run of `label` that
/// stored output, the content store's failure when it cannot read a digest, and
/// [`Error::MissingOutput`] when either digest of the run names content `cas` no longer holds.
pub fn log<H: RunHistory, C: ContentStore>(history: &H, cas: &C, label: &Label) -> Result<Shown> {
    let last = last_run(&history.lines()?, label)?;
    let stdout = read(cas, last.run.stdout.as_ref(), label)?;
    let stderr = read(cas, last.run.stderr.as_ref(), label)?;
    Ok(Shown {
        last,
        stdout,
        stderr,
    })
}

/// The content under `digest`, or [`Error::MissingOutput`] for `label` when there is no digest
/// or the store does not hold it.
fn read<C: ContentStore>(cas: &C, digest: Option<&Digest>, label: &Label) -> Result<Vec<u8>> {
    digest
        .map(|digest| cas.get(digest))
        .transpose()?
        .flatten()
        .ok_or_else(|| Error::MissingOutput {
            label: label.clone(),
        })
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use super::read;
    use crate::error::{Error, PortName, Result};
    use crate::ports::ContentStore;
    use crate::types::{Diagnostic, Digest, FileMode, Label, OutputPath};

    /// A store holding one blob; every other method is out of scope for a read.
    struct OneBlob(Vec<u8>);

    impl ContentStore for OneBlob {
        fn put(&self, _bytes: &[u8]) -> Result<Digest> {
            Err(unused())
        }

        fn contains(&self, digest: &Digest) -> bool {
            *digest == Digest::of(&self.0)
        }

        fn get(&self, digest: &Digest) -> Result<Option<Vec<u8>>> {
            Ok(self.contains(digest).then(|| self.0.clone()))
        }

        fn clear(&self, _at: &OutputPath) -> Result<()> {
            Err(unused())
        }

        fn place(&self, _digest: &Digest, _mode: FileMode, _at: &OutputPath) -> Result<()> {
            Err(unused())
        }
    }

    fn unused() -> Error {
        Error::Adapter {
            port: PortName::ContentStore,
            diagnostic: Diagnostic::new("not used by a read"),
        }
    }

    fn label() -> Label {
        Label::parse("//:app").unwrap()
    }

    #[test]
    fn a_held_digest_reads_its_bytes() {
        let cas = OneBlob(b"out\n".to_vec());
        let bytes = read(&cas, Some(&Digest::of(b"out\n")), &label()).unwrap();
        assert_eq!(bytes, b"out\n");
    }

    #[test]
    fn a_digest_the_store_does_not_hold_is_missing_output() {
        let cas = OneBlob(b"out\n".to_vec());
        let err = read(&cas, Some(&Digest::of(b"other")), &label()).unwrap_err();
        assert!(matches!(err, Error::MissingOutput { label: named } if named == label()));
    }

    #[test]
    fn no_digest_is_missing_output() {
        let cas = OneBlob(Vec::new());
        let err = read(&cas, None, &label()).unwrap_err();
        assert!(matches!(err, Error::MissingOutput { .. }));
    }
}
