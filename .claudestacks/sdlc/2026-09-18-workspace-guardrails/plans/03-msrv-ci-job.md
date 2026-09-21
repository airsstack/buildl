---
status: approved
created: 2026-09-18
---

# MSRV CI Job Implementation Plan

**Goal:** Build the workspace on the Rust version its manifest declares, so `rust-version` stops
being an unchecked assertion.

**Architecture:** One new GitHub Actions job, `msrv`, in the existing workflow. It reads the
version out of the workspace `Cargo.toml` rather than hard-coding it, installs that toolchain, and
runs `cargo check` — not the full gate — across the workspace with `--all-targets`. It is CI-only:
no `cargo make` task and no `dod` step, because running it locally means installing a second
toolchain, and the chain's intent constrains new guards to tooling a contributor should not be
required to run.

**Tech Stack:** GitHub Actions, rustup, cargo 1.98.1 (host), the `1.94` toolchain (target).

---

## Context an implementer needs

The workspace root `Cargo.toml` declares `rust-version = "1.94"` under `[workspace.package]`.
`rust-toolchain.toml` pins the *development* toolchain to `stable` with `profile = "minimal"` and
`components = ["rustfmt", "clippy"]`. CI (`.github/workflows/ci.yml`) has two jobs today: `dod`
(a two-OS matrix running `cargo make dod`) and `deny` (supply chain). Neither ever compiles on
1.94, so the declared floor has never been tested. `docs/roadmap.md` §5 row 9 records this.

All work happens in the git worktree, on a branch, never on `main`. Commits follow Conventional
Commits: `type(scope): summary`, scope `repo`. One commit per task; task 1 is evidence only and
commits nothing.

### File map

```
.github/workflows/ci.yml  — [modify] add the `msrv` job after the existing `deny` job (task 2)
```

---

## Task 1 — Prove the floor holds, and that a break would be caught

This task changes no file. It is the red/evidence step: it establishes that the job about to be
written starts green, and that it would actually fail on drift.

**Steps:**

1. Confirm the toolchain is available, installing it if not:

   ```
   $ rustup toolchain list
   stable-aarch64-apple-darwin (active, default)
   1.94-aarch64-apple-darwin
   …
   ```

   If `1.94` is absent: `rustup toolchain install 1.94 --profile minimal`.

2. Confirm the version the job will resolve comes out of the manifest:

   ```
   $ sed -n 's/^rust-version *= *"\(.*\)"/\1/p' Cargo.toml
   1.94
   ```

   Exactly one line of output. The expression anchors at `^`, so it matches the
   `[workspace.package]` key and nothing indented beneath another table.

3. Confirm the workspace compiles on the floor:

   ```
   $ cargo +1.94 check --workspace --all-targets --all-features
       Checking buildl-core v0.1.0 (…)
       Checking buildl-lua v0.1.0 (…)
       Checking buildl v0.1.0 (…)
       Checking buildl-cli v0.1.0 (…)
       Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.75s
   ```

   `+1.94` overrides `rust-toolchain.toml`; the pin to `stable` does not interfere.

4. Falsify. Add a call to an API stabilised after 1.94 into a `buildl-core` source file — for
   example `u32::bit_width`, stable since 1.97. Create `crates/buildl-core/src/probe.rs`:

   ```rust
   //! Probe.
   /// Probe.
   #[must_use]
   pub fn probe(n: u32) -> u32 { n.bit_width() }
   ```

   and append `pub mod probe;` to `crates/buildl-core/src/lib.rs`. Then run both toolchains:

   ```
   $ cargo check -p buildl-core
       Finished `dev` profile …
   $ cargo +1.94 check --workspace --all-targets --all-features
   error[E0658]: use of unstable library feature `uint_bit_width`
   ```

   Stable is green and the floor is red — which is the entire value of the job.

5. Revert the probe and confirm both are green again:

   ```
   $ rm crates/buildl-core/src/probe.rs
   $ git checkout crates/buildl-core/src/lib.rs
   $ cargo +1.94 check --workspace --all-targets --all-features
       Finished `dev` profile …
   $ git status --short
   ```

   `git status --short` must print nothing.

6. Nothing to commit. Record step 4's exact output in this plan's `## Probe results` section.

---

## Task 2 — Add the `msrv` job

**Files:**
- Modify `.github/workflows/ci.yml`

**Steps:**

1. Append a third job after the existing `deny` job (the file currently ends at
   `.github/workflows/ci.yml:94`). Keep the two-space job indentation the file already uses:

   ```yaml
     msrv:
       name: Minimum supported Rust
       runs-on: ubuntu-latest
       timeout-minutes: 30
       # `rust-version` in the workspace manifest is a promise to downstream
       # crates, and nothing else in CI compiles on it — the `dod` matrix runs on
       # stable. This job is the only thing that makes the declared floor true.
       #
       # Deliberately CI-only, with no cargo-make task: running it locally means
       # installing a second toolchain, which no contributor should be required to
       # do. Anyone who does have it can run the last step's command directly.
       steps:
         - uses: actions/checkout@v5

         # The manifest stays the single source of truth for the floor. Hard-coding
         # the version here would create a second place to update and a silent
         # disagreement the day only one of them moves.
         - name: Resolve the declared rust-version
           id: msrv
           run: echo "version=$(sed -n 's/^rust-version *= *"\(.*\)"/\1/p' Cargo.toml)" >> "$GITHUB_OUTPUT"

         - name: Install the declared toolchain
           run: rustup toolchain install ${{ steps.msrv.outputs.version }} --profile minimal

         - uses: Swatinem/rust-cache@v2
           with:
             shared-key: msrv

         # `check`, not the full gate: MSRV is a compilation question. `--all-targets`
         # extends it to test and example code, where a post-floor API is just as
         # likely to appear. `+<version>` overrides rust-toolchain.toml's `stable` pin.
         - name: cargo check on the declared floor
           run: cargo +${{ steps.msrv.outputs.version }} check --workspace --all-targets --all-features
   ```

