//! The port through which a build asks a remote cache for a result, stores one there, and asks
//! it to run an action.
//!
//! Its own file because a remote cache is a different kind of store from the local one: it is
//! keyed by a description of the work rather than by buildl's own key, it answers with a result
//! that has not been checked, and it is reached over a network by several workers at once.
//!
//! Responsibilities: [`RemoteCache`] and [`BlobSource`].
//!
//! Non-responsibilities: implementing them, and judging what a remote answers. The first belongs
//! to an adapter crate; the second to the phase that executes, which checks every answer before
//! it uses one. An adapter only moves and encodes.
//!
//! This file holds only trait definitions, so it carries no logic to unit-test.

use crate::error::Result;
use crate::types::{
    Blob, DecodedTree, Digest, Executed, RemoteAction, RemoteHit, RemoteUpload, Uploaded,
};

/// A cache of results shared between machines, asked and filled by the action that made them,
/// and able to run an action itself.
///
/// Blocking, and called from several worker threads at once. Every failure is an
/// [`Error::Adapter`](crate::Error::Adapter) naming
/// [`PortName::RemoteCache`](crate::PortName::RemoteCache); a miss is not a failure.
pub trait RemoteCache: Send + Sync {
    /// The result the remote holds for `action`, as it was stored and not yet checked; `None`
    /// when it holds none.
    ///
    /// # Errors
    ///
    /// Returns an error when the remote cannot be reached or answers in a way this port cannot
    /// read.
    fn lookup(&self, action: &RemoteAction) -> Result<Option<RemoteHit>>;

    /// The bytes of each of `blobs`, in the same order.
    ///
    /// # Errors
    ///
    /// Returns an error when the remote cannot be reached, or does not hold one of them.
    fn download(&self, blobs: &[Blob]) -> Result<Vec<Vec<u8>>>;

    /// What a directory tree the remote stores holds: its files in path order, or the first
    /// thing in it a build cannot place, a symlink or an empty directory, named by its path
    /// below the tree's root.
    ///
    /// # Errors
    ///
    /// Returns an error when the port cannot decode at all; bytes it cannot read as a tree are
    /// [`DecodedTree::Unreadable`].
    fn decode_tree(&self, bytes: &[u8]) -> Result<DecodedTree>;

    /// Stores what a run produced as the result for `action`, with the input files and the
    /// output and log blobs it names. The output and log blobs are read from `blobs`.
    ///
    /// # Errors
    ///
    /// Returns an error when the remote cannot be reached or refuses what it is sent.
    fn upload(
        &self,
        action: &RemoteAction,
        produced: &RemoteUpload,
        blobs: &dyn BlobSource,
    ) -> Result<Uploaded>;

    /// Asks the remote to run `action`, and returns how it ended: the result, as it was
    /// answered and not yet checked, or the reason there is none.
    ///
    /// # Errors
    ///
    /// Returns an error when the remote cannot be reached, does not run actions, or answers in
    /// a way this port cannot read.
    fn execute(&self, action: &RemoteAction) -> Result<Executed>;
}

/// Where the bytes of the blobs an upload names are read from.
///
/// It lets a remote adapter read what the local content store holds without importing the store.
pub trait BlobSource: Sync {
    /// The bytes held under `digest`; `None` when none are held.
    ///
    /// # Errors
    ///
    /// Returns an error when the store cannot be read.
    fn read(&self, digest: &Digest) -> Result<Option<Vec<u8>>>;
}
