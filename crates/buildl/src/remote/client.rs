//! The blocking client of the remote cache.
//!
//! A [`Client`] owns a runtime, the way to the cache and the login every call carries, and
//! exposes the calls of the REAPI that buildl makes as plain blocking methods over the
//! generated message types: any thread may call them, any number at once. The channel to the
//! cache is connected by the first call that needs it, and a failed connect is kept, so a cache
//! that is down fails the first call with the reason and every later one at once. The cache's
//! capabilities are asked for before the first call that needs them and kept, a failed ask
//! included, so a cache that does not answer is found down once and not once per caller. Every
//! unary call is bounded whole by the settings' timeout, each message of a `ByteStream` read by
//! the same timeout, and a `ByteStream` write by the same timeout for each chunk the cache takes.

use std::future::Future;
use std::path::Path;
use std::pin::pin;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use buildl_core::{InstanceName, RemoteSettings};
use tokio::time::Instant;
use tonic::service::interceptor::InterceptedService;
use tonic::transport::Channel;
use tonic::{Code, Status};

use super::capabilities::Capabilities;
use super::channel::{self, Pending};
use super::error::RemoteError;
use super::login::Login;
use super::proto::build::bazel::remote::execution::v2::action_cache_client::ActionCacheClient;
use super::proto::build::bazel::remote::execution::v2::capabilities_client::CapabilitiesClient;
use super::proto::build::bazel::remote::execution::v2::content_addressable_storage_client::ContentAddressableStorageClient;
use super::proto::build::bazel::remote::execution::v2::{
    ActionResult, Digest, GetActionResultRequest, UpdateActionResultRequest, digest_function,
};
use super::proto::google::bytestream::byte_stream_client::ByteStreamClient;
use super::runtime::Runtime;
use super::transfer;
use super::vocabulary::Method;

/// A channel that adds the login to each call.
type Intercepted = InterceptedService<Channel, Login>;

/// When an outbound stream last gave the transport a message.
///
/// The stream stamps it as the transport takes each message; [`Wire::bounded_by_progress`]
/// reads it to tell a write that is moving from one that has stopped.
#[derive(Clone, Debug)]
pub(super) struct Progress {
    last: Arc<Mutex<Instant>>,
}

impl Progress {
    /// Progress as of now.
    pub(super) fn start() -> Self {
        Self {
            last: Arc::new(Mutex::new(Instant::now())),
        }
    }

    /// Notes that a message has just been taken.
    pub(super) fn record(&self) {
        *self.last.lock().unwrap_or_else(PoisonError::into_inner) = Instant::now();
    }

    /// When the last message was taken, or when the progress started if none was.
    pub(super) fn last(&self) -> Instant {
        *self.last.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// What one call needs: the channel, the login, the instance name and the time allowed.
///
/// It is made by the [`Client`] for the length of a call, and builds the generated service
/// clients, which are cheap to make.
#[derive(Clone)]
pub(super) struct Wire<'a> {
    channel: Channel,
    login: &'a Login,
    instance: Option<&'a InstanceName>,
    timeout: Duration,
}

impl Wire<'_> {
    /// The instance name, empty when the settings name none.
    pub(super) fn instance(&self) -> &str {
        self.instance.map_or("", InstanceName::as_str)
    }

    pub(super) fn capabilities(&self) -> CapabilitiesClient<Intercepted> {
        CapabilitiesClient::with_interceptor(self.channel.clone(), self.login.clone())
    }

    pub(super) fn cache(&self) -> ActionCacheClient<Intercepted> {
        ActionCacheClient::with_interceptor(self.channel.clone(), self.login.clone())
    }

    pub(super) fn cas(&self) -> ContentAddressableStorageClient<Intercepted> {
        ContentAddressableStorageClient::with_interceptor(self.channel.clone(), self.login.clone())
    }

    pub(super) fn bytestream(&self) -> ByteStreamClient<Intercepted> {
        ByteStreamClient::with_interceptor(self.channel.clone(), self.login.clone())
    }

