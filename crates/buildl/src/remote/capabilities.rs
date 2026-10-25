//! What the remote cache says it can do.
//!
//! The client asks `GetCapabilities` once and keeps the answer: whether the cache speaks
//! SHA-256, whether clients may write, and how much a batch call may carry.

use super::client::Wire;
use super::error::RemoteError;
use super::proto::build::bazel::remote::execution::v2::{
    GetCapabilitiesRequest, ServerCapabilities, digest_function,
};
use super::vocabulary::{Method, Updates};

/// The largest message tonic accepts by default, which bounds a batch whatever the cache says.
pub(super) const MESSAGE_CAP: usize = 4 * 1024 * 1024;

/// Room left under the message cap for what a batch carries besides its entries: the instance
/// name and the message's own fields.
pub(super) const HEADROOM: usize = 64 * 1024;

/// What the cache offers, once it is known to speak SHA-256.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Capabilities {
    updates: Updates,
    /// The cache's own limit on a batch, `None` when it states none.
    batch_limit: Option<usize>,
}

impl Capabilities {
    /// Reads the cache's answer.
    ///
    /// # Errors
    ///
    /// Returns [`RemoteError::NoSha256`] when SHA-256 is not among the digest functions.
    pub(crate) fn from_response(response: &ServerCapabilities) -> Result<Self, RemoteError> {
        let cache = response
            .cache_capabilities
            .as_ref()
            .ok_or(RemoteError::NoSha256)?;
        if !cache
            .digest_functions
            .contains(&(digest_function::Value::Sha256 as i32))
        {
            return Err(RemoteError::NoSha256);
        }
        Ok(Self {
            updates: if cache
                .action_cache_update_capabilities
                .as_ref()
                .is_some_and(|updates| updates.update_enabled)
            {
                Updates::Enabled
            } else {
                Updates::Disabled
            },
            batch_limit: usize::try_from(cache.max_batch_total_size_bytes)
                .ok()
                .filter(|limit| *limit > 0),
        })
    }

    /// Asks the cache what it offers.
    ///
    /// # Errors
    ///
    /// Returns the call's failure, or [`RemoteError::NoSha256`].
    pub(crate) async fn fetch(wire: &Wire<'_>) -> Result<Self, RemoteError> {
        let mut client = wire.capabilities();
        let request = GetCapabilitiesRequest {
            instance_name: wire.instance().to_owned(),
        };
        let response = wire
            .bounded(Method::GetCapabilities, client.get_capabilities(request))
            .await?;
        Self::from_response(&response.into_inner())
    }

    /// Whether the cache refuses to let this client write.
    ///
    /// # Errors
    ///
    /// Returns [`RemoteError::UpdatesDisabled`] when the cache does not enable updates.
    pub(crate) const fn require_updates(&self) -> Result<(), RemoteError> {
        match self.updates {
            Updates::Enabled => Ok(()),
            Updates::Disabled => Err(RemoteError::UpdatesDisabled),
        }
    }

    /// The most blob data one batch call may carry: the cache's limit, when it states one,
    /// but never more than the message cap less its headroom.
    pub(crate) fn batch_threshold(&self) -> usize {
        let cap = MESSAGE_CAP - HEADROOM;
        self.batch_limit.map_or(cap, |limit| limit.min(cap))
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use super::{Capabilities, HEADROOM, MESSAGE_CAP};
    use crate::remote::error::RemoteError;
    use crate::remote::proto::build::bazel::remote::execution::v2::{
        ActionCacheUpdateCapabilities, CacheCapabilities, ServerCapabilities, digest_function,
    };

    fn response(function: digest_function::Value, updates: bool, limit: i64) -> ServerCapabilities {
        ServerCapabilities {
            cache_capabilities: Some(CacheCapabilities {
                digest_functions: vec![function as i32],
                action_cache_update_capabilities: Some(ActionCacheUpdateCapabilities {
                    update_enabled: updates,
                }),
                max_batch_total_size_bytes: limit,
                ..CacheCapabilities::default()
            }),
            ..ServerCapabilities::default()
        }
    }

    fn threshold(limit: i64) -> usize {
        Capabilities::from_response(&response(digest_function::Value::Sha256, true, limit))
            .unwrap()
            .batch_threshold()
    }

    #[test]
    fn a_smaller_server_limit_is_the_threshold() {
        assert_eq!(threshold(1024), 1024);
    }

    #[test]
    fn without_a_server_limit_the_threshold_is_the_message_cap_less_headroom() {
        assert_eq!(threshold(0), MESSAGE_CAP - HEADROOM);
    }

    #[test]
    fn a_larger_server_limit_gives_way_to_the_message_cap() {
        assert_eq!(threshold(64 * 1024 * 1024), MESSAGE_CAP - HEADROOM);
    }

    #[test]
    fn a_server_limit_under_the_headroom_still_batches() {
        assert_eq!(threshold(i64::try_from(HEADROOM).unwrap()), HEADROOM);
    }

    #[test]
    fn a_negative_limit_counts_as_none() {
        assert_eq!(threshold(-1), MESSAGE_CAP - HEADROOM);
    }

    #[test]
    fn a_response_without_sha256_is_refused() {
        let outcome = Capabilities::from_response(&response(digest_function::Value::Sha1, true, 0));

        assert!(matches!(outcome, Err(RemoteError::NoSha256)), "{outcome:?}");
    }

    #[test]
    fn a_response_without_cache_capabilities_is_refused() {
        let outcome = Capabilities::from_response(&ServerCapabilities::default());

        assert!(matches!(outcome, Err(RemoteError::NoSha256)), "{outcome:?}");
    }

    #[test]
    fn updates_are_required_only_to_write() {
        let enabled =
            Capabilities::from_response(&response(digest_function::Value::Sha256, true, 0));
        let disabled =
            Capabilities::from_response(&response(digest_function::Value::Sha256, false, 0));

        assert!(enabled.is_ok_and(|each| each.require_updates().is_ok()));
        assert!(matches!(
            disabled.map(|each| each.require_updates()),
            Ok(Err(RemoteError::UpdatesDisabled))
        ));
    }
}
