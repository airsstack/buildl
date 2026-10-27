//! An in-memory remote cache on loopback, for tests.
//!
//! [`TestServer`] serves the `ActionCache`, `ContentAddressableStorage`, `Capabilities`,
//! `ByteStream` and `Execution` services of the REAPI from memory, on a port the operating
//! system picks and on a runtime of its own. It records every call it receives, can be told to
//! fail or to hold back the answer to a named call, and reports what it advertises through
//! [`ServerOptions`], and serves TLS when given a [`TlsIdentity`]. It can delay each message of
//! a `ByteStream` read answer ([`TestServer::pace_stream`]). It checks the protocol's size rules
//! (a batch within the advertised limit, a write whose bytes match the size in its resource
//! name) so a client that gets them wrong fails here as it would against a real server.
//!
//! `Execute` runs the action it names: the `Action`, `Command` and input root are read from the
//! blobs the server holds, the command runs as a process of the machine in a temporary
//! directory with the environment variables of the `Command` and no others, and the logs and
//! the outputs it listed are stored as blobs. An action with a blob missing is refused as the
//! protocol asks, and the result of one that exits with 0 is kept in the action cache. The
//! operation streams `QUEUED`, `EXECUTING` and then the answer, and `WaitExecution` streams the
//! rest of one the server holds. It can be told to break the next streams after their first
//! message ([`TestServer::break_streams`]), to forget every operation
//! ([`TestServer::forget_operations`]), to report a blob missing once, as a call error or in the
//! answer ([`TestServer::report_missing_once`], [`TestServer::report_missing_in_status_once`]),
//! to answer with a status instead of running ([`TestServer::status_in_response`]), and to hold
//! each action back, with its stream silent, before it runs ([`TestServer::hold_execution`]).
//!
//! ```
//! use buildl::remote::test_server::{Method, TestServer};
//!
//! let server = TestServer::start().expect("a loopback port is free");
//! assert!(server.calls_to(Method::GetCapabilities).is_empty());
//! ```
//!
//! The server never verifies that a blob's bytes hash to the digest it is stored under, and
//! it serves the whole blob to a `ByteStream` read whatever limit the request carries.

use std::collections::BTreeMap;
use std::io;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use buildl_core::Digest as BlobDigest;
use prost::Message;
use prost_types::Any;
use tonic::codegen::BoxStream;
use tonic::codegen::tokio_stream::{self, Empty, StreamExt};
use tonic::metadata::KeyAndValueRef;
use tonic::transport::server::TcpIncoming;
use tonic::transport::{Identity, Server, ServerTlsConfig};
use tonic::{Code, Request, Response, Status, Streaming};

use super::proto::build::bazel::remote::execution::v2::action_cache_server::{
    ActionCache, ActionCacheServer,
};
use super::proto::build::bazel::remote::execution::v2::capabilities_server::{
    Capabilities, CapabilitiesServer,
};
use super::proto::build::bazel::remote::execution::v2::content_addressable_storage_server::{
    ContentAddressableStorage, ContentAddressableStorageServer,
};
use super::proto::build::bazel::remote::execution::v2::execution_server::{
    Execution, ExecutionServer,
};
use super::proto::build::bazel::remote::execution::v2::{
    ActionCacheUpdateCapabilities, ActionResult, BatchReadBlobsRequest, BatchReadBlobsResponse,
    BatchUpdateBlobsRequest, BatchUpdateBlobsResponse, CacheCapabilities, Digest,
    ExecuteOperationMetadata, ExecuteRequest, ExecuteResponse, ExecutionCapabilities,
    FindMissingBlobsRequest, FindMissingBlobsResponse, GetActionResultRequest,
    GetCapabilitiesRequest, GetChunkMappingRequest, GetChunkMappingResponse, GetTreeRequest,
    GetTreeResponse, RegisterChunkMappingRequest, RegisterChunkMappingResponse, ServerCapabilities,
    SpliceBlobRequest, SpliceBlobResponse, SplitBlobRequest, SplitBlobResponse,
    UpdateActionResultRequest, WaitExecutionRequest, batch_read_blobs_response,
    batch_update_blobs_response, digest_function, execution_stage,
};
use super::proto::google::bytestream::byte_stream_server::{ByteStream, ByteStreamServer};
use super::proto::google::bytestream::{
    QueryWriteStatusRequest, QueryWriteStatusResponse, ReadRequest, ReadResponse, WriteRequest,
    WriteResponse,
};
use super::proto::google::longrunning::{Operation, operation};
use super::proto::google::rpc;
use super::test_execution::{self, Execution as Run, Refusal};
pub use super::vocabulary::{Method, Updates};

/// The size of one message of a `ByteStream` read answer.
const READ_CHUNK: usize = 256 * 1024;

/// How long a stream waits between two looks at whether its action has finished.
const OPERATION_POLL: Duration = Duration::from_millis(5);

/// How a scripted call fails: the status the client sees.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    /// `UNAVAILABLE`.
    Unavailable,
    /// `RESOURCE_EXHAUSTED`.
    ResourceExhausted,
    /// `ABORTED`.
    Aborted,
    /// `CANCELLED`.
    Cancelled,
    /// `UNAUTHENTICATED`.
    Unauthenticated,
    /// `PERMISSION_DENIED`.
    PermissionDenied,
    /// `NOT_FOUND`.
    NotFound,
    /// `INTERNAL`.
    Internal,
}

impl Fault {
    const fn code(self) -> Code {
        match self {
            Self::Unavailable => Code::Unavailable,
            Self::ResourceExhausted => Code::ResourceExhausted,
            Self::Aborted => Code::Aborted,
            Self::Cancelled => Code::Cancelled,
            Self::Unauthenticated => Code::Unauthenticated,
            Self::PermissionDenied => Code::PermissionDenied,
            Self::NotFound => Code::NotFound,
            Self::Internal => Code::Internal,
        }
    }

    fn status(self) -> Status {
        Status::new(self.code(), "scripted failure")
    }
}

/// The digest function the server advertises.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Hashing {
    /// SHA-256, the only function buildl uses.
    #[default]
    Sha256,
    /// SHA-1 alone, so a client that needs SHA-256 must refuse the server.
    Sha1,
}

/// Whether the server runs the actions it is asked to execute.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Exec {
    /// `exec_enabled` is advertised and `Execute` runs actions.
    #[default]
    Enabled,
    /// `exec_enabled` is withheld and `Execute` is refused.
    Disabled,
}

/// The certificate chain and private key a server presents when it serves TLS, both PEM.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TlsIdentity {
    /// The server's certificate, then any certificates that lead to the authority.
    pub certificate_pem: &'static str,
    /// The private key of the server's certificate.
    pub key_pem: &'static str,
}

/// What the server advertises in `GetCapabilities`, and the limits it enforces to match.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServerOptions {
    /// `max_batch_total_size_bytes`: zero means the server states no limit.
    pub max_batch_total_size_bytes: i64,
    /// The digest function advertised.
    pub hashing: Hashing,
    /// Whether action results may be written.
    pub updates: Updates,
    /// Whether actions may be executed.
    pub exec: Exec,
    /// The identity to serve TLS with; `None` serves plain HTTP/2.
    pub tls: Option<TlsIdentity>,
}

impl Default for ServerOptions {
    fn default() -> Self {
        Self {
            max_batch_total_size_bytes: 4 * 1024 * 1024,
            hashing: Hashing::default(),
            updates: Updates::default(),
            exec: Exec::default(),
            tls: None,
        }
    }
}

/// One call the server received.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Call {
    /// The call.
    pub method: Method,
    /// The request's ASCII metadata, names lowercase, as the server saw it.
    pub headers: BTreeMap<String, String>,
    /// The `ByteStream` resource name, for [`Method::Read`] and [`Method::Write`].
    pub resource_name: Option<String>,
    /// The size of the data in each message of a [`Method::Write`] stream, in arrival order.
    pub message_sizes: Vec<usize>,
}

/// An in-memory remote cache listening on `127.0.0.1`, stopped when dropped.
#[derive(Debug)]
pub struct TestServer {
    address: SocketAddr,
    shared: Arc<Shared>,
    /// Owns the threads that serve; dropping it ends the server.
    _runtime: tokio::runtime::Runtime,
}

impl TestServer {
    /// Starts a server that advertises [`ServerOptions::default`].
    ///
    /// # Errors
    ///
    /// Returns the operating system's error when the runtime or the listening socket cannot be
    /// created.
    pub fn start() -> io::Result<Self> {
        Self::start_with(ServerOptions::default())
    }

    /// Starts a server that advertises `options`.
    ///
    /// # Errors
    ///
    /// As [`TestServer::start`], and also when `options` carries a TLS identity that cannot be
    /// used.
    pub fn start_with(options: ServerOptions) -> io::Result<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("buildl-test-server")
            .enable_all()
            .build()?;
        let incoming = {
            let _entered = runtime.enter();
            TcpIncoming::bind(SocketAddr::from(([127, 0, 0, 1], 0)))?
        };
        let address = incoming.local_addr()?;

        let shared = Arc::new(Shared {
            options,
            state: Mutex::new(State::default()),
        });
        let service = Service {
            shared: Arc::clone(&shared),
        };
        let mut builder = Server::builder();
        if let Some(identity) = options.tls {
            let identity = Identity::from_pem(identity.certificate_pem, identity.key_pem);
            builder = builder
                .tls_config(ServerTlsConfig::new().identity(identity))
                .map_err(io::Error::other)?;
        }
        let router = builder
            .add_service(CapabilitiesServer::new(service.clone()))
            .add_service(ActionCacheServer::new(service.clone()))
            .add_service(ContentAddressableStorageServer::new(service.clone()))
            .add_service(ExecutionServer::new(service.clone()))
            .add_service(ByteStreamServer::new(service));
        runtime.spawn(router.serve_with_incoming(incoming));

