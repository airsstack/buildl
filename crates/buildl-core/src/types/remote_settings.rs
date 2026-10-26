//! The remote cache a workspace names, validated at construction.
//!
//! One file for the settings and the six values they hold, because each part is meaningless
//! alone and the settings' one cross-field rule — a token never travels over plain `grpc://` —
//! needs the URL and the token source side by side. Every part is parsed once, where
//! `buildl.toml` names it, so no later phase re-checks a URL, an instance name or a header.
//!
//! Responsibilities: [`RemoteSettings`], [`RemoteSettingsBuilder`], [`RemoteUrl`], [`RemoteScheme`], [`InstanceName`],
//! [`HostPath`], [`TokenSource`] and [`HeaderName`], their constructors, and their rendering.
//!
//! Non-responsibilities: connecting to the cache, reading a certificate or a token, and reading
//! the environment. A [`HostPath`] and an [`EnvName`] are names; nothing here opens a file or
//! asks for a variable's value.

use core::fmt;
use core::num::NonZeroU64;
use core::str::FromStr;

use crate::error::{Error, NameKind, Result};
use crate::types::{EnvName, Timeout};

/// The path segments an instance name may not use, because the remote cache's resource names
/// give them their own meaning.
const RESERVED_SEGMENTS: [&str; 7] = [
    "blobs",
    "uploads",
    "actions",
    "actionResults",
    "operations",
    "capabilities",
    "compressed-blobs",
];

/// The header a token is sent in unless the workspace names another.
const DEFAULT_HEADER: &str = "authorization";

/// How long one request to the remote cache may take unless the workspace says otherwise: 30
/// seconds.
const DEFAULT_TIMEOUT: Timeout = Timeout::from_secs(NonZeroU64::MIN.saturating_add(29));

/// The refusal of `raw` as a value of `kind`.
fn invalid(kind: NameKind, raw: &str, reason: &'static str) -> Error {
    Error::InvalidName {
        kind,
        value: raw.to_owned(),
        reason,
    }
}

/// The transport a remote cache is reached over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RemoteScheme {
    /// Plain gRPC, which sends everything unencrypted.
    Grpc,
    /// gRPC over TLS.
    Grpcs,
}

impl RemoteScheme {
    /// The scheme as it begins a URL, such as `grpcs`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Grpc => "grpc",
            Self::Grpcs => "grpcs",
        }
    }
}

impl fmt::Display for RemoteScheme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Splits the scheme off the front of `raw`, returning it with what follows `://`.
fn split_scheme(raw: &str) -> Option<(RemoteScheme, &str)> {
    [RemoteScheme::Grpcs, RemoteScheme::Grpc]
        .into_iter()
        .find_map(|scheme| {
            raw.strip_prefix(scheme.as_str())
                .and_then(|rest| rest.strip_prefix("://"))
                .map(|rest| (scheme, rest))
        })
}

/// Reads a port, which is a whole number from 1 to 65535 written without a sign or leading zero.
fn port_grammar(text: &str) -> core::result::Result<u16, &'static str> {
    const REASON: &str = "port must be a whole number from 1 to 65535";
    if text.is_empty() || text.starts_with('0') || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(REASON);
    }
    text.parse::<u16>().map_err(|_| REASON)
}

/// The grammar of a remote cache URL: its scheme, host and optional port, or the reason a
/// candidate is rejected.
fn url_grammar(
    raw: &str,
) -> core::result::Result<(RemoteScheme, String, Option<u16>), &'static str> {
    let Some((scheme, rest)) = split_scheme(raw) else {
        return Err("must start with grpc:// or grpcs://");
    };
    let (host, port) = match rest.split_once(':') {
        Some((host, port)) => (host, Some(port_grammar(port)?)),
        None => (rest, None),
    };
    if host.is_empty() {
        return Err("must name a host");
    }
    if !host
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
    {
        return Err("must hold only a host and an optional port after the scheme");
    }
    if host
        .split('.')
        .any(|label| label.is_empty() || label.starts_with('-') || label.ends_with('-'))
    {
        return Err("must be dot-separated labels, none empty or starting or ending with '-'");
    }
    Ok((scheme, host.to_owned(), port))
}

