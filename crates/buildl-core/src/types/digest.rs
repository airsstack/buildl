//! The single SHA-256 content identity used for files, outputs, action keys and log blobs.
//!
//! Its own file because one type serves every one of those uses: if each invented its own,
//! "the key of X" and "the file of X" could disagree. SHA-256 rather than a faster hash for
//! compatibility with the Remote Execution API's digest model, so a remote cache is later a
//! transport problem rather than a re-keying.
//!
//! Responsibilities: [`Digest`], hashing in-memory bytes — whole with [`Digest::of`], or in
//! pieces with [`Hasher`] — and the hexadecimal form.
//!
//! Non-responsibilities: reading anything. Hashing a *file* means touching the filesystem,
//! which is a port's job; this type hashes a byte slice it is handed.

use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Serialize};
use sha2::Digest as _;
use sha2::Sha256;

use crate::error::{Error, NameKind, Result};

/// A SHA-256 content digest.
///
/// Renders as 64 lowercase hexadecimal characters, and [`Digest::from_hex`] accepts only that
/// form, so every digest has exactly one spelling.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct Digest([u8; 32]);

/// The value of one lowercase hexadecimal digit, or `None`.
const fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

impl Digest {
    /// Hashes an in-memory byte slice.
    #[must_use]
    pub fn of(bytes: &[u8]) -> Self {
        let out = Sha256::digest(bytes);
        let mut raw = [0_u8; 32];
        raw.copy_from_slice(&out);
        Self(raw)
    }

    /// Parses the 64-character lowercase hexadecimal form.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidName`] when `raw` is not exactly 64 characters, or holds a
    /// character outside `0-9a-f` — uppercase included.
    pub fn from_hex(raw: &str) -> Result<Self> {
        let invalid = |reason: &'static str| Error::InvalidName {
            kind: NameKind::Digest,
            value: raw.to_owned(),
            reason,
        };

        if raw.len() != 64 {
            return Err(invalid("must be exactly 64 hexadecimal characters"));
        }
        let bytes = raw.as_bytes();
        let mut out = [0_u8; 32];
        for (index, byte) in out.iter_mut().enumerate() {
            let high = bytes.get(index * 2).copied().and_then(hex_value);
            let low = bytes.get(index * 2 + 1).copied().and_then(hex_value);
            match (high, low) {
                (Some(high), Some(low)) => *byte = (high << 4) | low,
                _ => return Err(invalid("must be lowercase hexadecimal")),
            }
        }
        Ok(Self(out))
    }

    /// The raw 32 bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// A SHA-256 digest taken over bytes handed in pieces.
///
/// For content too large to hold whole: whoever reads it hands each piece over as it is read.
/// The digest is the one [`Digest::of`] gives for the pieces joined, so the two never disagree
/// about one content.
///
/// # Examples
///
/// ```
/// use buildl_core::{Digest, Hasher};
///
/// let mut hasher = Hasher::new();
/// hasher.update(b"hello, ");
/// hasher.update(b"world");
/// assert_eq!(hasher.finish(), Digest::of(b"hello, world"));
/// ```
#[derive(Debug, Clone, Default)]
pub struct Hasher(Sha256);

impl Hasher {
    /// A hasher that has been handed nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `bytes` to what is hashed.
    pub fn update(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }

    /// The digest of every byte added, in order: equal to [`Digest::of`] over them joined.
    #[must_use]
    pub fn finish(self) -> Digest {
        let out = self.0.finalize();
        let mut raw = [0_u8; 32];
        raw.copy_from_slice(&out);
        Digest(raw)
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Digest(\"{self}\")")
    }
}

impl FromStr for Digest {
    type Err = Error;
    fn from_str(raw: &str) -> Result<Self> {
        Self::from_hex(raw)
    }
}

impl From<Digest> for String {
    fn from(value: Digest) -> Self {
        value.to_string()
    }
}

impl TryFrom<String> for Digest {
    type Error = Error;
    fn try_from(raw: String) -> Result<Self> {
        Self::from_hex(&raw)
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use super::Digest;

    #[test]
    fn bytes_handed_in_pieces_hash_like_the_bytes_joined() {
        let bytes: Vec<u8> = (0..200_000_u32).map(|n| n.to_le_bytes()[0]).collect();
        for piece in [1_usize, 7, 64, 65_536, 200_000, 300_000] {
            let mut hasher = super::Hasher::new();
            for chunk in bytes.chunks(piece) {
                hasher.update(chunk);
            }
            assert_eq!(hasher.finish(), Digest::of(&bytes), "pieces of {piece}");
        }
    }

    #[test]
    fn a_hasher_handed_nothing_gives_the_digest_of_nothing() {
        assert_eq!(super::Hasher::new().finish(), Digest::of(&[]));
        let mut handed_empty = super::Hasher::new();
        handed_empty.update(&[]);
        assert_eq!(handed_empty.finish(), Digest::of(&[]));
    }

    #[test]
    fn matches_the_known_vectors() {
        assert_eq!(
            Digest::of(b"").to_string(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            Digest::of(b"buildl").to_string(),
            "4363840e4d122eb82ebe6c08be71a793135ecd8af8ad8ff7b984414a49b09064"
        );
    }

    #[test]
    fn round_trips_through_hex() {
        let digest = Digest::of(b"buildl");
        assert_eq!(Digest::from_hex(&digest.to_string()).unwrap(), digest);
        assert_eq!(digest.as_bytes().len(), 32);
    }

    #[test]
    fn rejects_every_form_that_is_not_64_lowercase_hex_characters() {
        assert!(Digest::from_hex(&"a".repeat(63)).is_err());
        assert!(Digest::from_hex(&"a".repeat(65)).is_err());
        assert!(Digest::from_hex(&"A".repeat(64)).is_err());
        assert!(Digest::from_hex(&"z".repeat(64)).is_err());
        assert!(Digest::from_hex("").is_err());
    }

    #[test]
    fn debug_shows_the_hex_not_the_byte_array() {
        assert_eq!(
            format!("{:?}", Digest::of(b"")),
            r#"Digest("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")"#
        );
    }

    #[test]
    fn round_trips_through_json_as_a_hex_string() {
        let digest = Digest::of(b"buildl");
        let json = serde_json::to_string(&digest).unwrap();
        assert_eq!(
            json,
            r#""4363840e4d122eb82ebe6c08be71a793135ecd8af8ad8ff7b984414a49b09064""#
        );
        assert_eq!(serde_json::from_str::<Digest>(&json).unwrap(), digest);
        assert!(serde_json::from_str::<Digest>(r#""not-a-digest""#).is_err());
    }

    #[test]
    fn from_str_routes_through_parse() {
        assert_eq!(
            "4363840e4d122eb82ebe6c08be71a793135ecd8af8ad8ff7b984414a49b09064"
                .parse::<Digest>()
                .unwrap(),
            Digest::of(b"buildl")
        );
        assert!("A".repeat(64).parse::<Digest>().is_err());
    }

    #[test]
    fn orders_by_byte_value() {
        let low = Digest::from_hex(&"00".repeat(32)).unwrap();
        let mid = Digest::from_hex(&"11".repeat(32)).unwrap();
        let high = Digest::from_hex(&"ff".repeat(32)).unwrap();
        let mut digests = [high, low, mid];
        digests.sort();
        assert_eq!(digests, [low, mid, high]);
    }
}
