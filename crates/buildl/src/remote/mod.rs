//! The remote-cache adapter.
//!
//! It holds the REAPI v2 message and service types, the runtime that drives the gRPC client, the
//! channel to the cache and the login every call carries, the cache's capabilities, the
//! transfer of blobs, the blocking client that makes the calls and the error it fails with, the
//! encoding of buildl's remote values as REAPI messages, the input root of an action, and
//! [`Reapi`], the remote cache built from them. Behind the `test-server` feature it also holds
//! an in-memory server to test against.

mod capabilities;
mod channel;
mod client;
mod encode;
mod error;
mod input_tree;
mod login;
mod proto;
mod reapi;
mod runtime;
#[cfg(feature = "test-server")]
pub mod test_server;
#[cfg(all(test, feature = "test-server"))]
mod tls_fixture;
mod transfer;
mod vocabulary;

pub use reapi::Reapi;
