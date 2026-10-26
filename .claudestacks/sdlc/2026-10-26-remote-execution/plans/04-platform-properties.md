---
status: approved
created: 2026-10-26
depends-on: [01]
built-on: 4704ec60505e
---

# Platform Properties Implementation Plan

**Goal:** Every `Action` buildl describes to the server carries the workspace's extra platform properties.

**Architecture:** `RemoteAction` (`buildl-core/src/types/remote_action.rs:71`) gains the
workspace's `PlatformProperties` (plan 01) beside `platform`. `remote_action()`
(`execute/remote_action.rs:35`) takes them from the `RemoteRun` the worker holds, which takes
them from `workspace.remote` where `RemoteRun` is built (`execute/assembly.rs:190-195`); a
workspace without a `[remote]` table has none. The adapter's `encode::platform`
(`buildl/src/remote/encode.rs:136-148`) writes them after `ISA` and `OSFamily`, the whole list
sorted by name then value. Because the lookup, the upload and (plan 08) the execution all
describe one `RemoteAction`, a result is found under the digest it was stored under (spec §3).

**Tech Stack:** Rust, `buildl-core`, `buildl`, prost, `cargo make dod`.

**Content authority:** spec §3 (`properties`: every `Action`, sorted, `OSFamily`/`ISA` reserved).

---

## Files that change

```
crates/buildl-core/src/types/remote_action.rs    — [modify] RemoteAction gains properties: PlatformProperties
crates/buildl-core/src/execute/remote_action.rs  — [modify] remote_action() takes the properties; tests (:130)
crates/buildl-core/src/execute/perform.rs        — [modify] RemoteRun (:68) gains properties; the two remote_action calls (:470, :544) pass them
crates/buildl-core/src/execute/assembly.rs       — [modify] RemoteRun literal (:190) takes workspace.remote's properties, or none
crates/buildl-core/src/execute/upload.rs         — [modify] test fixture literal (:299)
crates/buildl-core/src/ports/no_remote.rs        — [modify] test fixture literal (:67)
crates/buildl/src/remote/encode.rs               — [modify] test fixture literal (:446); platform() (:136) adds the properties, sorted; tests
crates/buildl/src/remote/reapi.rs                — [modify] test fixture literal (:515); tests (:460): a stored result is found only with the same properties
```

Every literal of `RemoteAction` is a hit of `grep -rn "RemoteAction {" crates/`:
`types/remote_action.rs:105`, `execute/remote_action.rs:80` and `:254`, `execute/upload.rs:299`,
`ports/no_remote.rs:67`, `buildl/src/remote/reapi.rs:515`, `buildl/src/remote/encode.rs:446`.
Task 1 updates all seven, so the workspace compiles at its commit; Task 2 changes what
`encode.rs` and `reapi.rs` do.

## Order of work

### Task 1 — The properties in the description

**Files:** `types/remote_action.rs`, `execute/remote_action.rs`, `execute/perform.rs`,
`execute/assembly.rs`, and the four fixture literals in `execute/upload.rs`, `ports/no_remote.rs`,
`remote/encode.rs`, `remote/reapi.rs`.

**Change:** `RemoteAction.properties` holds the workspace's `PlatformProperties`; `RemoteRun`
carries a reference to them; `remote_action(task, execution, properties, cas)` copies them in.
A workspace without `[remote]` passes an empty set, so its keys and descriptions are unchanged.

**Proof:** written first, seen failing, then passing —
- `the_workspace_properties_are_carried` (`execute/remote_action.rs` tests) — a run with
  `{ pool = "large" }` gives a `RemoteAction` whose `properties` hold exactly that
- `no_properties_leave_the_description_as_before` — an empty set gives `properties` empty and every other field equal to today's
- the existing `remote_on_or_off_gives_identical_keys_and_rows` (`tests/flows/remote.rs:477`) still passes: properties never touch the action key

**Done when:** `cargo test -p buildl-core` passes; `cargo make dod` green. Commit
`feat(buildl-core): carry platform properties in the remote action`.

### Task 2 — The properties on the wire

**Files:** `encode.rs`, `reapi.rs`.

**Change:** `platform()` returns `ISA`, `OSFamily` and every extra property as
`Platform.Property` entries, sorted by name and then by value (`REAPI:965-969`), so the
`Action` digest is stable for one set of properties.

**Proof:** in `encode.rs` tests and `reapi.rs` tests (`:460`) —
- `the_platform_lists_every_property_sorted_by_name` — `{ pool = "large", arch-hint = "x" }`
  encodes as `ISA`, `OSFamily`, `arch-hint`, `pool` in code-point order (upper case sorts first)
- `properties_change_the_action_digest` — the same action with and without `{ pool = "large" }` has two digests
- `a_result_uploaded_with_properties_is_found_only_with_them` — an upload with `{ pool = "large" }` is found by a lookup with them and missed by one without

**Done when:** `cargo test -p buildl remote` passes; `cargo make dod` green. Commit
`feat(buildl): send the workspace's platform properties`.

## Risks

- Sorting must be by UTF-8 bytes (`REAPI:965-969`), not by a locale; `str` ordering in Rust is
  byte ordering, and the test with an upper-case name checks it.
- Changing `remote_action()`'s signature touches every caller; the compiler lists them.

## Proof

`cargo make dod`; the flow test `remote_on_or_off_gives_identical_keys_and_rows` unchanged and green.
