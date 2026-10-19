//! The port through which a store's contents are listed and removed.
//!
//! Its own file, and apart from [`ContentStore`](crate::ContentStore), because a build holds a
//! `ContentStore` and must not be able to remove what it stored; only the command that clears the
//! store is handed this.
//!
//! Responsibilities: [`Sweep`].
//!
//! Non-responsibilities: implementing it, and choosing what to remove. The first belongs to an
//! adapter crate; the second to the module that decides what is still wanted.
//!
//! This file holds only a trait definition, so it carries no logic to unit-test.

use std::collections::BTreeSet;

use crate::error::Result;
use crate::types::Digest;

/// Lists and removes what a content store holds. Kept apart from `ContentStore`, so a build,
/// which holds a `ContentStore`, cannot remove what it stored.
pub trait Sweep {
    /// Every digest the store holds a blob or a twin for, in digest order.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Adapter`](crate::Error::Adapter), naming
    /// [`PortName::Sweep`](crate::PortName::Sweep), when the store cannot be listed.
    fn held(&self) -> Result<BTreeSet<Digest>>;

    /// Removes the blob and the twin held under `digest`; returns the bytes they took.
    /// Nothing held is `Ok(0)`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Adapter`](crate::Error::Adapter), naming
    /// [`PortName::Sweep`](crate::PortName::Sweep), when something held cannot be removed.
    fn remove(&self, digest: &Digest) -> Result<u64>;
}
