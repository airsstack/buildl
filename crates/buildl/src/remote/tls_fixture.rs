//! A TLS test server and the authority that signs it, for the tests of the channel and the login.
//!
//! The certificates are the files under `tests/fixtures/tls`: a test authority and a server
//! certificate for `127.0.0.1` and `localhost`, both valid for a century. They secure nothing.

#![expect(
    clippy::unwrap_used,
    reason = "test support unwraps known-valid fixtures; a panic is the intended failure signal"
)]

use std::path::Path;

use buildl_core::{HostPath, RemoteSettings, RemoteSettingsBuilder, RemoteUrl};

use super::test_server::{ServerOptions, TestServer, TlsIdentity};

/// The authority's certificate, relative to the crate's directory, which tests use as the
/// workspace root.
pub(super) const CA_RELATIVE: &str = "tests/fixtures/tls/ca.pem";

/// The crate's directory, standing in for a workspace root.
pub(super) fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// A server that serves TLS with the fixture certificate.
pub(super) fn tls_server() -> TestServer {
    let identity = TlsIdentity {
        certificate_pem: include_str!("../../tests/fixtures/tls/server.pem"),
        key_pem: include_str!("../../tests/fixtures/tls/server.key"),
    };
    TestServer::start_with(ServerOptions {
        tls: Some(identity),
        ..ServerOptions::default()
    })
    .unwrap()
}

/// The settings of a `grpcs://` cache at `server`, before a token or authority is named.
pub(super) fn secure_settings(server: &TestServer) -> RemoteSettingsBuilder {
    let url = RemoteUrl::parse(&format!("grpcs://{}", server.address())).unwrap();
    RemoteSettings::builder(url)
}

/// The settings of a `grpcs://` cache at `server` that trusts the fixture authority.
pub(super) fn trusting_settings(server: &TestServer) -> RemoteSettingsBuilder {
    secure_settings(server).ca(HostPath::parse(CA_RELATIVE).unwrap())
}