    /// Waits for `call`, which sends a stream, for as long as the stream keeps moving.
    ///
    /// The call fails with [`RemoteError::TimedOut`] once a whole timeout has passed since
    /// `progress` last moved: since the transport took the stream's latest message, which
    /// includes the wait for the cache's answer after the last one. A cache that stops reading
    /// stops the transport taking messages, so a stalled write fails within about a timeout
    /// however long the blob is.
    pub(super) async fn bounded_by_progress<T>(
        &self,
        name: Method,
        progress: &Progress,
        call: impl Future<Output = Result<T, Status>>,
    ) -> Result<T, RemoteError> {
        let mut call = pin!(call);
        loop {
            let deadline = progress.last() + self.timeout;
            match tokio::time::timeout_at(deadline, call.as_mut()).await {
                Ok(Ok(answer)) => return Ok(answer),
                Ok(Err(source)) => return Err(RemoteError::Status { call: name, source }),
                Err(_) if progress.last() + self.timeout > Instant::now() => {}
                Err(_) => {
                    return Err(RemoteError::TimedOut {
                        call: name,
                        timeout: self.timeout,
                    });
                }
            }
        }
    }

    /// Waits for `call` for at most the timeout.
    ///
    /// The timer is created when this future is first polled, so the future must be driven by
    /// the runtime.
    pub(super) async fn bounded<T>(
        &self,
        name: Method,
        call: impl Future<Output = Result<T, Status>>,
    ) -> Result<T, RemoteError> {
        match tokio::time::timeout(self.timeout, call).await {
            Ok(Ok(answer)) => Ok(answer),
            Ok(Err(source)) => Err(RemoteError::Status { call: name, source }),
            Err(_) => Err(RemoteError::TimedOut {
                call: name,
                timeout: self.timeout,
            }),
        }
    }
}

/// What asking the cache for its capabilities came to.
#[derive(Clone, Debug)]
enum Asked {
    /// The cache answered.
    Known(Capabilities),
    /// The ask failed, with this reason; the cache is not asked again.
    Failed(String),
}

/// How far the connection to the cache has come.
enum Link {
    /// Not yet connected; the first call that needs the channel connects it.
    Pending(Box<Pending>),
    /// Connected, for every call to share.
    Open(Channel),
}

/// A connection to a remote cache, usable from any thread.
pub(crate) struct Client {
    runtime: Runtime,
    link: Mutex<Link>,
    url: String,
    login: Login,
    instance: Option<InstanceName>,
    timeout: Duration,
    /// The outcome of asking for the cache's capabilities, once asked.
    capabilities: Mutex<Option<Asked>>,
}

impl Client {
    /// Prepares a client of the cache `settings` name, and reads its token.
    ///
    /// Nothing is sent: the channel is connected by the first call that needs it. A relative
    /// `ca` or token file is resolved against `root`.
    ///
    /// # Errors
    ///
    /// Returns [`RemoteError::Runtime`] when the runtime cannot start, [`RemoteError::Token`]
    /// when the token cannot be read, and [`RemoteError::Channel`] when the certificate
    /// authority file cannot be read or the transport refuses the address.
    pub(crate) fn new(settings: &RemoteSettings, root: &Path) -> Result<Self, RemoteError> {
        let runtime = Runtime::new().map_err(RemoteError::Runtime)?;
        let login = Login::read(settings, root).map_err(RemoteError::Token)?;
        let pending = channel::prepare(settings, root)?;
        Ok(Self {
            runtime,
            link: Mutex::new(Link::Pending(Box::new(pending))),
            url: settings.url().to_string(),
            login,
            instance: settings.instance().cloned(),
            timeout: settings.timeout().as_duration(),
            capabilities: Mutex::new(None),
        })
    }

    /// The address of the cache, as the settings spell it.
    pub(crate) fn url(&self) -> &str {
        &self.url
    }

