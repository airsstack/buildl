---
status: approved
created: 2026-09-18
---

# Crate-Edge Guards Implementation Plan

**Goal:** Make the workspace's crate dependency edges fail the gate whenever they stop matching
`architecture-building-blocks.md` §7.

**Architecture:** Two artifacts, because no single tool does both halves. `deny.toml`'s
`[bans].deny` list names, per crate, the only direct parents it may have — that is the
*forbidden* half, and cargo-deny is the only tool that expresses it. A checked-in golden file,
`crates/expected-edges.txt`, holds every member's direct dependencies as `cargo tree` prints
them; a cargo-make script task diffs the live graph against it — that is the *required* half, and
it also catches a forbidden dependency on a crate that is not in the graph yet. The golden-file
task attaches as a dependency of the existing `clippy` task so it runs inside `cargo make` and
`cargo make dod` without adding a sixth named step to `dod` (`Makefile.toml:99-103` records why
`dod` stays at five).

**Tech Stack:** cargo-deny 0.20.2, cargo-make 0.37.24, `cargo tree` (cargo 1.98.1), POSIX shell.

---

## Context an implementer needs

This is a four-crate Rust workspace. The dependency rules
(`docs/architecture-building-blocks.md` §7) are:

```text
  buildl-core  ◄── buildl-lua ◄── buildl ◄── buildl-cli
       ▲                            │
       └────────────────────────────┘
```

- `buildl-core` depends on no buildl crate, no airsl, no I/O API.
- `buildl-lua` depends on `buildl-core` and airsl, never on `buildl`.
- `buildl` is the single composition root; nothing below it depends on it.
- `buildl-cli` has exactly one library dependency, `buildl`.

Today nothing enforces any of this. With both dependency lines deleted from
`crates/buildl/Cargo.toml`, `cargo deny check bans` reports `bans ok` and `cargo make dod`
reports `Build Done`. Task 1 reproduces that before fixing it.

All work happens in the git worktree, on a branch, never on `main`. Commits follow Conventional
Commits: `type(scope): summary`, scope `repo` for cross-cutting tooling changes. One commit per
task.

### File map

```
deny.toml                    — [modify] populate the empty `[bans].deny` list (task 1)
Makefile.toml                — [modify] `-D unused-wrapper` on `[tasks.deny]` (task 2);
                               new `[tasks.guard-crate-edges]` (task 3);
                               `dependencies` on `[tasks.clippy]` (task 4)
.github/workflows/ci.yml     — [modify] `-D unused-wrapper` on the deny job (task 2)
crates/expected-edges.txt    — [create] the golden dump of direct dependencies (task 3)
```

---

## Task 1 — Ban the forbidden crate edges in `deny.toml`

**Files:**
- Modify `deny.toml`

**Steps:**

1. Reproduce the gap first. Append a forbidden edge to `crates/buildl-cli/Cargo.toml` — under its
   `[dependencies]` table, add:

   ```toml
   buildl-core = { workspace = true }
   ```

2. Run the supply-chain check and confirm it does **not** notice:

   ```
   $ cargo deny check bans
   bans ok
   ```

   That is the defect. Leave the edge in place for step 5.

3. Revert the manifest for now so the config change is written against a correct graph:

   ```
   $ git checkout crates/buildl-cli/Cargo.toml
   $ git checkout Cargo.lock
   ```

