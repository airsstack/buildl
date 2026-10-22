---
status: done
created: 2026-10-22
depends-on: [01]
built-on: 5d10ef9b2f2e
---

# Approver Port Implementation Plan

**Goal:** `build`, `test` and `verify` put the planned wanted set to an approver port and run nothing when it says no.

**Architecture:** The port trait lives in `ports/approver.rs` (spec §4). Its answer
`Approval` and `DenialReason` live in `types/approval.rs`, not in the port file: `Error`
carries `DenialReason` (`Error::NotApproved`), and `error` may name `types` but not `ports`
(`crates/expected-module-edges.txt`: `error -> types` only; `ports -> error`). `grants`
(made in plan 01) gains `negotiate`. `Ports` gains `type Approver`, and so every `impl Ports`
and `PortSet` literal changes in the same task; in `buildl` that is `LocalPorts`, given the one
adapter it needs today, `ManifestApprover` (always granted), so the workspace builds as it did.
Plan 03 adds the other modes.

**Tech Stack:** Rust, `buildl-core`, `buildl`, `buildl-cli` (exit mapping), `cargo make dod`,
`cargo deny check`. Guideline: `claudestacks-guideline-rust:rust-guidelines` (strong types,
static dispatch, `mod.rs` export-only, colocated unit tests).

**Content authority:** spec §2, §4 (Port, Modes table rows `manifest`), §5, §8 (core row),
§9 (core unit `negotiate`, core flows rows).

---

## Files that change

```
crates/buildl-core/src/types/approval.rs        — [create] Approval { Granted, Denied { unapproved, reason } }, DenialReason newtype
crates/buildl-core/src/types/approver_mode.rs   — [create] ApproverMode { Manifest (default), Interactive, Deny }
crates/buildl-core/src/types/authority.rs       — [modify] WantedSet: Deserialize, covers, minus, union, is_empty, Display
crates/buildl-core/src/types/workspace.rs       — [modify] Workspace gains `approver: ApproverMode`
crates/buildl-core/src/types/mod.rs             — [modify] declare and re-export the new types
crates/buildl-core/src/error.rs                 — [modify] Error::NotApproved; PortName::Approver
crates/buildl-core/src/ports/approver.rs        — [create] trait Approver; blanket impl for &mut A
crates/buildl-core/src/ports/mod.rs             — [modify] `pub mod approver;`, re-export
crates/buildl-core/src/ports/bundle.rs          — [modify] `type Approver: Approver`; doc example impl
crates/buildl-core/src/ports/port_set.rs        — [modify] `approver` field; Debug impl; test bundle
crates/buildl-core/src/grants/negotiate.rs      — [create] negotiate(wanted, approver)
crates/buildl-core/src/grants/mod.rs            — [modify] `pub use negotiate`
crates/buildl-core/src/pipeline/driver.rs       — [modify] build, test, verify call negotiate after plan
crates/buildl-core/src/lib.rs                   — [modify] re-exports; Status paragraph
crates/expected-module-edges.txt                — [modify] grants -> ports, grants -> error, pipeline -> grants
crates/buildl-core/src/plan/pinned.rs           — [modify] Workspace literal (:55) gains approver
crates/buildl-core/src/plan/authority.rs        — [modify] Workspace literal in tests (:162)
crates/buildl-core/tests/flows/common.rs        — [modify] FakeApprover; FakePorts; PortSet and Workspace literals
crates/buildl-core/tests/flows/build.rs, test.rs, verify.rs, plan.rs — [modify] approver flow tests
crates/buildl/src/approver/mod.rs               — [create] module docs, `mod`/`pub use` only
crates/buildl/src/approver/manifest.rs          — [create] ManifestApprover: always Granted
crates/buildl/src/lib.rs                        — [modify] `pub mod approver;`
crates/buildl/src/local_ports.rs                — [modify] `type Approver = ManifestApprover`; PortSet literal (:343)
crates/buildl/src/manifest/interpret.rs         — [modify] Workspace literal (:63): approver ApproverMode::Manifest for now
crates/buildl/src/report/plan_text.rs           — [modify] Workspace literal (:140), test bundle `impl Ports` (:322)
crates/buildl-cli/src/exit.rs                   — [modify] NotApproved → Refused (4); exit 4's doc text
```

## Order of work

### Task 1 — The approval values, `WantedSet` methods, and the new error

**Files:** `types/approval.rs`, `types/approver_mode.rs`, `types/authority.rs`,
`types/workspace.rs`, `types/mod.rs`, `error.rs`, `plan/pinned.rs`, `plan/authority.rs`,
`tests/flows/common.rs` (Workspace literal `:1054`), `ports/bundle.rs` and `ports/port_set.rs`
(Workspace literals in docs/tests, `bundle.rs:57`, `port_set.rs:124`),
`buildl/src/manifest/interpret.rs` (`:63`), `buildl/src/report/plan_text.rs` (`:140`),
`buildl-cli/src/exit.rs`.

