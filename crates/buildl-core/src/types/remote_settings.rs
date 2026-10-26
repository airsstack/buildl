//! The remote cache a workspace names, validated at construction.
//!
//! One file for the settings and the values they hold, because each part is meaningless
//! alone and the settings' one cross-field rule — a token never travels over plain `grpc://` —
//! needs the URL and the token source side by side. Every part is parsed once, where
//! `buildl.toml` names it, so no later phase re-checks a URL, an instance name or a header.
//!
//! Responsibilities: [`RemoteSettings`], [`RemoteSettingsBuilder`], [`RemoteUrl`], [`RemoteScheme`], [`InstanceName`],
//! [`HostPath`], [`TokenSource`], [`HeaderName`], [`RemoteJobs`], [`RetryCount`],
//! [`PlatformPropertyName`] and [`PlatformProperties`], their constructors, and their rendering.
//!
//! Non-responsibilities: connecting to the cache, reading a certificate or a token, and reading
//! the environment. A [`HostPath`] and an [`EnvName`] are names; nothing here opens a file or
//! asks for a variable's value.

use core::fmt;
use core::num::NonZeroU64;
use core::str::FromStr;
use std::collections::BTreeMap;

use crate::error::{Error, NameKind, Result};
use crate::types::{EnvName, RemotePlatform, Timeout};

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

/// How long a remote execution may wait in the cache's queue unless the workspace says
/// otherwise: one hour.
const DEFAULT_QUEUE_TIMEOUT: Timeout = Timeout::from_secs(NonZeroU64::MIN.saturating_add(3_599));

/// The fewest remote calls that may run at once.
const MIN_JOBS: u16 = 1;

/// The most remote calls that may run at once.
const MAX_JOBS: u16 = 1024;

/// How many remote calls run at once unless the workspace says otherwise.
const DEFAULT_JOBS: u16 = 128;

/// The most retries after a first call.
const MAX_RETRIES: u8 = 10;

/// How many retries follow a failed first call unless the workspace says otherwise.
const DEFAULT_RETRIES: u8 = 5;

/// The platform property names buildl sends itself, which a workspace may not replace.
const RESERVED_PROPERTIES: [&str; 2] = [
    RemotePlatform::OS_FAMILY_PROPERTY,
    RemotePlatform::ISA_PROPERTY,
];

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

/// The reason a job count is refused.
const JOBS_REASON: &str = "must be a whole number from 1 to 1024";

/// How many remote calls a run keeps in flight at once: a whole number from 1 to 1024.
///
/// The default is 128.
///
/// # Examples
///
/// ```
/// use buildl_core::RemoteJobs;
///
/// assert_eq!(RemoteJobs::parse("64")?.get(), 64);
/// assert_eq!(RemoteJobs::default().get(), 128);
/// assert!(RemoteJobs::parse("0").is_err());
/// assert!(RemoteJobs::from_integer(1025).is_err());
/// # Ok::<(), buildl_core::Error>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RemoteJobs(u16);

impl RemoteJobs {
    /// Reads the count a manifest states as the integer `count`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidName`] when `count` is below 1 or above 1024.
    pub fn from_integer(count: i64) -> Result<Self> {
        u16::try_from(count)
            .ok()
            .filter(|count| (MIN_JOBS..=MAX_JOBS).contains(count))
            .map(Self)
            .ok_or_else(|| invalid(NameKind::RemoteJobs, &count.to_string(), JOBS_REASON))
    }

    /// Reads the count a flag states as the text `raw`: a whole number in decimal digits.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidName`] when `raw` is empty, holds anything but ASCII digits — a
    /// sign and a space included — or is below 1 or above 1024.
    pub fn parse(raw: &str) -> Result<Self> {
        if raw.is_empty() || !raw.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(invalid(NameKind::RemoteJobs, raw, JOBS_REASON));
        }
        raw.parse::<i64>()
            .ok()
            .and_then(|count| Self::from_integer(count).ok())
            .ok_or_else(|| invalid(NameKind::RemoteJobs, raw, JOBS_REASON))
    }

    /// The count.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

impl Default for RemoteJobs {
    fn default() -> Self {
        Self(DEFAULT_JOBS)
    }
}