4. In `deny.toml`, replace the line `deny = []` (currently `deny.toml:74`, inside the `[bans]`
   table) with this list. Each `reason` is a TOML **multi-line basic string** (`"""…"""`): the
   single-line form with a trailing backslash does not parse — cargo-deny rejects it with
   `failed to parse config … invalid escape character in string: \n`.

   ```toml
   deny = [
       { crate = "airsl", wrappers = ["buildl-lua"], reason = """\
           design §5: only buildl-lua evaluates build files, so an airsl upgrade's blast radius \
           stays one crate deep""" },
       { crate = "buildl-core", wrappers = ["buildl-lua", "buildl"], reason = """\
           ABB §7 rule 4: the binary reaches core types through buildl's re-exports, so \
           buildl-cli keeps exactly one library dependency""" },
       { crate = "buildl-lua", wrappers = ["buildl"], reason = """\
           ABB §7 rule 2: buildl is the single composition root; buildl-core must never reach \
           an adapter""" },
       { crate = "buildl", wrappers = ["buildl-cli"], reason = """\
           ABB §7 rule 3: nothing below the composition root may depend on it""" },
       { crate = "mlua", wrappers = ["airsl"], reason = """\
           the Lua runtime is airsl's implementation detail; no buildl crate names it directly""" },
       { crate = "toml", wrappers = ["airsl"], reason = """\
           ABB §1.2: buildl.toml parsing belongs to the Manifest adapter in buildl, never to \
           buildl-core""" },
       { crate = "walkdir", wrappers = ["airsl"], reason = """\
           ABB §1.2: buildl-core does no filesystem traversal""" },
       { crate = "tempfile", wrappers = ["airsl"], reason = """\
           ABB §1.2: temp-file-then-rename is a storage-adapter concern""" },
       { crate = "globset", wrappers = ["airsl"], reason = """\
           ABB §1.2: b.sources() globbing is a filesystem read, not core logic""" },
   ]
   ```

   The first four entries are §7's internal rules. The last five are the crates
   `architecture-building-blocks.md` §1.2 forbids `buildl-core` to depend on that are actually in
   the graph; each reaches the workspace only through airsl, which is why every wrapper list
   names `airsl`. `rayon` and `clap` are in §1.2's forbidden column but are **not** in the
   dependency graph (`cargo tree --invert rayon` → `error: package ID specification 'rayon' did
   not match any packages`), so a ban on them would be inert — task 3's golden file is what
   covers them.

5. Confirm the correct graph still passes:

   ```
   $ cargo deny check bans
   bans ok
   ```

6. Falsify it. Re-add the forbidden edge from step 1 to `crates/buildl-cli/Cargo.toml` and run:

   ```
   $ cargo deny check bans
   error[banned]: crate 'buildl-core = 0.1.0' is explicitly banned
      ┌─ /…/deny.toml:78:16
      │
   78 │       { crate = "buildl-core", wrappers = ["buildl-lua", "buildl"], reason = """\
      │                  ━━━━━━━━━━━ banned here
      │ ╭───────────────────────────────────────────────────────────────────────────────┘
   79 │ │         ABB §7 rule 4: the binary reaches core types through buildl's re-exports, so \
   80 │ │         buildl-cli keeps exactly one library dependency""" },
      │ ╰───────────────────────────────────────────────────────┘ reason
      │
      ├ buildl-core v0.1.0
        ├── buildl v0.1.0
        │   └── buildl-cli v0.1.0
        ├── buildl-cli v0.1.0 (*)
        └── buildl-lua v0.1.0
            └── buildl v0.1.0 (*)

   bans FAILED
   ```

   The span points into `deny.toml`, at the offending *entry* — not at `Cargo.lock` — and the
   multi-line `reason` is underlined and labelled `reason` beneath it. The inclusion tree shows
   every path to the banned crate, so the same error is emitted twice here (once per offending
   parent). What must appear is `error[banned]` naming `buildl-core`, the entry's `reason`, and
   the trailing `bans FAILED`.

   Adding a dependency rewrites `Cargo.lock`, so reverting the probe means reverting that too —
   step 8 does both.

7. Falsify the second rule. Revert `crates/buildl-cli/Cargo.toml`, then add to
   `crates/buildl-core/Cargo.toml` under `[dependencies]`:

   ```toml
   airsl = { workspace = true }
   ```

   ```
   $ cargo deny check bans
   warning[unmatched-wrapper]: …
   error[banned]: crate 'airsl = 0.1.4' is explicitly banned
   bans FAILED
   ```

8. Revert the probe and confirm the tree is clean:

   ```
   $ git checkout crates/buildl-core/Cargo.toml
   $ git checkout Cargo.lock
   $ git status --short
    M deny.toml
   ```

   `Cargo.lock` must be reverted explicitly — every probe in steps 1, 6 and 7 rewrites it.

