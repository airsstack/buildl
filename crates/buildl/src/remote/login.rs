//! The credential every call to the remote cache carries.
//!
//! [`Login`] reads the token the settings name, once, and adds it to each request as the header
//! the settings choose: `Bearer <token>` under `authorization`, the bare token under any other
//! name. It is a tonic interceptor, so the generated clients take it as they are built. The
//! token never appears in a message, a `Debug` rendering or an error.

use std::path::Path;

use buildl_core::{EnvName, Error, HeaderName, RemoteSettings, Result, TokenSource};
use tonic::metadata::{Ascii, MetadataKey, MetadataValue};
use tonic::service::Interceptor;
use tonic::{Request, Status};

/// The header named `authorization`, which carries its token behind the `Bearer` scheme.
const AUTHORIZATION: &str = "authorization";

/// The header a token is sent in, and the value it is sent as.
#[derive(Clone)]
struct Credential {
    name: MetadataKey<Ascii>,
    value: MetadataValue<Ascii>,
}

/// The header added to every call to the remote cache, when the settings name a token.
#[derive(Clone)]
pub(crate) struct Login {
    credential: Option<Credential>,
}

impl Login {
    /// Reads the token `settings` name from the process environment or from a file, with a
    /// relative file resolved against `root`.
    ///
    /// # Errors
    ///
    /// As [`Login::read_with`].
    pub(crate) fn read(settings: &RemoteSettings, root: &Path) -> Result<Self> {
        Self::read_with(settings, root, |name| std::env::var(name.as_str()).ok())
    }

    /// Reads the token `settings` name, taking the value of a variable from `lookup`.
    ///
    /// A token from a file loses one trailing newline. Settings without a token give a login
    /// that adds nothing.
    ///
    /// # Errors
    ///
    /// Returns [`Error::RemoteToken`] when the variable is unset or not text, when the file
    /// cannot be read as text, or when the token cannot travel in a header. The error names the
    /// variable or the file, never the token.
    pub(crate) fn read_with(
        settings: &RemoteSettings,
        root: &Path,
        lookup: impl FnOnce(&EnvName) -> Option<String>,
    ) -> Result<Self> {
        let Some(source) = settings.token() else {
            return Ok(Self { credential: None });
        };
        let unreadable = || Error::RemoteToken {
            from: source.clone(),
        };
        let token = match source {
            TokenSource::Env(name) => lookup(name).ok_or_else(unreadable)?,
            TokenSource::File(path) => {
                let file = if path.is_absolute() {
                    Path::new(path.as_str()).to_path_buf()
                } else {
                    root.join(path.as_str())
                };
                let mut text = std::fs::read_to_string(file).map_err(|_| unreadable())?;
                if text.ends_with('\n') {
                    text.pop();
                }
                text
            }
        };
        Self::from_token(settings.header(), &token).ok_or_else(unreadable)
    }

    /// The login that sends `token` in `header`, or `None` when the token holds a character a
    /// header cannot carry.
    fn from_token(header: &HeaderName, token: &str) -> Option<Self> {
        let value = if header.as_str() == AUTHORIZATION {
            format!("Bearer {token}")
        } else {
            token.to_owned()
        };
        let name = MetadataKey::from_bytes(header.as_str().as_bytes()).ok()?;
        let mut value = MetadataValue::try_from(value.as_str()).ok()?;
        value.set_sensitive(true);
        Some(Self {
            credential: Some(Credential { name, value }),
        })
    }
}

impl core::fmt::Debug for Login {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match &self.credential {
            Some(credential) => write!(f, "Login({}: <redacted>)", credential.name),
            None => f.write_str("Login(no token)"),
        }
    }
}

impl Interceptor for Login {
    fn call(&mut self, mut request: Request<()>) -> core::result::Result<Request<()>, Status> {
        if let Some(credential) = &self.credential {
            request
                .metadata_mut()
                .insert(credential.name.clone(), credential.value.clone());
        }
        Ok(request)
    }
}

