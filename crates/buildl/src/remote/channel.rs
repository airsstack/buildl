//! The connection to the remote cache.
//!
//! [`prepare`] turns the workspace's [`RemoteSettings`] into a [`Pending`] connection, and
//! [`Pending::open`] makes the one gRPC channel that every worker shares: `grpc://` is plain
//! HTTP/2, and `grpcs://` is TLS that trusts the system's roots and, when the settings name
//! one, the certificate authority in the file at `ca`. Preparing does not touch the network, so
//! a cache that is down is found when the channel is opened and not when it is prepared.
//!
//! The channel carries no request timeout. A timeout on the channel would bound a streamed
//! transfer as a whole, so each caller applies [`RemoteSettings::timeout`] to the calls it
//! makes: to a unary call whole, to each message a `ByteStream` read waits for, and to the time
//! a `ByteStream` write goes without taking a chunk.

use std::error::Error as StdError;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use buildl_core::{RemoteScheme, RemoteSettings};
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Endpoint};

use super::runtime::Runtime;

/// Why the channel to the remote cache could not be made.
///
/// The text does not name the cache's address: whoever reports the failure puts it in front.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ChannelError {
    /// The file named by `ca` could not be read.
    #[error("cannot read the certificate authority file {}: {source}", .path.display())]
    CaUnreadable {
        /// The file, with a relative name resolved against the workspace root.
        path: PathBuf,
        /// The operating system's reason.
        #[source]
        source: io::Error,
    },
    /// The transport refused the cache's address or the TLS configuration.
    #[error("cannot set up a connection: {reason}")]
    Setup {
        /// What the transport reported, including its causes.
        reason: String,
    },
    /// The cache could not be reached, did not complete its handshakes in time, or failed the TLS
    /// handshake.
    #[error("cannot connect: {reason}")]
    Connect {
        /// What the transport reported, including its causes.
        reason: String,
    },
}