        Ok(Self {
            address,
            shared,
            _runtime: runtime,
        })
    }

    /// The loopback address the server listens on.
    #[must_use]
    pub const fn address(&self) -> SocketAddr {
        self.address
    }

    /// Every call received so far, in arrival order.
    #[must_use]
    pub fn calls(&self) -> Vec<Call> {
        self.shared.lock().calls.clone()
    }

    /// The calls received so far to `method`, in arrival order.
    #[must_use]
    pub fn calls_to(&self, method: Method) -> Vec<Call> {
        self.calls()
            .into_iter()
            .filter(|call| call.method == method)
            .collect()
    }

    /// The most calls that were being answered at the same moment.
    #[must_use]
    pub fn peak_in_flight(&self) -> usize {
        self.shared.lock().peak_in_flight
    }

    /// Makes every later call to `method` fail with `fault`, after any stall.
    pub fn fail(&self, method: Method, fault: Fault) {
        self.shared.lock().failures.insert(
            method,
            Failing {
                fault,
                remaining: None,
            },
        );
    }

    /// Makes the next `times` calls to `method` fail with `fault`, after any stall, and answers
    /// the calls after them as usual.
    pub fn fail_times(&self, method: Method, fault: Fault, times: u32) {
        self.shared.lock().failures.insert(
            method,
            Failing {
                fault,
                remaining: Some(times),
            },
        );
    }

    /// Makes the next `times` calls to `method`, which is `BatchReadBlobs` or
    /// `BatchUpdateBlobs`, succeed as calls but carry `fault` as the status of every entry of
    /// their answer, and answers the calls after them as usual.
    pub fn fail_entries(&self, method: Method, fault: Fault, times: u32) {
        self.shared.lock().entry_failures.insert(
            method,
            Failing {
                fault,
                remaining: Some(times),
            },
        );
    }

    /// Makes every later call to `method` wait `delay` before it is answered.
    pub fn stall(&self, method: Method, delay: Duration) {
        self.shared.lock().stalls.insert(method, delay);
    }

    /// Makes every later `ByteStream` read wait `delay` before each message of its answer.
    pub fn pace_stream(&self, delay: Duration) {
        self.shared.lock().stream_delay = delay;
    }

    /// Makes the next `n` streams, `Execute` or `WaitExecution` counted in the order they
    /// arrive, send their first message and then end without a `done` message. The operation
    /// stays held and keeps running, so a `WaitExecution` can finish it.
    pub fn break_streams(&self, n: u32) {
        self.shared.lock().broken_streams = n;
    }

    /// Makes every later `WaitExecution` answer `NOT_FOUND`, whatever name it asks for.
    pub fn forget_operations(&self) {
        self.shared.lock().forgetful = true;
    }

    /// Makes the next `Execute` that would run an action answer `FAILED_PRECONDITION` with a
    /// `PreconditionFailure` that names `digest` as missing, and drops the blob so that it has
    /// to be uploaded again. The size in the blob's name is that of the blob the server holds
    /// when the call arrives, or zero when it holds none.
    pub fn report_missing_once(&self, digest: &BlobDigest) {
        self.shared.lock().missing_once = Some(digest.to_string());
    }

    /// As [`TestServer::report_missing_once`], but the call succeeds: the `done` message carries
    /// an `ExecuteResponse` with no result whose `status` is `FAILED_PRECONDITION` and holds the
    /// `PreconditionFailure` in its details.
    pub fn report_missing_in_status_once(&self, digest: &BlobDigest) {
        self.shared.lock().missing_in_status_once = Some(digest.to_string());
    }

    /// Makes the next `n` `Execute` calls answer with an `ExecuteResponse` whose `status` is
    /// `code` and which holds no result. The action is not run.
    pub fn status_in_response(&self, code: Code, n: u32) {
        self.shared.lock().status_responses = Some((code, n));
    }

    /// Makes every later action wait `delay` before it runs. Its `Execute` stream sends nothing
    /// meanwhile, not even its first message, though the response headers are sent at once.
    pub fn hold_execution(&self, delay: Duration) {
        self.shared.lock().hold = delay;
    }

    /// Stores `bytes` as a blob and returns its digest.
    #[must_use]
    pub fn put_blob(&self, bytes: &[u8]) -> BlobDigest {
        let digest = BlobDigest::of(bytes);
        self.shared
            .lock()
            .blobs
            .insert(digest.to_string(), bytes.to_vec());
        digest
    }

    /// The blob stored under `digest`, if any.
    #[must_use]
    pub fn blob(&self, digest: &BlobDigest) -> Option<Vec<u8>> {
        self.shared.lock().blobs.get(&digest.to_string()).cloned()
    }
}

/// A scripted failure of a method: how it fails and for how many more calls.
#[derive(Clone, Copy, Debug)]
struct Failing {
    fault: Fault,
    /// The calls left to fail; `None` fails every call.
    remaining: Option<u32>,
}

impl Failing {
    /// The fault the arriving call is to fail with, counting the call off; `None` once the
    /// scripted calls have all failed.
    const fn next(&mut self) -> Option<Fault> {
        match &mut self.remaining {
            None => Some(self.fault),
            Some(0) => None,
            Some(left) => {
                *left -= 1;
                Some(self.fault)
            }
        }
    }
}

/// What the server holds and what it has been told to do.
#[derive(Debug, Default)]
struct State {
    /// Blob bytes by lowercase hexadecimal hash.
    blobs: BTreeMap<String, Vec<u8>>,
    /// Action results by the hash of the action digest.
    results: BTreeMap<String, ActionResult>,
    calls: Vec<Call>,
    failures: BTreeMap<Method, Failing>,
    /// The batch methods whose answers carry a failed status on each entry.
    entry_failures: BTreeMap<Method, Failing>,
    stalls: BTreeMap<Method, Duration>,
    /// The wait before each message of a `ByteStream` read answer.
    stream_delay: Duration,
    in_flight: usize,
    peak_in_flight: usize,
    /// The executions the server has accepted, by name.
    operations: BTreeMap<String, Held>,
    /// How many operations have been named, so that no name is used twice.
    named: u64,
    /// The streams still to end after their first message.
    broken_streams: u32,
    /// Whether `WaitExecution` answers `NOT_FOUND` for every name.
    forgetful: bool,
    /// The hash of the blob the next run is refused for as a call error.
    missing_once: Option<String>,
    /// The hash of the blob the next run is refused for in the answer.
    missing_in_status_once: Option<String>,
    /// The status the next executions answer with, and how many are left.
    status_responses: Option<(Code, u32)>,
    /// The wait before each action runs.
    hold: Duration,
}

impl State {
    /// Accepts an execution of `action` and returns the name it is known by; `response` is its
    /// answer when that is already known.
    fn open(&mut self, action: &Digest, response: Option<ExecuteResponse>) -> String {
        let name = format!("operations/{}", self.named);
        self.named += 1;
        self.operations.insert(
            name.clone(),
            Held {
                action: action.clone(),
                response,
            },
        );
        name
    }
}

impl State {
    /// Drops the blob stored under `hash` and returns its digest as the protocol names it; the
    /// size is zero when the server holds no such blob.
    fn forget_blob(&mut self, hash: &str) -> Digest {
        let size_bytes = self.blobs.remove(hash).map_or(0, |bytes| size_of(&bytes));
        Digest {
            hash: hash.to_owned(),
            size_bytes,
        }
    }
}

/// An execution the server has accepted, ready to be streamed.
struct Accepted {
    /// The name of its operation.
    name: String,
    /// The stages its stream reports before the answer.
    stages: Vec<execution_stage::Value>,
    /// How long its stream stays silent, and the action unrun, at first.
    hold: Duration,
}

/// An execution the server has accepted.
#[derive(Debug)]
struct Held {
    action: Digest,
    /// The answer, once the action has finished.
    response: Option<ExecuteResponse>,
}

#[derive(Debug)]
struct Shared {
    options: ServerOptions,
    state: Mutex<State>,
}

impl Shared {
    /// The state, usable even when a panicking test thread poisoned the lock: the state is
    /// plain data that no half-finished update leaves inconsistent.
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The fault the entries of the arriving `method` call are to carry, counting the call off.
    fn entry_fault(&self, method: Method) -> Option<Fault> {
        self.lock()
            .entry_failures
            .get_mut(&method)
            .and_then(Failing::next)
    }