9. Commit `build(repo): ban the forbidden crate edges in deny.toml`.

---

## Task 2 — Make a stale wrapper declaration fail, locally and in CI

**Files:**
- Modify `Makefile.toml`
- Modify `.github/workflows/ci.yml`

**Steps:**

1. Understand what is being promoted. cargo-deny has two distinct wrapper lints and accepts
   either after `-D`:

   | Lint | Means |
   |---|---|
   | `unused-wrapper` | a wrapper was declared for a crate not in the graph |
   | `unmatched-wrapper` | a direct parent of a banned crate was not marked as a wrapper |

   `unused-wrapper` is the one promoted here: it is what makes a wrapper list describe the graph
   as it is, rather than as someone once hoped. Note that the *label text* cargo-deny prints
   under an `unused-wrapper` diagnostic reads `unmatched wrapper` — key off the lint name, never
   the label. Both names are real; a made-up one is rejected outright
   (`error: invalid value 'bogus-lint' for '--deny <DENY>'`), which is the control proving the
   flag is read.

2. Confirm the gap. In `deny.toml`, temporarily add a tenth entry naming a crate that is not in
   the graph:

   ```toml
       { crate = "rayon", wrappers = ["buildl"], reason = """\
           probe""" },
   ```

   ```
   $ cargo deny check bans
   warning[unused-wrapper]: …
   bans ok
   ```

   A warning, and a pass. That is what task 2 closes. Leave the probe entry for step 5.

3. In `Makefile.toml`, change `[tasks.deny]`'s `args` from `["deny", "check"]` to:

   ```toml
   [tasks.deny]
   category = "Supply chain"
   description = "cargo-deny: security advisories, licenses, duplicate versions, sources"
   # Deliberately not a dependency of `dod`. The Definition of Done is the five
   # commands the guideline skill owns; adding a sixth here would make this file
   # disagree with its source of truth. This check also answers to a moving
   # advisory database rather than to the working tree, so it can fail on a commit
   # that changed nothing.
   #
   # `-D unused-wrapper` promotes cargo-deny's own warning to an error: a wrapper
   # declared for a crate that is not in the graph is a ban list describing a
   # dependency graph that no longer exists. The same flag is on the CI step.
   command = "cargo"
   args = ["deny", "check", "-D", "unused-wrapper"]
   ```

4. In `.github/workflows/ci.yml`, the `deny` job's final step currently reads
   (`.github/workflows/ci.yml:93-94`):

   ```yaml
      # Advisories, licences, duplicate versions and source registries. Policy
      # and every suppression live in deny.toml.
      - name: cargo deny check
        run: cargo deny check
   ```

   Change it to:

   ```yaml
      # Advisories, licences, duplicate versions and source registries. Policy
      # and every suppression live in deny.toml. `-D unused-wrapper` must match
      # Makefile.toml's `deny` task — CI invokes the binary directly rather than
      # through cargo-make, so the promotion has to be written in both places.
      - name: cargo deny check
        run: cargo deny check -D unused-wrapper
   ```

   **Both commands change.** Changing only `Makefile.toml` would leave the promotion local-only:
   a stale wrapper would fail a contributor's `cargo make deny` while staying a warning in CI.

5. Falsify. With the `rayon` probe entry from step 2 still in `deny.toml`:

   ```
   $ cargo make deny
   error[unused-wrapper]: wrapper for banned crate was not encountered
   bans FAILED
   ```

   and the task exits non-zero.

6. Remove the probe entry from `deny.toml` and confirm green:

   ```
   $ cargo make deny
   advisories ok
   bans ok
   licenses ok
   sources ok
   ```

7. Confirm the workflow file still parses as YAML:

   ```
   $ python3 -c "import yaml,sys; yaml.safe_load(open('.github/workflows/ci.yml')); print('yaml ok')"
   yaml ok
   ```

8. Commit `build(repo): fail on a cargo-deny wrapper the graph no longer has`.

