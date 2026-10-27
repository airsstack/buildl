//! Why a call to the remote cache failed.

use std::io;
use std::time::Duration;

use super::channel::ChannelError;
use super::proto::build::bazel::remote::execution::v2::Digest;
use super::retry::{Failed, Failure};
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
    /// The cache was found unusable earlier, when the channel to it was connected or its
    /// capabilities were asked for.
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
    /// The server cannot execute actions.
    #[error("the remote server cannot execute actions: {reason}")]
    NoExecution {
        /// Why: execution is not enabled, or the server has no SHA-256 for it.
        reason: &'static str,
    },
    /// An execution was still unfinished when the time given to it passed.
    #[error("{} did not finish within {limit:?}", Method::Execute)]
    WaitPassed {
        /// The time the whole execution was given.
        limit: Duration,
    },
    /// One blob of a transfer failed.
    #[error("{call}: blob {}/{} failed with {code:?}: {message}", .digest.hash, .digest.size_bytes)]
    Blob {
        /// The call that carried the blob.
        call: Method,
        /// The blob.
        digest: Digest,
        /// The status the cache gave the blob.
        code: tonic::Code,
        /// The cache's explanation.
        message: String,
    },
    /// A blob is larger than the protocol's digest can state.
    #[error("a blob of {size} bytes is larger than a digest can name")]
    TooLarge {
        /// The blob's size in bytes.
        size: u64,
    },
    /// The cache's answer breaks the protocol.
    #[error("{call} was answered wrongly: {detail}")]
    Protocol {
        /// The call.
        call: Method,
        /// What is wrong with the answer.
        detail: String,
    },
    /// A call was made again after a failure and failed again.
    #[error("{last} (after {calls} calls)")]
    GaveUp {
        /// The number of times the call was made.
        calls: u32,
        /// How the last try failed.
        #[source]
        last: Box<Self>,
    },
}

/// Why an execution failed: the remote server, or the caller that supplies the blobs it lacks.
#[derive(Debug)]
pub(crate) enum ExecuteError<E> {
    /// The server or the connection failed.
    Remote(RemoteError),
    /// The caller could not supply the blobs the server said it lacks.
    Supply(E),
}

impl<E> From<RemoteError> for ExecuteError<E> {
    fn from(error: RemoteError) -> Self {
        Self::Remote(error)
    }
}

impl Failure for RemoteError {
    /// The code the cache answered with. A call that timed out and a connection that cannot be
    /// made count as `UNAVAILABLE`: the cache may be back by the next try. A failure that is
    /// not an answer of the cache, such as a certificate file that cannot be read, has none.
    fn code(&self) -> Option<tonic::Code> {
        match self {
            Self::Status { source, .. } => Some(source.code()),
            Self::Blob { code, .. } => Some(*code),
            Self::TimedOut { .. } | Self::Channel(ChannelError::Connect { .. }) => {
                Some(tonic::Code::Unavailable)
            }
            Self::Channel(_)
            | Self::Token(_)
            | Self::Runtime(_)
            | Self::Unusable { .. }
            | Self::NoSha256
            | Self::UpdatesDisabled
            | Self::NoExecution { .. }
            | Self::WaitPassed { .. }
            | Self::TooLarge { .. }
            | Self::Protocol { .. }
            | Self::GaveUp { .. } => None,
        }
    }
}

impl From<Failed<Self>> for RemoteError {
    /// The error of a call that was made once as it is, and that of one made again with the
    /// number of calls.
    fn from(failed: Failed<Self>) -> Self {
        if failed.calls <= 1 {
            failed.error
        } else {
            Self::GaveUp {
                calls: failed.calls,
                last: Box::new(failed.error),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use tonic::Code;

    use super::RemoteError;
    use crate::remote::channel::ChannelError;
    use crate::remote::proto::build::bazel::remote::execution::v2::Digest;
    use crate::remote::retry::{Failed, Failure};
    use crate::remote::vocabulary::Method;

    fn status(code: Code) -> RemoteError {
        RemoteError::Status {
            call: Method::GetActionResult,
            source: tonic::Status::new(code, "no"),
        }
    }

    #[test]
    fn a_status_and_a_blob_carry_the_code_the_cache_gave() {
        assert_eq!(status(Code::Aborted).code(), Some(Code::Aborted));
        let blob = RemoteError::Blob {
            call: Method::BatchReadBlobs,
            digest: Digest::default(),
            code: Code::ResourceExhausted,
            message: "full".to_owned(),
        };
        assert_eq!(blob.code(), Some(Code::ResourceExhausted));
    }

    #[test]
    fn a_timeout_and_a_connection_that_fails_count_as_unavailable() {
        let timed_out = RemoteError::TimedOut {
            call: Method::Read,
            timeout: Duration::from_secs(1),
        };
        let connect = RemoteError::Channel(ChannelError::Connect {
            reason: "refused".to_owned(),
        });
        assert_eq!(timed_out.code(), Some(Code::Unavailable));
        assert_eq!(connect.code(), Some(Code::Unavailable));
    }

    #[test]
    fn a_failure_that_is_not_an_answer_of_the_cache_has_no_code() {
        let unreadable = RemoteError::Channel(ChannelError::CaUnreadable {
            path: PathBuf::from("ca.pem"),
            source: std::io::Error::other("gone"),
        });
        let setup = RemoteError::Channel(ChannelError::Setup {
            reason: "bad address".to_owned(),
        });
        for error in [
            unreadable,
            setup,
            RemoteError::NoSha256,
            RemoteError::UpdatesDisabled,
            RemoteError::NoExecution { reason: "off" },
            RemoteError::WaitPassed {
                limit: Duration::from_secs(1),
            },
            RemoteError::TooLarge { size: 1 },
            RemoteError::Unusable {
                reason: "down".to_owned(),
            },
        ] {
            assert_eq!(error.code(), None, "{error:?}");
        }
    }

    #[test]
    fn a_call_made_once_fails_with_its_own_error() {
        let error = RemoteError::from(Failed {
            error: status(Code::PermissionDenied),
            calls: 1,
        });

        assert!(matches!(error, RemoteError::Status { .. }), "{error:?}");
    }

    #[test]
    fn a_call_made_again_fails_saying_how_often() {
        let error = RemoteError::from(Failed {
            error: RemoteError::Status {
                call: Method::GetActionResult,
                source: tonic::Status::unavailable("down"),
            },
            calls: 3,
        });

        assert_eq!(
            error.to_string(),
            "ActionCache.GetActionResult failed with Unavailable: down (after 3 calls)"
        );
    }

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
    fn a_server_that_cannot_execute_says_why() {
        let error = RemoteError::NoExecution {
            reason: "execution is not enabled",
        };

        assert_eq!(
            error.to_string(),
            "the remote server cannot execute actions: execution is not enabled"
        );
    }

    #[test]
    fn an_execution_that_ran_out_of_time_names_the_call_and_the_limit() {
        let error = RemoteError::WaitPassed {
            limit: Duration::from_secs(90),
        };

        assert_eq!(
            error.to_string(),
            "Execution.Execute did not finish within 90s"
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
