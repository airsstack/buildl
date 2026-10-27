---
status: approved
created: 2026-10-26
depends-on: [01, 09]
built-on: 24837f1a4158
---

# Doctor Execution Implementation Plan

**Goal:** `buildl doctor` prints the remote execution settings, without connecting.

**Architecture:** `doctor_text.rs`'s `remote_lines` (`:226`) prints milestone 6's remote section
from the workspace's `RemoteSettings` and the machine's switches (`RemoteReport`, `:52`, built at
`run.rs:332`). It gains the execution lines, the same way `upload` and `strict` are printed "as
this machine's variables set them" (design §7, `docs/design.md:319`).

**Tech Stack:** Rust, `buildl-cli`, `cargo make dod`.

**Content authority:** spec §9 (`doctor`).

---

## Files that change

```
crates/buildl-cli/src/doctor_text.rs   — [modify] RemoteReport (:52) gains the execute variable's reading; remote_lines (:226) prints execute, jobs, queue_timeout, retries, properties; tests (:298-505)
crates/buildl-cli/src/run.rs           — [modify] RemoteReport built at :332 includes the execute switch
crates/buildl-cli/tests/cli/remote.rs  — [modify] doctor_prints_the_remote_section (:227) expects the new lines
```

## Order of work

### Task 1 — The execution lines

**Files:** all three.

**Change:** after milestone 6's `upload` and `strict` lines, `doctor` prints `execute` as
`buildl.toml` holds it and as `BUILDL_REMOTE_EXECUTE` sets it (`on`, `off`, or invalid with the
value, as `Switch`'s `Display` words it, `remote_switches.rs:59`), with the note that the flags
change it for one run; then `jobs`, `queue_timeout`, `retries`, and one line per property in name
order, or `properties: none`. A workspace with no table still prints `remote: none` (`:228`).

**Proof:** written first, seen failing, then passing (`doctor_text.rs` tests) —
- `the_remote_section_lists_the_execution_settings` — beside `a_remote_section_lists_every_setting_and_the_machine` (`:427`)
- `the_execute_variable_is_shown_as_this_machine_sets_it` — `0`, `1`, unset, `yes`
- `properties_are_listed_in_name_order`
- `no_properties_say_none`
- `a_full_report_matches_the_documented_layout` (`:505`) updated to the new layout
- CLI: `doctor_prints_the_remote_section` (`tests/cli/remote.rs:227`) includes the new lines, and `doctor_says_none_when_the_workspace_names_no_remote` (`:284`) is unchanged

**Done when:** `cargo test -p buildl-cli doctor` and `--test cli remote` pass; `cargo make dod`
green. Commit `feat(buildl-cli): doctor shows remote execution settings`.

## Risks

- The layout test (`:505`) pins the documented layout; plan 12 updates design §7 to match.

## Proof

`cargo make dod` green; `doctor` still makes no network connection (the existing
`doctor_prints_the_remote_section` runs against a closed URL and passes).
