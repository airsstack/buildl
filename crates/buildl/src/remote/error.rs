//! Why a call to the remote cache failed.

use std::io;
use std::time::Duration;

use super::channel::ChannelError;
use super::proto::build::bazel::remote::execution::v2::Digest;
use super::vocabulary::Method;

/// A failure of the remote cache's client.
///
/// Every variant but [`RemoteError::Token`] is a remote failure in the sense of a cache that
/// cannot be used, and none of them says the build is wrong. `Token` is a mistake in the
/// workspace's configuration. The variants keep the cause apart so the caller decides whether
/// to stop or to carry on without the cache.
#[derive(Debug, thiserror::Error)]
pub(crate) enum RemoteError {
    /// The channel to the cache could not be made.
    #[error(transparent)]
    Channel(#[from] ChannelError),
    /// The token the settings name cannot be read or sent.
    #[error(transparent)]
    Token(buildl_core::Error),
    /// The client's runtime could not be started.
    #[error("cannot start the remote cache client's runtime: {0}")]
    Runtime(#[source] io::Error),
    /// The cache answered a call with a failed status.
    #[error("{call} failed with {:?}: {}", .source.code(), .source.message())]
    Status {
        /// The call.
        call: Method,
        /// What the cache answered.
        #[source]
        source: tonic::Status,
    },
    /// A call, or one message of a stream, took longer than the configured timeout.
    #[error("{call} did not answer within {timeout:?}")]
    TimedOut {
        /// The call.
        call: Method,
        /// The time it was given.
        timeout: Duration,
    },
    /// The cache was found unusable earlier, when its capabilities were asked for.
    #[error("the remote cache is unusable: {reason}")]
    Unusable {
        /// Why the first ask failed.
        reason: String,
    },
    /// The cache does not offer SHA-256, the only digest function buildl uses.
    #[error("the remote cache does not support SHA-256")]
    NoSha256,
    /// The cache does not let clients write.
    #[error("the remote cache does not accept uploads: it reports action cache updates disabled")]
    UpdatesDisabled,
    /// One blob of a transfer failed.
    #[error("blob {}/{} failed with {code:?}: {message}", .digest.hash, .digest.size_bytes)]
    Blob {
        /// The blob.
        digest: Digest,
        /// The status the cache gave the blob.
        code: tonic::Code,
        /// The cache's explanation.
        message: String,
    },
    /// The cache's answer breaks the protocol.
    #[error("{call} was answered wrongly: {detail}")]
    Protocol {
        /// The call.
        call: Method,
        /// What is wrong with the answer.
        detail: String,
    },
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::RemoteError;
    use crate::remote::vocabulary::Method;

    #[test]
    fn a_status_error_names_the_call_and_the_code() {
        let error = RemoteError::Status {
            call: Method::GetActionResult,
            source: tonic::Status::unavailable("down"),
        };

        assert_eq!(
            error.to_string(),
            "ActionCache.GetActionResult failed with Unavailable: down"
        );
    }

    #[test]
    fn an_unusable_cache_gives_the_first_failure() {
        let error = RemoteError::Unusable {
            reason: "Capabilities.GetCapabilities did not answer within 30s".to_owned(),
        };

        assert_eq!(
            error.to_string(),
            "the remote cache is unusable: Capabilities.GetCapabilities did not answer within 30s"
        );
    }

    #[test]
    fn a_timeout_names_the_call_and_the_time() {
        let error = RemoteError::TimedOut {
            call: Method::Read,
            timeout: Duration::from_millis(200),
        };

        assert_eq!(
            error.to_string(),
            "ByteStream.Read did not answer within 200ms"
        );
    }
}
