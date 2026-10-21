---
status: approved
created: 2026-10-21
depends-on: [01]
built-on: 6c7f83a41771
---

# Workspace Verify Implementation Plan

**Goal:** `Workspace::verify` re-runs a real workspace's cached actions in an input sandbox that leaves `out/` alone.

**Architecture:** `InputSandbox` gains an `OutPaths` field (`Cleared` today, `Untouched` for
verify) that decides whether `prepare` clears and makes the real output paths (spec §3.4, V8).
`Wiring` gains the verify refusal and verify's ports, built with the untouched sandbox.
`Workspace::verify` follows `build`'s order: refusal → lock → ports → `Pipeline::verify`, with
the `Silent` reporter. End-to-end tests use a new fixture with a deterministic chain, a directory
output and a target that writes its process id.

**Tech Stack:** Rust, `buildl`, `cargo make dod`, `cargo deny check`,
`cargo make guard-adapter-isolation`. Guideline: `claudestacks-guideline-rust:rust-guidelines`.

**Content authority:** spec §2 (Strategy, Re-run, Nothing recorded), §3.4, §4 (first two rows),
§5 (adapter and e2e rows).

---

## Files that change

```
crates/buildl/src/exec/sandbox.rs            — [modify] OutPaths; prepare clears out/ only for Cleared
crates/buildl/src/exec/mod.rs                — [modify] re-export OutPaths
crates/buildl/src/exec/local.rs              — [modify] the verify refusal text, beside Local::refusal
crates/buildl/src/local_ports.rs             — [modify] Wiring::verifies, verify ports
crates/buildl/src/workspace.rs               — [modify] Workspace::verify
crates/buildl/src/lib.rs                     — [modify] crate docs name verify; re-exports
crates/buildl/tests/fixtures/verified/       — [create] buildl.toml, build.lua, a.txt, copy.sh, assets/one.txt, assets/sub/two.txt, stamp.sh
crates/buildl/tests/e2e/verify.rs            — [create] end-to-end tests
crates/buildl/tests/e2e/main.rs              — [modify] `mod verify;`
```

## Order of work

### Task 1 — A sandbox that leaves `out/` alone

**Files:** `exec/sandbox.rs`, `exec/mod.rs`.

**Change:** add `pub enum OutPaths { Cleared, Untouched }`, a field of `InputSandbox`, taken by
`InputSandbox::new` (`sandbox.rs:52`). In `prepare` (`:92-101`) the loop that clears each output
path under the real `out/` and makes its parent runs only for `Cleared`. Everything else is the
same for both. `local_ports.rs:240` passes `OutPaths::Cleared`, and so do the existing tests, so
`build` and `test` are unchanged.

**Proof:** written first, seen failing, then passing —
- `an_untouched_sandbox_leaves_the_real_output_paths_alone` — a file at an output path in `out/`
  survives `prepare`; the counterpart of `prepare_clears_the_real_output_paths` (`:407`), which
  keeps passing with `Cleared`.
- `an_untouched_sandbox_still_copies_inputs_and_dependency_outputs` — the exec dir holds the input
  and the dependency output as for `Cleared`.

**Done when:** `cargo test -p buildl --lib exec::` passes and `cargo make dod` is green. Commit
`feat(buildl): let the input sandbox leave the output directory alone`.

### Task 2 — `Wiring` and `Workspace::verify`

**Files:** `exec/local.rs`, `local_ports.rs`, `workspace.rs`, `lib.rs`.

**Change:**
- `Local::verify_refusal()` in `exec/local.rs` (beside `refusal`, `:54`): `Error::Adapter { port:
  Manifest }` with spec §4's text: `buildl.toml: [execution] strategy is "bare", and verify re-runs
  actions in the input sandbox only: write strategy = "sandbox" under [execution]`.
- `Wiring::verifies(&self) -> Result<()>`: `Sandbox` → `Ok`; `Bare` → `Local::verify_refusal()`;
  `OsSandbox`/`Container` → `Local::refusal` (as `runs`, `local_ports.rs:158`).
- Verify's ports: a new `Wiring::verify_ports(&lock, reporter)` beside `ports` (`:275`). Both call
  one private builder that takes the strategy; `ports` passes what `strategy` (`:236`) makes today
  (`OutPaths::Cleared`), `verify_ports` passes `Local::Sandbox` with `OutPaths::Untouched`.
  No bool parameter.
- `Workspace::verify(&self, scope, overrides, sample) -> Result<Verification>`:
  `wiring.verifies()?` → `wiring.lock()?` → verify ports with `Silent` → `Pipeline::verify`. The
  adapters are dropped before the lock, as `build` does (`workspace.rs:205-216`). No cache fold:
  verify records no row.
- `lib.rs`: the crate docs list `verify` with the other `Workspace` commands; re-export `Sample`,
  `Verification`, `Check` through `buildl_core` as the CLI needs them.

**Proof:** written first, seen failing, then passing —
- `exec/local.rs` tests: `the_verify_refusal_names_bare_and_the_line_to_write`.
- `local_ports.rs` tests: `verify_runs_only_under_sandbox` (sandbox Ok; bare → the verify
  refusal text; os-sandbox and container → `Local::refusal`'s text).
- `workspace.rs` tests: `verify_under_bare_takes_no_lock` (the lock file is not created).

**Done when:** `cargo test -p buildl --lib` passes, `cargo make guard-adapter-isolation` OK, and
`cargo make dod` is green. Commit `feat(buildl): verify a workspace's cached targets`.

