//! The two paths the content store may hold one digest's bytes at.
//!
//! Its own file because every method of the store turns a digest into a path, and the rule must
//! be spelled once: the first two hexadecimal characters of the digest name a directory, the
//! other sixty-two name the file in it, and the executable twin is that name with `.x` added.
//! Two characters of directory keep any one directory from holding the whole store.
//!
//! Responsibilities: [`BlobPath`].
//!
//! Non-responsibilities: reading or writing either file, which the store does.

use std::path::{Path, PathBuf};

use buildl_core::{Digest, FileMode};

/// The two files the store may hold for one digest.
#[derive(Debug)]
pub(crate) struct BlobPath {
    plain: PathBuf,
    runnable: PathBuf,
}

impl BlobPath {
    /// The paths for `digest` in the store whose directory is `blobs`.
    pub(crate) fn of(blobs: &Path, digest: &Digest) -> Self {
        let hex = digest.to_string();
        let (shard, rest) = hex.split_at(2);
        let shard = blobs.join(shard);
        Self {
            plain: shard.join(rest),
            runnable: shard.join(format!("{rest}.x")),
        }
    }

    /// The file a placement of that mode is made from: the blob for a file that may not be
    /// run, its executable twin for one that may.
    pub(crate) fn for_mode(&self, mode: FileMode) -> &Path {
        match mode {
            FileMode::Plain => &self.plain,
            FileMode::Executable => &self.runnable,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use buildl_core::{Digest, FileMode};

    use super::BlobPath;

    #[test]
    fn a_digest_splits_into_shard_and_rest() {
        let paths = BlobPath::of(Path::new("/ws/.buildl/cas"), &Digest::of(b"hello"));
        assert_eq!(
            paths.for_mode(FileMode::Plain),
            Path::new(
                "/ws/.buildl/cas/2c/f24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
            )
        );
        assert_eq!(
            paths.for_mode(FileMode::Executable),
            Path::new(
                "/ws/.buildl/cas/2c/f24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824.x"
            )
        );
    }
}
