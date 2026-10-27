//! What the remote cache says it can do.
//!
//! The client asks `GetCapabilities` once and keeps the answer: whether the cache speaks
//! SHA-256, whether clients may write, how much a batch call may carry, and whether the server
//! also executes actions.

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

/// Whether the server executes actions, as its execution capabilities state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Execution {
    /// The server states no execution capabilities.
    Absent,
    /// The server has execution switched off.
    Disabled,
    /// The server executes, but not with SHA-256.
    NoSha256,
    /// The server executes with SHA-256.
    Enabled,
}

impl Execution {
    /// Reads the execution half of the server's answer. The list of digest functions is
    /// consulted first and the legacy single one only when the list is empty.
    fn from_response(response: &ServerCapabilities) -> Self {
        let Some(execution) = response.execution_capabilities.as_ref() else {
            return Self::Absent;
        };
        let sha256 = digest_function::Value::Sha256 as i32;
        let speaks_sha256 = if execution.digest_functions.is_empty() {
            execution.digest_function == sha256
        } else {
            execution.digest_functions.contains(&sha256)
        };
        if !execution.exec_enabled {
            Self::Disabled
        } else if speaks_sha256 {
            Self::Enabled
        } else {
            Self::NoSha256
        }
    }
}

/// What the cache offers, once it is known to speak SHA-256.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Capabilities {
    updates: Updates,
    execution: Execution,
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
            execution: Execution::from_response(response),
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

    /// Whether the server can execute actions.
    ///
    /// # Errors
    ///
    /// Returns [`RemoteError::NoExecution`] when the server states no execution capabilities,
    /// has execution switched off, or does not execute with SHA-256.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "no caller asks for execution yet")
    )]
    pub(crate) const fn require_execution(&self) -> Result<(), RemoteError> {
        let reason = match self.execution {
            Execution::Enabled => return Ok(()),
            Execution::Absent => "it states no execution capabilities",
            Execution::Disabled => "execution is not enabled",
            Execution::NoSha256 => "it does not execute with SHA-256",
        };
        Err(RemoteError::NoExecution { reason })
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
        ActionCacheUpdateCapabilities, CacheCapabilities, ExecutionCapabilities,
        ServerCapabilities, digest_function,
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

    fn with_execution(execution: ExecutionCapabilities) -> ServerCapabilities {
        ServerCapabilities {
            execution_capabilities: Some(execution),
            ..response(digest_function::Value::Sha256, true, 0)
        }
    }

    fn execution(
        exec_enabled: bool,
        functions: &[digest_function::Value],
    ) -> ExecutionCapabilities {
        ExecutionCapabilities {
            exec_enabled,
            digest_functions: functions.iter().map(|each| *each as i32).collect(),
            ..ExecutionCapabilities::default()
        }
    }

    fn can_execute(response: &ServerCapabilities) -> Result<(), RemoteError> {
        Capabilities::from_response(response)
            .unwrap()
            .require_execution()
    }

    #[test]
    fn execution_needs_exec_enabled() {
        let enabled = with_execution(execution(true, &[digest_function::Value::Sha256]));
        let disabled = with_execution(execution(false, &[digest_function::Value::Sha256]));

        assert!(can_execute(&enabled).is_ok());
        assert!(matches!(
            can_execute(&disabled),
            Err(RemoteError::NoExecution { .. })
        ));
    }

    #[test]
    fn execution_needs_sha256_in_its_digest_functions() {
        let without = with_execution(execution(true, &[digest_function::Value::Sha1]));
        let among = with_execution(execution(
            true,
            &[digest_function::Value::Sha1, digest_function::Value::Sha256],
        ));

        assert!(matches!(
            can_execute(&without),
            Err(RemoteError::NoExecution { .. })
        ));
        assert!(can_execute(&among).is_ok());
    }

    #[test]
    fn the_legacy_digest_function_counts_when_the_list_is_empty() {
        let legacy = |function: digest_function::Value| ExecutionCapabilities {
            digest_function: function as i32,
            ..execution(true, &[])
        };

        assert!(can_execute(&with_execution(legacy(digest_function::Value::Sha256))).is_ok());
        assert!(matches!(
            can_execute(&with_execution(legacy(digest_function::Value::Sha1))),
            Err(RemoteError::NoExecution { .. })
        ));
    }

    #[test]
    fn a_server_without_execution_capabilities_cannot_execute() {
        let outcome = can_execute(&response(digest_function::Value::Sha256, true, 0));

        assert!(
            matches!(outcome, Err(RemoteError::NoExecution { .. })),
            "{outcome:?}"
        );
    }

    #[test]
    fn the_cache_half_is_checked_as_before() {
        let cache_only =
            Capabilities::from_response(&response(digest_function::Value::Sha256, true, 0));
        let refused = Capabilities::from_response(&ServerCapabilities {
            execution_capabilities: Some(execution(true, &[digest_function::Value::Sha256])),
            ..ServerCapabilities::default()
        });
        let writable = with_execution(execution(false, &[]));

        assert!(cache_only.is_ok_and(|each| each.require_updates().is_ok()));
        assert!(matches!(refused, Err(RemoteError::NoSha256)), "{refused:?}");
        assert!(
            Capabilities::from_response(&writable).is_ok_and(|each| each.require_updates().is_ok())
        );
    }
}