    /// Accepts the execution of `action`. The action runs on the server's runtime.
    ///
    /// A scripted status answers at once. A result held for the action answers at once unless
    /// `skip_cache_lookup` is set. A blob scripted as missing refuses the run.
    fn accept(
        self: &Arc<Self>,
        action: &Digest,
        skip_cache_lookup: bool,
    ) -> Result<Accepted, Status> {
        let mut state = self.lock();
        let running = vec![
            execution_stage::Value::Queued,
            execution_stage::Value::Executing,
        ];
        let scripted = match &mut state.status_responses {
            Some((code, left)) if *left > 0 => {
                *left -= 1;
                Some(*code)
            }
            _ => None,
        };
        if let Some(code) = scripted {
            let response = ExecuteResponse {
                status: Some(status_of(code, "scripted status")),
                ..ExecuteResponse::default()
            };
            let name = state.open(action, Some(response));
            return Ok(Accepted {
                name,
                stages: running,
                hold: Duration::ZERO,
            });
        }
        let cached = if skip_cache_lookup {
            None
        } else {
            state.results.get(&action.hash).cloned()
        };
        if let Some(result) = cached {
            let response = ExecuteResponse {
                result: Some(result),
                cached_result: true,
                status: Some(status_of(Code::Ok, "")),
                ..ExecuteResponse::default()
            };
            let name = state.open(action, Some(response));
            return Ok(Accepted {
                name,
                stages: vec![execution_stage::Value::CacheCheck],
                hold: Duration::ZERO,
            });
        }
        if let Some(hash) = state.missing_once.take() {
            let missing = state.forget_blob(&hash);
            return Err(refusal_status(Refusal::Missing(vec![missing])));
        }
        if let Some(hash) = state.missing_in_status_once.take() {
            let missing = state.forget_blob(&hash);
            let response = ExecuteResponse {
                status: Some(missing_status(&[missing])),
                ..ExecuteResponse::default()
            };
            let name = state.open(action, Some(response));
            return Ok(Accepted {
                name,
                stages: running,
                hold: Duration::ZERO,
            });
        }
        let job = test_execution::prepare(&state.blobs, action).map_err(refusal_status)?;
        let name = state.open(action, None);
        let hold = state.hold;
        drop(state);

        let do_not_cache = job.do_not_cache();
        let shared = Arc::clone(self);
        let (operation, action) = (name.clone(), action.clone());
        tokio::spawn(async move {
            tokio::time::sleep(hold).await;
            let outcome = tokio::task::spawn_blocking(move || job.run())
                .await
                .unwrap_or_else(|error| Err(io::Error::other(error)));
            shared.complete(&operation, &action, do_not_cache, outcome);
        });
        Ok(Accepted {
            name,
            stages: running,
            hold,
        })
    }

    /// Stores what a run left behind, keeps its result when the protocol allows, and sets the
    /// answer of the operation named `name`.
    fn complete(&self, name: &str, action: &Digest, do_not_cache: bool, outcome: io::Result<Run>) {
        let mut state = self.lock();
        let response = match outcome {
            Ok(run) => {
                for bytes in &run.blobs {
                    state
                        .blobs
                        .insert(BlobDigest::of(bytes).to_string(), bytes.clone());
                }
                let status = if run.timed_out {
                    status_of(
                        Code::DeadlineExceeded,
                        "the action ran past its timeout and was killed",
                    )
                } else {
                    status_of(Code::Ok, "")
                };
                if run.result.exit_code == 0 && !run.timed_out && !do_not_cache {
                    state
                        .results
                        .insert(action.hash.clone(), run.result.clone());
                }
                ExecuteResponse {
                    result: Some(run.result),
                    status: Some(status),
                    ..ExecuteResponse::default()
                }
            }
            Err(error) => ExecuteResponse {
                status: Some(status_of(Code::Internal, &error.to_string())),
                ..ExecuteResponse::default()
            },
        };
        if let Some(held) = state.operations.get_mut(name) {
            held.response = Some(response);
        }
    }

    /// Counts the arriving stream off the streams scripted to break; true when it is to break.
    fn break_next(&self) -> bool {
        let mut state = self.lock();
        let broken = state.broken_streams > 0;
        state.broken_streams = state.broken_streams.saturating_sub(1);
        broken
    }

    /// The stream of operation `name`: nothing for `hold`, then one message for each of
    /// `stages`, then the answer once the action has finished. A stream that is `broken` ends
    /// after its first message, before the answer.
    fn operation_stream(
        self: &Arc<Self>,
        name: String,
        action: &Digest,
        stages: Vec<execution_stage::Value>,
        hold: Duration,
        broken: bool,
    ) -> BoxStream<Operation> {
        let progress: Vec<_> = stages
            .into_iter()
            .take(if broken { 1 } else { usize::MAX })
            .map(|stage| {
                Ok(Operation {
                    name: name.clone(),
                    metadata: metadata_of(stage, action),
                    done: false,
                    result: None,
                })
            })
            .collect();
        let shared = Arc::clone(self);
        let answer = tokio_stream::once(()).then(move |()| {
            let shared = Arc::clone(&shared);
            let name = name.clone();
            async move { shared.finished(&name).await }
        });
        let silence = tokio_stream::once(())
            .then(move |()| tokio::time::sleep(hold))
            .filter_map(|()| None);
        if broken {
            Box::pin(silence.chain(tokio_stream::iter(progress)))
        } else {
            Box::pin(silence.chain(tokio_stream::iter(progress)).chain(answer))
        }
    }

    /// The `done` message of operation `name`, once its action has finished.
    async fn finished(&self, name: &str) -> Result<Operation, Status> {
        loop {
            let found = self
                .lock()
                .operations
                .get(name)
                .map(|held| (held.action.clone(), held.response.clone()));
            match found {
                None => return Err(Status::not_found(format!("no operation {name}"))),
                Some((action, Some(response))) => {
                    return Ok(Operation {
                        name: name.to_owned(),
                        metadata: metadata_of(execution_stage::Value::Completed, &action),
                        done: true,
                        result: Any::from_msg(&response)
                            .ok()
                            .map(operation::Result::Response),
                    });
                }
                Some((_, None)) => tokio::time::sleep(OPERATION_POLL).await,
            }
        }
    }

    /// Records a call, then holds it for its scripted stall and fails it if so scripted.
    /// The returned guard counts the call as in flight until it is dropped.
    async fn begin(
        self: &Arc<Self>,
        method: Method,
        headers: BTreeMap<String, String>,
        resource_name: Option<String>,
    ) -> Result<InFlight, Status> {
        let (index, stall, fault) = {
            let mut state = self.lock();
            let index = state.calls.len();
            state.calls.push(Call {
                method,
                headers,
                resource_name,
                message_sizes: Vec::new(),
            });
            state.in_flight += 1;
            state.peak_in_flight = state.peak_in_flight.max(state.in_flight);
            (
                index,
                state.stalls.get(&method).copied(),
                state.failures.get_mut(&method).and_then(Failing::next),
            )
        };
        let guard = InFlight {
            shared: Arc::clone(self),
            index,
        };
        if let Some(delay) = stall {
            tokio::time::sleep(delay).await;
        }
        fault.map_or(Ok(guard), |fault| Err(fault.status()))
    }
}

/// A call being answered; counted in [`State::in_flight`] until dropped.
struct InFlight {
    shared: Arc<Shared>,
    /// The call's position in [`State::calls`].
    index: usize,
}

impl InFlight {
    fn record_message_sizes(&self, sizes: Vec<usize>) {
        if let Some(call) = self.shared.lock().calls.get_mut(self.index) {
            call.message_sizes = sizes;
        }
    }
}

impl Drop for InFlight {
    fn drop(&mut self) {
        let mut state = self.shared.lock();
        state.in_flight = state.in_flight.saturating_sub(1);
    }
}

fn headers_of<T>(request: &Request<T>) -> BTreeMap<String, String> {
    request
        .metadata()
        .iter()
        .filter_map(|entry| match entry {
            KeyAndValueRef::Ascii(name, value) => value
                .to_str()
                .ok()
                .map(|value| (name.as_str().to_owned(), value.to_owned())),
            KeyAndValueRef::Binary(..) => None,
        })
        .collect()
}

/// The length of `bytes` as the protocol's signed size.
fn size_of(bytes: &[u8]) -> i64 {
    i64::try_from(bytes.len()).unwrap_or(i64::MAX)
}

/// The hash and size of a blob resource name: `…/blobs/{hash}/{size}`, with or without the
/// upload prefix before `blobs`.
fn parse_blob_resource(name: &str) -> Result<(String, usize), Status> {
    let invalid = || Status::invalid_argument(format!("not a blob resource name: {name}"));
    let mut parts = name.rsplitn(3, '/');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(size), Some(hash), Some(rest)) if rest == "blobs" || rest.ends_with("/blobs") => {
            let size = size.parse().map_err(|_| invalid())?;
            Ok((hash.to_owned(), size))
        }
        _ => Err(invalid()),
    }
}

fn status_of(code: Code, message: &str) -> rpc::Status {
    rpc::Status {
        code: i32::from(code),
        message: message.to_owned(),
        details: Vec::new(),
    }
}

/// The metadata of an operation of `action` in `stage`.
fn metadata_of(stage: execution_stage::Value, action: &Digest) -> Option<Any> {
    Any::from_msg(&ExecuteOperationMetadata {
        stage: stage as i32,
        action_digest: Some(action.clone()),
        ..ExecuteOperationMetadata::default()
    })
    .ok()
}

/// The call error that refuses an action: `FAILED_PRECONDITION` with a `PreconditionFailure`
/// naming each missing blob when blobs are missing, as the protocol asks.
fn refusal_status(refusal: Refusal) -> Status {
    match refusal {
        Refusal::Malformed(message) => Status::invalid_argument(message),
        Refusal::Missing(missing) => {
            let status = missing_status(&missing);
            Status::with_details(
                Code::FailedPrecondition,
                status.message.clone(),
                status.encode_to_vec().into(),
            )
        }
    }
}