2. Confirm the file still parses as YAML and that the job list is what you expect:

   ```
   $ python3 -c "import yaml; print(sorted(yaml.safe_load(open('.github/workflows/ci.yml'))['jobs']))"
   ['deny', 'dod', 'msrv']
   ```

3. Confirm the step the job will run is the one task 1 proved green:

   ```
   $ cargo +1.94 check --workspace --all-targets --all-features
       Finished `dev` profile …
   ```

4. Confirm the local gates are untouched — this plan changes no Rust and no cargo-make task:

   ```
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   $ cargo deny check
   advisories ok
   bans ok
   licenses ok
   sources ok
   ```

5. Commit `ci(repo): build the workspace on the declared minimum Rust version`.

---

## Notes for whoever reads the first CI run

`docs/roadmap.md` §5 row 10 asks whether the runners' rustup is new enough for
`rustup toolchain install` with no argument, which the two existing jobs rely on. This job passes
an explicit version argument and does not depend on the answer, so it neither closes nor blocks
that row.

---

## Review findings

One reviewer pass over the combined plan 03 + plan 04 diff. Findings landing in this plan:

| # | Tier | Finding | Disposition |
|---|---|---|---|
| 1 | risk | `.github/workflows/ci.yml:98` — nothing in the gate catches removal of the `msrv` job itself | declined — out of scope, and the same class as the guard-removal gap plans 01 and 02 both left open. Recorded rather than fixed, so the chain's record stays consistent about it. |
| 2 | cleanup | `:117` — the resolved version is never asserted non-empty; if `rust-version` moves or is reformatted, `sed` yields an empty string | declined — it fails closed, not open. `cargo +` with no version errors out, so the job turns red; only the message is unhelpful. |
| 3 | cleanup | `:122` — `Swatinem/rust-cache` keys on the runner's default toolchain (stable), not the floor this job installs, and the step is the only unnamed one in the file | declined — `shared-key: msrv` isolates the entry, so the miskeying costs cache efficiency, not correctness. The YAML is spec §4's verbatim. |
| 4 | cleanup | This plan's line 28 cites `docs/roadmap.md` §5 row 9, which plan 04 deletes | declined — this plan is a historical record of what was true when it was written. |

The reviewer re-ran the Definition of Done itself rather than reading the implementers' receipts.
`cargo make dod`, `cargo fmt --check`, `cargo clippy -D warnings`, `cargo deny check -D unused-wrapper`
and `cargo +1.94 check --workspace --all-targets --all-features` all exited 0, so the new job starts
green.

## Probe results

Task 1 step 4, falsification. `u32::bit_width` behaved exactly as the plan predicted — no
substitution was needed. Stable here is `rustc 1.98.1 (48a229cea 2026-09-01)`.

Stable accepts it:

```
$ cargo check -p buildl-core
    Checking buildl-core v0.1.0 (…/crates/buildl-core)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.63s
```

The declared floor rejects it:

```
$ cargo +1.94 check --workspace --all-targets --all-features
    Checking buildl-core v0.1.0 (…/crates/buildl-core)
error[E0658]: use of unstable library feature `uint_bit_width`
 --> crates/buildl-core/src/probe.rs:4:33
  |
4 | pub fn probe(n: u32) -> u32 { n.bit_width() }
  |                                 ^^^^^^^^^
  |
  = note: see issue #142326 <https://github.com/rust-lang/rust/issues/142326> for more information

For more information about this error, try `rustc --explain E0658`.
error: could not compile `buildl-core` (lib test) due to 1 previous error
warning: build failed, waiting for other jobs to finish...
error: could not compile `buildl-core` (lib) due to 1 previous error
```

Exit code 101. Stable green, floor red — which is the entire value of the job. The probe was then
removed and both toolchains confirmed green again.

One probe the plan does not call for was added, because the job's correctness rests on it and
nothing else in the plan would catch a failure. The `Resolve the declared rust-version` step embeds
a `sed` expression with backslash groups inside `$(…)` inside YAML — the exact shape a YAML
round-trip mangles silently. The step's `run:` string was read back through PyYAML and then
actually executed, with `GITHUB_OUTPUT` pointed at a scratch file:

```
post-parse run string:
'echo "version=$(sed -n \'s/^rust-version *= *"\\(.*\\)"/\\1/p\' Cargo.toml)" >> "$GITHUB_OUTPUT"'

exit 0, stderr ''
GITHUB_OUTPUT contents: 'version=1.94\n'
```

So the step does not merely parse — it resolves the floor to `1.94` for real.

## Deviations

1. **The plan says `.github/workflows/ci.yml` "currently ends at :94". It ends at 96.** The `msrv`
   job was appended after the real last line (`run: cargo deny check -D unused-wrapper`). No other
   consequence.

2. **Task 1 step 5's tree-clean assertion was run scoped to `crates/buildl-core`.** Three other
   tasks were editing `docs/` concurrently in the same run, so the unscoped form the plan specifies
   would have reported their work and could never have passed. The scoped form printed nothing, and
   `crates/buildl-core/src/` was confirmed to hold only `lib.rs`.

3. **Task 2 was executed directly rather than by its assigned implementer.** That implementer
   returned having performed no tool calls at all, and reported so rather than claiming a green
   result. `.github/workflows/ci.yml` was verified untouched before the work was redone.

4. **An extra verification was added to task 2** — the post-YAML-parse execution of the `sed` step,
   recorded under Probe results above. The plan asks only that the file parse as YAML, which would
   not have caught a mangled expression.
