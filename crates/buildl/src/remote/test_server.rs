//! An in-memory remote cache on loopback, for tests.
//!
//! [`TestServer`] serves the `ActionCache`, `ContentAddressableStorage`, `Capabilities` and
//! `ByteStream` services of the REAPI from memory, on a port the operating system picks and
//! on a runtime of its own. It records every call it receives, can be told to fail or to hold
//! back the answer to a named call, and reports what it advertises through
//! [`ServerOptions`]. It checks the protocol's size rules (a batch within the advertised
//! limit, a write whose bytes match the size in its resource name) so a client that gets them
//! wrong fails here as it would against a real server.
//!
//! The server never verifies that a blob's bytes hash to the digest it is stored under, and
//! it serves the whole blob to a `ByteStream` read whatever limit the request carries.

use std::collections::BTreeMap;
use std::io;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use buildl_core::Digest as BlobDigest;
use tonic::codegen::tokio_stream::{self, Empty, Iter};
use tonic::metadata::KeyAndValueRef;
use tonic::transport::Server;
use tonic::transport::server::TcpIncoming;
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
use super::proto::build::bazel::remote::execution::v2::{
    ActionCacheUpdateCapabilities, ActionResult, BatchReadBlobsRequest, BatchReadBlobsResponse,
    BatchUpdateBlobsRequest, BatchUpdateBlobsResponse, CacheCapabilities, Digest,
    FindMissingBlobsRequest, FindMissingBlobsResponse, GetActionResultRequest,
    GetCapabilitiesRequest, GetChunkMappingRequest, GetChunkMappingResponse, GetTreeRequest,
    GetTreeResponse, RegisterChunkMappingRequest, RegisterChunkMappingResponse, ServerCapabilities,
    SpliceBlobRequest, SpliceBlobResponse, SplitBlobRequest, SplitBlobResponse,
    UpdateActionResultRequest, batch_read_blobs_response, batch_update_blobs_response,
    digest_function,
};
use super::proto::google::bytestream::byte_stream_server::{ByteStream, ByteStreamServer};
use super::proto::google::bytestream::{
    QueryWriteStatusRequest, QueryWriteStatusResponse, ReadRequest, ReadResponse, WriteRequest,
    WriteResponse,
};
use super::proto::google::rpc;

/// The size of one message of a `ByteStream` read answer.
const READ_CHUNK: usize = 256 * 1024;

/// A call of the REAPI that the server answers.
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

/// How a scripted call fails: the status the client sees.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    /// `UNAVAILABLE`.
    Unavailable,
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
    fn status(self) -> Status {
        let code = match self {
            Self::Unavailable => Code::Unavailable,
            Self::Unauthenticated => Code::Unauthenticated,
            Self::PermissionDenied => Code::PermissionDenied,
            Self::NotFound => Code::NotFound,
            Self::Internal => Code::Internal,
        };
        Status::new(code, "scripted failure")
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

/// Whether the server lets a client write action results.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Updates {
    /// `update_enabled` is advertised and `UpdateActionResult` is accepted.
    #[default]
    Enabled,
    /// `update_enabled` is withheld and `UpdateActionResult` is refused.
    Disabled,
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
}

impl Default for ServerOptions {
    fn default() -> Self {
        Self {
            max_batch_total_size_bytes: 4 * 1024 * 1024,
            hashing: Hashing::default(),
            updates: Updates::default(),
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
    /// As [`TestServer::start`].
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
        let router = Server::builder()
            .add_service(CapabilitiesServer::new(service.clone()))
            .add_service(ActionCacheServer::new(service.clone()))
            .add_service(ContentAddressableStorageServer::new(service.clone()))
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
        self.shared.lock().failures.insert(method, fault);
    }

    /// Makes every later call to `method` wait `delay` before it is answered.
    pub fn stall(&self, method: Method, delay: Duration) {
        self.shared.lock().stalls.insert(method, delay);
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

/// What the server holds and what it has been told to do.
#[derive(Debug, Default)]
struct State {
    /// Blob bytes by lowercase hexadecimal hash.
    blobs: BTreeMap<String, Vec<u8>>,
    /// Action results by the hash of the action digest.
    results: BTreeMap<String, ActionResult>,
    calls: Vec<Call>,
    failures: BTreeMap<Method, Fault>,
    stalls: BTreeMap<Method, Duration>,
    in_flight: usize,
    peak_in_flight: usize,
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
                state.failures.get(&method).copied(),
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

/// The four services over one shared state.
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

        let mut state = self.shared.lock();
        let responses = message
            .requests
            .into_iter()
            .map(|each| {
                let status = match each.digest.as_ref() {
                    Some(digest) if usize::try_from(digest.size_bytes) == Ok(each.data.len()) => {
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
                    status: Some(blob.map_or_else(
                        || status_of(Code::NotFound, "no such blob"),
                        |_| status_of(Code::Ok, ""),
                    )),
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
impl ByteStream for Service {
    type ReadStream = Iter<std::vec::IntoIter<Result<ReadResponse, Status>>>;

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
        Ok(Response::new(tokio_stream::iter(messages)))
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
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::time::Instant;

    use tonic::transport::{Channel, Endpoint};

    use super::*;
    use crate::remote::proto::build::bazel::remote::execution::v2::action_cache_client::ActionCacheClient;
    use crate::remote::proto::build::bazel::remote::execution::v2::batch_update_blobs_request::Request as UpdateRequest;
    use crate::remote::proto::build::bazel::remote::execution::v2::capabilities_client::CapabilitiesClient;
    use crate::remote::proto::build::bazel::remote::execution::v2::content_addressable_storage_client::ContentAddressableStorageClient;
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
}