/// The `FAILED_PRECONDITION` status whose details hold a `PreconditionFailure` with one
/// `MISSING` violation for each of `missing`.
fn missing_status(missing: &[Digest]) -> rpc::Status {
    let failure = rpc::PreconditionFailure {
        violations: missing
            .iter()
            .map(|digest| rpc::precondition_failure::Violation {
                r#type: "MISSING".to_owned(),
                subject: format!("blobs/{}/{}", digest.hash, digest.size_bytes),
                description: String::new(),
            })
            .collect(),
    };
    rpc::Status {
        code: i32::from(Code::FailedPrecondition),
        message: "blobs the action needs are missing".to_owned(),
        details: Any::from_msg(&failure).ok().into_iter().collect(),
    }
}

fn digest_of(digest: Option<&Digest>) -> Result<&Digest, Status> {
    digest.ok_or_else(|| Status::invalid_argument("a digest is missing"))
}

/// Refuses a batch whose payload exceeds the limit the server advertises.
fn check_batch_limit(limit: i64, total: usize) -> Result<(), Status> {
    if limit > 0 && i64::try_from(total).unwrap_or(i64::MAX) > limit {
        return Err(Status::invalid_argument(format!(
            "batch of {total} bytes exceeds the limit of {limit}"
        )));
    }
    Ok(())
}

/// The five services over one shared state.
#[derive(Clone)]
struct Service {
    shared: Arc<Shared>,
}

#[tonic::async_trait]
impl Capabilities for Service {
    async fn get_capabilities(
        &self,
        request: Request<GetCapabilitiesRequest>,
    ) -> Result<Response<ServerCapabilities>, Status> {
        let _call = self
            .shared
            .begin(Method::GetCapabilities, headers_of(&request), None)
            .await?;
        let options = self.shared.options;
        let function = match options.hashing {
            Hashing::Sha256 => digest_function::Value::Sha256,
            Hashing::Sha1 => digest_function::Value::Sha1,
        };
        Ok(Response::new(ServerCapabilities {
            cache_capabilities: Some(CacheCapabilities {
                digest_functions: vec![function as i32],
                action_cache_update_capabilities: Some(ActionCacheUpdateCapabilities {
                    update_enabled: options.updates == Updates::Enabled,
                }),
                max_batch_total_size_bytes: options.max_batch_total_size_bytes,
                ..CacheCapabilities::default()
            }),
            execution_capabilities: Some(ExecutionCapabilities {
                digest_function: function as i32,
                exec_enabled: options.exec == Exec::Enabled,
                digest_functions: vec![function as i32],
                ..ExecutionCapabilities::default()
            }),
            ..ServerCapabilities::default()
        }))
    }
}

#[tonic::async_trait]
impl ActionCache for Service {
    async fn get_action_result(
        &self,
        request: Request<GetActionResultRequest>,
    ) -> Result<Response<ActionResult>, Status> {
        let _call = self
            .shared
            .begin(Method::GetActionResult, headers_of(&request), None)
            .await?;
        let digest = digest_of(request.get_ref().action_digest.as_ref())?;
        self.shared
            .lock()
            .results
            .get(&digest.hash)
            .cloned()
            .map(Response::new)
            .ok_or_else(|| Status::not_found("no such action result"))
    }

    async fn update_action_result(
        &self,
        request: Request<UpdateActionResultRequest>,
    ) -> Result<Response<ActionResult>, Status> {
        let _call = self
            .shared
            .begin(Method::UpdateActionResult, headers_of(&request), None)
            .await?;
        if self.shared.options.updates == Updates::Disabled {
            return Err(Status::permission_denied("action results are read-only"));
        }
        let message = request.into_inner();
        let digest = digest_of(message.action_digest.as_ref())?;
        let result = message
            .action_result
            .clone()
            .ok_or_else(|| Status::invalid_argument("an action result is missing"))?;
        self.shared
            .lock()
            .results
            .insert(digest.hash.clone(), result.clone());
        Ok(Response::new(result))
    }
}

#[tonic::async_trait]
impl ContentAddressableStorage for Service {
    async fn find_missing_blobs(
        &self,
        request: Request<FindMissingBlobsRequest>,
    ) -> Result<Response<FindMissingBlobsResponse>, Status> {
        let _call = self
            .shared
            .begin(Method::FindMissingBlobs, headers_of(&request), None)
            .await?;
        let held = self.shared.lock();
        let missing_blob_digests = request
            .get_ref()
            .blob_digests
            .iter()
            .filter(|digest| !held.blobs.contains_key(&digest.hash))
            .cloned()
            .collect();
        Ok(Response::new(FindMissingBlobsResponse {
            missing_blob_digests,
        }))
    }

    async fn batch_update_blobs(
        &self,
        request: Request<BatchUpdateBlobsRequest>,
    ) -> Result<Response<BatchUpdateBlobsResponse>, Status> {
        let _call = self
            .shared
            .begin(Method::BatchUpdateBlobs, headers_of(&request), None)
            .await?;
        let message = request.into_inner();
        let total = message.requests.iter().map(|each| each.data.len()).sum();
        check_batch_limit(self.shared.options.max_batch_total_size_bytes, total)?;
        let entry_fault = self.shared.entry_fault(Method::BatchUpdateBlobs);

        let mut state = self.shared.lock();
        let responses = message
            .requests
            .into_iter()
            .map(|each| {
                let status = match (entry_fault, each.digest.as_ref()) {
                    (Some(fault), _) => status_of(fault.code(), "scripted failure"),
                    (None, Some(digest))
                        if usize::try_from(digest.size_bytes) == Ok(each.data.len()) =>
                    {
                        state.blobs.insert(digest.hash.clone(), each.data);
                        status_of(Code::Ok, "")
                    }
                    _ => status_of(
                        Code::InvalidArgument,
                        "the data does not match the digest's size",
                    ),
                };
                batch_update_blobs_response::Response {
                    digest: each.digest,
                    status: Some(status),
                }
            })
            .collect();
        Ok(Response::new(BatchUpdateBlobsResponse { responses }))
    }

    async fn batch_read_blobs(
        &self,
        request: Request<BatchReadBlobsRequest>,
    ) -> Result<Response<BatchReadBlobsResponse>, Status> {
        let _call = self
            .shared
            .begin(Method::BatchReadBlobs, headers_of(&request), None)
            .await?;
        let entry_fault = self.shared.entry_fault(Method::BatchReadBlobs);
        let state = self.shared.lock();
        let responses: Vec<_> = request
            .get_ref()
            .digests
            .iter()
            .map(|digest| {
                let blob = state.blobs.get(&digest.hash);
                batch_read_blobs_response::Response {
                    digest: Some(digest.clone()),
                    data: blob.cloned().unwrap_or_default(),
                    status: Some(match (entry_fault, blob) {
                        (Some(fault), _) => status_of(fault.code(), "scripted failure"),
                        (None, None) => status_of(Code::NotFound, "no such blob"),
                        (None, Some(_)) => status_of(Code::Ok, ""),
                    }),
                    ..batch_read_blobs_response::Response::default()
                }
            })
            .collect();
        let total = responses.iter().map(|each| each.data.len()).sum();
        check_batch_limit(self.shared.options.max_batch_total_size_bytes, total)?;
        Ok(Response::new(BatchReadBlobsResponse { responses }))
    }

    type GetTreeStream = Empty<Result<GetTreeResponse, Status>>;

    async fn get_tree(
        &self,
        _request: Request<GetTreeRequest>,
    ) -> Result<Response<Self::GetTreeStream>, Status> {
        Err(Status::unimplemented("GetTree"))
    }

    async fn split_blob(
        &self,
        _request: Request<SplitBlobRequest>,
    ) -> Result<Response<SplitBlobResponse>, Status> {
        Err(Status::unimplemented("SplitBlob"))
    }

    type GetChunkMappingStream = Empty<Result<GetChunkMappingResponse, Status>>;

    async fn get_chunk_mapping(
        &self,
        _request: Request<GetChunkMappingRequest>,
    ) -> Result<Response<Self::GetChunkMappingStream>, Status> {
        Err(Status::unimplemented("GetChunkMapping"))
    }

    async fn splice_blob(
        &self,
        _request: Request<SpliceBlobRequest>,
    ) -> Result<Response<SpliceBlobResponse>, Status> {
        Err(Status::unimplemented("SpliceBlob"))
    }

    async fn register_chunk_mapping(
        &self,
        _request: Request<Streaming<RegisterChunkMappingRequest>>,
    ) -> Result<Response<RegisterChunkMappingResponse>, Status> {
        Err(Status::unimplemented("RegisterChunkMapping"))
    }
}

#[tonic::async_trait]
impl Execution for Service {
    type ExecuteStream = BoxStream<Operation>;

    async fn execute(
        &self,
        request: Request<ExecuteRequest>,
    ) -> Result<Response<Self::ExecuteStream>, Status> {
        let _call = self
            .shared
            .begin(Method::Execute, headers_of(&request), None)
            .await?;
        if self.shared.options.exec == Exec::Disabled {
            return Err(Status::unimplemented("remote execution is disabled"));
        }
        let message = request.into_inner();
        let action = digest_of(message.action_digest.as_ref())?;
        let accepted = self.shared.accept(action, message.skip_cache_lookup)?;
        let broken = self.shared.break_next();
        Ok(Response::new(self.shared.operation_stream(
            accepted.name,
            action,
            accepted.stages,
            accepted.hold,
            broken,
        )))
    }

    type WaitExecutionStream = BoxStream<Operation>;