/// The message of `error` followed by each of its causes, separated by `: `.
fn with_causes(error: &(dyn StdError + 'static)) -> String {
    let mut text = error.to_string();
    let mut cause = error.source();
    while let Some(next) = cause {
        text.push_str(": ");
        text.push_str(&next.to_string());
        cause = next.source();
    }
    text
}

/// The cache's address, checked and ready to be connected to.
///
/// Making one reads the certificate authority file and has the transport check the address,
/// but does not touch the network: [`Pending::open`] does that, when a call first needs the
/// channel.
#[derive(Clone, Debug)]
pub(crate) struct Pending {
    endpoint: Endpoint,
    limit: Duration,
}

/// Prepares the connection to the cache `settings` name.
///
/// A relative `ca` is resolved against `root`.
///
/// # Errors
///
/// Returns [`ChannelError::CaUnreadable`] when the `ca` file cannot be read, and
/// [`ChannelError::Setup`] when the transport refuses the address or the certificate.
pub(crate) fn prepare(settings: &RemoteSettings, root: &Path) -> Result<Pending, ChannelError> {
    let url = settings.url();
    let setup = |error: tonic::transport::Error| ChannelError::Setup {
        reason: with_causes(&error),
    };

    let scheme = match url.scheme() {
        RemoteScheme::Grpc => "http",
        RemoteScheme::Grpcs => "https",
    };
    let mut endpoint = Endpoint::from_shared(format!("{scheme}://{}", authority(settings)))
        .map_err(setup)?
        .connect_timeout(settings.timeout().as_duration());
    if url.scheme() == RemoteScheme::Grpcs {
        let mut tls = ClientTlsConfig::new().with_native_roots();
        if let Some(ca) = settings.ca() {
            let path = if ca.is_absolute() {
                PathBuf::from(ca.as_str())
            } else {
                root.join(ca.as_str())
            };
            let pem = std::fs::read(&path).map_err(|source| ChannelError::CaUnreadable {
                path: path.clone(),
                source,
            })?;
            tls = tls.ca_certificate(Certificate::from_pem(pem));
        }
        endpoint = endpoint.tls_config(tls).map_err(setup)?;
    }

    Ok(Pending {
        endpoint,
        limit: settings.timeout().as_duration(),
    })
}

impl Pending {
    /// Connects to the cache, and returns the channel its workers share.
    ///
    /// The handshake is made before this returns, so a cache that is down or whose certificate
    /// is not trusted is an error here and not at the first call over the channel. The whole
    /// attempt, the TCP connection and the TLS and HTTP/2 handshakes together, is bounded by
    /// the settings' timeout.
    ///
    /// # Errors
    ///
    /// Returns [`ChannelError::Connect`] when the cache cannot be reached or does not complete
    /// the TLS handshake in time.
    pub(crate) fn open(&self, runtime: &Runtime) -> Result<Channel, ChannelError> {
        let connect_error = |reason: String| ChannelError::Connect { reason };
        runtime
            .block_on(async { tokio::time::timeout(self.limit, self.endpoint.connect()).await })
            .map_err(|_| connect_error(format!("no answer within {:?}", self.limit)))?
            .map_err(|error| connect_error(with_causes(&error)))
    }
}

/// The `host` or `host:port` of the cache's address.
fn authority(settings: &RemoteSettings) -> String {
    let url = settings.url();
    url.port().map_or_else(
        || url.host().to_owned(),
        |port| format!("{}:{port}", url.host()),
    )
}

#[cfg(all(test, feature = "test-server"))]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::fs;
    use std::net::TcpListener;
    use std::path::Path;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    use buildl_core::{HostPath, RemoteSettings, RemoteUrl, Timeout};
    use tonic::transport::Channel;

    use super::{ChannelError, prepare};
    use crate::remote::proto::build::bazel::remote::execution::v2::GetCapabilitiesRequest;
    use crate::remote::proto::build::bazel::remote::execution::v2::capabilities_client::CapabilitiesClient;
    use crate::remote::runtime::Runtime;
    use crate::remote::test_server::TestServer;
    use crate::remote::tls_fixture::{
        CA_RELATIVE, root, secure_settings, tls_server, trusting_settings,
    };

    /// Prepares the connection and opens it.
    fn connect(
        runtime: &Runtime,
        settings: &RemoteSettings,
        root: &Path,
    ) -> Result<Channel, ChannelError> {
        prepare(settings, root)?.open(runtime)
    }

    /// Makes one `GetCapabilities` call over `channel`.
    fn call(runtime: &Runtime, channel: Channel) -> Result<(), tonic::Status> {
        let mut client = CapabilitiesClient::new(channel);
        runtime
            .block_on(client.get_capabilities(GetCapabilitiesRequest::default()))
            .map(|_| ())
    }

    #[test]
    fn a_plain_url_connects_without_tls() {
        let server = TestServer::start().unwrap();
        let url = RemoteUrl::parse(&format!("grpc://{}", server.address())).unwrap();
        let settings = RemoteSettings::builder(url).build().unwrap();
        let runtime = Runtime::new().unwrap();

        let channel = connect(&runtime, &settings, root()).unwrap();

        call(&runtime, channel).unwrap();
    }

    #[test]
    fn a_dead_address_is_prepared_and_fails_only_when_opened() {
        let address = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap();
        let url = RemoteUrl::parse(&format!("grpc://{address}")).unwrap();
        let settings = RemoteSettings::builder(url)
            .timeout(Timeout::parse("2s").unwrap())
            .build()
            .unwrap();
        let runtime = Runtime::new().unwrap();

        let pending = prepare(&settings, root()).unwrap();
        let outcome = pending.open(&runtime);

        assert!(
            matches!(&outcome, Err(ChannelError::Connect { .. })),
            "{outcome:?}"
        );
    }

    #[test]
    fn a_tls_url_trusts_the_configured_ca() {
        let server = tls_server();
        let settings = trusting_settings(&server).build().unwrap();
        let runtime = Runtime::new().unwrap();

        let channel = connect(&runtime, &settings, root()).unwrap();

        call(&runtime, channel).unwrap();
    }

    #[test]
    fn an_absolute_ca_path_does_not_depend_on_the_workspace_root() {
        let server = tls_server();
        let absolute = root().join(CA_RELATIVE);
        let settings = secure_settings(&server)
            .ca(HostPath::parse(absolute.to_str().unwrap()).unwrap())
            .build()
            .unwrap();
        let runtime = Runtime::new().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();

        let channel = connect(&runtime, &settings, elsewhere.path()).unwrap();

        call(&runtime, channel).unwrap();
    }

    #[test]
    fn a_tls_url_without_the_ca_is_refused() {
        let server = tls_server();
        let settings = secure_settings(&server).build().unwrap();
        let runtime = Runtime::new().unwrap();

        let outcome = connect(&runtime, &settings, root());

        assert!(
            matches!(outcome, Err(ChannelError::Connect { .. })),
            "{outcome:?}"
        );
    }

    #[test]
    fn an_unreadable_ca_file_is_an_error_naming_it() {
        let server = tls_server();
        let settings = secure_settings(&server)
            .ca(HostPath::parse("certs/missing.pem").unwrap())
            .build()
            .unwrap();
        let runtime = Runtime::new().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();

        let error = connect(&runtime, &settings, elsewhere.path()).unwrap_err();

        assert!(
            matches!(&error, ChannelError::CaUnreadable { path, .. } if path.ends_with(Path::new("certs/missing.pem"))),
            "{error:?}"
        );
        assert!(error.to_string().contains("certs/missing.pem"), "{error}");
    }

    #[test]
    fn a_ca_file_that_is_not_a_certificate_is_refused() {
        let server = tls_server();
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("ca.pem"), "not a certificate").unwrap();
        let settings = secure_settings(&server)
            .ca(HostPath::parse("ca.pem").unwrap())
            .build()
            .unwrap();
        let runtime = Runtime::new().unwrap();

        let outcome = connect(&runtime, &settings, dir.path());

        assert!(outcome.is_err(), "{outcome:?}");
    }

    #[test]
    fn a_cache_that_accepts_and_never_answers_times_out() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = RemoteUrl::parse(&format!("grpcs://{}", listener.local_addr().unwrap())).unwrap();
        let settings = RemoteSettings::builder(url)
            .timeout(Timeout::parse("300ms").unwrap())
            .build()
            .unwrap();
        let runtime = Runtime::new().unwrap();
        let (done, outcome) = mpsc::channel();
        thread::spawn(move || {
            let result = connect(&runtime, &settings, root()).map(|_| ());
            let _ = done.send(result);
        });

        // A hang is unbounded: any answer within this wide bound means the 300ms timeout fired.
        let result = outcome.recv_timeout(Duration::from_secs(20)).unwrap();

        assert!(
            matches!(&result, Err(ChannelError::Connect { .. })),
            "{result:?}"
        );
        drop(listener);
    }
}
