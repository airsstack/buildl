//! The remote-cache adapter.
//!
//! It holds the REAPI v2 message and service types, the runtime that drives the gRPC client, the
//! channel to the cache and the login every call carries, and, behind the `test-server`
//! feature, an in-memory server to test against.

#[cfg_attr(
    not(all(test, feature = "test-server")),
    expect(
        dead_code,
        reason = "no code outside this module's own tests connects a channel yet"
    )
)]
mod channel;
#[cfg_attr(
    not(all(test, feature = "test-server")),
    expect(
        dead_code,
        reason = "no code outside this module's own tests logs in yet"
    )
)]
mod login;
mod proto;
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "no code outside this module's own tests names the runtime"
    )
)]
mod runtime;
#[cfg(feature = "test-server")]
pub mod test_server;
#[cfg(all(test, feature = "test-server"))]
mod tls_fixture;
