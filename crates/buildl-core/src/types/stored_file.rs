//! One regular file an action left, as the content store holds it.
//!
//! Its own file because a file is more than its bytes: whether it may be run is a second fact,
//! and a store that holds content by digest alone would lose it. Keeping the two together is
//! what lets a built program be run again after it was put back from the store.
//!
//! Responsibilities: [`FileMode`] and [`StoredFile`], with their serialized forms.
//!
//! Non-responsibilities: reading a file's mode and setting it, which are the ports'.

use serde::{Deserialize, Serialize};

use crate::types::Digest;

/// Whether a file may be run.
///
/// This is the one fact about a file's permissions that is kept. Every other permission, the
/// owner and the times of a file put in place are the content store's to choose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileMode {
    /// It may not be run.
    Plain,
    /// It may be run.
    Executable,
}

impl FileMode {
    /// The mode of a file whose Unix permission bits are `bits`: executable when any of the
    /// three execute bits is set.
    #[must_use]
    pub const fn of_unix_bits(bits: u32) -> Self {
        if bits & 0o111 == 0 {
            Self::Plain
        } else {
            Self::Executable
        }
    }
}

/// One regular file: the digest of its bytes, and whether it may be run.
///
/// Two files with the same bytes and different modes are different files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoredFile {
    /// The digest of the file's bytes.
    pub digest: Digest,
    /// Whether the file may be run.
    pub mode: FileMode,
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use super::{FileMode, StoredFile};
    use crate::json::canonical;
    use crate::types::Digest;

    #[test]
    fn any_execute_bit_makes_a_file_executable() {
        for bits in [0o000, 0o644, 0o600, 0o4_644] {
            assert_eq!(FileMode::of_unix_bits(bits), FileMode::Plain, "{bits:o}");
        }
        for bits in [0o755, 0o100, 0o010, 0o001, 0o610] {
            assert_eq!(
                FileMode::of_unix_bits(bits),
                FileMode::Executable,
                "{bits:o}"
            );
        }
    }

    #[test]
    fn a_mode_serializes_as_its_lowercase_name_and_reads_back() {
        for (mode, text) in [
            (FileMode::Plain, r#""plain""#),
            (FileMode::Executable, r#""executable""#),
        ] {
            assert_eq!(canonical::to_string(&mode).unwrap(), text);
            assert_eq!(serde_json::from_str::<FileMode>(text).unwrap(), mode);
        }
        assert!(serde_json::from_str::<FileMode>(r#""Plain""#).is_err());
        assert!(serde_json::from_str::<FileMode>(r#""setuid""#).is_err());
    }

    #[test]
    fn a_file_round_trips_through_canonical_json() {
        let digest = Digest::of(b"app");
        let file = StoredFile {
            digest,
            mode: FileMode::Executable,
        };
        let json = canonical::to_string(&file).unwrap();
        assert_eq!(
            json,
            format!(r#"{{"digest":"{digest}","mode":"executable"}}"#)
        );
        assert_eq!(serde_json::from_str::<StoredFile>(&json).unwrap(), file);
    }

    #[test]
    fn a_file_with_a_field_it_does_not_have_is_not_read() {
        let digest = Digest::of(b"app");
        let extra = format!(r#"{{"digest":"{digest}","mode":"plain","owner":"root"}}"#);
        assert!(serde_json::from_str::<StoredFile>(&extra).is_err());
        let short = format!(r#"{{"digest":"{digest}"}}"#);
        assert!(serde_json::from_str::<StoredFile>(&short).is_err());
    }

    #[test]
    fn two_files_with_the_same_bytes_and_another_mode_are_not_equal() {
        let digest = Digest::of(b"app");
        let plain = StoredFile {
            digest,
            mode: FileMode::Plain,
        };
        let executable = StoredFile {
            digest,
            mode: FileMode::Executable,
        };
        assert_ne!(plain, executable);
    }
}
