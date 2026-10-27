//! Regenerates the REAPI v2, ByteStream and googleapis error-details Rust code that `buildl`
//! commits under `crates/buildl/generated/reapi/`.
//!
//! Run through `cargo make reapi-codegen`. The protos are read from
//! `crates/buildl/proto/`, compiled with `protox` (no `protoc` needed), and written
//! out as one file per protobuf package. The server halves of the services are
//! gated behind the `test-server` feature of `buildl`, so a normal build carries
//! the client alone.
//!
//! No inline tests, by exemption: the file holds one constant and a few library calls
//! (`protox::compile`, `tonic_prost_build`), no logic of its own. Its output is checked
//! in CI by regenerating and diffing `crates/buildl/generated/`.

use std::error::Error;
use std::path::{Path, PathBuf};

/// The protos whose services and messages `buildl` uses; everything they import is
/// compiled with them.
const ROOT_PROTOS: [&str; 3] = [
    "build/bazel/remote/execution/v2/remote_execution.proto",
    "google/bytestream/bytestream.proto",
    "google/rpc/error_details.proto",
];

fn main() -> Result<(), Box<dyn Error>> {
    let buildl = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/buildl");
    let proto_dir: PathBuf = buildl.join("proto");
    let out_dir: PathBuf = buildl.join("generated/reapi");

    let descriptors = protox::compile(ROOT_PROTOS, [&proto_dir])?;

    std::fs::create_dir_all(&out_dir)?;
    // Type names give every message its protobuf name, which is how a status's `Any` details
    // are told apart; the domain makes each `type_url` read as a server writes it.
    let mut config = tonic_prost_build::Config::new();
    config.enable_type_names();
    config.type_name_domain(["."], "type.googleapis.com");
    tonic_prost_build::configure()
        .build_client(true)
        .build_server(true)
        .server_mod_attribute(".", "#[cfg(feature = \"test-server\")]")
        .out_dir(&out_dir)
        .compile_fds_with_config(descriptors, config)?;
    Ok(())
}
