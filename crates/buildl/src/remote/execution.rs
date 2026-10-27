//! One execution on the remote server, driven to its end.
//!
//! [`run`] asks the server to execute an action and reads the operation's messages until one
//! says it is done. The whole execution shares one [`Retry`] budget, and each try is one
//! `Execute` or `WaitExecution` stream:
//!
//! - a stream that ends, or fails with a code the cache may recover from (`UNAVAILABLE`,
//!   `RESOURCE_EXHAUSTED`, `ABORTED`, `CANCELLED`), after delivering a message before the
//!   operation is done is resumed with `WaitExecution` on the operation's name; one that fails
//!   with `UNKNOWN` or `INTERNAL` after a message, as a reset connection does, is resumed the
//!   same way but uses one of the retries and does not start the count over; any other failure
//!   of a stream is a failure. A `WaitExecution` the server answers with `NOT_FOUND` starts
//!   the execution again with `Execute`;
//! - a `FAILED_PRECONDITION` whose details name only missing blobs, as the call's error or as
//!   the status of the final response, has the blobs sent and `Execute` called again; any other
//!   precondition failure is a failure;
//! - a final response whose status the cache may recover from (`UNAVAILABLE`,
//!   `RESOURCE_EXHAUSTED`, `ABORTED`, `CANCELLED`) is retried as the same code from a call is;
//! - an operation that ends with an error, or that holds no response, breaks the protocol.
//!
//! A try that received messages and then ended, or failed with a code the cache may recover
//! from, started the count of retries over; a try that ended in an answer, whatever the answer,
//! or failed with `UNKNOWN` or `INTERNAL`, did not. The time between two messages is not
//! bounded by the client's timeout, since an action may run a long time; the whole execution
//! is bounded by the limit the caller gives, every pause included.
//!
//! # Responsibilities
//!
//! - Driving the streams, and choosing between `Execute` and `WaitExecution` for each try.
//! - Reading the final response out of the last operation message.
//! - Reading the blobs a precondition failure names as missing.
//!
//! # Non-responsibilities
//!
//! - Judging an `ExecuteResponse` beyond what is retried here: a response with a result, an
//!   exit code or another status is the caller's to interpret.
//! - Reading the bytes of the blobs the server lacks: the caller supplies them.

use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use prost::{Message, Name};
use prost_types::Any;
use tonic::{Code, Status, Streaming};

use super::client::{Progress, Wire};
use super::error::{ExecuteError, RemoteError};
use super::proto::build::bazel::remote::execution::v2::{
    Digest, ExecuteRequest, ExecuteResponse, WaitExecutionRequest, digest_function,
};
use super::proto::google::longrunning::{Operation, operation};
use super::proto::google::rpc::{self, PreconditionFailure};
use super::retry::{Failure, Retry};
use super::transfer;
use super::vocabulary::Method;

/// The violation type that says a blob the action needs is not in the storage.
const MISSING: &str = "MISSING";

/// What the next try of the execution calls.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Next {
    /// `Execute`: nothing is known of the operation, or the server has lost it.
    Execute,
    /// `WaitExecution` on the operation of this name.
    Wait(String),
}

/// How a try failed.
enum Attempt<E> {
    /// A failure of a call, retried when its code says the server may recover.
    Remote(RemoteError),
    /// The caller could not supply the blobs the server lacks.
    Supply(E),
    /// A stream failed with `UNKNOWN` or `INTERNAL` after a message: retried as an
    /// `UNAVAILABLE` is, though it counts for no progress.
    Broke {
        /// The call that was made.
        call: Method,
        /// What the stream failed with.
        source: Status,
    },
    /// The try cannot go on and the next one can: it is retried as an `UNAVAILABLE` is.
    Again {
        /// The call that was made.
        call: Method,
        /// Why the try stopped.
        detail: String,
    },
}

impl<E> Failure for Attempt<E> {
    fn code(&self) -> Option<Code> {
        match self {
            Self::Remote(error) => error.code(),
            Self::Again { .. } | Self::Broke { .. } => Some(Code::Unavailable),
            Self::Supply(_) => None,
        }
    }
}

/// What a precondition failure says about the blobs of an action.
#[derive(Debug, PartialEq, Eq)]
enum Violations {
    /// Every violation says a blob is missing; these are the blobs.
    Missing(Vec<Digest>),
    /// There is no precondition failure, or it says something besides missing blobs.
    Other,
}