---

## Task 3 — Assert the required edges against a golden file

**Files:**
- Create `crates/expected-edges.txt`
- Modify `Makefile.toml`

**Steps:**

1. Capture the golden file. Run each member's direct-dependency dump and write the result to
   `crates/expected-edges.txt`:

   ```
   $ cargo tree -p buildl-core --depth 1 --edges normal,dev --prefix none --format '{lib}'
   buildl_core
   $ cargo tree -p buildl-lua --depth 1 --edges normal,dev --prefix none --format '{lib}'
   buildl_lua
   airsl
   buildl_core
   $ cargo tree -p buildl --depth 1 --edges normal,dev --prefix none --format '{lib}'
   buildl
   buildl_core
   buildl_lua
   $ cargo tree -p buildl-cli --depth 1 --edges normal,dev --prefix none --format '{lib}'

   buildl
   ```

   The first line of each dump is the member itself and is dropped. For `buildl-cli`, which has
   no library target, that first line is **empty** — verified with `| cat -n`, which prints
   `1<TAB>` then `2<TAB>buildl`.

   Three flags are load-bearing:

   | Flag | Why |
   |---|---|
   | `--format '{lib}'` | prints the library target name and no path, so the file is identical on every machine. The default `{p}` embeds an absolute path and cannot be committed. |
   | `--edges normal,dev` | covers regular *and* dev-dependencies, so a test-only dependency cannot slip past. Build-dependencies are excluded because no member has one. |
   | `--depth 1` | direct dependencies only; the transitive graph is cargo-deny's business. |

   Write `crates/expected-edges.txt` with exactly this content:

   ```text
   # buildl-core
   # buildl-lua
   airsl
   buildl_core
   # buildl
   buildl_core
   buildl_lua
   # buildl-cli
   buildl
   ```

2. Add the task to `Makefile.toml`, in the `# Supply chain` section beneath `[tasks.deny]`:

   ```toml
   # ---------------------------------------------------------------------------
   # Architecture guards
   # ---------------------------------------------------------------------------

   [tasks.guard-crate-edges]
   category = "Gate"
   description = "Assert every member's direct dependencies against crates/expected-edges.txt"
   # cargo-deny states which edges are forbidden; nothing there states which edges
   # must exist, and nothing there sees a forbidden dependency on a crate that is
   # not in the graph yet. A committed dump of the live graph covers both.
   script_runner = "@shell"
   script = '''
   expected="crates/expected-edges.txt"
   actual="$(mktemp)"
   trap 'rm -f "${actual}"' EXIT

   for member in buildl-core buildl-lua buildl buildl-cli; do
       echo "# ${member}" >> "${actual}"
       cargo tree -p "${member}" --depth 1 --edges normal,dev --prefix none --format '{lib}' \
           | tail -n +2 >> "${actual}"
   done

   if ! diff -u "${expected}" "${actual}"; then
       echo "guard: crate edges do not match ${expected}" >&2
       echo "       fix the manifest, or update the golden file if the change is intended" >&2
       exit 1
   fi
   '''
   ```

3. Confirm it passes on the current graph:

   ```
   $ cargo make guard-crate-edges
   [cargo-make] INFO - Running Task: guard-crate-edges
   [cargo-make] INFO - Build Done in 0.44 seconds.
   ```

4. Falsify it. Delete both dependency lines from `crates/buildl/Cargo.toml`'s `[dependencies]`
   table (`buildl-core` and `buildl-lua`), then:

   ```
   $ cargo make guard-crate-edges
   --- crates/expected-edges.txt
   +++ /var/folders/…/T/tmp.XXXXXX
   @@
    # buildl
   -buildl_core
   -buildl_lua
    # buildl-cli
   guard: crate edges do not match crates/expected-edges.txt
          fix the manifest, or update the golden file if the change is intended
   [cargo-make] ERROR - Error while executing command, exit code: 1
   $ echo $?
   105
   ```

   105 is cargo-make's exit code for a failed task.

