//! A content store held in memory, for the tests of this module.
//!
//! Its own file because two files of the module need one: what stands at an output path is
//! stored as it is found, and a strategy stores through whatever store it is handed. A store on
//! disk belongs to another module, which this one may not name.
//!
//! Responsibilities: [`MemoryStore`].

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use buildl_core::{
    ContentStore, Diagnostic, Digest, Error, FileMode, OutputPath, PortName, Result,
};

/// Holds content by its digest, in memory. It places nothing: it has no workspace.
#[derive(Debug, Default)]
pub(crate) struct MemoryStore {
    blobs: Mutex<BTreeMap<Digest, Vec<u8>>>,
    broken: bool,
}

impl MemoryStore {
    /// A store that holds nothing yet.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// A store that refuses everything it is handed.
    pub(crate) fn broken() -> Self {
        Self {
            broken: true,
            ..Self::default()
        }
    }

    /// How many different contents are held.
    pub(crate) fn len(&self) -> usize {
        self.blobs().len()
    }

    fn blobs(&self) -> MutexGuard<'_, BTreeMap<Digest, Vec<u8>>> {
        self.blobs.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl ContentStore for MemoryStore {
    fn put(&self, bytes: &[u8]) -> Result<Digest> {
        if self.broken {
            return Err(Error::Adapter {
                port: PortName::ContentStore,
                diagnostic: Diagnostic::new("the store is broken"),
            });
        }
        let digest = Digest::of(bytes);
        self.blobs().insert(digest, bytes.to_vec());
        Ok(digest)
    }

    fn contains(&self, digest: &Digest) -> bool {
        self.blobs().contains_key(digest)
    }

    fn get(&self, digest: &Digest) -> Result<Option<Vec<u8>>> {
        Ok(self.blobs().get(digest).cloned())
    }

    fn clear(&self, _at: &OutputPath) -> Result<()> {
        Ok(())
    }

    fn place(&self, _digest: &Digest, _mode: FileMode, _at: &OutputPath) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use buildl_core::{ContentStore, Digest, Error, PortName};

    use super::MemoryStore;

    #[test]
    fn the_store_holds_what_it_is_handed_and_a_broken_one_refuses() {
        let store = MemoryStore::new();
        let digest = store.put(b"some bytes").unwrap();

        assert_eq!(digest, Digest::of(b"some bytes"));
        assert!(store.contains(&digest));
        assert_eq!(store.get(&digest).unwrap(), Some(b"some bytes".to_vec()));
        assert_eq!(store.get(&Digest::of(b"other")).unwrap(), None);
        assert_eq!(store.len(), 1);

        let broken = MemoryStore::broken();
        let refused = broken.put(b"some bytes").unwrap_err();

        assert!(matches!(
            refused,
            Error::Adapter {
                port: PortName::ContentStore,
                ..
            }
        ));
        assert_eq!(broken.len(), 0);
    }
}