impl Violations {
    /// Reads the details of a `google.rpc.Status`.
    fn of(details: &[Any]) -> Self {
        let mut missing = Vec::new();
        let mut seen = false;
        for any in details {
            if !names(any, &PreconditionFailure::full_name()) {
                continue;
            }
            let Ok(failure) = PreconditionFailure::decode(any.value.as_slice()) else {
                return Self::Other;
            };
            for violation in &failure.violations {
                match digest_of_subject(&violation.subject) {
                    Some(digest) if violation.r#type == MISSING => missing.push(digest),
                    _ => return Self::Other,
                }
                seen = true;
            }
        }
        if seen {
            Self::Missing(missing)
        } else {
            Self::Other
        }
    }

    /// Reads the details a failed call carries: a `google.rpc.Status`, encoded.
    fn of_call(status: &Status) -> Self {
        rpc::Status::decode(status.details()).map_or(Self::Other, |inner| Self::of(&inner.details))
    }
}

/// Whether `any` holds a message whose full name is `full_name`, whatever prefix its URL has.
fn names(any: &Any, full_name: &str) -> bool {
    any.type_url.rsplit('/').next() == Some(full_name)
}

/// The digest a violation's subject names: `blobs/{hash}/{size}`, or
/// `blobs/{digest_function}/{hash}/{size}`.
fn digest_of_subject(subject: &str) -> Option<Digest> {
    let mut parts = subject.split('/');
    if parts.next()? != "blobs" {
        return None;
    }
    let rest: Vec<_> = parts.collect();
    let [.., hash, size] = rest.as_slice() else {
        return None;
    };
    if rest.len() > 3 || hash.is_empty() {
        return None;
    }
    Some(Digest {
        hash: (*hash).to_owned(),
        size_bytes: size.parse().ok().filter(|size| *size >= 0)?,
    })
}

/// Reads the `ExecuteResponse` an operation holds.
fn response_of(any: &Any) -> Result<ExecuteResponse, String> {
    if !names(any, &ExecuteResponse::full_name()) {
        return Err(format!("the response is a {}", any.type_url));
    }
    ExecuteResponse::decode(any.value.as_slice()).map_err(|error| error.to_string())
}

/// The state of one execution while its tries are made.
struct Drive<'a, S> {
    wire: &'a Wire<'a>,
    /// The most blob data one batch call may carry.
    threshold: usize,
    request: ExecuteRequest,
    supply: S,
    next: Mutex<Next>,
}