5. Restore and confirm green:

   ```
   $ git checkout crates/buildl/Cargo.toml
   $ git checkout Cargo.lock
   $ cargo make guard-crate-edges
   [cargo-make] INFO - Build Done in 0.44 seconds.
   ```

6. Commit `build(repo): assert the required crate edges against a golden file`.

---

## Task 4 — Run the edge guard before every commit

**Files:**
- Modify `Makefile.toml`

**Steps:**

1. `Makefile.toml:99-103` records why the `dod` task stays at exactly five steps: "The Definition
   of Done is the five commands the guideline skill owns; adding a sixth here would make this
   file disagree with its source of truth." So the guard attaches as a **dependency of the
   existing `clippy` task**, not as a new `dod` step. Change `[tasks.clippy]` to:

   ```toml
   [tasks.clippy]
   category = "Gate"
   description = "Lint every target, warnings are errors"
   # The architecture guards hang here rather than on `dod`: `dod` mirrors the
   # rust-guidelines five-command Definition of Done and gains no sixth step (see
   # the `deny` task's comment). Hanging them off `clippy` puts both in front of
   # anyone about to commit, because `cargo make`, `cargo make dod` and
   # `cargo make clippy` all run them.
   dependencies = ["guard-crate-edges"]
   # `--all-targets` covers tests, examples and benches — a lint living only in
   # test code passes silently without it. `-D warnings` also promotes plain rustc
   # warnings, which is why the gate has no separate `cargo build` step.
   command = "cargo"
   args = [
     "clippy",
     "--workspace",
     "--all-targets",
     "--all-features",
     "--",
     "-D",
     "warnings",
   ]
   ```

   Plan `02-core-purity-bans` adds `"guard-core-purity"` to this same list — that is why it
   declares `depends-on: [01]`.

2. Confirm the guard now runs inside the single-step invocation:

   ```
   $ cargo make clippy
   [cargo-make] INFO - Running Task: guard-crate-edges
   [cargo-make] INFO - Running Task: clippy
   [cargo-make] INFO - Build Done in …
   ```

3. Confirm `dod` still names five steps and is green:

   ```
   $ cargo make dod
   [cargo-make] INFO - Running Task: fmt-check
   [cargo-make] INFO - Running Task: guard-crate-edges
   [cargo-make] INFO - Running Task: clippy
   [cargo-make] INFO - Running Task: doc
   [cargo-make] INFO - Running Task: test
   [cargo-make] INFO - Running Task: test-doc
   [cargo-make] INFO - Running Task: dod
   [cargo-make] INFO - Build Done in …
   ```

4. Confirm the supply-chain gate is green too:

   ```
   $ cargo deny check
   advisories ok
   bans ok
   licenses ok
   sources ok
   ```

5. Commit `build(repo): run the crate-edge guard before every commit`.

---

## Review findings

Reviewed once over the whole four-task diff by `claudestacks:reviewer`, which re-ran the
Definition of Done itself (`cargo make dod` → exit 0; `cargo deny check` → all four checks `ok`;
cargo-deny 0.20.2, cargo-make 0.37.24, cargo 1.98.1) and independently falsified both guards
against scratchpad copies of the config rather than the repo files.

Verdict: **spec compliant**, scope clean — exactly the three modified files plus the one new file
the file map names. `Cargo.lock` and all four crate manifests untouched, so every probe in tasks 1
and 3 was reverted correctly.

