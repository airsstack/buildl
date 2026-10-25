//! The remote-cache adapter: the REAPI v2 message and service types, the runtime that drives
//! the gRPC client, and, behind the `test-server` feature, an in-memory server to test against.

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