/// The address of a remote cache: `grpc://` or `grpcs://`, a host, and an optional `:port`.
///
/// Nothing may follow the port, so a path, a query or a fragment is refused. The host is a
/// name or an IPv4 address, made of dot-separated labels that are not empty and do not start or
/// end with `-`; a bracketed IPv6 address is not accepted.
///
/// # Examples
///
/// ```
/// use buildl_core::{RemoteScheme, RemoteUrl};
///
/// let url = RemoteUrl::parse("grpcs://cache.example.com:443")?;
/// assert_eq!(url.scheme(), RemoteScheme::Grpcs);
/// assert_eq!(url.host(), "cache.example.com");
/// assert_eq!(url.port(), Some(443));
/// assert!(RemoteUrl::parse("https://cache.example.com").is_err());
/// # Ok::<(), buildl_core::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RemoteUrl {
    scheme: RemoteScheme,
    host: String,
    port: Option<u16>,
}

impl RemoteUrl {
    /// Validates `raw` and reads the parts of the address it states.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidName`] when `raw` does not begin `grpc://` or `grpcs://`, names
    /// no host, holds a character other than ASCII letters, digits, `.`, `-` and `_` in the
    /// host, has a host label that is empty or starts or ends with `-`, or states a port that is
    /// not a whole number from 1 to 65535.
    pub fn parse(raw: &str) -> Result<Self> {
        match url_grammar(raw) {
            Ok((scheme, host, port)) => Ok(Self { scheme, host, port }),
            Err(reason) => Err(invalid(NameKind::RemoteUrl, raw, reason)),
        }
    }

    /// The transport the address is reached over.
    #[must_use]
    pub const fn scheme(&self) -> RemoteScheme {
        self.scheme
    }

    /// The host, as written.
    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    /// The port, when the address states one.
    #[must_use]
    pub const fn port(&self) -> Option<u16> {
        self.port
    }
}

/// Writes the address back as it was given.
impl fmt::Display for RemoteUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}://{}", self.scheme, self.host)?;
        if let Some(port) = self.port {
            write!(f, ":{port}")?;
        }
        Ok(())
    }
}

impl FromStr for RemoteUrl {
    type Err = Error;
    fn from_str(raw: &str) -> Result<Self> {
        Self::parse(raw)
    }
}

/// The grammar of an instance name, or the reason a candidate is rejected.
fn instance_grammar(raw: &str) -> core::result::Result<(), &'static str> {
    if raw.is_empty() {
        return Err("must not be empty");
    }
    if raw.starts_with('/') || raw.ends_with('/') {
        return Err("must not start or end with '/'");
    }
    let mut segments = raw.split('/');
    if segments.clone().any(str::is_empty) {
        return Err("must not hold an empty path segment");
    }
    if segments.any(|segment| RESERVED_SEGMENTS.contains(&segment)) {
        return Err(
            "must not use blobs, uploads, actions, actionResults, operations, capabilities or compressed-blobs as a path segment",
        );
    }
    Ok(())
}

/// The name a remote cache's storage is addressed under, such as `ci` or `team/ci`.
///
/// Non-empty, with no leading, trailing or doubled `/`, and no path segment equal to one the
/// remote cache's resource names reserve: `blobs`, `uploads`, `actions`, `actionResults`,
/// `operations`, `capabilities` or `compressed-blobs`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct InstanceName(String);

impl InstanceName {
    /// Validates `raw` and wraps it.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidName`] when `raw` is empty, starts or ends with `/`, holds an
    /// empty path segment, or has a path segment the remote cache reserves.
    pub fn parse(raw: impl Into<String>) -> Result<Self> {
        let raw = raw.into();
        match instance_grammar(&raw) {
            Ok(()) => Ok(Self(raw)),
            Err(reason) => Err(invalid(NameKind::InstanceName, &raw, reason)),
        }
    }

    /// The name as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for InstanceName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The grammar of a host path, or the reason a candidate is rejected.
fn host_path_grammar(raw: &str) -> core::result::Result<(), &'static str> {
    if raw.is_empty() {
        return Err("must not be empty");
    }
    if !raw.starts_with('/') && raw.split('/').any(|segment| segment == "..") {
        return Err("a relative path must not hold a '..' segment");
    }
    Ok(())
}

/// A file the host reads, named in `buildl.toml`: an absolute path, or one relative to the
/// workspace root.
///
/// A relative path may not hold a `..` segment, so it stays inside the workspace. This is a
/// name only; nothing here checks that the file exists.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HostPath(String);

