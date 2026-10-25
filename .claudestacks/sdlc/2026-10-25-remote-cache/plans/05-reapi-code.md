---
status: approved
created: 2026-10-25
built-on: 3d9113128fe5
---

# REAPI Code Implementation Plan

**Goal:** `buildl` carries REAPI v2 client code generated from vendored protos, passing every workspace gate.

**Architecture:** The protos are vendored under `crates/buildl/proto/` (spec §5); a generator
kept outside the workspace (`tools/reapi-codegen/`, excluded from the root `Cargo.toml`) runs
`protox` and `tonic-prost-build` and writes the Rust into `crates/buildl/generated/reapi/`,
which is committed (P19). The generated files live **outside `src/`**: prost writes
`super::super` paths (13 lines in the probe's output, e.g. `google.longrunning.rs:39`), which
`guard-adapter-isolation` refuses anywhere under `crates/buildl/src` (`Makefile.toml`, the
guard's first `grep`), and files outside `src/` are outside the unit-test mandate's scope
("Each `.rs` file under `src/`", `unit-test-mandate.md:9`) and outside `cargo fmt`. One
ordinary file, `crates/buildl/src/remote/proto.rs` — not a `mod.rs`, so the export-only rule
does not bind it — declares the package-shaped modules and pulls each generated file in with
`include!(concat!(env!("CARGO_MANIFEST_DIR"), "/generated/reapi/<file>.rs"))`, carrying the
`#[expect]`s the lints need, and has its own unit tests. The server halves are generated
behind `#[cfg(feature = "test-server")]` (`server_mod_attribute`,
`tonic-prost-build-0.14.6/src/lib.rs:544`).

**Tech Stack:** Rust; `tonic` 0.14.6, `tonic-prost`, `prost` 0.14, `prost-types` 0.14, `tokio`,
`uuid`; generator-only `protox` 0.10, `tonic-prost-build` 0.14; `cargo make dod`,
`cargo deny check`. Guideline: `claudestacks-guideline-rust:rust-guidelines`.

**Content authority:** spec §5 (Generated code), §8 (dependencies, `deny.toml`, feature
`test-server`), P8, P16-P19.

---

## Files that change

```
crates/buildl/proto/build/bazel/remote/execution/v2/remote_execution.proto — [create] vendored, remote-apis c2c07e3d718d3d490a4958344d9c3e47de0bfaf3
crates/buildl/proto/build/bazel/semver/semver.proto                        — [create] vendored, same commit
crates/buildl/proto/google/api/{annotations,http,client,launch_stage,field_behavior}.proto — [create] vendored, googleapis dff1cd0c7ff8c26ff4c9f7a87cbe175fb5124d4e
crates/buildl/proto/google/longrunning/operations.proto                    — [create] vendored, same googleapis commit
crates/buildl/proto/google/rpc/status.proto                                — [create] vendored, same googleapis commit
crates/buildl/proto/google/bytestream/bytestream.proto                     — [create] vendored, same googleapis commit
crates/buildl/proto/README.md                                              — [create] each source, its commit, its license (Apache-2.0), how to regenerate
tools/reapi-codegen/Cargo.toml                                             — [create] a package with its own empty [workspace]; protox, tonic-prost-build
tools/reapi-codegen/Cargo.lock                                             — [create] committed, so a regeneration resolves the same generator
tools/reapi-codegen/src/main.rs                                            — [create] compile the protos, generate client and server code, write generated/reapi/
crates/buildl/generated/reapi/*.rs                                         — [create] the generated code, committed
crates/buildl/src/remote/proto.rs                                          — [create] the package module tree over generated/reapi/, its #[expect]s, its unit tests
crates/buildl/src/remote/mod.rs                                            — [create] table of contents: `mod proto` (crate-private)
crates/buildl/src/lib.rs                                                   — [modify] `pub mod remote` beside the other adapters (:172-188)
Cargo.toml                                                                 — [modify] `exclude = ["tools/reapi-codegen"]`; workspace dependencies with reasons
crates/buildl/Cargo.toml                                                   — [modify] the dependencies; `[features] test-server`
deny.toml                                                                  — [modify] licenses, skips, bans of spec §8
crates/expected-edges.txt                                                  — [modify] buildl's new direct dependencies
Makefile.toml                                                              — [modify] task `reapi-codegen` running the generator
```

The vendored googleapis set is the closure of the two root protos' imports: `re.proto:19-26`
and `grep -h "^import"` over the googleapis files, which lists
`google/api/{annotations,client,field_behavior,http,launch_stage}`, `google/rpc/status` and
`google/protobuf/*` (shipped by `protox`). The googleapis commit is the one whose copies of
these eight files are byte-identical to the ones P16-P19 compiled (checked with `cmp` against
`dff1cd0c7ff8c26ff4c9f7a87cbe175fb5124d4e`: all eight `same`).

## Order of work

### Task 1 — Vendor the protos

**Files:** `crates/buildl/proto/**`, `proto/README.md`.

**Change:** copy the files above byte for byte from their pinned commits; the README names both
commits and each file's license.

**Proof:** for each vendored file, `curl -sSL <raw url at its commit> | cmp - <file>` exits 0.

**Done when:** that check passes for every file. Commit `chore(buildl): vendor the REAPI v2 and ByteStream protos`.

### Task 2 — The generator and the generated code

**Files:** `tools/reapi-codegen/**`, `crates/buildl/generated/reapi/*.rs`, `remote/proto.rs`,
`remote/mod.rs`, `lib.rs`, `Cargo.toml`, `crates/buildl/Cargo.toml`, `Makefile.toml`.

**Change:** the generator compiles `remote_execution.proto` and `bytestream.proto` with
`protox` (no `protoc`, P19), generates client and server code with `tonic-prost-build`
(`build_client(true)`, `build_server(true)`, `server_mod_attribute(".", "#[cfg(feature =
\"test-server\")]")`, `out_dir` `crates/buildl/generated/reapi`); `cargo make reapi-codegen`
runs it. The root manifest excludes the tool and adds `tonic`, `tonic-prost`, `prost`,
`prost-types`, `tokio` (`rt`, `rt-multi-thread`) and `uuid` (`v4`) to
`[workspace.dependencies]`, each with its reason; `crates/buildl/Cargo.toml` takes them and
declares `test-server = ["tonic/server", "tonic/router", "tokio/net"]` — `router` because the
test server serves four services on one address, which tonic 0.14.6 offers only through
`Router` (`tonic-0.14.6/src/transport/server/mod.rs:149`, `:510`); with it, `cargo deny check`
still prints `advisories ok, bans ok, licenses ok, sources ok` (probe, 2026-10-25).
`remote/proto.rs` builds `build::bazel::remote::execution::v2`, `build::bazel::semver` and
`google::{api, longrunning, rpc, bytestream}` over the generated files and carries the
`#[expect(...)]`s the gate needs on them, each with its reason — the set is whatever
`cargo make dod` reports, nothing more (Risks).

**Proof:**
- `proto.rs` unit tests, written first: `prost_encodes_in_tag_order` — a `Command` with fields 1, 5, 6, 7 set encodes with tags `[1, 5, 6, 7]` (P8, kept as a regression); `an_action_round_trips` — encode then decode gives the same `Action`.
- `cargo make reapi-codegen` run twice leaves `git status` clean.
- `cargo check -p buildl` (feature off) and `cargo check --workspace --all-targets --all-features` succeed; `cargo +1.94 check -p buildl` succeeds (P16).
- `cargo package -p buildl --list --allow-dirty` lists `generated/reapi/*.rs`, so the published crate builds.

**Done when:** `cargo make dod` green. Commit `feat(buildl): generated REAPI v2 client code`.

### Task 3 — `cargo deny` and the crate edges

**Files:** `deny.toml`, `crates/expected-edges.txt`.

**Change:** `licenses.allow` gains `BSD-3-Clause` and `ISC`; `bans.skip` gains
`getrandom@0.2.17`, `syn@2.0.119`, `windows-sys@0.52.0`, each with its reason (P17);
`bans.deny` gains `tonic` (wrappers `buildl`, `tonic-prost`), `prost` (`buildl`, `prost-types`,
`tonic-prost`) and `uuid` (`buildl`), with reasons citing spec §8. The edges golden lists
buildl's new direct dependencies.

**Proof:** `cargo deny check` prints `advisories ok, bans ok, licenses ok, sources ok` (P18);
`cargo make guard-crate-edges` passes.

**Done when:** `cargo make dod`, `cargo deny check` and every `guard-*` task green. Commit
`chore(repo): admit the gRPC client's licenses and duplicates`.

## Risks

- **Generated code under the lint gate:** clippy pedantic and nursery, `missing_docs` and
  rustdoc's link checks may fire on generated items (proto comments hold reference-style links
  such as `[Action][build.bazel.remote.execution.v2.Action]`). Task 2 settles it by running the
  gate: an `#[expect]` per lint that fires, or `disable_comments`
  (`tonic-prost-build-0.14.6/src/lib.rs:674`) if the comments are what fails rustdoc; the
  choice is recorded in the execution record.
- Versions resolved when the task runs may differ from the probe's (`getrandom@0.2.17`, …); the
  skip entries name whatever versions `cargo deny` reports, and its output is the proof.
- The generator's own dependencies never enter the workspace graph; it is built only when
  someone regenerates.

## Proof

`cargo make dod`, `cargo deny check` and every `guard-*` task green with the generated code
compiled in; regenerating changes nothing; the packaged crate carries the generated files.
