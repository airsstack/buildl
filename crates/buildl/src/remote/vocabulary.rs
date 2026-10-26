//! The names the remote adapter's modules and its test server share.
//!
//! [`Method`] names a call of the REAPI, and [`Updates`] says whether a cache accepts writes.
//! Each is defined here once, so an error, the capabilities and the test server cannot disagree
//! about what a call or a setting is called.

use std::fmt;

/// A call of the REAPI that buildl makes.
///
/// Public only so the test server can re-export it; without that feature nothing outside the
/// crate can name it.
#[cfg_attr(
    not(feature = "test-server"),
    expect(unreachable_pub, reason = "re-exported only by the test server")
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Method {
    /// `Capabilities.GetCapabilities`.
    GetCapabilities,
    /// `ContentAddressableStorage.FindMissingBlobs`.
    FindMissingBlobs,
    /// `ContentAddressableStorage.BatchUpdateBlobs`.
    BatchUpdateBlobs,
    /// `ContentAddressableStorage.BatchReadBlobs`.
    BatchReadBlobs,
    /// `ByteStream.Read`.
    Read,
    /// `ByteStream.Write`.
    Write,
    /// `ActionCache.GetActionResult`.
    GetActionResult,
    /// `ActionCache.UpdateActionResult`.
    UpdateActionResult,
}

impl Method {
    /// The call as `Service.Method`.
    #[cfg_attr(
        not(feature = "test-server"),
        expect(unreachable_pub, reason = "re-exported only by the test server")
    )]
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::GetCapabilities => "Capabilities.GetCapabilities",
            Self::FindMissingBlobs => "ContentAddressableStorage.FindMissingBlobs",
            Self::BatchUpdateBlobs => "ContentAddressableStorage.BatchUpdateBlobs",
            Self::BatchReadBlobs => "ContentAddressableStorage.BatchReadBlobs",
            Self::Read => "ByteStream.Read",
            Self::Write => "ByteStream.Write",
            Self::GetActionResult => "ActionCache.GetActionResult",
            Self::UpdateActionResult => "ActionCache.UpdateActionResult",
        }
    }
}

impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Whether a cache lets a client write action results and blobs.
///
/// Public only so the test server can re-export it; without that feature nothing outside the
/// crate can name it.
#[cfg_attr(
    not(feature = "test-server"),
    expect(unreachable_pub, reason = "re-exported only by the test server")
)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Updates {
    /// `update_enabled` is advertised and `UpdateActionResult` is accepted.
    #[default]
    Enabled,
    /// `update_enabled` is withheld and `UpdateActionResult` is refused.
    Disabled,
}

#[cfg(test)]
mod tests {
    use super::Method;

    #[test]
    fn a_method_displays_as_service_and_method() {
        assert_eq!(Method::Read.to_string(), "ByteStream.Read");
        assert_eq!(
            Method::BatchUpdateBlobs.to_string(),
            "ContentAddressableStorage.BatchUpdateBlobs"
        );
    }
}
