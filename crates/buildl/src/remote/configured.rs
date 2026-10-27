//! The remote cache a run is given: the one `buildl.toml` names, or none.
//!
//! Its own file because the choice between the two is made once, from the workspace and the
//! run's policy, and every port of the bundle then calls the result without asking again.
//!
//! Responsibilities: [`Remote`], and building it from the workspace's settings.
//!
//! Non-responsibilities: talking to a cache, which [`Reapi`] does; and the policy's meaning,
//! which the phase that executes applies.

use std::path::Path;

use buildl_core::{
    Blob, BlobSource, DecodedTree, Executed, NoRemote, RemoteAction, RemoteCache, RemoteHit,
    RemotePolicy, RemoteSettings, RemoteUpload, RemoteUse, Result, Uploaded,
};

use super::Reapi;

/// The remote cache of one run: a cache that is reached, or none.
#[derive(Debug)]
pub enum Remote {
    /// No remote: every lookup misses and every upload is dropped, as [`NoRemote`] does.
    Off,
    /// The cache `buildl.toml` names. Boxed because the adapter is far larger than `Off`.
    Reapi(Box<Reapi>),
}

impl Remote {
    /// The remote of a run under `policy`, in the workspace at `root` whose `[remote]` table
    /// is `settings`.
    ///
    /// It is [`Remote::Off`] when the policy never consults the remote or the workspace names
    /// none, and builds no adapter then. Otherwise the adapter is made, which reads the token
    /// and sends nothing: a cache that is down is the failure of its first call.
    ///
    /// # Errors
    ///
    /// Returns [`Error::RemoteToken`](buildl_core::Error::RemoteToken) when the token cannot
    /// be read, and an [`Error::Adapter`](buildl_core::Error::Adapter) naming the remote cache
    /// when the adapter cannot be set up.
    pub fn of(
        settings: Option<&RemoteSettings>,
        policy: RemotePolicy,
        root: &Path,
    ) -> Result<Self> {
        match settings {
            Some(settings) if policy.use_ != RemoteUse::Off => {
                Reapi::new(settings, root).map(|reapi| Self::Reapi(Box::new(reapi)))
            }
            _ => Ok(Self::Off),
        }
    }
}

impl RemoteCache for Remote {
    fn lookup(&self, action: &RemoteAction) -> Result<Option<RemoteHit>> {
        match self {
            Self::Off => NoRemote.lookup(action),
            Self::Reapi(reapi) => reapi.lookup(action),
        }
    }

    fn download(&self, blobs: &[Blob]) -> Result<Vec<Vec<u8>>> {
        match self {
            Self::Off => NoRemote.download(blobs),
            Self::Reapi(reapi) => reapi.download(blobs),
        }
    }

    fn decode_tree(&self, bytes: &[u8]) -> Result<DecodedTree> {
        match self {
            Self::Off => NoRemote.decode_tree(bytes),
            Self::Reapi(reapi) => reapi.decode_tree(bytes),
        }
    }

    fn upload(
        &self,
        action: &RemoteAction,
        produced: &RemoteUpload,
        blobs: &dyn BlobSource,
    ) -> Result<Uploaded> {
        match self {
            Self::Off => NoRemote.upload(action, produced, blobs),
            Self::Reapi(reapi) => reapi.upload(action, produced, blobs),
        }
    }

    fn execute(&self, action: &RemoteAction) -> Result<Executed> {
        match self {
            Self::Off => NoRemote.execute(action),
            Self::Reapi(reapi) => reapi.execute(action),
        }
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::path::Path;

    use buildl_core::{
        EnvName, Error, RemoteExecution, RemoteFailure, RemotePolicy, RemoteSettings, RemoteUrl,
        RemoteUse, TokenSource,
    };

    use super::Remote;

    /// Settings for a cache at a loopback address nothing listens on. A token is only accepted
    /// over `grpcs://`, so the address is a secure one whenever a token is named.
    fn settings(token: Option<&str>) -> RemoteSettings {
        let scheme = if token.is_some() { "grpcs" } else { "grpc" };
        let mut builder =
            RemoteSettings::builder(RemoteUrl::parse(&format!("{scheme}://127.0.0.1:1")).unwrap());
        if let Some(name) = token {
            builder = builder.token(TokenSource::Env(EnvName::parse(name).unwrap()));
        }
        builder.build().unwrap()
    }

    const READING: RemotePolicy = RemotePolicy {
        use_: RemoteUse::Read,
        failure: RemoteFailure::Lenient,
        execution: RemoteExecution::Off,
    };

    #[test]
    fn a_workspace_without_a_remote_table_has_none() {
        let remote = Remote::of(None, READING, Path::new("/")).unwrap();

        assert!(matches!(remote, Remote::Off));
    }

    #[test]
    fn a_policy_that_never_consults_the_remote_builds_no_adapter() {
        // The token cannot be read, so building the adapter would fail.
        let settings = settings(Some("BUILDL_UNIT_TOKEN_THAT_IS_NOT_SET"));

        let remote = Remote::of(Some(&settings), RemotePolicy::OFF, Path::new("/")).unwrap();

        assert!(matches!(remote, Remote::Off));
    }

    #[test]
    fn a_named_remote_is_reached_under_a_policy_that_reads() {
        let remote = Remote::of(Some(&settings(None)), READING, Path::new("/")).unwrap();

        assert!(matches!(remote, Remote::Reapi(_)));
    }

    #[test]
    fn an_unreadable_token_is_refused_when_the_adapter_is_made() {
        let settings = settings(Some("BUILDL_UNIT_TOKEN_THAT_IS_NOT_SET"));

        let error = Remote::of(Some(&settings), READING, Path::new("/")).unwrap_err();

        assert!(matches!(error, Error::RemoteToken { .. }), "{error:?}");
    }
}