**Change:**
- `ApproverMode` — `Manifest` (`Default`), `Interactive`, `Deny`; `parse` from the three words;
  any other text is `Error::InvalidName { kind: NameKind::Approver, .. }`, as `Isolation::parse`
  does (`types/isolation.rs:60-68`; `NameKind::Isolation` at `error.rs:826`, its words at
  `:853`), so `NameKind` gains `Approver` ("approver mode"); plan 03's manifest reader turns it
  into a located error. `Display` gives the same word back.
- `DenialReason` — a newtype over text, made with `DenialReason::new(impl Into<String>)`,
  `Display`.
- `Approval` — `Granted`; `Denied { unapproved: WantedSet, reason: DenialReason }`.
- `WantedSet` (`types/authority.rs:39`): derive `Deserialize`; `covers(&self, other) -> bool`
  (every member of `other` is in `self`); `minus(&self, other) -> WantedSet` (members of `self`
  not in `other`); `union(&self, other) -> WantedSet` (needed to write "remembered ∪ wanted",
  spec §4 Interactive); `is_empty`; `Display` in the prompt's layout (spec §4): one line per
  non-empty kind, in this order and wording — `run programs:`, `read variables:`,
  `write into:`, `reach the network:` — indented two spaces, each label padded with spaces to
  the longest label among the lines shown plus one space, then the members joined by `, ` in
  the set's order; no line for an empty kind. (Spec §4's example aligns three labels and not
  the fourth; this rule is the reading that gives the same text on every run.)
- `Workspace` (`types/workspace.rs:18`) gains `pub approver: ApproverMode`; every struct literal
  (seven: `plan/pinned.rs:55`, `plan/authority.rs:162`, `ports/bundle.rs:57`,
  `ports/port_set.rs:124`, `tests/flows/common.rs:1054`, `buildl/src/manifest/interpret.rs:63`,
  `buildl/src/report/plan_text.rs:140`) sets `ApproverMode::Manifest`.
- `Error::NotApproved { unapproved: Box<WantedSet>, reason: DenialReason }` (amended
  2026-10-22, execution: boxed, as `Refused` boxes its ceiling — inline, `Error` passes clippy's
  `result_large_err` threshold); Display: the reason, a newline, then the unapproved wants as
  `WantedSet`'s `Display` gives them (spec §5), the reason alone when that set is empty.
  `PortName::Approver` (`error.rs:593`).
- `buildl-cli/src/exit.rs`: `Error::NotApproved { .. }` maps to `Exit::Refused` (beside `:110`);
  `Refused = 4`'s doc (`:32`) becomes "The build wants more than the workspace allows or than
  was approved." `PortName::Approver` falls to the existing `Adapter` else-branch
  (`Infrastructure`, 3), which is spec §5's row.

**Proof:** written first, seen failing, then passing —
- `types/authority.rs`: `a_set_covers_its_subset`, `a_set_does_not_cover_a_larger_set`,
  `minus_leaves_what_the_other_lacks`, `union_holds_both`, `an_empty_set_is_covered_by_any`,
  `a_set_round_trips_through_canonical_json`, `display_lists_each_non_empty_kind_in_order`,
  `display_leaves_out_an_empty_kind`.
- `types/approver_mode.rs`: `the_default_is_manifest`, `each_word_parses_and_displays_back`,
  `another_word_is_refused`.
- `types/approval.rs`: `a_denial_reason_shows_its_text`.
- `error.rs`: `not_approved_shows_its_reason_then_the_wants`.
- `buildl-cli/src/exit.rs`: `not_approved_exits_four` (in the existing exit mapping tests).

**Done when:** `cargo test --workspace` passes and `cargo make dod` is green. Commit
`feat(repo): add approval values and the not-approved error` (scope `repo`: the task edits
`buildl` and `buildl-cli` too).

### Task 2 — The `Approver` port and `negotiate`

**Files:** `ports/approver.rs`, `ports/mod.rs`, `ports/bundle.rs`, `ports/port_set.rs`,
`grants/negotiate.rs`, `grants/mod.rs`, `lib.rs`, `crates/expected-module-edges.txt`,
`tests/flows/common.rs`, `buildl/src/approver/mod.rs`, `buildl/src/approver/manifest.rs`,
`buildl/src/lib.rs`, `buildl/src/local_ports.rs`, `buildl/src/report/plan_text.rs`.

