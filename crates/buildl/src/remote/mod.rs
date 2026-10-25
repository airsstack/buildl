//! The remote-cache adapter.
//!
//! It holds the REAPI v2 message and service types, the runtime that drives the gRPC client, the
//! channel to the cache and the login every call carries, the cache's capabilities, the
//! transfer of blobs, the blocking client that makes the calls and the error it fails with,
//! and, behind the `test-server` feature, an in-memory server to test against.

// Only `client` is an entry point that nothing outside the tests reaches. Whether the modules it
// uses are reported as dead too depends on the compiler (current stable reports them, the
// 1.94 floor does not), so they carry `allow`, as `proto` does, where an `expect` would be
// unfulfilled on one of the two.
#[cfg_attr(
    not(all(test, feature = "test-server")),
    allow(
        dead_code,
        reason = "no code outside this module's own tests asks for capabilities yet"
    )
)]
mod capabilities;
#[cfg_attr(
    not(all(test, feature = "test-server")),
    allow(
        dead_code,
        reason = "no code outside this module's own tests connects a channel yet"
    )
)]
mod channel;
#[cfg_attr(
    not(all(test, feature = "test-server")),
    expect(
        dead_code,
        reason = "no code outside this module's own tests makes a client yet"
    )
)]
mod client;
#[cfg_attr(
    not(all(test, feature = "test-server")),
    allow(
        dead_code,
        reason = "no code outside this module's own tests fails with a remote error yet"
    )
)]
mod error;
#[cfg_attr(
    not(all(test, feature = "test-server")),
    allow(
        dead_code,
        reason = "no code outside this module's own tests logs in yet"
    )
)]
mod login;
mod proto;
#[cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "no code outside this module's own tests names the runtime"
    )
)]
mod runtime;
#[cfg(feature = "test-server")]
pub mod test_server;
#[cfg(all(test, feature = "test-server"))]
mod tls_fixture;
#[cfg_attr(
    not(all(test, feature = "test-server")),
    allow(
        dead_code,
        reason = "no code outside this module's own tests moves blobs yet"
    )
)]
mod transfer;