impl HostPath {
    /// Validates `raw` and wraps it.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidName`] when `raw` is empty, or is relative and holds a `..`
    /// segment.
    pub fn parse(raw: impl Into<String>) -> Result<Self> {
        let raw = raw.into();
        match host_path_grammar(&raw) {
            Ok(()) => Ok(Self(raw)),
            Err(reason) => Err(invalid(NameKind::HostPath, &raw, reason)),
        }
    }

    /// Whether the path is absolute, rather than relative to the workspace root.
    #[must_use]
    pub fn is_absolute(&self) -> bool {
        self.0.starts_with('/')
    }

    /// The path as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for HostPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Where the token sent to a remote cache is read from.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TokenSource {
    /// An environment variable, named here, holding the token.
    Env(EnvName),
    /// A file, named here, holding the token.
    File(HostPath),
}

/// The names the gRPC transport sets itself and drops from the metadata of a request.
const TRANSPORT_HEADERS: [&str; 3] = ["te", "content-type", "user-agent"];

/// The grammar of a request header name, or the reason a candidate is rejected.
fn header_grammar(raw: &str) -> core::result::Result<(), &'static str> {
    if raw.is_empty() {
        return Err("must not be empty");
    }
    if !raw
        .bytes()
        .all(|b| b.is_ascii_digit() || b.is_ascii_lowercase() || matches!(b, b'_' | b'-' | b'.'))
    {
        return Err("may hold only lowercase ASCII letters, digits, '_', '-' and '.'");
    }
    if raw.starts_with("grpc-") {
        return Err("must not start with 'grpc-', which gRPC reserves");
    }
    if raw.ends_with("-bin") {
        return Err("must not end with '-bin', which gRPC reads as binary");
    }
    if TRANSPORT_HEADERS.contains(&raw) {
        return Err("must not be te, content-type or user-agent, which the gRPC transport sets");
    }
    Ok(())
}

/// The name of the request header a remote cache's token is sent in.
///
/// One or more of the lowercase ASCII letters, digits, `_`, `-` and `.` that a gRPC header name
/// is made of, not starting `grpc-`, not ending `-bin`, and not `te`, `content-type` or
/// `user-agent`, which the transport sets itself. The default is `authorization`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HeaderName(String);

impl HeaderName {
    /// Validates `raw` and wraps it.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidName`] when `raw` is empty, holds a character outside lowercase
    /// ASCII letters, digits, `_`, `-` and `.`, starts with `grpc-`, ends with `-bin`, or is
    /// `te`, `content-type` or `user-agent`.
    pub fn parse(raw: impl Into<String>) -> Result<Self> {
        let raw = raw.into();
        match header_grammar(&raw) {
            Ok(()) => Ok(Self(raw)),
            Err(reason) => Err(invalid(NameKind::HeaderName, &raw, reason)),
        }
    }

    /// The name as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for HeaderName {
    fn default() -> Self {
        Self(DEFAULT_HEADER.to_owned())
    }
}

impl fmt::Display for HeaderName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The remote cache a workspace names: where it is, how to reach it, and how to sign in.
///
/// A token is only accepted over `grpcs://`, so a credential never crosses the network in clear
/// text.
///
/// # Examples
///
/// ```
/// use buildl_core::{EnvName, RemoteSettings, RemoteUrl, TokenSource};
///
/// let token = TokenSource::Env(EnvName::parse("CACHE_TOKEN")?);
/// let secure = RemoteUrl::parse("grpcs://cache.example.com")?;
/// let settings = RemoteSettings::builder(secure).token(token.clone()).build()?;
/// assert_eq!(settings.header().as_str(), "authorization");
///
/// let plain = RemoteUrl::parse("grpc://cache.example.com")?;
/// assert!(RemoteSettings::builder(plain).token(token).build().is_err());
/// # Ok::<(), buildl_core::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteSettings {
    url: RemoteUrl,
    instance: Option<InstanceName>,
    ca: Option<HostPath>,
    token: Option<TokenSource>,
    header: HeaderName,
    timeout: Timeout,
}

impl RemoteSettings {
    /// Starts the settings of the cache at `url`; every other part is optional.
    pub const fn builder(url: RemoteUrl) -> RemoteSettingsBuilder {
        RemoteSettingsBuilder {
            url,
            instance: None,
            ca: None,
            token: None,
            header: None,
            timeout: None,
        }
    }

