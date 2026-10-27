//! The remote-cache adapter.
//!
//! It holds the REAPI v2 message and service types, the runtime that drives the gRPC client, the
//! channel to the cache and the login every call carries, the cache's capabilities, the
//! transfer of blobs, the driving of one execution on the server to its end, the blocking client
//! that makes the calls and the error it fails with, the rule that says when a failed call is
//! made again, the encoding of buildl's remote values as REAPI messages, the input root of an
//! action, and [`Reapi`], the remote cache built from them, and [`Remote`], the cache a run is given: that one or none; and [`token_is_present`], which
//! says whether the token the settings name can be found. Behind the `test-server` feature it
//! also holds an in-memory server to test against, which stores blobs and action results and
//! runs the actions it is asked to execute.

mod capabilities;
mod channel;
mod client;
mod configured;
mod encode;
mod error;
mod execution;
mod input_tree;
mod login;
mod proto;
mod reapi;
mod retry;
mod runtime;
#[cfg(feature = "test-server")]
mod test_execution;
#[cfg(feature = "test-server")]
pub mod test_server;
#[cfg(all(test, feature = "test-server"))]
mod tls_fixture;
mod token_presence;
mod transfer;
mod vocabulary;

pub use configured::Remote;
pub use reapi::Reapi;
pub use token_presence::token_is_present;