    async fn wait_execution(
        &self,
        request: Request<WaitExecutionRequest>,
    ) -> Result<Response<Self::WaitExecutionStream>, Status> {
        let _call = self
            .shared
            .begin(Method::WaitExecution, headers_of(&request), None)
            .await?;
        let name = request.into_inner().name;
        let (action, stage) = {
            let known = self.shared.lock();
            let held = if known.forgetful {
                None
            } else {
                known.operations.get(&name)
            };
            held.map(|held| {
                let stage = if held.response.is_some() {
                    execution_stage::Value::Completed
                } else {
                    execution_stage::Value::Executing
                };
                (held.action.clone(), stage)
            })
        }
        .ok_or_else(|| Status::not_found(format!("no operation {name}")))?;
        let broken = self.shared.break_next();
        Ok(Response::new(self.shared.operation_stream(
            name,
            &action,
            vec![stage],
            Duration::ZERO,
            broken,
        )))
    }
}

#[tonic::async_trait]
impl ByteStream for Service {
    type ReadStream = BoxStream<ReadResponse>;

    async fn read(
        &self,
        request: Request<ReadRequest>,
    ) -> Result<Response<Self::ReadStream>, Status> {
        let resource_name = request.get_ref().resource_name.clone();
        let _call = self
            .shared
            .begin(
                Method::Read,
                headers_of(&request),
                Some(resource_name.clone()),
            )
            .await?;
        if request.get_ref().read_offset != 0 {
            return Err(Status::invalid_argument("a read offset is not supported"));
        }
        let (hash, _size) = parse_blob_resource(&resource_name)?;
        let blob = self
            .shared
            .lock()
            .blobs
            .get(&hash)
            .cloned()
            .ok_or_else(|| Status::not_found("no such blob"))?;
        let messages: Vec<_> = blob
            .chunks(READ_CHUNK)
            .map(|chunk| {
                Ok(ReadResponse {
                    data: chunk.to_vec(),
                })
            })
            .collect();
        let delay = self.shared.lock().stream_delay;
        let paced = tokio_stream::iter(messages).then(move |message| async move {
            tokio::time::sleep(delay).await;
            message
        });
        Ok(Response::new(Box::pin(paced)))
    }

    async fn write(
        &self,
        request: Request<Streaming<WriteRequest>>,
    ) -> Result<Response<WriteResponse>, Status> {
        let headers = headers_of(&request);
        let mut stream = request.into_inner();

        let first = stream.message().await?;
        let resource_name = first.as_ref().map(|message| message.resource_name.clone());
        let call = self
            .shared
            .begin(Method::Write, headers, resource_name.clone())
            .await?;

        let mut data = Vec::new();
        let mut sizes = Vec::new();
        let mut finished = false;
        let mut next = first;
        while let Some(message) = next {
            sizes.push(message.data.len());
            data.extend(message.data);
            finished |= message.finish_write;
            next = stream.message().await?;
        }
        call.record_message_sizes(sizes);

        let (hash, size) = parse_blob_resource(&resource_name.unwrap_or_default())?;
        if !finished {
            return Err(Status::invalid_argument("the write was never finished"));
        }
        if data.len() != size {
            return Err(Status::invalid_argument(format!(
                "{} bytes written for a blob of {size}",
                data.len()
            )));
        }
        let committed_size = size_of(&data);
        self.shared.lock().blobs.insert(hash, data);
        Ok(Response::new(WriteResponse { committed_size }))
    }

