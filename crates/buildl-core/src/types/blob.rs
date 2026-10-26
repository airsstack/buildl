//! A content digest together with the size of the content, as a remote cache names a blob.
//!
//! Its own file because a remote cache never names bytes by hash alone: the size travels with the
//! hash, so that neither is passed around without the other.
//!
//! Responsibilities: [`Blob`] and the constructor that takes both halves from bytes.
//!
//! Non-responsibilities: holding or moving the bytes, which a port does.

use crate::types::Digest;

/// The SHA-256 digest of some bytes and how many bytes there are.
///
/// The pair the Remote Execution API calls a digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Blob {
    /// The digest of the content.
    pub digest: Digest,
    /// The length of the content in bytes.
    pub size: u64,
}

impl Blob {
    /// The blob that `bytes` are.
    #[must_use]
    pub fn of(bytes: &[u8]) -> Self {
        Self {
            digest: Digest::of(bytes),
            size: bytes.len() as u64,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Blob;
    use crate::types::Digest;

    #[test]
    fn a_blob_of_bytes_holds_their_digest_and_length() {
        let blob = Blob::of(b"hello");
        assert_eq!(blob.digest, Digest::of(b"hello"));
        assert_eq!(blob.size, 5);
    }

    #[test]
    fn the_empty_content_is_a_blob_of_size_zero() {
        assert_eq!(Blob::of(b"").size, 0);
    }

    #[test]
    fn the_same_digest_with_another_size_is_another_blob() {
        let blob = Blob::of(b"hello");
        assert_ne!(blob, Blob { size: 6, ..blob });
    }
}