impl fmt::Display for RemoteJobs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// How many times a failed remote call is tried again after the first: a whole number from 0
/// to 10, where `0` turns retrying off.
///
/// The default is 5.
///
/// # Examples
///
/// ```
/// use buildl_core::RetryCount;
///
/// assert_eq!(RetryCount::from_integer(0)?.get(), 0);
/// assert_eq!(RetryCount::default().get(), 5);
/// assert!(RetryCount::from_integer(11).is_err());
/// # Ok::<(), buildl_core::Error>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RetryCount(u8);

impl RetryCount {
    /// Reads the count a manifest states as the integer `count`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidName`] when `count` is below 0 or above 10.
    pub fn from_integer(count: i64) -> Result<Self> {
        u8::try_from(count)
            .ok()
            .filter(|count| *count <= MAX_RETRIES)
            .map(Self)
            .ok_or_else(|| {
                invalid(
                    NameKind::RetryCount,
                    &count.to_string(),
                    "must be a whole number from 0 to 10",
                )
            })
    }

    /// The count.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl Default for RetryCount {
    fn default() -> Self {
        Self(DEFAULT_RETRIES)
    }
}

impl fmt::Display for RetryCount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// The grammar of a platform property name, or the reason a candidate is rejected.
fn property_name_grammar(raw: &str) -> core::result::Result<(), &'static str> {
    if raw.is_empty() {
        return Err("must not be empty");
    }
    if RESERVED_PROPERTIES.contains(&raw) {
        return Err("must not be OSFamily or ISA, which buildl sends itself");
    }
    Ok(())
}

/// The name of one platform property a workspace asks the remote executor for, such as `pool`.
///
/// Non-empty, and neither `OSFamily` nor `ISA`, which buildl sends itself. Names are compared as
/// written, so `osfamily` is a different name from `OSFamily`.
///
/// # Examples
///
/// ```
/// use buildl_core::PlatformPropertyName;
///
/// assert_eq!(PlatformPropertyName::parse("pool")?.as_str(), "pool");
/// assert!(PlatformPropertyName::parse("OSFamily").is_err());
/// assert!(PlatformPropertyName::parse("").is_err());
/// # Ok::<(), buildl_core::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PlatformPropertyName(String);

impl PlatformPropertyName {
    /// Validates `raw` and wraps it.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidName`] when `raw` is empty, or is exactly `OSFamily` or `ISA`.
    pub fn parse(raw: impl Into<String>) -> Result<Self> {
        let raw = raw.into();
        match property_name_grammar(&raw) {
            Ok(()) => Ok(Self(raw)),
            Err(reason) => Err(invalid(NameKind::PlatformProperty, &raw, reason)),
        }
    }

    /// The name as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PlatformPropertyName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The platform properties a workspace asks the remote executor for, sorted by name.
///
/// Built from a map, so each name appears once and the order is settled at construction rather
/// than by whoever reads the list.
///
/// # Examples
///
/// ```
/// use std::collections::BTreeMap;
///
/// use buildl_core::{PlatformPropertyName, PlatformProperties};
///
/// let properties = PlatformProperties::new(BTreeMap::from([
///     (PlatformPropertyName::parse("z")?, "1".to_owned()),
///     (PlatformPropertyName::parse("a")?, "2".to_owned()),
/// ]));
/// let names: Vec<&str> = properties.iter().map(|(name, _)| name.as_str()).collect();
/// assert_eq!(names, ["a", "z"]);
/// # Ok::<(), buildl_core::Error>(())
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlatformProperties(Vec<(PlatformPropertyName, String)>);

impl PlatformProperties {
    /// The properties in `map`, listed in the map's order: by name.
    #[must_use]
    pub fn new(map: BTreeMap<PlatformPropertyName, String>) -> Self {
        Self(map.into_iter().collect())
    }

    /// The properties, by name, each with its value.
    pub fn iter(&self) -> impl Iterator<Item = (&PlatformPropertyName, &str)> {
        self.0.iter().map(|(name, value)| (name, value.as_str()))
    }

