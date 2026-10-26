---
status: done
created: 2026-10-26
built-on: 4704ec60505e
---

# Remote Execution Settings Implementation Plan

**Goal:** `buildl.toml`'s five new `[remote]` keys are read strictly into the workspace's `RemoteSettings`.

**Architecture:** The value types live in `buildl-core` beside the existing remote settings
(`types/remote_settings.rs`), one parsed type per key that has a grammar, each refusing a bad
value at construction (parse, don't validate). `RemoteSettings` and its builder gain the five
fields with defaults; the `buildl` manifest adapter reads them from `RemoteTable` and refuses a
bad value with the key's line and column, as it does for the six existing keys. Nothing uses
the values yet: plans 03, 04, 05 and 09 read them.

**Tech Stack:** Rust, `buildl-core`, `buildl`, serde/toml, `cargo make dod`, `cargo deny check`.
Guideline: `claudestacks-guideline-rust:rust-guidelines` (strong types, unit-test mandate).

**Content authority:** spec §3 (the keys, their grammars and defaults, the property refusals).

---

## Files that change

```
crates/buildl-core/src/types/remote_settings.rs  — [modify] RemoteJobs, RetryCount, PlatformProperty(Name), PlatformProperties; five fields, builder setters, accessors; unit tests
crates/buildl-core/src/types/mod.rs              — [modify] re-export the new types
crates/buildl-core/src/lib.rs                    — [modify] re-export the new public types
crates/buildl-core/src/error.rs                  — [modify] NameKind (:1043-1096) gains RemoteJobs, RetryCount, PlatformProperty; Display arms
crates/buildl-core/src/types/remote_action.rs    — [modify] RemotePlatform gains OS_FAMILY_PROPERTY and ISA_PROPERTY, the one place the reserved names live
crates/buildl/src/remote/encode.rs               — [modify] fn platform names the two properties through those constants
crates/buildl/src/manifest/schema.rs             — [modify] RemoteTable (:182-195) gains execute, jobs, queue_timeout, retries, properties
crates/buildl/src/manifest/interpret.rs          — [modify] fn remote (:334) reads the five keys; tests (:542)
crates/buildl-cli/src/exit.rs                    — [modify] only if its NameKind list (grep `NameKind::` there) must name the new kinds
```

## Order of work

### Task 1 — The value types

**Files:** `types/remote_settings.rs`, `types/mod.rs`, `lib.rs`, `error.rs`.

**Change:** add, each with one purpose (spec §3):
- `RemoteJobs` — a whole number from 1 to 1024, default 128; `parse(&str)` for the flag of plan 09
  and a constructor from the TOML integer.
- `RetryCount` — a whole number from 0 to 10, default 5; the retries after the first call.
- `PlatformPropertyName` — a non-empty string that is neither `OSFamily` nor `ISA`, compared as
  written (case-sensitive, spec §3 citing `REAPI:953`).
- `PlatformProperties` — a name-sorted list of `(PlatformPropertyName, String)`; built from a
  `BTreeMap`, so it is sorted once at construction (spec §3, `REAPI:965-969`).
- `queue_timeout` reuses `Timeout` (`types/timeout.rs:81`, grammar `:38`), default 1 h through
  `Timeout::from_secs` (`:106`).

`RemoteSettings` (`:411`) gains `execute: bool`, `jobs`, `queue_timeout`, `retries`,
`properties`, with builder setters beside `instance` (`:485`) and accessors beside `timeout`
(`:465`). Refusals go through the existing `invalid(kind, raw, reason)` (`:42`) with the new
`NameKind` variants.

**Proof:** written first, seen failing, then passing (in `remote_settings.rs`'s `mod tests`, `:536`) —
- `jobs_reads_one_to_1024` — `1` and `1024` read; `0`, `1025` and `-1` are refused naming the kind
- `retries_reads_zero_to_ten` — `0` and `10` read; `11` is refused
- `a_property_named_like_a_platform_key_is_refused` — `OSFamily` and `ISA` are refused; `osfamily` reads
- `an_empty_property_name_is_refused`
- `properties_are_sorted_by_name` — `{ z = "1", a = "2" }` lists `a` first
- `the_execution_settings_have_their_defaults` — a builder with only a url gives `execute` false,
  `jobs` 128, `queue_timeout` 1 h, `retries` 5, no properties

**Done when:** `cargo test -p buildl-core remote_settings` passes and `cargo make dod` is green.
Commit `feat(buildl-core): remote execution settings`.

### Task 2 — Reading the keys from `buildl.toml`

**Files:** `manifest/schema.rs`, `manifest/interpret.rs`, `exit.rs` (only if needed).

**Change:** `RemoteTable` gains `execute: Option<bool>`, `jobs: Option<i64>`,
`queue_timeout: Option<String>`, `retries: Option<i64>`, `properties: Option<BTreeMap<Spanned<String>,
String>>` (keys spanned, as `plugins` is, so a refused name is placed at the name itself), each
spanned as the existing fields are; `deny_unknown_fields` (`:181`) stays. `fn
remote` (`interpret.rs:334`) passes each to the builder and turns a refusal into the manifest
error that names the key with its line and column, as `timeout` does (`:369`). A property
value that is not a string is a type error from the TOML reader, refused like any mistyped key.

**Proof:** in `interpret.rs`'s `mod tests` (`:542`), beside `a_full_remote_table_is_read` (`:703`) —
- `the_execution_keys_are_read` — a table with all five keys gives the matching `RemoteSettings`
- `the_execution_keys_default_when_absent` — `no_remote_table_means_no_remote`'s sibling: a table
  with only `url` gives the defaults
- `a_bad_execution_key_is_named_with_its_place` — `jobs = 0`, `retries = 11`,
  `queue_timeout = "0s"`, `properties = { OSFamily = "x" }` each name the key, line and column,
  as `every_refused_remote_value_is_named_with_its_place` (`:812`) does
- `a_non_string_property_is_refused`

**Done when:** `cargo test -p buildl manifest` passes and `cargo make dod` is green. Commit
`feat(buildl): read the remote execution keys of buildl.toml`.

## Risks

- `NameKind` is matched in more than one crate; adding variants can break an exhaustive match
  outside `error.rs`. `cargo make dod` finds every such match; fix each in this plan.
- Nothing reads the values yet, so dead-code lints may fire on accessors; they are `pub` in a
  library crate, which the lints do not flag.

## Proof

`cargo make dod` and `cargo deny check` green; `cargo test -p buildl manifest` and
`cargo test -p buildl-core remote_settings` include every test named above.

## Review findings

- test-coverage — `name_kind_renders_a_human_phrase` lacked the three new kinds — `crates/buildl-core/src/error.rs` — fixed; verified by `cargo test -p buildl-core` → `test result: ok. 945 passed; 0 failed`
- modularity — `OSFamily`/`ISA` written in two places — `types/remote_settings.rs`, `remote/encode.rs` — fixed: `RemotePlatform::OS_FAMILY_PROPERTY`/`ISA_PROPERTY`; verified by `cargo test -p buildl remote::encode` → `test result: ok. 10 passed; 0 failed`
- strong-types — `execute` is a `bool` — `types/remote_settings.rs` — kept: the approved plan prescribes it
- docs — `RemoteSettings` doc described only the cache — fixed
- style — import grouping in `remote_settings.rs` — fixed
- test-coverage — `Display` of the three new types untested — fixed: `the_execution_values_render_as_plain_text`
- naming — test name carried a process word — fixed: `the_execution_settings_have_their_defaults`
- docs — `PlatformPropertyName` had no example — fixed: doctest
- docs — `jobs` field doc said "remote calls"; it caps executions — `manifest/schema.rs` — fixed
- docs — `fn remote` and `execution_keys` docs called `retries` an execution key — `manifest/interpret.rs` — fixed
- test-coverage — `a_non_string_property_is_refused` checked only the line — fixed: checks `3:23` and "expected a string"; verified by `cargo test -p buildl a_non_string_property_is_refused` → `test result: ok. 1 passed; 0 failed`
- spec — spanned property keys not recorded in the plan — fixed: Task 2 amended
- combined gate after all fixes: `cargo make dod` → exit 0, `Build Done in 54.28 seconds`; `cargo deny check` → `advisories ok, bans ok, licenses ok, sources ok`

## Probe results

- structure cited by the plan — `grep -n` over `timeout.rs`, `remote_settings.rs`, `error.rs`, `schema.rs`, `interpret.rs` → `Timeout` :81, `from_secs` :106, `invalid` :42, `RemoteSettings` :411, `mod tests` :536, `NameKind` :1042 (plan said :1043), `RemoteTable` :182, `fn remote` :334, tests :542/:692/:703/:812 — as planned
- `NameKind` matched outside buildl-core needs a new arm — `cargo make dod` → no change needed (`NameKind` is `#[non_exhaustive]`); `exit.rs` untouched

## Deviations

- 2026-10-26 — `properties` keys are `Spanned<String>` so a refused name is placed at the name; plan amended.
- 2026-10-26 — the reserved property names live once, as `RemotePlatform` constants, used by `encode::platform` too (review finding); plan's file list amended.
- 2026-10-26 — test `the_new_settings_default_as_the_spec_says` named `the_execution_settings_have_their_defaults`; plan amended.