    /// What one call needs, connecting the channel if no call has yet.
    ///
    /// A failed connect is not remembered here: every call asks [`Client::capabilities`] first,
    /// which keeps the failure of its first ask and so keeps this method from being reached
    /// again.
    fn wire(&self) -> Result<Wire<'_>, RemoteError> {
        let mut link = self.link.lock().unwrap_or_else(PoisonError::into_inner);
        let channel = match &*link {
            Link::Open(channel) => channel.clone(),
            Link::Pending(pending) => {
                let channel = pending.open(&self.runtime)?;
                *link = Link::Open(channel.clone());
                channel
            }
        };
        drop(link);
        Ok(Wire {
            channel,
            login: &self.login,
            instance: self.instance.as_ref(),
            timeout: self.timeout,
        })
    }

    /// The cache's capabilities, asked for on the first call and kept.
    ///
    /// A failed ask is kept too. The first caller gets the failure itself; callers that were
    /// waiting, and every later one, get [`RemoteError::Unusable`] at once, so a cache that
    /// does not answer costs one timeout and not one per caller.
    fn capabilities(&self) -> Result<Capabilities, RemoteError> {
        let mut held = self
            .capabilities
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        match held.clone() {
            Some(Asked::Known(known)) => return Ok(known),
            Some(Asked::Failed(reason)) => return Err(RemoteError::Unusable { reason }),
            None => {}
        }
        let asked = match self.wire() {
            Ok(wire) => self.runtime.block_on(Capabilities::fetch(&wire)),
            Err(error) => Err(error),
        };
        *held = Some(match &asked {
            Ok(known) => Asked::Known(*known),
            Err(error) => Asked::Failed(error.to_string()),
        });
        drop(held);
        asked
    }

    /// The members of `digests` the cache does not hold.
    ///
    /// # Errors
    ///
    /// Returns the failure of the call, or of the capabilities it needs.
    pub(crate) fn find_missing_blobs(
        &self,
        digests: &[Digest],
    ) -> Result<Vec<Digest>, RemoteError> {
        self.capabilities()?;
        let wire = self.wire()?;
        self.runtime
            .block_on(transfer::find_missing(&wire, digests))
    }

    /// Reads the blobs `digests` name, in the same order.
    ///
    /// # Errors
    ///
    /// Returns the failure of a call, or of the first blob that failed.
    pub(crate) fn read_blobs(&self, digests: &[Digest]) -> Result<Vec<Vec<u8>>, RemoteError> {
        let threshold = self.capabilities()?.batch_threshold();
        let wire = self.wire()?;
        self.runtime
            .block_on(transfer::read_blobs(&wire, threshold, digests))
    }

    /// Writes `blobs`, each with the digest it is stored under.
    ///
    /// # Errors
    ///
    /// Returns [`RemoteError::UpdatesDisabled`] when the cache does not accept uploads, and
    /// otherwise the failure of a call or of the first blob that failed.
    pub(crate) fn write_blobs(&self, blobs: &[(Digest, Vec<u8>)]) -> Result<(), RemoteError> {
        let capabilities = self.capabilities()?;
        capabilities.require_updates()?;
        let wire = self.wire()?;
        self.runtime.block_on(transfer::write_blobs(
            &wire,
            capabilities.batch_threshold(),
            blobs,
        ))
    }

    /// The result the cache holds for `action`, or `None` when it holds none.
    ///
    /// # Errors
    ///
    /// Returns the failure of the call, or of the capabilities it needs; a `NOT_FOUND` answer
    /// is `None`, not a failure.
    pub(crate) fn get_action_result(
        &self,
        action: &Digest,
    ) -> Result<Option<ActionResult>, RemoteError> {
        self.capabilities()?;
        let wire = self.wire()?;
        let request = GetActionResultRequest {
            instance_name: wire.instance().to_owned(),
            action_digest: Some(action.clone()),
            digest_function: digest_function::Value::Sha256 as i32,
            ..GetActionResultRequest::default()
        };
        self.runtime.block_on(async {
            let mut client = wire.cache();
            match wire
                .bounded(Method::GetActionResult, client.get_action_result(request))
                .await
            {
                Ok(found) => Ok(Some(found.into_inner())),
                Err(RemoteError::Status { source, .. }) if source.code() == Code::NotFound => {
                    Ok(None)
                }
                Err(other) => Err(other),
            }
        })
    }

    /// Stores `result` as the result of `action` and returns what the cache keeps.
    ///
    /// # Errors
    ///
    /// Returns [`RemoteError::UpdatesDisabled`] when the cache does not accept updates, and
    /// otherwise the failure of the call.
    pub(crate) fn update_action_result(
        &self,
        action: &Digest,
        result: ActionResult,
    ) -> Result<ActionResult, RemoteError> {
        self.capabilities()?.require_updates()?;
        let wire = self.wire()?;
        let request = UpdateActionResultRequest {
            instance_name: wire.instance().to_owned(),
            action_digest: Some(action.clone()),
            action_result: Some(result),
            digest_function: digest_function::Value::Sha256 as i32,
            ..UpdateActionResultRequest::default()
        };
        self.runtime.block_on(async {
            let mut client = wire.cache();
            wire.bounded(
                Method::UpdateActionResult,
                client.update_action_result(request),
            )
            .await
            .map(tonic::Response::into_inner)
        })
    }
}