| # | Location | Finding | Disposition |
|---|---|---|---|
| 1 | `deny.toml` `buildl-lua` entry | reason cites "ABB §7 rule 2" but quotes rule 3's content ("buildl is the single composition root") and rule 1's ("buildl-core must never reach an adapter"); what the entry enforces is rules 1 and 4 | **applied** — re-cited |
| 2 | `deny.toml` `buildl` entry | reason cites "ABB §7 rule 3" but quotes rule 2's content ("nothing below the composition root may depend on it") | **applied** — re-cited |
| 3 | `deny.toml` `airsl` entry | reason cites `design §5`, which is "The Lua declaration API" (`docs/design.md:114`) and says nothing about which crate evaluates build files | **applied** — re-cited to ABB §7 rule 1 / §1.2 |
| 4 | `deny.toml` `toml` entry | reason cites ABB §1.2 for the Manifest-adapter placement; §1.2 carries only the "must not" half, the placement is the ports table at `architecture-building-blocks.md:246` | **applied** — each half cited to the section that carries it |
| 5 | `Makefile.toml` `[tasks.deny]` comment | comment scopes `-D unused-wrapper` to "a crate that is not in the graph"; it also errors when the crate IS in the graph but a declared wrapper is not a direct parent, so the comment understates the guarantee | **deferred** — comment text, no behavioural effect |
| 6 | `Makefile.toml` `[tasks.guard-crate-edges]` | member list is hardcoded to the four current crates; a fifth member (ABB §11 anticipates one) is checked by neither artifact. Deriving the loop from `cargo metadata --no-deps` would close it | **deferred to follow-up** — amends an approved plan's verbatim block |
| 7 | `Makefile.toml` `[tasks.guard-crate-edges]` | `--edges normal,dev` omits build edges, so a `[build-dependencies]` entry in `buildl-core` on a crate absent from the ban list passes both guards. cargo-deny does see build edges, so only the unbanned-crate case leaks | **deferred to follow-up** — same reason |
| 8 | `Makefile.toml` `[tasks.guard-crate-edges]` | `cargo tree … \| tail -n +2` discards cargo tree's exit status — no `set -o pipefail`. For `buildl-core`, whose expected section is empty, a failing `cargo tree` emits `# buildl-core` and nothing, which equals the golden file: the guard passes on an invocation that never ran | **deferred to follow-up** — same reason; the portable fix is not a one-liner under `@shell` |
| 9 | `Makefile.toml` `[tasks.guard-crate-edges]` | the failure message says "update the golden file if the change is intended" without naming the four-flag `cargo tree` command that regenerates it | **deferred to follow-up** — same reason |
| 10 | nothing committed re-proves the guards fire | reverting `deny.toml`'s list to `[]`, dropping `-D unused-wrapper`, or deleting `dependencies = ["guard-crate-edges"]` all leave DoD and CI green. The falsification steps were manual probes | **deferred to follow-up** — a committed regression test for the guards is outside plan 01's file map |
| 11 | `docs/architecture-building-blocks.md` §7 "Enforced by" table | still attributes rules 1, 4 and the cross-crate half of 2 to "the compiler" alone | **out of scope by design** — plan `04-doc-amendments` owns this edit |

Findings 6–10 are guard-hardening amendments to an approved plan's verbatim content, which is the
author's call, not execution's. They are carried to `docs/roadmap.md` §5 rather than applied here.

## Probe results

Each falsification step's real output. Every probe was reverted; `Cargo.lock` was restored
explicitly after each manifest probe.

**Task 1 step 2 — the gap.** With `buildl-core` added to `crates/buildl-cli/Cargo.toml` and
`deny = []`: `bans ok`. cargo-deny does not notice the forbidden edge.

**Task 1 step 5 — green on the correct graph.** `bans ok`.

**Task 1 step 6 — `buildl-cli → buildl-core` rejected** (exit 2):

```
warning[unmatched-wrapper]: direct parent 'buildl-cli = 0.1.0' of banned crate 'buildl-core = 0.1.0' was not marked as a wrapper
error[banned]: crate 'buildl-core = 0.1.0' is explicitly banned
bans FAILED
```

**Task 1 step 7 — `buildl-core → airsl` rejected** (exit 2):

```
warning[unmatched-wrapper]: direct parent 'buildl-core = 0.1.0' of banned crate 'airsl = 0.1.4' was not marked as a wrapper
error[banned]: crate 'airsl = 0.1.4' is explicitly banned
bans FAILED
```

**Task 2 step 2 — the gap.** Probe entry `{ crate = "rayon", wrappers = ["buildl"] }`, plain
`cargo deny check bans`:

```
warning[unused-wrapper]: wrapper for banned crate was not encountered
bans ok
```