#[cfg(all(test, feature = "test-server"))]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::fs;

    use buildl_core::{EnvName, Error, HeaderName, HostPath, RemoteSettings, TokenSource};
    use tonic::transport::Channel;

    use super::Login;
    use crate::remote::channel;
    use crate::remote::proto::build::bazel::remote::execution::v2::GetCapabilitiesRequest;
    use crate::remote::proto::build::bazel::remote::execution::v2::capabilities_client::CapabilitiesClient;
    use crate::remote::runtime::Runtime;
    use crate::remote::test_server::{Method, TestServer};
    use crate::remote::tls_fixture::{root, tls_server, trusting_settings};

    /// Settings of the TLS test server that read the token from a file holding `contents`,
    /// and send it in `header` when one is named.
    fn settings_with_token_file(
        server: &TestServer,
        dir: &tempfile::TempDir,
        contents: &str,
        header: Option<&str>,
    ) -> RemoteSettings {
        let file = dir.path().join("token");
        fs::write(&file, contents).unwrap();
        let source = TokenSource::File(HostPath::parse(file.to_str().unwrap()).unwrap());
        let mut builder = trusting_settings(server).token(source);
        if let Some(header) = header {
            builder = builder.header(HeaderName::parse(header).unwrap());
        }
        builder.build().unwrap()
    }

    /// Makes one call through a channel to `server` that carries `login`, and returns what the
    /// server recorded of it.
    fn recorded_headers(
        server: &TestServer,
        settings: &RemoteSettings,
        login: Login,
    ) -> std::collections::BTreeMap<String, String> {
        let runtime = Runtime::new().unwrap();
        let channel: Channel = channel::connect(&runtime, settings, root()).unwrap();
        let mut client = CapabilitiesClient::with_interceptor(channel, login);
        runtime
            .block_on(client.get_capabilities(GetCapabilitiesRequest::default()))
            .unwrap();
        server.calls_to(Method::GetCapabilities).remove(0).headers
    }

    #[test]
    fn the_authorization_header_carries_a_bearer_token() {
        let server = tls_server();
        let dir = tempfile::tempdir().unwrap();
        let settings = settings_with_token_file(&server, &dir, "s3cret", None);

        let login = Login::read(&settings, root()).unwrap();

        let headers = recorded_headers(&server, &settings, login);
        assert_eq!(headers["authorization"], "Bearer s3cret");
    }

    #[test]
    fn another_header_carries_the_bare_token() {
        let server = tls_server();
        let dir = tempfile::tempdir().unwrap();
        let settings = settings_with_token_file(&server, &dir, "s3cret", Some("x-api-key"));

        let login = Login::read(&settings, root()).unwrap();

        let headers = recorded_headers(&server, &settings, login);
        assert_eq!(headers["x-api-key"], "s3cret");
        assert!(!headers.contains_key("authorization"));
    }

    #[test]
    fn a_token_from_a_file_drops_one_trailing_newline() {
        let server = tls_server();
        let dir = tempfile::tempdir().unwrap();
        let settings = settings_with_token_file(&server, &dir, "s3cret\n", Some("x-api-key"));

        let login = Login::read(&settings, root()).unwrap();

        let headers = recorded_headers(&server, &settings, login);
        assert_eq!(headers["x-api-key"], "s3cret");
    }

    #[test]
    fn a_relative_token_file_is_read_from_the_workspace_root() {
        let server = tls_server();
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("token"), "s3cret").unwrap();
        let settings = trusting_settings(&server)
            .token(TokenSource::File(HostPath::parse("token").unwrap()))
            .build()
            .unwrap();

        let login = Login::read(&settings, dir.path()).unwrap();

        let headers = recorded_headers(&server, &settings, login);
        assert_eq!(headers["authorization"], "Bearer s3cret");
    }

    #[test]
    fn a_token_from_a_variable_is_sent_as_it_is() {
        let server = tls_server();
        let settings = trusting_settings(&server)
            .token(TokenSource::Env(EnvName::parse("CACHE_TOKEN").unwrap()))
            .header(HeaderName::parse("x-api-key").unwrap())
            .build()
            .unwrap();

        let login = Login::read_with(&settings, root(), |name| {
            (name.as_str() == "CACHE_TOKEN").then(|| "s3cret".to_owned())
        })
        .unwrap();

        let headers = recorded_headers(&server, &settings, login);
        assert_eq!(headers["x-api-key"], "s3cret");
    }

    #[test]
    fn no_token_adds_no_header() {
        let server = tls_server();
        let settings = trusting_settings(&server).build().unwrap();

        let login = Login::read_with(&settings, root(), |_| None).unwrap();

        let headers = recorded_headers(&server, &settings, login);
        assert!(!headers.contains_key("authorization"));
    }

    #[test]
    fn an_unset_token_variable_is_an_error_that_does_not_name_the_token() {
        let server = tls_server();
        let settings = trusting_settings(&server)
            .token(TokenSource::Env(EnvName::parse("CACHE_TOKEN").unwrap()))
            .build()
            .unwrap();

        let error = Login::read_with(&settings, root(), |_| None).unwrap_err();

        assert!(
            matches!(&error, Error::RemoteToken { from } if Some(from) == settings.token()),
            "{error:?}"
        );
        assert_eq!(
            error.to_string(),
            "the remote cache token cannot be read from environment variable CACHE_TOKEN"
        );
    }

    #[test]
    fn an_unreadable_token_file_is_an_error_naming_the_file() {
        let server = tls_server();
        let dir = tempfile::tempdir().unwrap();
        let settings = trusting_settings(&server)
            .token(TokenSource::File(HostPath::parse("missing").unwrap()))
            .build()
            .unwrap();

        let error = Login::read(&settings, dir.path()).unwrap_err();

        assert!(
            matches!(&error, Error::RemoteToken { from: TokenSource::File(path) } if path.as_str() == "missing"),
            "{error:?}"
        );
    }

    #[test]
    fn a_token_that_cannot_travel_in_a_header_is_an_error_that_does_not_show_it() {
        let server = tls_server();
        let dir = tempfile::tempdir().unwrap();
        let settings = settings_with_token_file(&server, &dir, "s3cret\nsecond line", None);

        let error = Login::read(&settings, root()).unwrap_err();

        assert!(matches!(error, Error::RemoteToken { .. }), "{error:?}");
        assert!(!format!("{error} {error:?}").contains("s3cret"));
    }

    #[test]
    fn a_login_does_not_show_its_token_when_printed() {
        let server = tls_server();
        let settings = trusting_settings(&server)
            .token(TokenSource::Env(EnvName::parse("CACHE_TOKEN").unwrap()))
            .build()
            .unwrap();

        let login = Login::read_with(&settings, root(), |_| Some("s3cret".to_owned())).unwrap();

        assert!(!format!("{login:?}").contains("s3cret"));
    }
}