    /// The cache's address.
    #[must_use]
    pub const fn url(&self) -> &RemoteUrl {
        &self.url
    }

    /// The instance name its storage is addressed under, when one is named.
    #[must_use]
    pub const fn instance(&self) -> Option<&InstanceName> {
        self.instance.as_ref()
    }

    /// The certificate authority file to trust, when one is named.
    #[must_use]
    pub const fn ca(&self) -> Option<&HostPath> {
        self.ca.as_ref()
    }

    /// Where the token is read from, when one is used.
    #[must_use]
    pub const fn token(&self) -> Option<&TokenSource> {
        self.token.as_ref()
    }

    /// The header the token is sent in.
    #[must_use]
    pub const fn header(&self) -> &HeaderName {
        &self.header
    }

    /// How long one request may take.
    #[must_use]
    pub const fn timeout(&self) -> Timeout {
        self.timeout
    }
}

/// The parts of a [`RemoteSettings`] as they are stated, before the defaults are filled in and
/// the token is checked against the URL.
#[derive(Debug, Clone)]
#[must_use = "a builder does nothing until `build` is called"]
pub struct RemoteSettingsBuilder {
    url: RemoteUrl,
    instance: Option<InstanceName>,
    ca: Option<HostPath>,
    token: Option<TokenSource>,
    header: Option<HeaderName>,
    timeout: Option<Timeout>,
}

impl RemoteSettingsBuilder {
    /// Names the instance the cache's storage is addressed under.
    pub fn instance(mut self, instance: InstanceName) -> Self {
        self.instance = Some(instance);
        self
    }

    /// Names a certificate authority file to trust.
    pub fn ca(mut self, ca: HostPath) -> Self {
        self.ca = Some(ca);
        self
    }

    /// Names where the token sent with every request is read from.
    pub fn token(mut self, token: TokenSource) -> Self {
        self.token = Some(token);
        self
    }

    /// Names the header the token is sent in, instead of `authorization`.
    pub fn header(mut self, header: HeaderName) -> Self {
        self.header = Some(header);
        self
    }