impl<E, S> Drive<'_, S>
where
    E: Send,
    S: Fn(&[Digest]) -> Result<Vec<(Digest, Vec<u8>)>, E> + Send + Sync,
{
    fn next(&self) -> Next {
        self.next
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn go(&self, next: Next) {
        *self.next.lock().unwrap_or_else(PoisonError::into_inner) = next;
    }

    /// One try: opens the stream the last try left for, and reads it to its end.
    async fn attempt(&self, progress: Progress) -> Result<ExecuteResponse, Attempt<E>> {
        let (method, mut stream) = self.open().await?;
        let mut moved = false;
        loop {
            let message = match stream.message().await {
                Ok(Some(message)) => message,
                Ok(None) => {
                    if moved {
                        progress.record();
                    }
                    return Err(Attempt::Again {
                        call: method,
                        detail: "the stream ended before the operation was done".to_owned(),
                    });
                }
                Err(source) => {
                    if moved && matches!(source.code(), Code::Unknown | Code::Internal) {
                        return Err(Attempt::Broke {
                            call: method,
                            source,
                        });
                    }
                    if moved {
                        progress.record();
                    }
                    return Err(self.stopped(method, source).await);
                }
            };
            moved = true;
            if !message.name.is_empty() {
                self.go(Next::Wait(message.name.clone()));
            }
            if message.done {
                return self.finish(method, message).await;
            }
        }
    }

    /// Makes the call the last try left for and returns the stream it answers with.
    async fn open(&self) -> Result<(Method, Streaming<Operation>), Attempt<E>> {
        let mut client = self.wire.execution();
        let (method, opened) = match self.next() {
            Next::Execute => (
                Method::Execute,
                self.wire
                    .bounded(Method::Execute, client.execute(self.request.clone()))
                    .await,
            ),
            Next::Wait(name) => (
                Method::WaitExecution,
                self.wire
                    .bounded(
                        Method::WaitExecution,
                        client.wait_execution(WaitExecutionRequest { name }),
                    )
                    .await,
            ),
        };
        match opened {
            Ok(response) => Ok((method, response.into_inner())),
            Err(RemoteError::Status { source, .. }) => Err(self.stopped(method, source).await),
            Err(other) => Err(Attempt::Remote(other)),
        }
    }

    /// What a status that ended the call, or the stream, means for the next try.
    async fn stopped(&self, method: Method, source: Status) -> Attempt<E> {
        match (method, source.code()) {
            (Method::WaitExecution, Code::NotFound) => {
                self.go(Next::Execute);
                Attempt::Again {
                    call: method,
                    detail: "the server no longer knows the operation".to_owned(),
                }
            }
            (_, Code::FailedPrecondition) => match Violations::of_call(&source) {
                Violations::Missing(digests) => self.resend(method, &digests).await,
                Violations::Other => Attempt::Remote(RemoteError::Status {
                    call: method,
                    source,
                }),
            },
            _ => Attempt::Remote(RemoteError::Status {
                call: method,
                source,
            }),
        }
    }

    /// Sends the blobs the server says it lacks, and leaves the next try to call `Execute`.
    async fn resend(&self, method: Method, digests: &[Digest]) -> Attempt<E> {
        let blobs = match (self.supply)(digests) {
            Ok(blobs) => blobs,
            Err(error) => return Attempt::Supply(error),
        };
        if let Err(error) = transfer::write_blobs(self.wire, self.threshold, &blobs).await {
            return Attempt::Remote(error);
        }
        self.go(Next::Execute);
        Attempt::Again {
            call: method,
            detail: "the server lacked blobs of the action, which were sent".to_owned(),
        }
    }

    /// Reads the done message of an operation.
    async fn finish(
        &self,
        method: Method,
        message: Operation,
    ) -> Result<ExecuteResponse, Attempt<E>> {
        let broken = |detail: String| {
            Attempt::Remote(RemoteError::Protocol {
                call: method,
                detail,
            })
        };
        let any = match message.result {
            Some(operation::Result::Response(any)) => any,
            Some(operation::Result::Error(error)) => {
                return Err(broken(format!(
                    "the operation ended with the error {:?}: {}",
                    Code::from_i32(error.code),
                    error.message
                )));
            }
            None => {
                return Err(broken(
                    "the operation is done but holds neither a response nor an error".to_owned(),
                ));
            }
        };
        let response = response_of(&any).map_err(broken)?;
        self.judge(method, response).await
    }

    /// Retries what the status of a response says to retry, and returns the rest.
    async fn judge(
        &self,
        method: Method,
        response: ExecuteResponse,
    ) -> Result<ExecuteResponse, Attempt<E>> {
        let Some(status) = response.status.as_ref() else {
            return Ok(response);
        };
        let code = Code::from_i32(status.code);
        let failed = |status: &rpc::Status| {
            Attempt::Remote(RemoteError::Status {
                call: method,
                source: Status::new(code, status.message.clone()),
            })
        };
        if Retry::transient(code) {
            self.go(Next::Execute);
            return Err(failed(status));
        }
        if code == Code::FailedPrecondition {
            return Err(match Violations::of(&status.details) {
                Violations::Missing(digests) => self.resend(method, &digests).await,
                Violations::Other => failed(status),
            });
        }
        Ok(response)
    }
}

/// Executes `action` and returns the response of the operation that finishes it.
///
/// `supply` is given the digests of the blobs the server says it lacks and returns their
/// bytes, which are sent before `Execute` is called again. The whole call, its pauses
/// included, is bounded by `limit`. Must be polled by the remote runtime.
///
/// # Errors
///
/// Returns [`ExecuteError::Supply`] with what `supply` failed with, and otherwise the failure
/// of a call, [`RemoteError::WaitPassed`] when `limit` passes first, and
/// [`RemoteError::GaveUp`] when the retries are used up.
pub(super) async fn run<E, S>(
    wire: &Wire<'_>,
    threshold: usize,
    limit: Duration,
    action: &Digest,
    supply: S,
) -> Result<ExecuteResponse, ExecuteError<E>>
where
    E: Send,
    S: Fn(&[Digest]) -> Result<Vec<(Digest, Vec<u8>)>, E> + Send + Sync,
{
    let drive = Drive {
        wire,
        threshold,
        request: ExecuteRequest {
            instance_name: wire.instance().to_owned(),
            action_digest: Some(action.clone()),
            skip_cache_lookup: false,
            digest_function: digest_function::Value::Sha256 as i32,
            ..ExecuteRequest::default()
        },
        supply,
        next: Mutex::new(Next::Execute),
    };
    let retry = *wire.retry();
    match tokio::time::timeout(limit, retry.run(|progress| drive.attempt(progress))).await {
        Ok(Ok(response)) => Ok(response),
        Ok(Err(failed)) => Err(match failed.error {
            Attempt::Supply(error) => ExecuteError::Supply(error),
            Attempt::Remote(error) => {
                ExecuteError::Remote(RemoteError::from(super::retry::Failed {
                    error,
                    calls: failed.calls,
                }))
            }
            Attempt::Broke { call, source } => {
                ExecuteError::Remote(RemoteError::from(super::retry::Failed {
                    error: RemoteError::Status { call, source },
                    calls: failed.calls,
                }))
            }
            Attempt::Again { call, detail } => {
                ExecuteError::Remote(RemoteError::from(super::retry::Failed {
                    error: RemoteError::Protocol { call, detail },
                    calls: failed.calls,
                }))
            }
        }),
        Err(_) => Err(ExecuteError::Remote(RemoteError::WaitPassed { limit })),
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use prost::Name;
    use prost_types::Any;

    use super::{Violations, digest_of_subject, names, response_of};
    use crate::remote::proto::build::bazel::remote::execution::v2::{Digest, ExecuteResponse};
    use crate::remote::proto::google::rpc::{PreconditionFailure, precondition_failure::Violation};

    fn violation(kind: &str, subject: &str) -> Violation {
        Violation {
            r#type: kind.to_owned(),
            subject: subject.to_owned(),
            description: String::new(),
        }
    }

    fn failure(violations: Vec<Violation>) -> Any {
        Any::from_msg(&PreconditionFailure { violations }).unwrap()
    }

    fn digest(hash: &str, size: i64) -> Digest {
        Digest {
            hash: hash.to_owned(),
            size_bytes: size,
        }
    }

    #[test]
    fn a_subject_names_a_digest() {
        assert_eq!(digest_of_subject("blobs/ab12/34"), Some(digest("ab12", 34)));
    }

    #[test]
    fn a_digest_function_in_a_subject_is_ignored() {
        assert_eq!(
            digest_of_subject("blobs/sha256/ab12/34"),
            Some(digest("ab12", 34))
        );
    }

    #[test]
    fn a_subject_that_does_not_name_a_blob_names_no_digest() {
        for subject in [
            "",
            "blobs",
            "blobs/ab12",
            "blobs//34",
            "blobs/ab12/many",
            "blobs/ab12/-1",
            "files/ab12/34",
            "blobs/a/b/c/34",
        ] {
            assert_eq!(digest_of_subject(subject), None, "{subject:?}");
        }
    }

    #[test]
    fn missing_violations_name_the_blobs() {
        let details = [failure(vec![
            violation("MISSING", "blobs/aa/1"),
            violation("MISSING", "blobs/bb/2"),
        ])];

        assert_eq!(
            Violations::of(&details),
            Violations::Missing(vec![digest("aa", 1), digest("bb", 2)])
        );
    }

    #[test]
    fn one_other_violation_makes_the_whole_failure_other() {
        let details = [failure(vec![
            violation("MISSING", "blobs/aa/1"),
            violation("UNSUPPORTED", "blobs/bb/2"),
        ])];

        assert_eq!(Violations::of(&details), Violations::Other);
    }

    #[test]
    fn a_missing_violation_whose_subject_names_no_blob_is_other() {
        let details = [failure(vec![violation("MISSING", "somewhere")])];

        assert_eq!(Violations::of(&details), Violations::Other);
    }

    #[test]
    fn details_without_a_precondition_failure_are_other() {
        assert_eq!(Violations::of(&[]), Violations::Other);
        assert_eq!(
            Violations::of(&[failure(Vec::new())]),
            Violations::Other,
            "a failure with no violation says nothing"
        );
        let unrelated = Any {
            type_url: "type.googleapis.com/google.rpc.RetryInfo".to_owned(),
            value: Vec::new(),
        };
        assert_eq!(Violations::of(&[unrelated]), Violations::Other);
    }

    #[test]
    fn a_type_is_named_whatever_prefix_its_url_has() {
        let any = |url: &str| Any {
            type_url: url.to_owned(),
            value: Vec::new(),
        };

        assert!(names(
            &any("type.googleapis.com/google.rpc.Status"),
            "google.rpc.Status"
        ));
        assert!(names(&any("google.rpc.Status"), "google.rpc.Status"));
        assert!(!names(
            &any("type.googleapis.com/google.rpc.Statuses"),
            "google.rpc.Status"
        ));
    }

    #[test]
    fn a_response_is_read_from_its_any() {
        let response = ExecuteResponse {
            cached_result: true,
            ..ExecuteResponse::default()
        };
        let any = Any::from_msg(&response).unwrap();

        assert_eq!(response_of(&any), Ok(response));
    }

    #[test]
    fn a_message_of_another_type_is_not_a_response() {
        let any = Any {
            type_url: Digest::type_url(),
            value: Vec::new(),
        };

        assert!(response_of(&any).is_err());
    }

    #[test]
    fn bytes_that_are_not_a_response_are_refused() {
        let any = Any {
            type_url: ExecuteResponse::type_url(),
            value: vec![0xff, 0xff, 0xff],
        };

        assert!(response_of(&any).is_err());
    }
}