A warning, and a pass — exit 0.

**Task 2 step 5 — promoted.** Same probe entry through `cargo make deny`:

```
error[unused-wrapper]: wrapper for banned crate was not encountered
advisories ok, bans FAILED, licenses ok, sources ok
Error while executing command, exit code: 2
```

Shell `$?` = 105 (cargo-make's failed-task code).

**Task 2 step 6 — probe removed.** `advisories ok, bans ok, licenses ok, sources ok`, exit 0.

**Task 2 step 7.** `yaml ok`.

**Task 3 step 3 — green on the current graph.** `[cargo-make] INFO - Build Done in 0.98 seconds.`

**Task 3 step 4 — both `buildl` dependency lines deleted.** The `diff -u` hunk printed, both
`guard:` lines printed, `Error while executing command, exit code: 1`, shell exit 105.

**Task 3 step 5 — restored.** `[cargo-make] INFO - Build Done in 0.73 seconds.`

**Task 4 steps 2–4.** `guard-crate-edges` runs before clippy's `Execute Command:`; `cargo make dod`
green; `cargo deny check` green on all four checks. `[tasks.dod]`'s `dependencies` array still
exactly `["fmt-check", "clippy", "doc", "test", "test-doc"]` (`Makefile.toml:44`).

**Reviewer's independent falsification.** Against scratchpad copies of the config, not the repo
files: mutated golden → diff + both guard lines + exit 105; `rayon` wrapper entry → `bans ok` plain
and `error[unused-wrapper]` / `bans FAILED` under `-D unused-wrapper`; control `-D bogus-lint` →
`error: invalid value 'bogus-lint' for '--deny <DENY>'`, proving the flag is read. All nine wrapper
lists re-derived with `cargo tree --invert` and confirmed to be exactly the real direct parents;
`rayon` and `clap` confirmed absent from the graph. `crates/expected-edges.txt` re-derived from the
four documented `cargo tree` commands and matched byte-for-byte, including `buildl-cli`'s empty
first line being dropped by `tail -n +2`.

## Deviations

Three differences between the plan's illustrative output and what the tools actually print. None
changes behaviour; all three are corrections to this plan's text.

1. **Task 1 step 6 shows two `error[banned]` blocks and no warning.** The real run emits one
   `warning[unmatched-wrapper]` (because `buildl-cli` is a direct parent not named as a wrapper)
   followed by a single `error[banned]` block. The full inclusion tree prints under the
   `warning[unmatched-wrapper]` block; the `error[banned]` block's own tree is elided to
   `├ buildl-core v0.1.0 (*)`. Every element the step requires — `error[banned]` naming
   `buildl-core`, the entry's `reason`, trailing `bans FAILED` — is present.

2. **Task 3 step 4's `diff -u` output is abbreviated in the plan.** The real diff also prints the
   file/timestamp header lines and an `@@ -3,7 +3,5 @@` range marker the plan's example omits.

3. **Task 4 steps 2 and 3 show a `Running Task: <name>` line per task.** cargo-make 0.37.24
   emits that line per task *kind*, not per position in the graph: a `script` task prints
   `Running Task: <name>`, whereas a `command` task prints `Execute Command: …` instead. The
   task named on the command line additionally gets a `Task: <name>` header — running
   `cargo make guard-crate-edges` directly prints **both** `Task: guard-crate-edges` and
   `Running Task: guard-crate-edges`. So `cargo make dod` prints `Task: dod`, one
   `Running Task: guard-crate-edges` (the only script task), and an `Execute Command:` line for
   each of `fmt-check`, `clippy`, `doc`, `test` and `test-doc` — not six `Running Task:` lines.
   The ordering the step asserts — the guard running before clippy's own command — is confirmed
   by `Running Task: guard-crate-edges` appearing strictly before
   `Execute Command: "cargo" "clippy" …`.

**Authorized departure from the plan as written.** Each task's final step is a `git commit`; all
four were skipped deliberately. The work sits unstaged in the working tree and the four commits are
the author's to make.