    /// Sets how long one request may take, instead of 30 seconds.
    pub const fn timeout(mut self, timeout: Timeout) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Assembles the settings, filling the header with `authorization` and the timeout with
    /// 30 seconds when they are not given.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InsecureToken`] when a token is given and the URL is `grpc://`.
    pub fn build(self) -> Result<RemoteSettings> {
        if self.token.is_some() && self.url.scheme() == RemoteScheme::Grpc {
            return Err(Error::InsecureToken { url: self.url });
        }
        Ok(RemoteSettings {
            url: self.url,
            instance: self.instance,
            ca: self.ca,
            token: self.token,
            header: self.header.unwrap_or_default(),
            timeout: self.timeout.unwrap_or(DEFAULT_TIMEOUT),
        })
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use core::time::Duration;

    use crate::error::{Error, NameKind};
    use crate::types::{
        EnvName, HeaderName, HostPath, InstanceName, RemoteScheme, RemoteSettings, RemoteUrl,
        Timeout, TokenSource,
    };

    fn settings(url: &str, token: Option<TokenSource>) -> crate::error::Result<RemoteSettings> {
        let builder = RemoteSettings::builder(RemoteUrl::parse(url)?);
        match token {
            Some(token) => builder.token(token).build(),
            None => builder.build(),
        }
    }

    fn token() -> TokenSource {
        TokenSource::Env(EnvName::parse("CACHE_TOKEN").unwrap())
    }

    #[test]
    fn a_grpcs_url_keeps_host_and_port() {
        let url = RemoteUrl::parse("grpcs://cache.example.com:443").unwrap();
        assert_eq!(url.scheme(), RemoteScheme::Grpcs);
        assert_eq!(url.host(), "cache.example.com");
        assert_eq!(url.port(), Some(443));
        assert_eq!(url.to_string(), "grpcs://cache.example.com:443");

        let bare = RemoteUrl::parse("grpc://cache").unwrap();
        assert_eq!(bare.scheme(), RemoteScheme::Grpc);
        assert_eq!(bare.port(), None);
        assert_eq!(bare.to_string(), "grpc://cache");
    }

    #[test]
    fn a_url_without_a_known_scheme_is_refused() {
        for raw in [
            "https://x",
            "grpc:/x",
            "grpc://",
            "grpc://h/path",
            "grpc://h:",
            "grpc://h:99999",
            "grpc://..",
            "grpc://-",
            "grpc://a..b",
            "grpc://a.",
            "grpc://.a",
            "grpc://-a.b",
            "grpc://a-.b",
            "",
        ] {
            let err = RemoteUrl::parse(raw).unwrap_err();
            assert!(
                matches!(
                    err,
                    Error::InvalidName {
                        kind: NameKind::RemoteUrl,
                        ..
                    }
                ),
                "{raw:?} should be refused as a remote URL"
            );
        }
    }

    #[test]
    fn an_instance_may_not_use_a_reserved_segment() {
        assert_eq!(InstanceName::parse("ci").unwrap().as_str(), "ci");
        assert!(InstanceName::parse("a/b").is_ok());
        for raw in [
            "a/blobs/b",
            "/ci",
            "ci/",
            "",
            "a//b",
            "uploads",
            "actionResults",
        ] {
            let err = InstanceName::parse(raw).unwrap_err();
            assert!(
                matches!(
                    err,
                    Error::InvalidName {
                        kind: NameKind::InstanceName,
                        ..
                    }
                ),
                "{raw:?} should be refused as an instance name"
            );
        }
    }

    #[test]
    fn a_header_name_follows_grpc_rules() {
        for raw in ["authorization", "x-api-key", "a_b.c-1"] {
            assert_eq!(HeaderName::parse(raw).unwrap().as_str(), raw);
        }
        for raw in [
            "X-Key",
            "grpc-x",
            "key-bin",
            "",
            "a b",
            "k:v",
            "te",
            "content-type",
            "user-agent",
        ] {
            let err = HeaderName::parse(raw).unwrap_err();
            assert!(
                matches!(
                    err,
                    Error::InvalidName {
                        kind: NameKind::HeaderName,
                        ..
                    }
                ),
                "{raw:?} should be refused as a header name"
            );
        }
    }

    #[test]
    fn a_relative_host_path_may_not_climb() {
        let relative = HostPath::parse("certs/ca.pem").unwrap();
        assert!(!relative.is_absolute());
        assert_eq!(relative.as_str(), "certs/ca.pem");
        assert!(HostPath::parse("/etc/ca.pem").unwrap().is_absolute());
        for raw in ["../ca.pem", "a/../b", "", "a/.."] {
            let err = HostPath::parse(raw).unwrap_err();
            assert!(
                matches!(
                    err,
                    Error::InvalidName {
                        kind: NameKind::HostPath,
                        ..
                    }
                ),
                "{raw:?} should be refused as a host path"
            );
        }
    }

    #[test]
    fn a_token_over_plain_grpc_is_refused() {
        let err = settings("grpc://cache:50051", Some(token())).unwrap_err();
        assert!(matches!(err, Error::InsecureToken { .. }));
        assert!(err.to_string().contains("grpc://cache:50051"));

        assert!(settings("grpc://cache:50051", None).is_ok());
        assert!(settings("grpcs://cache:443", Some(token())).is_ok());
        let file = TokenSource::File(HostPath::parse("secrets/token").unwrap());
        assert!(settings("grpc://cache", Some(file)).is_err());
    }

    #[test]
    fn the_default_header_and_timeout() {
        let settings = settings("grpcs://cache", None).unwrap();
        assert_eq!(settings.header().as_str(), "authorization");
        assert_eq!(settings.timeout().as_duration(), Duration::from_secs(30));
        assert_eq!(settings.url().host(), "cache");
        assert!(settings.instance().is_none());
        assert!(settings.ca().is_none());
        assert!(settings.token().is_none());
    }

    #[test]
    fn stated_values_override_the_defaults() {
        let settings = RemoteSettings::builder(RemoteUrl::parse("grpcs://cache").unwrap())
            .instance(InstanceName::parse("ci").unwrap())
            .ca(HostPath::parse("certs/ca.pem").unwrap())
            .token(token())
            .header(HeaderName::parse("x-api-key").unwrap())
            .timeout(Timeout::parse("5s").unwrap())
            .build()
            .unwrap();
        assert_eq!(settings.instance().unwrap().as_str(), "ci");
        assert_eq!(settings.ca().unwrap().as_str(), "certs/ca.pem");
        assert_eq!(settings.token(), Some(&token()));
        assert_eq!(settings.header().as_str(), "x-api-key");
        assert_eq!(settings.timeout().as_duration(), Duration::from_secs(5));
    }
}