**Change:**
- `ports/approver.rs`: `pub trait Approver { fn approve(&mut self, wanted: &WantedSet) ->
  Result<Approval>; }`, doc as spec §4 (never called for a want outside the ceiling; can only
  narrow; an error is never approval). A blanket `impl<A: Approver + ?Sized> Approver for &mut A`
  as `Reporter` has (`ports/reporter.rs:65`).
- `Ports` (`ports/bundle.rs:251`) gains `type Approver: Approver;` `PortSet` gains
  `pub approver: P::Approver` and its Debug impl names it. Every `impl Ports` (`bundle.rs:234`
  doc, `port_set.rs:293` test, `tests/flows/common.rs:1033`, `buildl/src/local_ports.rs:45`,
  `buildl/src/report/plan_text.rs:322` test) and `PortSet` literal (`common.rs:1075`,
  `local_ports.rs:343`) gains it. The two destructurings (`execute/assembly.rs:121`,
  `verify/assembly.rs:92`) add `..` or the field as needed.
- `grants/negotiate.rs`: `pub fn negotiate<A: Approver>(wanted: &WantedSet, approver: &mut A)
  -> Result<()>`: `Granted` → `Ok(())`; `Denied { unapproved, reason }` →
  `Err(Error::NotApproved { unapproved: Box::new(unapproved), reason })` (amended 2026-10-22,
  see Task 1); an approver error → that error, unchanged.
- `buildl/src/approver/manifest.rs`: `ManifestApprover` — unit struct, `approve` returns
  `Granted`. `buildl/src/lib.rs`: `pub mod approver;` with a module doc naming the
  responsibilities plan 03 completes. `LocalPorts`: `type Approver = ManifestApprover;`
  `assemble` sets `approver: ManifestApprover`.
- `tests/flows/common.rs`: `FakeApprover` — gives one set answer (`Granted`, `Denied` of the
  whole asked set, or an `Error::Adapter { port: PortName::Approver, .. }`; amended 2026-10-22,
  execution: each flow asks once, so one answer covers them) and records each `WantedSet` it was asked
  about; built from the public API only (`lib.rs` rule 5).
- Edges: `grants -> ports`, `grants -> error`.
- `approver` is an adapter module: `guard-adapter-isolation` (`Makefile.toml:209-287`) finds
  every top-level module of `crates/buildl/src` itself, and allows it to name only `state` and
  `search_path`. `ManifestApprover` names only `buildl_core`.

**Proof:** written first, seen failing, then passing —
- `ports/approver.rs`: `a_borrowed_approver_answers_as_the_approver` (the blanket impl).
- `grants/negotiate.rs`: `a_granted_set_goes_on`, `a_denial_is_not_approved_naming_what_and_why`,
  `an_approver_error_is_returned_not_read_as_approval`.
- `buildl/src/approver/manifest.rs`: `the_manifest_approver_grants_everything` (including an
  empty set).