    /// Whether there is no property.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// The remote cache a workspace names: where it is, how to reach it, and how it is used.
///
/// Besides the address and the sign-in, it holds whether the cache also runs actions, how many
/// calls run at once, how long an execution may queue, how often a failed call is retried, and
/// which platform properties are asked for.
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
    execute: bool,
    jobs: RemoteJobs,
    queue_timeout: Timeout,
    retries: RetryCount,
    properties: PlatformProperties,
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
            execute: false,
            jobs: None,
            queue_timeout: None,
            retries: None,
            properties: None,
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

    /// Whether the cache also runs the actions it has no result for.
    #[must_use]
    pub const fn execute(&self) -> bool {
        self.execute
    }

    /// How many remote calls run at once.
    #[must_use]
    pub const fn jobs(&self) -> RemoteJobs {
        self.jobs
    }

    /// How long an execution may wait in the cache's queue.
    #[must_use]
    pub const fn queue_timeout(&self) -> Timeout {
        self.queue_timeout
    }

    /// How many times a failed call is tried again after the first.
    #[must_use]
    pub const fn retries(&self) -> RetryCount {
        self.retries
    }

    /// The platform properties asked of the remote executor.
    #[must_use]
    pub const fn properties(&self) -> &PlatformProperties {
        &self.properties
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
    execute: bool,
    jobs: Option<RemoteJobs>,
    queue_timeout: Option<Timeout>,
    retries: Option<RetryCount>,
    properties: Option<PlatformProperties>,
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

    /// Sets whether the cache also runs the actions it has no result for, instead of `false`.
    pub const fn execute(mut self, execute: bool) -> Self {
        self.execute = execute;
        self
    }

    /// Sets how many remote calls run at once, instead of 128.
    pub const fn jobs(mut self, jobs: RemoteJobs) -> Self {
        self.jobs = Some(jobs);
        self
    }

    /// Sets how long an execution may wait in the cache's queue, instead of one hour.
    pub const fn queue_timeout(mut self, queue_timeout: Timeout) -> Self {
        self.queue_timeout = Some(queue_timeout);
        self
    }

    /// Sets how many times a failed call is tried again, instead of 5.
    pub const fn retries(mut self, retries: RetryCount) -> Self {
        self.retries = Some(retries);
        self
    }

    /// Names the platform properties to ask of the remote executor, instead of none.
    pub fn properties(mut self, properties: PlatformProperties) -> Self {
        self.properties = Some(properties);
        self
    }

    /// Assembles the settings, filling every part that is not given with its default: the
    /// header with `authorization`, the timeout with 30 seconds, `execute` with `false`, the
    /// job count with 128, the queue timeout with one hour, the retries with 5, and the
    /// properties with none.
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
            execute: self.execute,
            jobs: self.jobs.unwrap_or_default(),
            queue_timeout: self.queue_timeout.unwrap_or(DEFAULT_QUEUE_TIMEOUT),
            retries: self.retries.unwrap_or_default(),
            properties: self.properties.unwrap_or_default(),
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
    use std::collections::BTreeMap;

    use crate::error::{Error, NameKind};
    use crate::types::{
        EnvName, HeaderName, HostPath, InstanceName, PlatformProperties, PlatformPropertyName,
        RemoteJobs, RemoteScheme, RemoteSettings, RemoteUrl, RetryCount, Timeout, TokenSource,
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

    #[test]
    fn jobs_reads_one_to_1024() {
        assert_eq!(RemoteJobs::from_integer(1).unwrap().get(), 1);
        assert_eq!(RemoteJobs::from_integer(1024).unwrap().get(), 1024);
        assert_eq!(RemoteJobs::parse("1").unwrap().get(), 1);
        assert_eq!(RemoteJobs::parse("1024").unwrap().get(), 1024);
        for raw in ["0", "1025", "-1", "", "+1", "1 ", "99999999999999999999"] {
            let err = RemoteJobs::parse(raw).unwrap_err();
            assert!(
                matches!(
                    err,
                    Error::InvalidName {
                        kind: NameKind::RemoteJobs,
                        ..
                    }
                ),
                "{raw:?} should be refused as a job count"
            );
        }
        for count in [0, 1025, -1, i64::MAX, i64::MIN] {
            let err = RemoteJobs::from_integer(count).unwrap_err();
            assert!(
                matches!(
                    err,
                    Error::InvalidName {
                        kind: NameKind::RemoteJobs,
                        ..
                    }
                ),
                "{count} should be refused as a job count"
            );
        }
    }

    #[test]
    fn retries_reads_zero_to_ten() {
        assert_eq!(RetryCount::from_integer(0).unwrap().get(), 0);
        assert_eq!(RetryCount::from_integer(10).unwrap().get(), 10);
        for count in [11, -1, i64::MAX, i64::MIN] {
            let err = RetryCount::from_integer(count).unwrap_err();
            assert!(
                matches!(
                    err,
                    Error::InvalidName {
                        kind: NameKind::RetryCount,
                        ..
                    }
                ),
                "{count} should be refused as a retry count"
            );
        }
    }

    #[test]
    fn a_property_named_like_a_platform_key_is_refused() {
        for raw in ["OSFamily", "ISA"] {
            let err = PlatformPropertyName::parse(raw).unwrap_err();
            assert!(
                matches!(
                    err,
                    Error::InvalidName {
                        kind: NameKind::PlatformProperty,
                        ..
                    }
                ),
                "{raw:?} should be refused as a property name"
            );
        }
        assert_eq!(
            PlatformPropertyName::parse("osfamily").unwrap().as_str(),
            "osfamily"
        );
        assert_eq!(
            PlatformPropertyName::parse("pool").unwrap().as_str(),
            "pool"
        );
    }

    #[test]
    fn an_empty_property_name_is_refused() {
        let err = PlatformPropertyName::parse("").unwrap_err();
        assert!(matches!(
            err,
            Error::InvalidName {
                kind: NameKind::PlatformProperty,
                ..
            }
        ));
    }

    #[test]
    fn properties_are_sorted_by_name() {
        let map = BTreeMap::from([
            (PlatformPropertyName::parse("z").unwrap(), "1".to_owned()),
            (PlatformPropertyName::parse("a").unwrap(), "2".to_owned()),
        ]);
        let properties = PlatformProperties::new(map);
        let listed: Vec<(&str, &str)> = properties
            .iter()
            .map(|(name, value)| (name.as_str(), value))
            .collect();
        assert_eq!(listed, [("a", "2"), ("z", "1")]);
        assert!(!properties.is_empty());
        assert!(PlatformProperties::default().is_empty());
    }

    #[test]
    fn the_execution_settings_have_their_defaults() {
        let settings = settings("grpcs://cache", None).unwrap();
        assert!(!settings.execute());
        assert_eq!(settings.jobs().get(), 128);
        assert_eq!(settings.jobs(), RemoteJobs::default());
        assert_eq!(
            settings.queue_timeout().as_duration(),
            Duration::from_secs(3600)
        );
        assert_eq!(settings.retries().get(), 5);
        assert_eq!(settings.retries(), RetryCount::default());
        assert!(settings.properties().is_empty());
    }

    #[test]
    fn the_execution_values_render_as_plain_text() {
        assert_eq!(RemoteJobs::from_integer(64).unwrap().to_string(), "64");
        assert_eq!(RetryCount::from_integer(3).unwrap().to_string(), "3");
        assert_eq!(
            PlatformPropertyName::parse("pool").unwrap().to_string(),
            "pool"
        );
    }

    #[test]
    fn stated_execution_values_override_the_defaults() {
        let properties = PlatformProperties::new(BTreeMap::from([(
            PlatformPropertyName::parse("pool").unwrap(),
            "large".to_owned(),
        )]));
        let settings = RemoteSettings::builder(RemoteUrl::parse("grpcs://cache").unwrap())
            .execute(true)
            .jobs(RemoteJobs::from_integer(8).unwrap())
            .queue_timeout(Timeout::parse("2m").unwrap())
            .retries(RetryCount::from_integer(0).unwrap())
            .properties(properties.clone())
            .build()
            .unwrap();
        assert!(settings.execute());
        assert_eq!(settings.jobs().get(), 8);
        assert_eq!(
            settings.queue_timeout().as_duration(),
            Duration::from_secs(120)
        );
        assert_eq!(settings.retries().get(), 0);
        assert_eq!(settings.properties(), &properties);
    }
}
