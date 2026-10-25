//! The blocking client of the remote cache.
//!
//! A [`Client`] owns a runtime, the channel to the cache and the login every call carries, and
//! exposes the calls of the REAPI that buildl makes as plain blocking methods over the
//! generated message types: any thread may call them, any number at once. The cache's
//! capabilities are asked for before the first call that needs them and kept. Every unary call
//! is bounded whole by the settings' timeout, and each message of a `ByteStream` read by the
//! same timeout.

use std::future::Future;
use std::path::Path;
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use buildl_core::RemoteSettings;
use tonic::service::interceptor::InterceptedService;
use tonic::transport::Channel;
use tonic::{Code, Status};

use super::capabilities::Capabilities;
use super::channel;
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

/// A channel that adds the login to each call.
type Intercepted = InterceptedService<Channel, Login>;

/// What one call needs: the channel, the login, the instance name and the time allowed.
///
/// It is borrowed from the [`Client`] for the length of a call, and builds the generated
/// service clients, which are cheap to make.
#[derive(Clone, Copy)]
pub(super) struct Wire<'a> {
    channel: &'a Channel,
    login: &'a Login,
    instance: &'a str,
    timeout: Duration,
}

impl Wire<'_> {
    /// The instance name, empty when the settings name none.
    pub(super) const fn instance(&self) -> &str {
        self.instance
    }

    /// The time one call, or one stream message, is allowed.
    pub(super) const fn timeout(&self) -> Duration {
        self.timeout
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

    /// Waits for `call` for at most the timeout.
    ///
    /// The timer is created when this future is first polled, so the future must be driven by
    /// the runtime.
    pub(super) async fn bounded<T>(
        &self,
        name: &'static str,
        call: impl Future<Output = Result<T, Status>>,
    ) -> Result<T, RemoteError> {
        self.bounded_for(name, self.timeout, call).await
    }

    /// Waits for `call` for at most `limit`.
    pub(super) async fn bounded_for<T>(
        &self,
        name: &'static str,
        limit: Duration,
        call: impl Future<Output = Result<T, Status>>,
    ) -> Result<T, RemoteError> {
        match tokio::time::timeout(limit, call).await {
            Ok(Ok(answer)) => Ok(answer),
            Ok(Err(source)) => Err(RemoteError::Status { call: name, source }),
            Err(_) => Err(RemoteError::TimedOut {
                call: name,
                timeout: limit,
            }),
        }
    }
}

/// A connection to a remote cache, usable from any thread.
pub(crate) struct Client {
    runtime: Runtime,
    channel: Channel,
    login: Login,
    instance: String,
    timeout: Duration,
    /// The cache's capabilities, once asked for.
    capabilities: Mutex<Option<Capabilities>>,
}

impl Client {
    /// Connects to the cache `settings` name and reads its token.
    ///
    /// A relative `ca` or token file is resolved against `root`.
    ///
    /// # Errors
    ///
    /// Returns [`RemoteError::Runtime`] when the runtime cannot start,
    /// [`RemoteError::Token`] when the token cannot be read, and [`RemoteError::Channel`] when
    /// the cache cannot be reached.
    pub(crate) fn connect(settings: &RemoteSettings, root: &Path) -> Result<Self, RemoteError> {
        let runtime = Runtime::new().map_err(RemoteError::Runtime)?;
        let login = Login::read(settings, root)?;
        let channel = channel::connect(&runtime, settings, root)?;
        Ok(Self {
            runtime,
            channel,
            login,
            instance: settings
                .instance()
                .map_or_else(String::new, |name| name.as_str().to_owned()),
            timeout: settings.timeout().as_duration(),
            capabilities: Mutex::new(None),
        })
    }

    fn wire(&self) -> Wire<'_> {
        Wire {
            channel: &self.channel,
            login: &self.login,
            instance: &self.instance,
            timeout: self.timeout,
        }
    }

    /// The cache's capabilities, asked for on the first call and kept.
    ///
    /// A failed ask is not kept: the next call asks again.
    fn capabilities(&self) -> Result<Capabilities, RemoteError> {
        let mut held = self
            .capabilities
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(known) = *held {
            return Ok(known);
        }
        let fetched = self.runtime.block_on(Capabilities::fetch(&self.wire()))?;
        *held = Some(fetched);
        drop(held);
        Ok(fetched)
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
        self.runtime
            .block_on(transfer::find_missing(&self.wire(), digests))
    }

    /// Reads the blobs `digests` name, in the same order.
    ///
    /// # Errors
    ///
    /// Returns the failure of a call, or of the first blob that failed.
    pub(crate) fn read_blobs(&self, digests: &[Digest]) -> Result<Vec<Vec<u8>>, RemoteError> {
        let threshold = self.capabilities()?.batch_threshold();
        self.runtime
            .block_on(transfer::read_blobs(&self.wire(), threshold, digests))
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
        self.runtime.block_on(transfer::write_blobs(
            &self.wire(),
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
        let wire = self.wire();
        let request = GetActionResultRequest {
            instance_name: self.instance.clone(),
            action_digest: Some(action.clone()),
            digest_function: digest_function::Value::Sha256 as i32,
            ..GetActionResultRequest::default()
        };
        self.runtime.block_on(async {
            let mut client = wire.cache();
            match wire
                .bounded(
                    "ActionCache.GetActionResult",
                    client.get_action_result(request),
                )
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
        let wire = self.wire();
        let request = UpdateActionResultRequest {
            instance_name: self.instance.clone(),
            action_digest: Some(action.clone()),
            action_result: Some(result),
            digest_function: digest_function::Value::Sha256 as i32,
            ..UpdateActionResultRequest::default()
        };
        self.runtime.block_on(async {
            let mut client = wire.cache();
            wire.bounded(
                "ActionCache.UpdateActionResult",
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

    use std::time::{Duration, Instant};

    use buildl_core::{InstanceName, RemoteSettings, RemoteUrl, Timeout};
    use uuid::Uuid;

    use super::Client;
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
        Client::connect(&settings(server, None, "30s"), root()).unwrap()
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
        let client = Client::connect(&settings(&server, Some("ci"), "30s"), root()).unwrap();
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
        server.stall(Method::FindMissingBlobs, Duration::from_secs(5));
        let client = Client::connect(&settings(&server, None, "200ms"), root()).unwrap();

        let started = Instant::now();
        let error = client.find_missing_blobs(&[digest_of(b"x")]).unwrap_err();

        assert!(
            started.elapsed() < Duration::from_secs(1),
            "{:?}",
            started.elapsed()
        );
        assert!(matches!(error, RemoteError::TimedOut { .. }), "{error:?}");
    }

    #[test]
    fn a_slow_stream_that_keeps_moving_completes() {
        let server = server_with(1024);
        server.pace_stream(Duration::from_millis(120));
        let client = Client::connect(&settings(&server, None, "300ms"), root()).unwrap();
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
}
