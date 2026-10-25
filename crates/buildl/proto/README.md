# Vendored protocol definitions

The `.proto` files the remote cache client is generated from, copied byte for byte from their
upstream repositories. Every file here is licensed under Apache-2.0, as its own header states.

| Files | Source | Commit |
|---|---|---|
| `build/bazel/remote/execution/v2/remote_execution.proto`, `build/bazel/semver/semver.proto` | [bazelbuild/remote-apis](https://github.com/bazelbuild/remote-apis) | `c2c07e3d718d3d490a4958344d9c3e47de0bfaf3` |
| `google/api/{annotations,client,field_behavior,http,launch_stage}.proto`, `google/longrunning/operations.proto`, `google/rpc/status.proto`, `google/bytestream/bytestream.proto` | [googleapis/googleapis](https://github.com/googleapis/googleapis) | `dff1cd0c7ff8c26ff4c9f7a87cbe175fb5124d4e` |

The `google/protobuf/*` well-known types these files import are not vendored: the generator's
`protox` compiler ships them.

The set is the import closure of `remote_execution.proto` and `bytestream.proto`. A file taken
from a different commit must replace every file from the same source, so the set stays one
upstream snapshot.

## Regenerating

The Rust code is generated into `crates/buildl/generated/reapi/` and committed, so building
buildl needs neither `protoc` nor the generator. After changing a file here, run

```sh
cargo make reapi-codegen
```

which builds `tools/reapi-codegen/` (outside the workspace) and rewrites the generated files.
Running it twice in a row leaves the tree unchanged.