#[cfg(all(test, feature = "test-server"))]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::net::TcpListener;
    use std::thread;
    use std::time::{Duration, Instant};

    use buildl_core::{HostPath, InstanceName, RemoteSettings, RemoteUrl, Timeout, TokenSource};
    use uuid::Uuid;

    use super::Client;
    use crate::remote::channel::ChannelError;
    use crate::remote::error::RemoteError;
    use crate::remote::proto::build::bazel::remote::execution::v2::{ActionResult, Digest};
    use crate::remote::test_server::{Fault, Hashing, Method, ServerOptions, TestServer, Updates};
    use crate::remote::tls_fixture::root;

    const MIB: usize = 1024 * 1024;

    fn settings(server: &TestServer, instance: Option<&str>, timeout: &str) -> RemoteSettings {
        let url = RemoteUrl::parse(&format!("grpc://{}", server.address())).unwrap();
        let mut builder = RemoteSettings::builder(url).timeout(Timeout::parse(timeout).unwrap());
        if let Some(instance) = instance {
            builder = builder.instance(InstanceName::parse(instance).unwrap());
        }
        builder.build().unwrap()
    }

    fn client(server: &TestServer) -> Client {
        Client::new(&settings(server, None, "30s"), root()).unwrap()
    }

    fn server_with(limit: i64) -> TestServer {
        TestServer::start_with(ServerOptions {
            max_batch_total_size_bytes: limit,
            ..ServerOptions::default()
        })
        .unwrap()
    }

    fn digest_of(bytes: &[u8]) -> Digest {
        Digest {
            hash: buildl_core::Digest::of(bytes).to_string(),
            size_bytes: i64::try_from(bytes.len()).unwrap(),
        }
    }

    fn bytes(len: usize, seed: u8) -> Vec<u8> {
        (0..len)
            .map(|index| u8::try_from(index % 251).unwrap().wrapping_add(seed))
            .collect()
    }

    /// Stores each of `contents` on `server` and returns their digests.
    fn put_all(server: &TestServer, contents: &[Vec<u8>]) -> Vec<Digest> {
        contents
            .iter()
            .map(|data| {
                let _ = server.put_blob(data);
                digest_of(data)
            })
            .collect()
    }

    fn dead_settings(timeout: &str) -> RemoteSettings {
        let address = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap();
        let url = RemoteUrl::parse(&format!("grpc://{address}")).unwrap();
        RemoteSettings::builder(url)
            .timeout(Timeout::parse(timeout).unwrap())
            .build()
            .unwrap()
    }

    #[test]
    fn a_dead_address_is_a_failure_of_the_first_call_and_not_of_the_client() {
        let client = Client::new(&dead_settings("2s"), root()).unwrap();

        let first = client.find_missing_blobs(&[]).unwrap_err();
        let second = client.get_action_result(&digest_of(b"x")).unwrap_err();

        assert!(
            matches!(first, RemoteError::Channel(ChannelError::Connect { .. })),
            "{first:?}"
        );
        assert!(
            matches!(second, RemoteError::Unusable { .. }),
            "a failed connect is remembered: {second:?}"
        );
    }

    #[test]
    fn a_missing_token_is_refused_before_any_call() {
        let url = RemoteUrl::parse("grpcs://127.0.0.1:1").unwrap();
        let settings = RemoteSettings::builder(url)
            .token(TokenSource::File(HostPath::parse("no-such-token").unwrap()))
            .build()
            .unwrap();

        let outcome = Client::new(&settings, root());

        assert!(
            matches!(outcome, Err(RemoteError::Token(_))),
            "{:?}",
            outcome.map(|_| ())
        );
    }

    #[test]
    fn the_channel_connects_on_the_first_call_and_serves_the_rest() {
        let server = TestServer::start().unwrap();
        let client = client(&server);

        let digests = [digest_of(b"x")];

        client.find_missing_blobs(&digests).unwrap();
        client.find_missing_blobs(&digests).unwrap();

        assert_eq!(server.calls_to(Method::FindMissingBlobs).len(), 2);
    }

    #[test]
    fn a_server_without_sha256_is_refused() {
        let server = TestServer::start_with(ServerOptions {
            hashing: Hashing::Sha1,
            ..ServerOptions::default()
        })
        .unwrap();

        let error = client(&server).find_missing_blobs(&[]).unwrap_err();

        assert!(matches!(error, RemoteError::NoSha256), "{error:?}");
        assert!(error.to_string().contains("SHA-256"), "{error}");
    }

    #[test]
    fn uploading_to_a_server_with_updates_disabled_is_refused() {
        let server = TestServer::start_with(ServerOptions {
            updates: Updates::Disabled,
            ..ServerOptions::default()
        })
        .unwrap();
        let client = client(&server);
        let data = bytes(10, 1);
        let digest = digest_of(&data);

        let blobs = client.write_blobs(&[(digest.clone(), data)]).unwrap_err();
        let result = client
            .update_action_result(&digest, ActionResult::default())
            .unwrap_err();

        assert!(matches!(blobs, RemoteError::UpdatesDisabled), "{blobs:?}");
        assert!(matches!(result, RemoteError::UpdatesDisabled), "{result:?}");
        assert_eq!(server.calls_to(Method::BatchUpdateBlobs).len(), 0);
        assert_eq!(server.calls_to(Method::UpdateActionResult).len(), 0);
    }

    #[test]
    fn capabilities_are_asked_for_once() {
        let server = TestServer::start().unwrap();
        let client = client(&server);

        client.find_missing_blobs(&[]).unwrap();
        client.find_missing_blobs(&[]).unwrap();

        assert_eq!(server.calls_to(Method::GetCapabilities).len(), 1);
    }

    #[test]
    fn small_blobs_travel_in_one_batch() {
        let server = TestServer::start().unwrap();
        let contents = [bytes(10, 1), bytes(20, 2), bytes(30, 3)];
        let digests = put_all(&server, &contents);

        let read = client(&server).read_blobs(&digests).unwrap();

        assert_eq!(read, contents);
        assert_eq!(server.calls_to(Method::BatchReadBlobs).len(), 1);
        assert_eq!(server.calls_to(Method::Read).len(), 0);
    }

    #[test]
    fn small_blobs_are_written_in_one_batch() {
        let server = TestServer::start().unwrap();
        let contents = [bytes(10, 1), bytes(20, 2), bytes(30, 3)];
        let blobs: Vec<_> = contents
            .iter()
            .map(|data| (digest_of(data), data.clone()))
            .collect();

        client(&server).write_blobs(&blobs).unwrap();

        assert_eq!(server.calls_to(Method::BatchUpdateBlobs).len(), 1);
        assert_eq!(server.calls_to(Method::Write).len(), 0);
        for data in &contents {
            let stored = server.blob(&buildl_core::Digest::of(data));
            assert_eq!(stored.as_deref(), Some(data.as_slice()));
        }
    }

    #[test]
    fn a_blob_at_the_threshold_batches_and_one_byte_more_streams() {
        let server = server_with(1024);
        let client = client(&server);
        let at = bytes(1024, 4);
        let over = bytes(1025, 5);
        let digests = put_all(&server, &[at.clone(), over.clone()]);

        assert_eq!(client.read_blobs(&digests[..1]).unwrap(), [at]);
        assert_eq!(server.calls_to(Method::BatchReadBlobs).len(), 1);
        assert_eq!(server.calls_to(Method::Read).len(), 0);

        assert_eq!(client.read_blobs(&digests[1..]).unwrap(), [over]);
        assert_eq!(server.calls_to(Method::BatchReadBlobs).len(), 1);
        assert_eq!(server.calls_to(Method::Read).len(), 1);
    }

    #[test]
    fn blobs_that_do_not_fit_one_batch_split_across_batches() {
        let server = server_with(1024);
        let contents = [bytes(600, 1), bytes(600, 2), bytes(300, 3)];
        let digests = put_all(&server, &contents);

        let read = client(&server).read_blobs(&digests).unwrap();

        assert_eq!(read, contents);
        assert_eq!(server.calls_to(Method::BatchReadBlobs).len(), 2);
        assert_eq!(server.calls_to(Method::Read).len(), 0);
    }

    #[test]
    fn no_server_limit_batches_up_to_the_message_cap() {
        let server = server_with(0);
        let client = client(&server);
        let data = bytes(MIB, 6);
        let digest = digest_of(&data);

        client
            .write_blobs(&[(digest.clone(), data.clone())])
            .unwrap();
        let read = client.read_blobs(&[digest]).unwrap();

        assert_eq!(read, [data]);
        assert_eq!(server.calls_to(Method::BatchUpdateBlobs).len(), 1);
        assert_eq!(server.calls_to(Method::BatchReadBlobs).len(), 1);
        assert_eq!(server.calls_to(Method::Write).len(), 0);
        assert_eq!(server.calls_to(Method::Read).len(), 0);
    }

    #[test]
    fn digests_that_pass_the_message_cap_are_asked_about_in_several_calls() {
        let server = TestServer::start().unwrap();
        let digests: Vec<_> = (0..60_000_i64)
            .map(|at| Digest {
                hash: format!("{at:064x}"),
                size_bytes: i64::MAX - at,
            })
            .collect();

        let missing = client(&server).find_missing_blobs(&digests).unwrap();

        assert_eq!(
            missing, digests,
            "none is held, so all are missing, in order"
        );
        assert_eq!(server.calls_to(Method::FindMissingBlobs).len(), 2);
    }

    #[test]
    fn a_large_write_is_chunked_under_an_upload_resource_name() {
        let server = server_with(1024);
        let client = client(&server);
        let data = bytes(2 * MIB + 512 * 1024, 7);
        let digest = digest_of(&data);

        client
            .write_blobs(&[(digest.clone(), data.clone())])
            .unwrap();
        client
            .write_blobs(&[(digest.clone(), data.clone())])
            .unwrap();

        let stored = server.blob(&buildl_core::Digest::of(&data));
        assert_eq!(stored.as_ref(), Some(&data));
        let writes = server.calls_to(Method::Write);
        assert_eq!(writes.len(), 2);
        assert_eq!(server.calls_to(Method::BatchUpdateBlobs).len(), 0);
        let size = data.len().to_string();
        let mut uuids = Vec::new();
        for write in &writes {
            assert_eq!(write.message_sizes, [MIB, MIB, 512 * 1024]);
            let name = write.resource_name.as_deref().unwrap();
            let parts: Vec<_> = name.split('/').collect();
            assert_eq!(parts.len(), 5, "{name}");
            assert_eq!(parts[0], "uploads");
            assert_eq!(Uuid::parse_str(parts[1]).unwrap().get_version_num(), 4);
            assert_eq!(&parts[2..], ["blobs", digest.hash.as_str(), size.as_str()]);
            uuids.push(parts[1].to_owned());
        }
        assert_ne!(uuids[0], uuids[1], "an upload name is fresh each time");
    }

    #[test]
    fn the_instance_name_prefixes_resource_names() {
        let server = server_with(1024);
        let client = Client::new(&settings(&server, Some("ci"), "30s"), root()).unwrap();
        let data = bytes(2048, 8);
        let digest = digest_of(&data);

        client.write_blobs(&[(digest.clone(), data)]).unwrap();
        client.read_blobs(std::slice::from_ref(&digest)).unwrap();

        let read = server.calls_to(Method::Read);
        let expected = format!("ci/blobs/{}/{}", digest.hash, digest.size_bytes);
        assert_eq!(read[0].resource_name.as_deref(), Some(expected.as_str()));
        let write = server.calls_to(Method::Write);
        assert!(
            write[0]
                .resource_name
                .as_deref()
                .unwrap()
                .starts_with("ci/uploads/"),
            "{write:?}"
        );
    }

    #[test]
    fn no_instance_drops_the_leading_slash() {
        let server = server_with(1024);
        let client = client(&server);
        let data = bytes(2048, 9);
        let digest = digest_of(&data);

        client.write_blobs(&[(digest.clone(), data)]).unwrap();
        client.read_blobs(std::slice::from_ref(&digest)).unwrap();

        let read = server.calls_to(Method::Read);
        let expected = format!("blobs/{}/{}", digest.hash, digest.size_bytes);
        assert_eq!(read[0].resource_name.as_deref(), Some(expected.as_str()));
        let write = server.calls_to(Method::Write);
        assert!(
            write[0]
                .resource_name
                .as_deref()
                .unwrap()
                .starts_with("uploads/"),
            "{write:?}"
        );
    }

    #[test]
    fn find_missing_returns_only_absent_blobs() {
        let server = TestServer::start().unwrap();
        let held_a = bytes(5, 1);
        let held_b = bytes(6, 2);
        let absent = bytes(7, 3);
        let _ = server.put_blob(&held_a);
        let _ = server.put_blob(&held_b);

        let missing = client(&server)
            .find_missing_blobs(&[digest_of(&held_a), digest_of(&absent), digest_of(&held_b)])
            .unwrap();

        assert_eq!(missing, [digest_of(&absent)]);
    }

    #[test]
    fn an_absent_action_result_is_none() {
        let server = TestServer::start().unwrap();

        let found = client(&server)
            .get_action_result(&digest_of(b"action"))
            .unwrap();

        assert_eq!(found, None);
    }

    #[test]
    fn an_updated_action_result_reads_back() {
        let server = TestServer::start().unwrap();
        let client = client(&server);
        let action = digest_of(b"action");
        let result = ActionResult {
            exit_code: 3,
            ..ActionResult::default()
        };

        client
            .update_action_result(&action, result.clone())
            .unwrap();

        assert_eq!(client.get_action_result(&action).unwrap(), Some(result));
    }

    #[test]
    fn a_failed_call_other_than_not_found_is_an_error() {
        let server = TestServer::start().unwrap();
        server.fail(Method::GetActionResult, Fault::Unavailable);

        let error = client(&server)
            .get_action_result(&digest_of(b"action"))
            .unwrap_err();

        assert!(matches!(error, RemoteError::Status { .. }), "{error:?}");
    }

    #[test]
    fn a_stalled_unary_call_times_out() {
        let server = TestServer::start().unwrap();
        let stall = Duration::from_secs(5);
        server.stall(Method::FindMissingBlobs, stall);
        let client = Client::new(&settings(&server, None, "200ms"), root()).unwrap();

        let started = Instant::now();
        let error = client.find_missing_blobs(&[digest_of(b"x")]).unwrap_err();

        // Ending before the server's stall does means the timeout cut the call short.
        assert!(started.elapsed() < stall, "{:?}", started.elapsed());
        assert!(matches!(error, RemoteError::TimedOut { .. }), "{error:?}");
    }

    #[test]
    fn a_slow_stream_that_keeps_moving_completes() {
        let server = server_with(1024);
        server.pace_stream(Duration::from_millis(100));
        let client = Client::new(&settings(&server, None, "300ms"), root()).unwrap();
        let data = bytes(MIB, 10);
        let digests = put_all(&server, std::slice::from_ref(&data));

        let started = Instant::now();
        let read = client.read_blobs(&digests).unwrap();

        assert_eq!(read, [data]);
        assert!(
            started.elapsed() > Duration::from_millis(300),
            "the stream must outlast one timeout: {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn a_failed_blob_in_a_batch_is_an_error_naming_it() {
        let server = TestServer::start().unwrap();
        let held = bytes(8, 1);
        let absent = bytes(9, 2);
        let _ = server.put_blob(&held);
        let missing = digest_of(&absent);

        let error = client(&server)
            .read_blobs(&[digest_of(&held), missing.clone()])
            .unwrap_err();

        assert!(matches!(error, RemoteError::Blob { .. }), "{error:?}");
        assert!(error.to_string().contains(&missing.hash), "{error}");
    }

    #[test]
    fn the_same_blob_asked_twice_in_one_read_is_answered_at_each_position() {
        let server = TestServer::start().unwrap();
        let contents = [bytes(10, 1), bytes(20, 2)];
        let digests = put_all(&server, &contents);
        let asked = [digests[0].clone(), digests[1].clone(), digests[0].clone()];

        let read = client(&server).read_blobs(&asked).unwrap();

        assert_eq!(
            read,
            [
                contents[0].clone(),
                contents[1].clone(),
                contents[0].clone()
            ]
        );
    }

    #[test]
    fn many_small_blobs_travel_in_batches_no_message_cap_refuses() {
        let server = TestServer::start().unwrap();
        let client = client(&server);
        let contents: Vec<Vec<u8>> = (0..12_000)
            .map(|index| format!("{index:0>300}").into_bytes())
            .collect();
        let blobs: Vec<_> = contents
            .iter()
            .map(|data| (digest_of(data), data.clone()))
            .collect();
        let digests: Vec<_> = blobs.iter().map(|(digest, _)| digest.clone()).collect();

        client.write_blobs(&blobs).unwrap();
        let read = client.read_blobs(&digests).unwrap();

        assert_eq!(read, contents);
        assert!(server.calls_to(Method::BatchUpdateBlobs).len() > 1);
        assert!(server.calls_to(Method::BatchReadBlobs).len() > 1);
    }

    #[test]
    fn a_stalled_stream_write_times_out_within_a_timeout_of_the_last_chunk() {
        let server = server_with(1024);
        let stall = Duration::from_secs(5);
        server.stall(Method::Write, stall);
        let client = Client::new(&settings(&server, None, "250ms"), root()).unwrap();
        let data = bytes(8 * MIB, 11);
        let digest = digest_of(&data);

        let started = Instant::now();
        let error = client.write_blobs(&[(digest, data)]).unwrap_err();

        assert!(matches!(error, RemoteError::TimedOut { .. }), "{error:?}");
        assert!(started.elapsed() < stall, "{:?}", started.elapsed());
    }

    #[test]
    fn a_stalled_stream_read_message_times_out() {
        let server = server_with(1024);
        server.pace_stream(Duration::from_millis(600));
        let client = Client::new(&settings(&server, None, "200ms"), root()).unwrap();
        let data = bytes(MIB, 12);
        let digests = put_all(&server, std::slice::from_ref(&data));

        let error = client.read_blobs(&digests).unwrap_err();

        assert!(matches!(error, RemoteError::TimedOut { .. }), "{error:?}");
    }

    #[test]
    fn concurrent_callers_find_a_stalled_capabilities_ask_failed_once() {
        let server = TestServer::start().unwrap();
        server.stall(Method::GetCapabilities, Duration::from_secs(5));
        let client = Client::new(&settings(&server, None, "300ms"), root()).unwrap();

        let errors: Vec<_> = thread::scope(|scope| {
            let callers: Vec<_> = (0..4)
                .map(|_| scope.spawn(|| client.find_missing_blobs(&[]).unwrap_err()))
                .collect();
            callers
                .into_iter()
                .map(|caller| caller.join().unwrap())
                .collect()
        });

        assert_eq!(server.calls_to(Method::GetCapabilities).len(), 1);
        assert_eq!(errors.len(), 4);
    }
}
