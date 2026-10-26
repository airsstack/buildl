//! The remote cache of a build that has none.
//!
//! Its own file because every bundle without a remote adapter names it, and a pure port that
//! holds nothing belongs in the crate that declares the port.
//!
//! Responsibilities: [`NoRemote`].
//!
//! Non-responsibilities: reaching a real remote, which an adapter does.

use crate::error::{Error, PortName, Result};
use crate::ports::{BlobSource, RemoteCache};
use crate::types::{
    Blob, Diagnostic, Executed, RemoteAction, RemoteHit, RemoteTreeFile, RemoteUpload, Uploaded,
};

/// A remote cache that holds nothing and stores nothing.
///
/// Every lookup misses and every upload is accepted and dropped, so a build that has no remote
/// behaves as one that was never offered one. It runs nothing: a request to run an action is
/// refused.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NoRemote;

impl RemoteCache for NoRemote {
    fn lookup(&self, _action: &RemoteAction) -> Result<Option<RemoteHit>> {
        Ok(None)
    }

    fn download(&self, _blobs: &[Blob]) -> Result<Vec<Vec<u8>>> {
        Err(Error::Adapter {
            port: PortName::RemoteCache,
            diagnostic: Diagnostic::new("there is no remote cache to download from"),
        })
    }

    fn decode_tree(&self, _bytes: &[u8]) -> Result<Option<Vec<RemoteTreeFile>>> {
        Ok(None)
    }

    fn upload(
        &self,
        _action: &RemoteAction,
        _produced: &RemoteUpload,
        _blobs: &dyn BlobSource,
    ) -> Result<Uploaded> {
        Ok(Uploaded::Done)
    }

    fn execute(&self, _action: &RemoteAction) -> Result<Executed> {
        Err(Error::Adapter {
            port: PortName::RemoteCache,
            diagnostic: Diagnostic::new("there is no remote cache to run an action"),
        })
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeMap;

    use super::NoRemote;
    use crate::error::{Error, PortName, Result};
    use crate::ports::{BlobSource, RemoteCache};
    use crate::types::{
        Argument, Blob, Command, Digest, Platform, PlatformProperties, RemoteAction,
        RemotePlatform, RemoteUpload, Timeout, Uploaded,
    };

    fn action() -> RemoteAction {
        RemoteAction {
            salt: Digest::of(b"key"),
            command: Command::new(vec![Argument::parse("cc").unwrap()]).unwrap(),
            env: BTreeMap::new(),
            outputs: Vec::new(),
            files: Vec::new(),
            platform: RemotePlatform::of(&Platform::parse("linux/x86_64").unwrap()),
            timeout: Timeout::default(),
            properties: PlatformProperties::default(),
        }
    }

    /// A source holding nothing.
    struct Empty;

    impl BlobSource for Empty {
        fn read(&self, _digest: &Digest) -> Result<Option<Vec<u8>>> {
            Ok(None)
        }
    }

    #[test]
    fn no_remote_never_holds_a_result() {
        assert_eq!(NoRemote.lookup(&action()).unwrap(), None);
    }

    #[test]
    fn no_remote_holds_no_blob() {
        let error = NoRemote.download(&[Blob::of(b"x")]).unwrap_err();

        assert!(
            matches!(
                error,
                Error::Adapter {
                    port: PortName::RemoteCache,
                    ..
                }
            ),
            "{error:?}"
        );
        assert!(error.to_string().starts_with("remote cache: "), "{error}");
    }

    #[test]
    fn no_remote_refuses_to_execute() {
        let error = NoRemote.execute(&action()).unwrap_err();

        assert!(
            matches!(
                error,
                Error::Adapter {
                    port: PortName::RemoteCache,
                    ..
                }
            ),
            "{error:?}"
        );
        assert!(error.to_string().starts_with("remote cache: "), "{error}");
    }

    #[test]
    fn no_remote_reads_no_tree() {
        assert_eq!(NoRemote.decode_tree(b"tree").unwrap(), None);
    }

    #[test]
    fn no_remote_accepts_an_upload_and_keeps_nothing() {
        let produced = RemoteUpload {
            outputs: Vec::new(),
            stdout: Blob::of(b""),
            stderr: Blob::of(b""),
        };

        let uploaded = NoRemote.upload(&action(), &produced, &Empty).unwrap();

        assert_eq!(uploaded, Uploaded::Done);
    }
}