    async fn query_write_status(
        &self,
        _request: Request<QueryWriteStatusRequest>,
    ) -> Result<Response<QueryWriteStatusResponse>, Status> {
        Err(Status::unimplemented("QueryWriteStatus"))
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        clippy::panic,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::time::Instant;

    use prost::Name;
    use tonic::transport::{Channel, Endpoint};

    use super::*;
    use crate::remote::proto::build::bazel::remote::execution::v2::action_cache_client::ActionCacheClient;
    use crate::remote::proto::build::bazel::remote::execution::v2::batch_update_blobs_request::Request as UpdateRequest;
    use crate::remote::proto::build::bazel::remote::execution::v2::capabilities_client::CapabilitiesClient;
    use crate::remote::proto::build::bazel::remote::execution::v2::command::EnvironmentVariable;
    use crate::remote::proto::build::bazel::remote::execution::v2::content_addressable_storage_client::ContentAddressableStorageClient;
    use crate::remote::proto::build::bazel::remote::execution::v2::execution_client::ExecutionClient;
    use crate::remote::proto::build::bazel::remote::execution::v2::{
        Action, Command, Directory, FileNode,
    };
    use crate::remote::proto::google::longrunning::{Operation, operation};
    use crate::remote::proto::google::bytestream::byte_stream_client::ByteStreamClient;
    use crate::remote::runtime::Runtime;

    /// A server, the runtime a test drives it with, and one channel to it.
    struct Fixture {
        server: TestServer,
        runtime: Runtime,
        channel: Channel,
    }

    impl Fixture {
        fn new() -> Self {
            Self::with(ServerOptions::default())
        }

        fn with(options: ServerOptions) -> Self {
            let server = TestServer::start_with(options).unwrap();
            let runtime = Runtime::new().unwrap();
            let endpoint = Endpoint::from_shared(format!("http://{}", server.address())).unwrap();
            let channel = runtime.block_on(endpoint.connect()).unwrap();
            Self {
                server,
                runtime,
                channel,
            }
        }

        fn bytestream(&self) -> ByteStreamClient<Channel> {
            ByteStreamClient::new(self.channel.clone())
        }

        fn storage(&self) -> ContentAddressableStorageClient<Channel> {
            ContentAddressableStorageClient::new(self.channel.clone())
        }

        fn write(&self, name: &str, chunks: &[&[u8]]) -> Result<WriteResponse, Status> {
            let last = chunks.len() - 1;
            let messages: Vec<_> = chunks
                .iter()
                .enumerate()
                .map(|(index, chunk)| WriteRequest {
                    resource_name: if index == 0 {
                        name.to_owned()
                    } else {
                        String::new()
                    },
                    finish_write: index == last,
                    data: chunk.to_vec(),
                    ..WriteRequest::default()
                })
                .collect();
            let mut client = self.bytestream();
            self.runtime
                .block_on(client.write(tokio_stream::iter(messages)))
                .map(Response::into_inner)
        }

        fn read(&self, name: &str) -> Result<Vec<u8>, Status> {
            let mut client = self.bytestream();
            self.runtime.block_on(async {
                let mut stream = client
                    .read(ReadRequest {
                        resource_name: name.to_owned(),
                        ..ReadRequest::default()
                    })
                    .await?
                    .into_inner();
                let mut bytes = Vec::new();
                while let Some(message) = stream.message().await? {
                    bytes.extend(message.data);
                }
                Ok(bytes)
            })
        }
    }

    fn proto_digest(bytes: &[u8]) -> Digest {
        Digest {
            hash: BlobDigest::of(bytes).to_string(),
            size_bytes: size_of(bytes),
        }
    }

    fn upload_name(digest: &Digest) -> String {
        format!(
            "uploads/{}/blobs/{}/{}",
            "00000000-0000-4000-8000-000000000000", digest.hash, digest.size_bytes
        )
    }

    fn read_name(digest: &Digest) -> String {
        format!("blobs/{}/{}", digest.hash, digest.size_bytes)
    }

    #[test]
    fn the_test_server_round_trips_a_blob() {
        let fixture = Fixture::new();
        let bytes: Vec<u8> = (0..=255).cycle().take(600 * 1024).collect();
        let digest = proto_digest(&bytes);

        let halves: Vec<&[u8]> = bytes.chunks(300 * 1024).collect();
        let written = fixture.write(&upload_name(&digest), &halves).unwrap();

        assert_eq!(written.committed_size, digest.size_bytes);
        assert_eq!(fixture.read(&read_name(&digest)).unwrap(), bytes);
        let mut storage = fixture.storage();
        let batch = fixture
            .runtime
            .block_on(storage.batch_read_blobs(BatchReadBlobsRequest {
                digests: vec![digest],
                ..BatchReadBlobsRequest::default()
            }))
            .unwrap()
            .into_inner();
        assert_eq!(batch.responses[0].data, bytes);
        assert_eq!(batch.responses[0].status.as_ref().unwrap().code, 0);
    }

    #[test]
    fn a_blob_written_in_a_batch_reads_back_identical() {
        let fixture = Fixture::new();
        let bytes = b"small blob".to_vec();
        let digest = proto_digest(&bytes);
        let mut storage = fixture.storage();

        let answer = fixture
            .runtime
            .block_on(storage.batch_update_blobs(BatchUpdateBlobsRequest {
                requests: vec![UpdateRequest {
                    digest: Some(digest.clone()),
                    data: bytes.clone(),
                    ..Default::default()
                }],
                ..BatchUpdateBlobsRequest::default()
            }))
            .unwrap()
            .into_inner();

        assert_eq!(answer.responses[0].status.as_ref().unwrap().code, 0);
        assert_eq!(fixture.read(&read_name(&digest)).unwrap(), bytes);
        assert_eq!(fixture.server.blob(&BlobDigest::of(&bytes)), Some(bytes));
    }

    #[test]
    fn a_write_whose_size_disagrees_with_its_name_is_refused() {
        let fixture = Fixture::new();
        let digest = Digest {
            hash: BlobDigest::of(b"abc").to_string(),
            size_bytes: 10,
        };
        let refusal = fixture.write(&upload_name(&digest), &[b"abc"]).unwrap_err();
        assert_eq!(refusal.code(), Code::InvalidArgument);
    }

    #[test]
    fn a_missing_blob_is_not_found() {
        let fixture = Fixture::new();
        let digest = proto_digest(b"never stored");
        assert_eq!(
            fixture.read(&read_name(&digest)).unwrap_err().code(),
            Code::NotFound
        );
    }

    #[test]
    fn find_missing_lists_only_the_absent_digests() {
        let fixture = Fixture::new();
        let held = fixture.server.put_blob(b"held");
        let held = Digest {
            hash: held.to_string(),
            size_bytes: 4,
        };
        let absent = proto_digest(b"absent");
        let mut storage = fixture.storage();

        let answer = fixture
            .runtime
            .block_on(storage.find_missing_blobs(FindMissingBlobsRequest {
                blob_digests: vec![held, absent.clone()],
                ..FindMissingBlobsRequest::default()
            }))
            .unwrap()
            .into_inner();

        assert_eq!(answer.missing_blob_digests, [absent]);
    }

    #[test]
    fn an_action_result_is_absent_until_it_is_updated() {
        let fixture = Fixture::new();
        let action = proto_digest(b"an action");
        let mut cache = ActionCacheClient::new(fixture.channel.clone());
        let get = || GetActionResultRequest {
            action_digest: Some(action.clone()),
            ..GetActionResultRequest::default()
        };

        let absent = fixture
            .runtime
            .block_on(cache.get_action_result(get()))
            .unwrap_err();
        assert_eq!(absent.code(), Code::NotFound);

        let result = ActionResult {
            exit_code: 3,
            ..ActionResult::default()
        };
        fixture
            .runtime
            .block_on(cache.update_action_result(UpdateActionResultRequest {
                action_digest: Some(action.clone()),
                action_result: Some(result.clone()),
                ..UpdateActionResultRequest::default()
            }))
            .unwrap();

        let found = fixture
            .runtime
            .block_on(cache.get_action_result(get()))
            .unwrap();
        assert_eq!(found.into_inner(), result);
    }

    #[test]
    fn capabilities_report_the_options_the_server_started_with() {
        let fixture = Fixture::with(ServerOptions {
            max_batch_total_size_bytes: 1024,
            hashing: Hashing::Sha1,
            updates: Updates::Disabled,
            exec: Exec::Enabled,
            tls: None,
        });
        let mut client = CapabilitiesClient::new(fixture.channel.clone());

        let caps = fixture
            .runtime
            .block_on(client.get_capabilities(GetCapabilitiesRequest::default()))
            .unwrap()
            .into_inner()
            .cache_capabilities
            .unwrap();

        assert_eq!(caps.max_batch_total_size_bytes, 1024);
        assert_eq!(caps.digest_functions, [digest_function::Value::Sha1 as i32]);
        assert!(
            !caps
                .action_cache_update_capabilities
                .unwrap()
                .update_enabled
        );
    }

    #[test]
    fn a_batch_over_the_advertised_limit_is_refused() {
        let fixture = Fixture::with(ServerOptions {
            max_batch_total_size_bytes: 8,
            ..ServerOptions::default()
        });
        let held = fixture.server.put_blob(b"0123456789");
        let digest = Digest {
            hash: held.to_string(),
            size_bytes: 10,
        };
        let mut storage = fixture.storage();

        let refusal = fixture
            .runtime
            .block_on(storage.batch_read_blobs(BatchReadBlobsRequest {
                digests: vec![digest],
                ..BatchReadBlobsRequest::default()
            }))
            .unwrap_err();

        assert_eq!(refusal.code(), Code::InvalidArgument);
    }

    #[test]
    fn calls_are_recorded_with_their_headers_and_resource_names() {
        let fixture = Fixture::new();
        let digest = proto_digest(b"x");
        let mut client = fixture.bytestream();
        let mut request = Request::new(ReadRequest {
            resource_name: read_name(&digest),
            ..ReadRequest::default()
        });
        request
            .metadata_mut()
            .insert("authorization", "Bearer s3cret".parse().unwrap());

        let _ = fixture.runtime.block_on(client.read(request));

        let calls = fixture.server.calls_to(Method::Read);
        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0].resource_name.as_deref(),
            Some(read_name(&digest).as_str())
        );
        assert_eq!(
            calls[0].headers.get("authorization").map(String::as_str),
            Some("Bearer s3cret")
        );
        assert_eq!(fixture.server.calls_to(Method::Write), []);
    }

    #[test]
    fn write_chunk_sizes_are_recorded() {
        let fixture = Fixture::new();
        let digest = proto_digest(b"abcdef");
        fixture
            .write(&upload_name(&digest), &[b"ab", b"cd", b"ef"])
            .unwrap();
        assert_eq!(
            fixture.server.calls_to(Method::Write)[0].message_sizes,
            [2, 2, 2]
        );
    }

    #[test]
    fn a_scripted_failure_fails_the_named_call_only() {
        let fixture = Fixture::new();
        fixture
            .server
            .fail(Method::GetCapabilities, Fault::Unavailable);
        let mut client = CapabilitiesClient::new(fixture.channel.clone());

        let failed = fixture
            .runtime
            .block_on(client.get_capabilities(GetCapabilitiesRequest::default()))
            .unwrap_err();
        assert_eq!(failed.code(), Code::Unavailable);

        let mut storage = fixture.storage();
        assert!(
            fixture
                .runtime
                .block_on(storage.find_missing_blobs(FindMissingBlobsRequest::default()))
                .is_ok()
        );
    }

    #[test]
    fn a_failure_scripted_for_some_calls_ends_after_them() {
        let fixture = Fixture::new();
        fixture
            .server
            .fail_times(Method::GetCapabilities, Fault::Aborted, 2);
        let mut client = CapabilitiesClient::new(fixture.channel.clone());

        let codes: Vec<_> = (0..3)
            .map(|_| {
                fixture
                    .runtime
                    .block_on(client.get_capabilities(GetCapabilitiesRequest::default()))
                    .map(|_| ())
                    .map_err(|status| status.code())
            })
            .collect();

        assert_eq!(
            codes,
            [Err(Code::Aborted), Err(Code::Aborted), Ok(())],
            "two scripted failures, then the server answers"
        );
        assert_eq!(fixture.server.calls_to(Method::GetCapabilities).len(), 3);
    }

    #[test]
    fn a_stalled_call_is_held_for_the_scripted_delay() {
        let fixture = Fixture::new();
        fixture
            .server
            .stall(Method::FindMissingBlobs, Duration::from_millis(200));
        let mut storage = fixture.storage();

        let started = Instant::now();
        fixture
            .runtime
            .block_on(storage.find_missing_blobs(FindMissingBlobsRequest::default()))
            .unwrap();

        assert!(started.elapsed() >= Duration::from_millis(200));
    }

    #[test]
    fn resource_names_parse_with_and_without_an_upload_prefix() {
        assert_eq!(
            parse_blob_resource("blobs/abc/5").unwrap(),
            ("abc".to_owned(), 5)
        );
        assert_eq!(
            parse_blob_resource("inst/ance/uploads/u/blobs/abc/5").unwrap(),
            ("abc".to_owned(), 5)
        );
        assert!(parse_blob_resource("abc/5").is_err());
        assert!(parse_blob_resource("blobs/abc/five").is_err());
    }

    /// A command that runs `script` under `/bin/sh` and lists `outputs`.
    fn command(script: &str, outputs: &[&str]) -> Command {
        Command {
            arguments: vec!["/bin/sh".to_owned(), "-c".to_owned(), script.to_owned()],
            output_paths: outputs.iter().map(|path| (*path).to_owned()).collect(),
            ..Command::default()
        }
    }

    impl Fixture {
        fn execution(&self) -> ExecutionClient<Channel> {
            ExecutionClient::new(self.channel.clone())
        }

        /// Stores `message` as a blob and returns its digest.
        fn store<M: Message>(&self, message: &M) -> Digest {
            let bytes = message.encode_to_vec();
            let _ = self.server.put_blob(&bytes);
            proto_digest(&bytes)
        }

        /// Stores an action that runs `command` over a root holding `inputs`.
        fn action(
            &self,
            command: &Command,
            inputs: &[(&str, &[u8])],
            timeout: Option<prost_types::Duration>,
        ) -> Digest {
            let files = inputs
                .iter()
                .map(|(name, bytes)| FileNode {
                    name: (*name).to_owned(),
                    digest: Some(self.store_bytes(bytes)),
                    ..FileNode::default()
                })
                .collect();
            let root = self.store(&Directory {
                files,
                ..Directory::default()
            });
            let command = self.store(command);
            self.store(&Action {
                command_digest: Some(command),
                input_root_digest: Some(root),
                timeout,
                ..Action::default()
            })
        }

        fn store_bytes(&self, bytes: &[u8]) -> Digest {
            let _ = self.server.put_blob(bytes);
            proto_digest(bytes)
        }

        /// Every message of the stream `Execute` answers with.
        fn execute(
            &self,
            action: &Digest,
            skip_cache_lookup: bool,
        ) -> Result<Vec<Operation>, Status> {
            let mut client = self.execution();
            self.runtime.block_on(async {
                let mut stream = client
                    .execute(ExecuteRequest {
                        action_digest: Some(action.clone()),
                        skip_cache_lookup,
                        ..ExecuteRequest::default()
                    })
                    .await?
                    .into_inner();
                let mut messages = Vec::new();
                while let Some(message) = stream.message().await? {
                    messages.push(message);
                }
                Ok(messages)
            })
        }

        /// Every message of the stream `WaitExecution` answers with.
        fn wait(&self, name: &str) -> Result<Vec<Operation>, Status> {
            let mut client = self.execution();
            self.runtime.block_on(async {
                let mut stream = client
                    .wait_execution(WaitExecutionRequest {
                        name: name.to_owned(),
                    })
                    .await?
                    .into_inner();
                let mut messages = Vec::new();
                while let Some(message) = stream.message().await? {
                    messages.push(message);
                }
                Ok(messages)
            })
        }
    }

    /// The stage an operation message reports.
    fn stage_of(message: &Operation) -> i32 {
        let metadata = message.metadata.as_ref().unwrap();
        assert_eq!(metadata.type_url, ExecuteOperationMetadata::type_url());
        ExecuteOperationMetadata::decode(metadata.value.as_slice())
            .unwrap()
            .stage
    }

    /// The response the last message of a finished stream carries.
    fn response_of(messages: &[Operation]) -> ExecuteResponse {
        let last = messages.last().unwrap();
        assert!(last.done, "the last message is the done one");
        match last.result.as_ref().unwrap() {
            operation::Result::Response(any) => {
                assert_eq!(any.type_url, ExecuteResponse::type_url());
                ExecuteResponse::decode(any.value.as_slice()).unwrap()
            }
            operation::Result::Error(error) => {
                panic!("an operation never carries an error: {error:?}")
            }
        }
    }

    #[test]
    fn an_action_is_run_and_its_outputs_stored() {
        let fixture = Fixture::new();
        let action = fixture.action(&command("printf hi > out", &["out"]), &[], None);

        let response = response_of(&fixture.execute(&action, false).unwrap());

        let result = response.result.unwrap();
        assert_eq!(result.exit_code, 0);
        assert!(!response.cached_result);
        assert_eq!(response.status.unwrap().code, 0);
        assert_eq!(result.output_files.len(), 1);
        assert_eq!(result.output_files[0].path, "out");
        assert_eq!(result.output_files[0].digest, Some(proto_digest(b"hi")));
        assert_eq!(
            fixture.server.blob(&BlobDigest::of(b"hi")),
            Some(b"hi".to_vec())
        );
    }

    #[test]
    fn the_action_sees_only_its_environment() {
        let fixture = Fixture::new();
        let mut command = command("env > out", &["out"]);
        command.environment_variables = vec![EnvironmentVariable {
            name: "FOO".to_owned(),
            value: "1".to_owned(),
        }];
        let action = fixture.action(&command, &[], None);

        let response = response_of(&fixture.execute(&action, false).unwrap());

        let digest = response.result.unwrap().output_files[0]
            .digest
            .clone()
            .unwrap();
        let listing = String::from_utf8(fixture.read(&read_name(&digest)).unwrap()).unwrap();
        assert!(listing.lines().any(|line| line == "FOO=1"), "{listing}");
        assert!(
            !listing.lines().any(|line| line.starts_with("HOME=")),
            "{listing}"
        );
        assert!(
            !listing.lines().any(|line| line.starts_with("PATH=")),
            "{listing}"
        );
    }

    #[test]
    fn an_action_reads_its_input_files() {
        let fixture = Fixture::new();
        let action = fixture.action(
            &command("cat in > out", &["out"]),
            &[("in", b"from the root")],
            None,
        );

        let response = response_of(&fixture.execute(&action, false).unwrap());

        let result = response.result.unwrap();
        assert_eq!(
            result.output_files[0].digest,
            Some(proto_digest(b"from the root"))
        );
    }

    #[test]
    fn an_action_with_a_non_zero_exit_reports_it() {
        let fixture = Fixture::new();
        let action = fixture.action(&command("printf oops >&2; exit 3", &[]), &[], None);

        let first = response_of(&fixture.execute(&action, false).unwrap());
        let second = response_of(&fixture.execute(&action, false).unwrap());

        let result = first.result.unwrap();
        assert_eq!(result.exit_code, 3);
        assert_eq!(first.status.unwrap().code, 0);
        assert_eq!(result.stderr_digest, Some(proto_digest(b"oops")));
        assert_eq!(
            fixture.server.blob(&BlobDigest::of(b"oops")),
            Some(b"oops".to_vec())
        );
        assert!(!second.cached_result, "a failed run is not cached");
    }

    #[test]
    fn a_held_result_is_answered_from_the_cache() {
        let fixture = Fixture::new();
        let action = fixture.action(&command("printf hi > out", &["out"]), &[], None);

        let first = response_of(&fixture.execute(&action, false).unwrap());
        let second = response_of(&fixture.execute(&action, false).unwrap());
        let skipped = response_of(&fixture.execute(&action, true).unwrap());

        assert!(!first.cached_result);
        assert!(second.cached_result);
        assert_eq!(second.result, first.result);
        assert!(
            !skipped.cached_result,
            "skip_cache_lookup runs the action again"
        );
    }

    #[test]
    fn a_missing_input_is_a_precondition_failure_naming_it() {
        let fixture = Fixture::new();
        let input = b"never uploaded";
        let command_digest = proto_digest(&command("true", &[]).encode_to_vec());
        let root = fixture.store(&Directory {
            files: vec![FileNode {
                name: "in".to_owned(),
                digest: Some(proto_digest(input)),
                ..FileNode::default()
            }],
            ..Directory::default()
        });
        let action = fixture.store(&Action {
            command_digest: Some(command_digest.clone()),
            input_root_digest: Some(root),
            ..Action::default()
        });

        let refusal = fixture.execute(&action, false).unwrap_err();

        assert_eq!(refusal.code(), Code::FailedPrecondition);
        let status = rpc::Status::decode(refusal.details()).unwrap();
        assert_eq!(status.code, i32::from(Code::FailedPrecondition));
        assert_eq!(status.details.len(), 1);
        assert_eq!(
            status.details[0].type_url,
            rpc::PreconditionFailure::type_url()
        );
        let failure = rpc::PreconditionFailure::decode(status.details[0].value.as_slice()).unwrap();
        assert!(
            failure
                .violations
                .iter()
                .all(|each| each.r#type == "MISSING")
        );
        let mut subjects: Vec<_> = failure
            .violations
            .iter()
            .map(|each| each.subject.clone())
            .collect();
        subjects.sort();
        let mut expected = vec![
            format!(
                "blobs/{}/{}",
                command_digest.hash, command_digest.size_bytes
            ),
            format!(
                "blobs/{}/{}",
                proto_digest(input).hash,
                proto_digest(input).size_bytes
            ),
        ];
        expected.sort();
        assert_eq!(subjects, expected);
    }

    #[test]
    fn an_action_past_its_timeout_is_deadline_exceeded() {
        let fixture = Fixture::new();
        let timeout = prost_types::Duration {
            seconds: 0,
            nanos: 800_000_000,
        };
        let action = fixture.action(
            &command("printf partial; exec sleep 30", &[]),
            &[],
            Some(timeout),
        );

        let started = Instant::now();
        let first = response_of(&fixture.execute(&action, false).unwrap());
        let second = response_of(&fixture.execute(&action, false).unwrap());

        assert!(
            started.elapsed() < Duration::from_secs(20),
            "the action was killed"
        );
        assert_eq!(
            first.status.unwrap().code,
            i32::from(Code::DeadlineExceeded)
        );
        assert_eq!(
            first.result.unwrap().stdout_digest,
            Some(proto_digest(b"partial"))
        );
        assert!(!second.cached_result, "a timed-out run is not cached");
    }

    #[test]
    fn the_stream_reports_queued_executing_then_done() {
        let fixture = Fixture::new();
        let action = fixture.action(&command("true", &[]), &[], None);

        let messages = fixture.execute(&action, false).unwrap();

        assert_eq!(messages.len(), 3);
        assert!(!messages[0].done && !messages[1].done && messages[2].done);
        assert_eq!(
            stage_of(&messages[0]),
            execution_stage::Value::Queued as i32
        );
        assert_eq!(
            stage_of(&messages[1]),
            execution_stage::Value::Executing as i32
        );
        assert_eq!(
            stage_of(&messages[2]),
            execution_stage::Value::Completed as i32
        );
        assert!(messages.iter().all(|each| each.name == messages[0].name));
        assert_ne!(messages[0].name, "");
        assert!(
            messages
                .iter()
                .all(|each| !matches!(each.result, Some(operation::Result::Error(_))))
        );
    }

    #[test]
    fn wait_execution_finishes_a_held_operation() {
        let fixture = Fixture::new();
        let action = fixture.action(&command("sleep 0.3; printf hi > out", &["out"]), &[], None);
        let mut client = fixture.execution();
        let name = fixture.runtime.block_on(async {
            let mut stream = client
                .execute(ExecuteRequest {
                    action_digest: Some(action.clone()),
                    ..ExecuteRequest::default()
                })
                .await
                .unwrap()
                .into_inner();
            stream.message().await.unwrap().unwrap().name
        });

        let messages = fixture.wait(&name).unwrap();

        assert!(!messages[0].done, "a stage message comes first");
        let result = response_of(&messages).result.unwrap();
        assert_eq!(result.output_files[0].digest, Some(proto_digest(b"hi")));
        assert_eq!(fixture.server.calls_to(Method::WaitExecution).len(), 1);
        assert_eq!(fixture.server.calls_to(Method::Execute).len(), 1);

        let again = fixture.wait(&name).unwrap();
        assert!(!again[0].done);
        assert_eq!(
            stage_of(&again[0]),
            execution_stage::Value::Completed as i32
        );
        assert_eq!(response_of(&again).result.unwrap(), result);
    }

    #[test]
    fn wait_execution_of_an_unknown_name_is_not_found() {
        let fixture = Fixture::new();
        let refusal = fixture.wait("operations/none").unwrap_err();
        assert_eq!(refusal.code(), Code::NotFound);
    }

    #[test]
    fn capabilities_say_whether_execution_is_enabled() {
        let execution = |options: ServerOptions| {
            let fixture = Fixture::with(options);
            let mut client = CapabilitiesClient::new(fixture.channel.clone());
            fixture
                .runtime
                .block_on(client.get_capabilities(GetCapabilitiesRequest::default()))
                .unwrap()
                .into_inner()
                .execution_capabilities
                .unwrap()
        };

        let enabled = execution(ServerOptions::default());
        assert!(enabled.exec_enabled);
        assert_eq!(
            enabled.digest_function,
            digest_function::Value::Sha256 as i32
        );
        assert_eq!(
            enabled.digest_functions,
            [digest_function::Value::Sha256 as i32]
        );

        let disabled = execution(ServerOptions {
            exec: Exec::Disabled,
            ..ServerOptions::default()
        });
        assert!(!disabled.exec_enabled);

        let sha1 = execution(ServerOptions {
            hashing: Hashing::Sha1,
            ..ServerOptions::default()
        });
        assert_eq!(sha1.digest_functions, [digest_function::Value::Sha1 as i32]);
    }

    /// The name of the operation an `Execute` of `action` opens, read off its first message.
    fn open_operation(fixture: &Fixture, action: &Digest) -> String {
        let mut client = fixture.execution();
        fixture.runtime.block_on(async {
            let mut stream = client
                .execute(ExecuteRequest {
                    action_digest: Some(action.clone()),
                    ..ExecuteRequest::default()
                })
                .await
                .unwrap()
                .into_inner();
            stream.message().await.unwrap().unwrap().name
        })
    }

    /// The blobs a `PreconditionFailure` carried by `status` names as missing.
    fn missing_subjects(status: &rpc::Status) -> Vec<String> {
        assert_eq!(status.code, i32::from(Code::FailedPrecondition));
        assert_eq!(status.details.len(), 1);
        assert_eq!(
            status.details[0].type_url,
            rpc::PreconditionFailure::type_url()
        );
        let failure = rpc::PreconditionFailure::decode(status.details[0].value.as_slice()).unwrap();
        assert!(
            failure
                .violations
                .iter()
                .all(|each| each.r#type == "MISSING")
        );
        failure
            .violations
            .iter()
            .map(|each| each.subject.clone())
            .collect()
    }

    #[test]
    fn a_broken_stream_ends_before_done() {
        let fixture = Fixture::new();
        let action = fixture.action(&command("printf hi > out", &["out"]), &[], None);
        fixture.server.break_streams(1);

        let broken = fixture.execute(&action, false).unwrap();

        assert_eq!(broken.len(), 1);
        assert!(!broken[0].done);
        assert_eq!(stage_of(&broken[0]), execution_stage::Value::Queued as i32);
        let whole = fixture.execute(&action, false).unwrap();
        assert!(whole.last().unwrap().done);
        assert_eq!(response_of(&whole).result.unwrap().exit_code, 0);
    }

    #[test]
    fn a_wait_execution_stream_can_be_broken_too() {
        let fixture = Fixture::new();
        let action = fixture.action(&command("sleep 0.3; printf hi > out", &["out"]), &[], None);
        fixture.server.break_streams(2);

        let first = fixture.execute(&action, false).unwrap();
        assert_eq!(first.len(), 1);
        assert!(!first[0].done);
        let name = first[0].name.clone();

        let second = fixture.wait(&name).unwrap();
        assert_eq!(second.len(), 1);
        assert!(!second[0].done);

        let third = fixture.wait(&name).unwrap();
        assert!(!third[0].done);
        let result = response_of(&third).result.unwrap();
        assert_eq!(result.output_files[0].digest, Some(proto_digest(b"hi")));
        assert_eq!(fixture.server.calls_to(Method::Execute).len(), 1);
        assert_eq!(fixture.server.calls_to(Method::WaitExecution).len(), 2);
    }

    #[test]
    fn forgotten_operations_are_not_found() {
        let fixture = Fixture::new();
        let action = fixture.action(&command("true", &[]), &[], None);
        let name = open_operation(&fixture, &action);
        assert!(fixture.wait(&name).is_ok());

        fixture.server.forget_operations();

        assert_eq!(fixture.wait(&name).unwrap_err().code(), Code::NotFound);
        assert_eq!(
            fixture.wait("operations/other").unwrap_err().code(),
            Code::NotFound
        );
    }

    #[test]
    fn a_blob_reported_missing_once_is_found_after_upload() {
        let fixture = Fixture::new();
        let input: &[u8] = b"an input";
        let action = fixture.action(&command("cat in > out", &["out"]), &[("in", input)], None);
        let digest = BlobDigest::of(input);
        fixture.server.report_missing_once(&digest);

        let refusal = fixture.execute(&action, false).unwrap_err();

        assert_eq!(refusal.code(), Code::FailedPrecondition);
        let status = rpc::Status::decode(refusal.details()).unwrap();
        let wanted = proto_digest(input);
        assert_eq!(
            missing_subjects(&status),
            [format!("blobs/{}/{}", wanted.hash, wanted.size_bytes)]
        );
        assert_eq!(
            fixture.server.blob(&digest),
            None,
            "it must be uploaded again"
        );

        let _ = fixture.server.put_blob(input);
        let response = response_of(&fixture.execute(&action, false).unwrap());
        assert_eq!(response.result.unwrap().exit_code, 0);
        assert_eq!(fixture.server.calls_to(Method::Execute).len(), 2);
    }

    #[test]
    fn a_blob_reported_missing_in_the_status_carries_the_detail() {
        let fixture = Fixture::new();
        let input: &[u8] = b"an input";
        let action = fixture.action(&command("cat in > out", &["out"]), &[("in", input)], None);
        let digest = BlobDigest::of(input);
        fixture.server.report_missing_in_status_once(&digest);

        let messages = fixture.execute(&action, false).unwrap();

        assert!(!messages[0].done, "a stage message comes first");
        let response = response_of(&messages);
        assert_eq!(response.result, None);
        let wanted = proto_digest(input);
        assert_eq!(
            missing_subjects(response.status.as_ref().unwrap()),
            [format!("blobs/{}/{}", wanted.hash, wanted.size_bytes)]
        );
        assert_eq!(
            fixture.server.blob(&digest),
            None,
            "it must be uploaded again"
        );

        let _ = fixture.server.put_blob(input);
        let response = response_of(&fixture.execute(&action, false).unwrap());
        assert_eq!(response.status.unwrap().code, 0);
        assert_eq!(response.result.unwrap().exit_code, 0);
    }

    #[test]
    fn a_status_in_the_response_is_sent_n_times() {
        let fixture = Fixture::new();
        let action = fixture.action(&command("printf hi > out", &["out"]), &[], None);
        fixture.server.status_in_response(Code::Unavailable, 2);

        for _ in 0..2 {
            let messages = fixture.execute(&action, false).unwrap();
            assert!(!messages[0].done);
            let response = response_of(&messages);
            assert_eq!(response.status.unwrap().code, i32::from(Code::Unavailable));
            assert_eq!(response.result, None);
        }
        assert_eq!(fixture.server.blob(&BlobDigest::of(b"hi")), None);

        let response = response_of(&fixture.execute(&action, false).unwrap());
        assert_eq!(response.status.unwrap().code, 0);
        assert_eq!(response.result.unwrap().exit_code, 0);
        assert_eq!(fixture.server.calls_to(Method::Execute).len(), 3);
    }

    #[test]
    fn a_held_execution_sends_nothing_until_it_runs() {
        let fixture = Fixture::new();
        let action = fixture.action(&command("printf hi > out", &["out"]), &[], None);
        let delay = Duration::from_millis(400);
        fixture.server.hold_execution(delay);
        let mut client = fixture.execution();

        let (headers_after, first_after, response) = fixture.runtime.block_on(async {
            let started = Instant::now();
            let mut stream = client
                .execute(ExecuteRequest {
                    action_digest: Some(action.clone()),
                    ..ExecuteRequest::default()
                })
                .await
                .unwrap()
                .into_inner();
            let headers_after = started.elapsed();
            let first = stream.message().await.unwrap().unwrap();
            let first_after = started.elapsed();
            assert!(!first.done);
            let mut messages = vec![first];
            while let Some(message) = stream.message().await.unwrap() {
                messages.push(message);
            }
            (headers_after, first_after, response_of(&messages))
        });

        assert!(
            headers_after < delay,
            "headers came after {headers_after:?}"
        );
        assert!(
            first_after >= delay,
            "first message came after {first_after:?}"
        );
        assert_eq!(response.result.unwrap().exit_code, 0);
    }

    #[test]
    fn an_execute_can_be_failed_like_any_call() {
        let fixture = Fixture::new();
        let action = fixture.action(&command("true", &[]), &[], None);
        fixture
            .server
            .fail_times(Method::Execute, Fault::Unavailable, 1);

        let refusal = fixture.execute(&action, false).unwrap_err();
        assert_eq!(refusal.code(), Code::Unavailable);
        let name = {
            let messages = fixture.execute(&action, false).unwrap();
            messages[0].name.clone()
        };
        assert_eq!(fixture.server.calls_to(Method::Execute).len(), 2);

        fixture.server.fail(Method::WaitExecution, Fault::Internal);
        assert_eq!(fixture.wait(&name).unwrap_err().code(), Code::Internal);
        assert_eq!(fixture.wait(&name).unwrap_err().code(), Code::Internal);
        assert_eq!(fixture.server.calls_to(Method::WaitExecution).len(), 2);
    }
}
