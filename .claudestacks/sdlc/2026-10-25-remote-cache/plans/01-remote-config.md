---
status: approved
created: 2026-10-25
built-on: 3d9113128fe5
---

# Remote Config Implementation Plan

**Goal:** `buildl.toml`'s `[remote]` table is read strictly into the workspace's remote settings.

**Architecture:** `buildl-core` gains the value types of the table — `RemoteSettings` and its
parsed parts — and `Workspace` gains `remote: Option<RemoteSettings>`, so `Checkup` (which holds
the `Workspace`, `types/checkup.rs:48`) carries it to `doctor` with no further plumbing. The
`buildl` manifest adapter gains a `RemoteTable` in `schema.rs` and a `remote()` interpreter in
`interpret.rs`, beside `execution()` (`interpret.rs:298`) and `approver()` (`:316`), each key
built through its own type and errors located by line and column as every other key is. No
behaviour reads the settings yet.

**Tech Stack:** Rust, `buildl-core`, `buildl` (`toml`, `serde`), `cargo make dod`,
`cargo deny check`. Guideline: `claudestacks-guideline-rust:rust-guidelines`.

**Content authority:** spec §6 (the table, its keys, its refusals), §9 (strong types), P9, P28.

---

## Files that change

```
crates/buildl-core/src/types/remote_settings.rs — [create] RemoteSettings, RemoteUrl, RemoteScheme, InstanceName, HostPath, TokenSource, HeaderName; parsing and unit tests
crates/buildl-core/src/types/mod.rs             — [modify] declare and re-export the new module
crates/buildl-core/src/types/workspace.rs       — [modify] Workspace gains `remote: Option<RemoteSettings>`
crates/buildl-core/src/lib.rs                   — [modify] re-export the new types
crates/buildl-core/src/error.rs                 — [modify] NameKind gains RemoteUrl, InstanceName, HostPath, HeaderName; Error::InsecureToken
crates/buildl/src/manifest/schema.rs            — [modify] Raw gains `remote: Option<RemoteTable>`; RemoteTable, TokenTable
crates/buildl/src/manifest/interpret.rs         — [modify] `remote(text, table)`; `workspace()` fills the field
crates/buildl-cli/src/doctor_text.rs            — [modify] test fixture Workspace literal (:257) gains `remote: None`
crates/buildl-core/src/types/checkup.rs         — [modify] test Workspace literal (:95) gains `remote: None`
crates/buildl-core/src/grants/outside.rs        — [modify] test Workspace literal (:61)
crates/buildl-core/src/grants/plugins.rs        — [modify] test Workspace literal (:123)
crates/buildl-core/src/plan/pinned.rs           — [modify] test Workspace literal (:56)
crates/buildl-core/src/plan/authority.rs        — [modify] test Workspace literal (:143)
crates/buildl-core/src/ports/port_set.rs        — [modify] test Workspace literal (:129)
crates/buildl/src/report/plan_text.rs           — [modify] test Workspace literal (:163)
crates/buildl-core/tests/flows/common.rs        — [modify] `open_workspace()` literal (:1178)
```

The Workspace literal sites are every `Workspace {` construction found by
`grep -rn "Workspace {" crates/*/src crates/*/tests`, less the hits that are a return type
(`-> Workspace {`, e.g. `buildl/tests/e2e/common.rs:212`, which calls the facade's
`Workspace::open`) or a definition (control: the same search finds the production site
`interpret.rs:66`).

## Order of work

### Task 1 — The remote settings types

**Files:** `types/remote_settings.rs`, `types/mod.rs`, `lib.rs`, `error.rs`.