**Done when:** `cargo test --workspace` passes, `cargo make guard-module-edges` and
`cargo make guard-adapter-isolation` are OK, and `cargo make dod` is green. Commit
`feat(repo): add the approver port` (scope `repo`: it adds `buildl`'s `approver` module).

### Task 3 — `build`, `test` and `verify` ask the approver

**Files:** `pipeline/driver.rs`, `lib.rs`, `crates/expected-module-edges.txt`,
`tests/flows/build.rs`, `tests/flows/test.rs`, `tests/flows/verify.rs`, `tests/flows/plan.rs`.

**Change:** in `Pipeline::build` (`driver.rs:164-165`), `Pipeline::test` (`:214-215`) and
`Pipeline::verify` (`:253-254`), call `negotiate(plan.wanted(), &mut self.ports.approver)?`
between `plan(...)` and `execute(...)`/`verify(...)`. `Pipeline::plan` (`:128`) does not. Each
method's `# Errors` doc adds `Error::NotApproved` and the approver's own failure after plan's
errors. `lib.rs` Status paragraph: build, test and verify ask the approver after plan, and a
refusal runs nothing, even when every target is cached (spec §2). Edge `pipeline -> grants`.

**Proof:** written first, seen failing, then passing, through the `Pipeline` against
`tests/flows/common.rs` fakes —
- `build.rs`: `a_build_asks_the_approver_once_with_the_planned_wanted_set`,
  `a_denied_build_runs_nothing_records_nothing_and_logs_nothing` (fake strategy, cache, log
  untouched; `Error::NotApproved`), `a_denied_build_of_cached_targets_is_still_refused`,
  `an_approver_error_stops_the_build` (`Error::Adapter` with `PortName::Approver`, nothing run).
- `test.rs`: `a_denied_test_run_runs_nothing`.
- `verify.rs`: `a_denied_verify_reruns_nothing`.
- `plan.rs`: `plan_never_asks_the_approver` (a fake that fails if asked).

**Done when:** `cargo test -p buildl-core` (lib and `--test flows`) passes and
`cargo make dod` is green. Commit `feat(buildl-core): ask the approver before build, test and
verify run anything`.

---

## Risks

- **Most risky: task 2's `Ports` change** touches every bundle in three crates at once; the
  workspace only compiles when all of them change together. The sites are listed above from a
  grep at `8707b21`; `cargo check --workspace --all-targets` finds any missed.
- `Error` is `#[non_exhaustive]`-style matched in the CLI's exit mapping with a `_` fallback
  (`exit.rs:138`): forgetting the `NotApproved` arm would silently give exit 7. Task 1's
  `not_approved_exits_four` guards it.
- `WantedSet`'s `Display` is the one rendering of the prompt and the refusal; plan 03's prompt
  must use it, not a second copy.
- `build` records the graph (`driver.rs:163`) before plan, so a denied build still writes
  `graph.json`; spec §9 requires only that no action runs, no row and no log line, which holds.
- Not done here: the interactive and deny adapters, the manifest's `[approver]` table (plan 03).

## Proof

- `cargo make dod` and `cargo deny check` green after each task.
- `cargo make guard-module-edges` and `cargo make guard-adapter-isolation` OK.
- Every existing flow, e2e and CLI test passes unchanged: under `manifest` nothing a user sees
  changes.

## Review findings

Commits: `1731364` (task 1), `83a501f` (task 2), `3f4c950` (task 3).

- correctness — `NotApproved`'s Display printed the reason and a trailing newline for an empty
  set — task 1 — fixed and verified: test
  `not_approved_with_nothing_unapproved_shows_only_its_reason`;
  `cargo test -p buildl-core --lib not_approved` → `test result: ok. 2 passed; 0 failed`.
- doc-comment — a stray empty `//!` line in `types/authority.rs` — task 1 — fixed.
- consistency — `WantedSet` lacked `#[serde(deny_unknown_fields)]` — task 1 — fixed.
- test — `every_error_variant_has_its_code` in `buildl-cli/src/exit.rs` left out `PinnedEnv`
  (it already did before this plan) — task 1 — fixed: counts 17 and 37.
- reversion guard — no test pinned `Adapter { port: Approver }` to exit 3 — task 1 — fixed in
  `only_the_manifests_adapter_is_a_definition_error`.
- plan — the `Box<WantedSet>` field was not in the plan — task 1 — amended (Task 1, Task 2).
- spec — spec §4's prompt example pads three of the four labels — task 1 — declined: Task 1
  sets the padding rule the code follows, and amending the spec would stale plans 02–05 for a
  layout example.
- reversion guard — the denial test denied the whole asked set, so a `negotiate` reporting the
  asked set would pass — task 2 — fixed and verified: test
  `a_partial_denial_names_only_what_the_approver_left_unapproved`; with the asked set put in the
  error it failed (`left {cc, ld}`, `right {ld}`); `cargo test -p buildl-core --lib
  grants::negotiate` → `test result: ok. 4 passed; 0 failed`.
- style — a 110-column `use` line in `ports/bundle.rs`'s doc example — task 2 — fixed.
- doc-comment — a hard count of fifteen ports in `ports/port_set.rs` and the core README —
  task 2 — fixed: states the rule.
- doc-comment — `approver/mod.rs` described approvers still to come — task 2 — fixed.
- plan — `FakeApprover` "answers from a script"; it gives one set answer — task 2 — amended.
- doc-comment — a 139-column line in `lib.rs` — task 3 — fixed.
- style — `planned_wants` spelled `buildl_core::WantedSet` in full — task 3 — fixed.
- test — "asked once" was checked for a granted `build` only — task 3 — fixed and verified:
  `a_test_run_asks_the_approver_once`, `a_verify_asks_the_approver_once`; with `negotiate`
  called twice they failed (2 vs 1, 4 vs 3); `cargo test -p buildl-core --test flows
  asks_the_approver` → `test result: ok. 4 passed; 0 failed`.

## Probe results

- Claim: seven `Workspace` struct literals, five `impl Ports`, two `PortSet` literals and two
  destructurings exist at the plan's sites — `grep -rn "Workspace {$"` / `"Ports for\|PortSet
  {$"` over `crates` — held (line numbers shifted by plan 01); both destructurings already
  used `..`.

## Deviations

- 2026-10-22 — `Error::NotApproved.unapproved` is `Box<WantedSet>`: inline, `Error` passed
  clippy's `result_large_err` threshold. `Refused` already boxes its ceiling.
- 2026-10-22 — `FakeApprover` gives one set answer rather than a script; every flow asks once.