### Task 3 — End to end

**Files:** `tests/fixtures/verified/` (new), `tests/e2e/verify.rs`, `tests/e2e/main.rs`.

**Change:** fixture `verified`: `buildl.toml` as `sandboxed`'s (`proc.run = ["cp", "sh"]`,
`strategy` absent so `sandbox`). Every file an action reads is declared, since the sandbox holds
only declared inputs (`sandbox.rs:126-134`), and no two targets produce the same bytes, so
removing one target's output blob touches no other row:
- `plain` — inputs `a.txt`; `cp $in $out`.
- `copied` — deps `plain`, inputs `copy.sh`; `sh $in $deps $out`, where `copy.sh` writes a
  `copied:` line and then the dependency's bytes, so its output differs from `plain`'s.
- `site` — inputs `assets/one.txt`, `assets/sub/two.txt` (files, not the directory: Plan refuses
  an input that is not a regular file, `error.rs:287`); `cp -R assets $out`, a tree output, as
  `tests/fixtures/chain/lib/build.lua:9` does.
- `stamp` — inputs `stamp.sh`; `sh $in $out`; `stamp.sh` writes `$$`, its process id, so two runs
  differ.
A test target is not needed: `narrow`'s handling is core-tested in plan 01.

**Proof:** written first, seen failing, then passing (each starts from `Fixture::copy_of("verified", …)` and a `build`):
- `verify_names_only_the_target_that_does_not_reproduce` — `plain`, `copied`, `site` verified;
  `stamp` mismatched naming output `stamp` with `File(Bytes)`.
- `verify_leaves_the_cache_and_the_log_as_they_were` — `cache.json` and the log are
  byte-identical before and after, and no journal appears (compare the snapshot's cache part and
  `log_lines`, not the whole `Fixture::snapshot`, which includes the cas that verify adds to,
  `tests/e2e/common.rs:177-180`).
- `verify_puts_cached_outputs_back_and_never_its_own` — after removing `out/`, verify restores
  `out/` to what `build` left, and `out/stamp` holds the built bytes, not the re-run's.
- `a_sample_of_one_checks_the_same_target_twice_in_a_row` — `Sample::Lowest(1)`: one checked,
  three not sampled, the same label both runs.
- `a_target_never_built_is_counted_not_cached` — a new target appended to `build.lua` after the build.
- `a_dependent_of_a_target_whose_content_is_gone_is_skipped` — delete only the blob of `plain`'s
  output file (the `StoredFile` digest its manifest names; never the printed blobs, which every
  quiet target shares): `plain` not cached, `copied` skipped naming `//:plain`, `site` and `stamp`
  still checked.
- `verify_is_refused_under_bare_and_os_sandbox` — the two refusal texts; nothing locked.
- `gc_after_verify_removes_what_the_reruns_stored` — the set of cas blobs after `gc` equals the set
  before verify.
- `exec_dirs_are_gone_after_verify` — the scratch directory holds nothing (`Fixture::scratch`).

**Done when:** `cargo test -p buildl --test e2e verify` passes and `cargo make dod` is green. Commit
`test(buildl): prove verify end to end`.

---

## Risks

- **`LocalCache::open` and the cache file.** `open` folds only when a journal exists, and a fold
  of nothing recorded changes nothing (`cache/local_cache.rs:52-103`); after a `build` there is no
  journal, so `cache.json` stays byte-identical.
- **Removing `plain`'s output blob** needs the cas layout (`state/layout.rs:90`); the test reads
  the digest from `plain`'s cached manifest and removes that one file.
- **`$$` in `stamp.sh`** differs between two shells in practice; the core tests, not this one,
  prove the comparison itself.

## Proof

- `cargo make dod`, `cargo deny check` and `cargo make guard-adapter-isolation` green after each task.
- The `sandbox`, `facade` and `identical` e2e suites pass unchanged (`build` is untouched).