**Change:** introduce, each parsed at construction (parse, don't validate):
- `RemoteScheme` — `Grpc` or `Grpcs`.
- `RemoteUrl` — `grpc://` or `grpcs://`, then a host, then an optional `:port`, nothing after;
  keeps scheme, host and port; `Display` writes it back as given.
- `InstanceName` — non-empty, no leading or trailing `/`, no path segment equal to `blobs`,
  `uploads`, `actions`, `actionResults`, `operations`, `capabilities` or `compressed-blobs`
  (`re.proto:229-231`).
- `HostPath` — a file the host reads, named in `buildl.toml`: absolute, or relative to the
  workspace root; non-empty, no `..` segment in a relative path.
- `TokenSource` — `Env(EnvName)` or `File(HostPath)`.
- `HeaderName` — one or more of `0-9 a-z _ - .` (gRPC `Header-Name`,
  `PROTOCOL-HTTP2.md:49`), not starting `grpc-` (`:66`), not ending `-bin` (`:68`); default
  `authorization`.
- `RemoteSettings` — `url`, `instance: Option<InstanceName>`, `ca: Option<HostPath>`,
  `token: Option<TokenSource>`, `header: HeaderName`, `timeout: Timeout` (default 30 s, the
  existing `Timeout` type, `types/timeout.rs:80`). Its constructor refuses a token with a
  `grpc://` url (spec §6) with a new `Error::InsecureToken { url }`.

Each other refusal is `Error::InvalidName` with a new `NameKind` naming the value's kind.

**Proof:** written first, seen failing, then passing —
- `a_grpcs_url_keeps_host_and_port` — `grpcs://cache.example.com:443` parses; scheme, host, port read back
- `a_url_without_a_known_scheme_is_refused` — `https://x`, `grpc:/x`, `grpc://`, `grpc://h/path` each refused
- `an_instance_may_not_use_a_reserved_segment` — `ci` ok, `a/blobs/b` and `/ci` refused
- `a_header_name_follows_grpc_rules` — `authorization`, `x-api-key` ok; `X-Key`, `grpc-x`, `key-bin`, `` refused
- `a_relative_host_path_may_not_climb` — `certs/ca.pem`, `/etc/ca.pem` ok; `../ca.pem` refused
- `a_token_over_plain_grpc_is_refused` — settings with `grpc://` and a token → `Error::InsecureToken`; without a token accepted
- `the_default_header_and_timeout` — `authorization` and 30 s

**Done when:** `cargo test -p buildl-core remote_settings` passes and `cargo make dod` is green.
Commit `feat(buildl-core): remote settings types`.

### Task 2 — `Workspace.remote`

**Files:** `types/workspace.rs` and every Workspace literal site listed above.

**Change:** `Workspace` gains `remote: Option<RemoteSettings>` (`None`: the workspace names no
server). Every literal gains `remote: None`; `interpret.rs:66` too, until Task 3.

**Proof:** the existing suites stay green — `cargo test --workspace --all-targets --all-features`.

**Done when:** `cargo make dod` is green. Commit `feat(buildl-core): a workspace may name a remote cache`.

### Task 3 — Reading `[remote]`

**Files:** `manifest/schema.rs`, `manifest/interpret.rs`.

**Change:** `RemoteTable` (`url`, `instance`, `ca`, `token`, `header`, `timeout`, all
`Spanned`, `deny_unknown_fields`) and `TokenTable` (`env`, `file`, `deny_unknown_fields`).
`remote(text, table)` builds `RemoteSettings`; `url` is required when the table exists; a
`token` with both or neither key is refused; every refusal names the key with its line and
column through the existing `position` (`interpret.rs:337`) path, as other keys do.

**Proof:** written first, seen failing, then passing (unit, beside the existing interpret tests) —
- `no_remote_table_means_no_remote` — `Workspace.remote` is `None`
- `a_full_remote_table_is_read` — every key of spec §6's example read back
- `a_remote_table_without_url_is_refused` — the error names `remote.url`
- `an_unknown_remote_key_is_refused_with_its_position` — `urll = …` names the key, line, column
- `a_token_with_env_and_file_is_refused` / `a_token_with_neither_is_refused` — each names `remote.token`
- `a_token_with_a_grpc_url_is_refused_with_its_position` — at the `token` key's line and column
- `a_bad_timeout_is_refused` — `timeout = "30"` names `remote.timeout`

**Done when:** `cargo test -p buildl manifest` passes and `cargo make dod` is green. Commit
`feat(buildl): read the [remote] table of buildl.toml`.

## Risks

- Adding a public field to `Workspace` touches every literal (listed above); a missed one fails
  to compile, so the risk is only churn.
- The settings are read but unused until plan 08; nothing changes for a workspace without the
  table.

## Proof

`cargo make dod` and `cargo deny check` green; a manifest with spec §6's example table reads
back exactly; every refusal of spec §6 names its key, line and column.
