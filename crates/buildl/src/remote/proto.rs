//! The REAPI v2, `ByteStream` and googleapis messages and services, as prost and tonic
//! generated them from the protos vendored under `proto/`.
//!
//! The generated files live in `generated/reapi/`, one per protobuf package, and are pulled
//! in here under modules named like the packages: the code in them reaches a message of
//! another package by a `super::` path that only resolves with this tree. Regenerate them
//! with `cargo make reapi-codegen`. The server halves of the services exist only with the
//! `test-server` feature.
#![expect(
    unreachable_pub,
    reason = "the generated items are `pub`, and the tree is crate-private"
)]
// `allow`, not `expect`: `dead_code` fires on current stable but not on the 1.94 floor, where
// an `expect` would itself warn.
#![allow(
    dead_code,
    reason = "the generated code is whole: it holds every message and call of the protos, and the adapter names the few it needs"
)]
#![expect(
    clippy::doc_markdown,
    clippy::doc_lazy_continuation,
    reason = "the doc comments are the protos' own text, copied as prost writes them"
)]
#![expect(
    clippy::derive_partial_eq_without_eq,
    clippy::missing_const_for_fn,
    clippy::trivially_copy_pass_by_ref,
    clippy::use_self,
    reason = "generated code is not written to this workspace's style lints"
)]
// These two fire in the server halves alone, which exist only with `test-server`.
#![cfg_attr(
    feature = "test-server",
    expect(
        clippy::default_trait_access,
        clippy::too_many_lines,
        reason = "generated code is not written to this workspace's style lints"
    )
)]

pub(crate) mod build {
    pub(crate) mod bazel {
        pub(crate) mod remote {
            pub(crate) mod execution {
                pub(crate) mod v2 {
                    include!(concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/generated/reapi/build.bazel.remote.execution.v2.rs"
                    ));
                }
            }
        }

        pub(crate) mod semver {
            include!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/generated/reapi/build.bazel.semver.rs"
            ));
        }
    }
}

pub(crate) mod google {
    pub(crate) mod api {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/generated/reapi/google.api.rs"
        ));
    }

    pub(crate) mod bytestream {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/generated/reapi/google.bytestream.rs"
        ));
    }

    pub(crate) mod longrunning {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/generated/reapi/google.longrunning.rs"
        ));
    }

    pub(crate) mod rpc {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/generated/reapi/google.rpc.rs"
        ));
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use prost::Message;
    use prost::encoding::{DecodeContext, WireType, decode_key, skip_field};

    use super::build::bazel::remote::execution::v2::{Action, Command, Digest, Platform};

    /// The field numbers of `bytes`, in the order they were written.
    fn tags(mut bytes: &[u8]) -> Vec<u32> {
        let mut found = Vec::new();
        while !bytes.is_empty() {
            let (tag, wire_type): (u32, WireType) = decode_key(&mut bytes).unwrap();
            skip_field(wire_type, tag, &mut bytes, DecodeContext::default()).unwrap();
            found.push(tag);
        }
        found
    }

    #[test]
    #[expect(
        deprecated,
        reason = "field 5 of `Command` is deprecated upstream; it is set here to pin that the encoder still writes it in tag order"
    )]
    fn prost_encodes_in_tag_order() {
        let command = Command {
            output_paths: vec!["out".to_owned()],
            working_directory: "work".to_owned(),
            platform: Some(Platform::default()),
            arguments: vec!["cc".to_owned()],
            ..Command::default()
        };
        // The platform is empty, so it is written as a field of length zero and still counts.
        assert_eq!(tags(&command.encode_to_vec()), [1, 5, 6, 7]);
    }

    #[test]
    fn an_action_round_trips() {
        let action = Action {
            command_digest: Some(Digest {
                hash: "a".repeat(64),
                size_bytes: 12,
            }),
            input_root_digest: Some(Digest {
                hash: "b".repeat(64),
                size_bytes: 34,
            }),
            do_not_cache: true,
            salt: vec![1, 2, 3],
            ..Action::default()
        };
        assert_eq!(
            Action::decode(action.encode_to_vec().as_slice()).unwrap(),
            action
        );
    }
}
