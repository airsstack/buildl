---
status: approved
created: 2026-09-18
---

# Spec: Machine-checking the workspace's architectural rules

Three rules the documents state and the code currently honours have no mechanism that would
notice if it stopped: the crate dependency edges, `buildl-core`'s freedom from I/O, process,
thread, environment and clock APIs, and the declared `rust-version`. This spec settles the
mechanism for each, where each runs, what its failure looks like, and how it is proven to work.
It changes configuration, CI and documentation only — `deny.toml`, `Makefile.toml`,
`.github/workflows/ci.yml` (both the existing `deny` job and a new `msrv` job), one new clippy
configuration file, one new golden file, three sentences in `docs/` and three rows of
`docs/roadmap.md` §5. No crate source is touched.

## 1. Design premises

Everything below was established by running commands against this workspace on 2026-09-18 with
cargo 1.98.1, rustc 1.98.1, clippy 0.1.98, cargo-deny 0.20.2, cargo-make 0.37.24 and rustup
1.29.1. Section 9 records each command and its output.

Three statements in the documents are false today, and all three are about the rules this chain is
enforcing:

| Document | Claim | Reality |
|---|---|---|
| `architecture-building-blocks.md:270` (§7 table) | "Rules 1, 4, and the cross-crate half of 2 — enforced by the compiler" | The compiler stops a crate importing what its `[dependencies]` omits. It does not stop anyone *adding* to `[dependencies]`, nor notice one being removed. With `buildl`'s two dependency lines deleted, `cargo deny check bans` reports `bans ok` and `cargo make dod` reports `Build Done`. |
| `architecture.md:216` (§5 rule 4) | "`buildl-core` cannot name the system clock at all, so the crate boundary enforces the rule rather than grep" | `std::time::SystemTime::now()` compiles inside `buildl-core` today. The crate boundary constrains *other crates'* APIs; `std` is in scope for every crate. |
| `architecture-building-blocks.md:259` | "The `Clock` port is `architecture.md` §5.4's quarantine of the wall clock, enforced by the crate boundary: `buildl-core` has no way to read time except through the port" | The same claim a second time, in the other document. |

Three further facts shape the design:

- **A crate-local `clippy.toml` scopes to its own crate and shadows the workspace root.** A file
  at `crates/buildl-core/clippy.toml` applies to `buildl-core` and to no sibling. When a root
  `clippy.toml` also exists, the crate-local file replaces it outright — the two are not merged.
  No root `clippy.toml` exists today; if one is ever added, `buildl-core` will not see it.
- **Bans must be enumerated item by item.** A module path is rejected (`expected a type, found a
  module`), so there is no way to ban `std::fs` wholesale.
- **A ban entry whose path stops resolving warns but does not fail.** Under `-D warnings`, an
  unresolvable entry produces `warning: … does not refer to a reachable function` and the run
  still exits 0. This is the single most dangerous property of the chosen mechanism and §3.3
  exists to close it.

## 2. Guard 1 — the crate edges

The rule has two halves, and one tool cannot do both. `deny.toml` states which edges are
**forbidden**; a golden file states which edges **exist**.

### 2.1 Forbidden edges — `deny.toml [bans].deny`

cargo-deny's `wrappers` key names the only direct parents a crate may have. Every edge
`architecture-building-blocks.md` §7 forbids is expressible this way. cargo-deny also accepts a
`reason` key per entry, so each ban carries its justification inline, matching the workspace
convention that every dependency in `Cargo.toml` is commented with why it is there:

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

Each `reason` is a TOML **multi-line basic string** (`"""…"""`). The single-line form with a
backslash continuation does not parse — cargo-deny rejects it with
`invalid escape character in string: \n` — and the reasons are too long for one line. cargo-deny
renders the string under the diagnostic, labelled `reason`, beside the banned span.

The first four are §7's internal rules. The last five are the crates
`architecture-building-blocks.md` §1.2 forbids `buildl-core` to depend on that are actually
present in the graph — all of them reach it transitively through airsl, which is why every
wrapper list names `airsl` rather than `buildl`.

`rayon` and `clap` appear in §1.2's forbidden column but are **not in the dependency graph** — no
member depends on them and airsl does not pull them in. A ban on an absent crate is inert, so
listing them buys nothing and §2.2 covers the case they describe.

Wrapper lists name only parents that exist **today**. When I7's adapters in `buildl` take
`walkdir`, `tempfile`, `globset` or `toml` directly, that slice adds `"buildl"` to the relevant
wrapper list — a deliberate, reviewed edit, which is the behaviour this guard exists to produce.
Declaring a future parent in advance is not possible, because §2.3 denies the `unused-wrapper`
lint.

> **Lint naming.** cargo-deny has two distinct wrapper lints and accepts either after `-D`:
> `unused-wrapper` ("a wrapper was declared for a crate not in the graph") and
> `unmatched-wrapper` ("a wrapper was declared for a crate that was not a direct parent"). §2.3
> denies `unused-wrapper`. Confusingly, the *label text* cargo-deny prints under a
> `unused-wrapper` diagnostic reads `unmatched wrapper`, so the plan must key off the lint name,
> not the label.

### 2.2 Required edges — `crates/expected-edges.txt`

No cargo-deny construct asserts that an edge *exists*, and none catches a forbidden dependency on
a crate that is not yet in the graph. Both gaps close with one artifact: a checked-in dump of
every member's direct dependencies, diffed against the live graph.

`{lib}` prints the library target name and no path, so the output is identical on any machine;
`{p}`, the default, embeds an absolute path and cannot be committed. `--edges normal,dev` covers
both regular and dev-dependencies, so a test-only dependency cannot slip past; build-dependencies
are excluded because no member has one. The first line is the member itself and is dropped; for
`buildl-cli`, which has no library target, that line is empty.

```toml
[tasks.guard-crate-edges]
category = "Gate"
description = "Assert every member's direct dependencies against crates/expected-edges.txt"
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

The golden file, captured from the current graph:

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

This is also the only guard that would catch `rayon` or any other catalogued-but-absent crate
being added to `buildl-core`: the new name appears in the dump and the diff fails.

The task is named `guard-crate-edges`, not `guard-core-edges`, because it covers all four
members' direct dependencies rather than `buildl-core`'s alone.

### 2.3 Invocation

The bans run in two places, and **both commands change**, because CI invokes the cargo-deny
binary directly rather than going through cargo-make:

```toml
# Makefile.toml
[tasks.deny]
args = ["deny", "check", "-D", "unused-wrapper"]
```

```yaml
# .github/workflows/ci.yml, the existing `deny` job's final step
- name: cargo deny check
  run: cargo deny check -D unused-wrapper
```

Changing only the cargo-make task would leave the promotion local-only: `ci.yml:94` today reads
`run: cargo deny check`, so a stale or pre-declared wrapper would fail a contributor's
`cargo make deny` while staying a warning in CI. Both must carry the flag for §2.1's argument to
hold.

The golden-file diff runs as `guard-crate-edges`, wired as a dependency of the existing `clippy`
task (§5).

## 3. Guard 2 — `buildl-core` names no I/O, process, thread, environment or clock API

### 3.1 Mechanism

A `crates/buildl-core/clippy.toml` enumerating the forbidden items under `disallowed-methods`,
`disallowed-types` and `disallowed-macros`. Each entry carries an inline `reason`, which clippy
renders as a `= note:` line beneath the offending span. The lints are on by default and need no
entry in the workspace lint table.

The mechanism catches a call reached through a module alias — `use std::fs; fs::metadata(…)` is
flagged by a ban on `std::fs::metadata` — and banning a type covers its inherent methods, so
`std::process::Command` covers `Command::new`.

It is a tripwire, not a proof. A `std` API stabilised after the ban list is written is not
covered until someone adds it. `#![no_std]` would be exhaustive and was considered and declined:
it taxes every line of a crate that is purely host-side, and it contradicts the sibling chain's
stated constraint that `buildl-core` may use `std` minus the named families.

### 3.2 Scope of the ban list

`architecture-building-blocks.md` §1.2 states the rule as "`std`, excluding its I/O, process,
thread, environment, and clock APIs". Most of the list is derived mechanically rather than
transcribed, from the `rust-src` component of the pinned stable toolchain. That component is not
installed by default — `rust-toolchain.toml` sets `profile = "minimal"` with
`components = ["rustfmt", "clippy"]` — so reproducing the derivation needs
`rustup component add rust-src` first.

```bash
cd "$(rustc --print sysroot)/lib/rustlib/src/rust/library/std/src"
grep -oE '^pub (unsafe )?fn [a-z_]+'  fs.rs env.rs process.rs
grep -oE '^pub struct [A-Za-z]+'      fs.rs env.rs process.rs time.rs
grep -hoE '^pub fn [a-z_]+'           thread/functions.rs thread/current.rs thread/scoped.rs
grep -hoE '^pub struct [A-Za-z]+'     thread/builder.rs thread/join_handle.rs \
                                      thread/scoped.rs thread/thread.rs thread/local.rs
grep -oE '^pub fn (stdin|stdout|stderr)' io/stdio.rs
grep -oE '    pub fn (exists|try_exists|is_file|is_dir|is_symlink|metadata|symlink_metadata|canonicalize|read_dir)' path.rs
```

Families and counts on rustc 1.98.1:

| Family | Free functions | Types | Derived by | Notes |
|---|---|---|---|---|
| `std::fs` | 22 | 10 | grep | |
| `std::env` | 15 | 6 | grep | includes `set_var`/`remove_var`, which are `unsafe fn` in edition 2024 and already unreachable under `unsafe_code = "forbid"`; banned anyway |
| `std::process` | 3 | 12 | grep | |
| `std::thread` | 13 | 7 | grep | |
| `std::time` | — | 3 | grep | `Instant`, `SystemTime`, `SystemTimeError`. `Duration` is `core` and stays allowed |
| `std::io` | 3 | 3 | grep | `stdin`/`stdout`/`stderr` and their handle types |
| `std::path` | 9 methods | — | grep | `Path`'s own inherent methods that hit the filesystem: `exists`, `try_exists`, `is_file`, `is_dir`, `is_symlink`, `metadata`, `symlink_metadata`, `canonicalize`, `read_dir`. `Path` and `PathBuf` themselves stay allowed — they are string manipulation and `buildl-core` needs them |
| `std::net` | — | 3 | hand-picked | `TcpListener`, `TcpStream`, `UdpSocket` |
| `std::os::unix::fs` | all | — | hand-picked | platform extension traits and free functions |
| macros | 6 | — | hand-picked | `env!`, `option_env!`, and the printing macros `print!`, `println!`, `eprint!`, `eprintln!` |

Roughly 110 entries.

Two families the greps do not reach are called out because they are reachable today, not future
drift. `Path::exists` and its siblings touch the filesystem through a type the crate legitimately
uses, and the printing macros write to the handles this list bans by name without naming them —
`println!` expands to a private path, so a ban on `std::io::stdout` does not catch it. Both are
included.

Some names the greps return are unstable or deprecated on the pinned toolchain and will not
resolve; §3.3's second assertion makes those fail the gate, so the implementing plan removes them
and records which ones and why. `include_str!` and `include_bytes!` are deliberately **not**
banned: they read files at compile time, which is deterministic and visible in the source.

### 3.3 Invocation — `guard-core-purity`

The existing `cargo make clippy` step would already fail on a real violation, but not on a ban
entry that stopped resolving (§1). A dedicated cargo-make script task makes both fatal and adds
two more assertions:

```toml
[tasks.guard-core-purity]
category = "Gate"
description = "Assert buildl-core names no I/O, process, thread, environment or clock API"
script_runner = "@shell"
script = '''
config="crates/buildl-core/clippy.toml"
if [ ! -f "${config}" ]; then
    echo "guard: ${config} is missing — every purity ban is gone" >&2
    exit 1
fi

out=$(cargo clippy -p buildl-core --all-targets --all-features -- -D warnings 2>&1) || {
    echo "${out}"
    exit 1
}
echo "${out}"

# Any diagnostic whose span points at the config file means a ban stopped resolving.
if echo "${out}" | grep -qE 'clippy\.toml:[0-9]+'; then
    echo "guard: a ban in crates/buildl-core/clippy.toml no longer resolves" >&2
    exit 1
fi

if grep -q 'allow-invalid' "${config}"; then
    echo "guard: allow-invalid disables rot detection for a ban entry" >&2
    exit 1
fi

if grep -rnE --include='*.rs' '#!?\[[^]]*disallowed_' crates/buildl-core; then
    echo "guard: a purity ban was suppressed in source" >&2
    echo "       move the call to an adapter behind a port, or delete the ban" >&2
    echo "       and record the decision in architecture.md §6" >&2
    exit 1
fi
'''
```

Five failure modes, five exits: a missing config, a violation, a rotted ban, a rot-detection
opt-out, a suppressed ban.

The missing-config check is not defensive padding. Without it, deleting `clippy.toml` leaves the
guard printing `grep: crates/buildl-core/clippy.toml: No such file or directory` and **exiting
0** — every ban gone, gate green. That is the same failure this chain exists to remove.

Three details that are load-bearing rather than incidental:

- **The rot check keys on the span path, not the message text.** Clippy prints config diagnostics
  with `--> …/crates/buildl-core/clippy.toml:L:C`, and no legitimate diagnostic points there.
  Matching on message text instead would trip on an unrelated rustc error containing "expected a
  type, found", which is common E0573/E0574 phrasing, and send the reader to the wrong file.
- **The suppression regex is `#!?\[`, not `#\[`.** The inner-attribute form interposes `!`, so
  `#\[` misses `#![allow(clippy::disallowed_methods)]` — a single line at the top of `lib.rs` that
  would silence every ban in the crate and leave the guard green. The `[^]]*` body also catches
  `#[cfg_attr(test, allow(clippy::disallowed_methods))]`.
- **Clippy advertises the loophole the suppression check closes.** Its own diagnostic ends
  `help: to override `-D warnings` add #[allow(clippy::disallowed_types)]`, so a contributor
  following the compiler's advice lands exactly on the thing §3.4 forbids. The guard, not the
  reader, is what stops it.
- **The grep scans `crates/buildl-core`, not `crates/buildl-core/src`.** The clippy run beside it
  is `--all-targets`, so it lints `tests/` and `benches/` too; a narrower grep would leave a
  suppression there linted but never checked. `--include='*.rs'` keeps it to source files.

### 3.4 Escape hatch

**There is none at the call site.** `#[expect(clippy::disallowed_methods, reason = "…")]` does
silence the lint cleanly — which is exactly why the third assertion above rejects it. Nor is
`allow-invalid = true`, which clippy itself suggests for an unresolvable entry, an acceptable
answer: it turns the rot detection off for that ban, so the fourth assertion rejects it too.

The documented path for a future legitimate need is:

1. Move the call into an adapter behind a port. This is the expected outcome in every case the
   architecture anticipates.
2. Failing that, delete the ban entry, amend `architecture-building-blocks.md` §1.2, and add a
   decision record to `architecture.md` §6.

Step 2 is a visible architectural change reviewed as one, rather than a suppression attribute
added at the moment of temptation.

## 4. Guard 3 — the declared MSRV is built

A CI job installs the toolchain named by `rust-version` and checks the workspace on it:

```yaml
msrv:
  name: Minimum supported Rust
  runs-on: ubuntu-latest
  timeout-minutes: 30
  steps:
    - uses: actions/checkout@v5
    - name: Resolve the declared rust-version
      id: msrv
      run: echo "version=$(sed -n 's/^rust-version *= *"\(.*\)"/\1/p' Cargo.toml)" >> "$GITHUB_OUTPUT"
    - name: Install the declared toolchain
      run: rustup toolchain install ${{ steps.msrv.outputs.version }} --profile minimal
    - uses: Swatinem/rust-cache@v2
      with:
        shared-key: msrv
    - name: cargo check on the declared floor
      run: cargo +${{ steps.msrv.outputs.version }} check --workspace --all-targets --all-features
```

Four decisions inside that job:

- **`rust-version` stays the single source of truth.** The `sed` expression yields `1.94` from
  the workspace manifest today. Hard-coding the version in the workflow would create a second
  place to update and a silent disagreement when only one moves.
- **`check`, not the full gate.** MSRV is a compilation question. `--all-targets` extends the
  check to test and example code, where a post-1.94 API is just as likely to appear.
- **CI only, with no `dod` step.** The intent constrains guards to tooling "a contributor cannot
  run locally", and requiring every contributor to install a second toolchain is exactly that
  burden. A contributor who has 1.94 installed can run the one-line command above; the plan
  records it in the workflow comment rather than inventing a `cargo make` task that fails
  confusingly for everyone else.
- **`+1.94` overrides `rust-toolchain.toml`.** Verified: the pin to `stable` does not interfere.

The workspace compiles clean on 1.94 today, so this job starts green.

## 5. Where each guard runs

`Makefile.toml`'s `dod` task deliberately mirrors the rust-guidelines five-command Definition of
Done, and `Makefile.toml:99-103` records why the `deny` task is kept out of it: "The Definition of
Done is the five commands the guideline skill owns; adding a sixth here would make this file
disagree with its source of truth." Both new guards therefore attach as dependencies of the
existing `clippy` task rather than as new `dod` steps:

```toml
[tasks.clippy]
category = "Gate"
dependencies = ["guard-core-purity", "guard-crate-edges"]
command = "cargo"
args = ["clippy", "--workspace", "--all-targets", "--all-features", "--", "-D", "warnings"]
```

`dod` keeps five named steps; `cargo make`, `cargo make dod` and `cargo make clippy` all run both
guards. `guard-crate-edges` is a dependency-graph check rather than a lint, so the placement is
semantically loose; it is chosen because it is the only one that puts the edge check in front of
the person about to commit, which the intent names as a requirement. `cargo tree` on a warm graph
costs well under a second.

| Guard | `cargo make` | `cargo make deny` | CI | Why there |
|---|---|---|---|---|
| `guard-crate-edges` | yes, via `clippy` | — | yes, inside `cargo make dod` | must be available before a commit |
| `guard-core-purity` | yes, via `clippy` | — | yes, inside `cargo make dod` | must be available before a commit |
| `deny.toml` bans | — | yes | yes, the `deny` job, now with `-D unused-wrapper` | answers to a moving advisory database, so `Makefile.toml:99-103` keeps it out of `dod`; still one local command |
| MSRV | — | — | yes, the new `msrv` job | needs a second toolchain, which no contributor should be required to install |

## 6. Failure output

Each guard names the artifact a reader must open:

| Guard | Failure |
|---|---|
| purity, violation | clippy span at `crates/buildl-core/src/…:L:C`, with `= note: <reason>` |
| purity, rotted ban | `guard: a ban in crates/buildl-core/clippy.toml no longer resolves`, preceded by clippy's own span into that file |
| purity, rot opt-out | `guard: allow-invalid disables rot detection for a ban entry` |
| purity, suppressed ban | `guard: a purity ban was suppressed in source`, preceded by the `grep -rn` hit |
| edges | `diff -u` against `crates/expected-edges.txt`, then `guard: crate edges do not match …` |
| bans, forbidden edge | `error[banned]: crate '<name>' is explicitly banned` plus the inclusion tree, plus the entry's `reason` |
| bans, stale wrapper | `error[unused-wrapper]: wrapper for banned crate was not encountered` |
| MSRV | the compiler's own error, e.g. `error[E0658]: use of unstable library feature …` |

## 7. Falsifiable demonstrations

Configuration is not unit-testable, so each guard ships a demonstration that it fails when the
rule is broken. **Every row below was run during design against the assembled cargo-make tasks**,
not against their parts: both task bodies were installed in `Makefile.toml`, the golden file and a
sample `clippy.toml` were created, each break was applied and reverted, and the probe was removed
afterwards. The implementing plan reproduces all of them and records the output in its execution
record.

| Guard | Break | Expected | Observed during design |
|---|---|---|---|
| bans | add `buildl-core = { workspace = true }` to `crates/buildl-cli/Cargo.toml` | `error[banned]` naming `buildl-core`, `bans FAILED` | yes, §9 row 3 |
| bans | add `airsl = { workspace = true }` to `crates/buildl-core/Cargo.toml` | `error[banned]` naming `airsl`, `bans FAILED` | yes, §9 row 4 |
| bans | declare a wrapper that is not a direct parent | `error[unused-wrapper]`, `bans FAILED` | yes, §9 row 6 |
| edges | delete both dependency lines from `crates/buildl/Cargo.toml` | `diff -u` showing the two missing names, then `guard: crate edges do not match …` | yes, §9 row 29 — task exit 105 |
| purity | add `std::time::SystemTime::now()` to a `buildl-core` source file | `error: use of a disallowed type`, gate red | yes, §9 row 30 — task exit 105 |
| purity | add `#![allow(clippy::disallowed_types)]` to that file | clippy passes; `guard: a purity ban was suppressed in source` | yes, §9 row 31 — task exit 105 |
| purity | change a ban path to one that does not resolve | `guard: a ban … no longer resolves`, on a cold **and** a warm cache | yes, §9 row 32 — task exit 105, three consecutive runs |
| purity | add `allow-invalid = true` to that entry | `guard: allow-invalid disables rot detection for a ban entry` | yes, §9 row 33 — task exit 105 |
| purity | delete `crates/buildl-core/clippy.toml` | `guard: … is missing — every purity ban is gone` | yes, §9 row 34 — found the guard exiting **0** before the check existed |
| MSRV | add `n.bit_width()` (stable since 1.97) to a `buildl-core` source file | stable green, `cargo +1.94 check` fails `E0658` | yes, §9 row 17 |

## 8. Documentation amendments

Three sentences describe an enforcement that did not exist and, after this chain, describe the
wrong mechanism. All three concern this chain's own rules, so they are corrected here rather than
deferred to the docs sweep that owns `docs/roadmap.md` §5 rows 6, 7 and 8:

- `architecture-building-blocks.md:270` (§7's "Enforced by" table) — replace "the compiler" with
  the actual enforcers, and say what the compiler does and does not cover.
- `architecture.md:216` (§5 rule 4) — replace "the crate boundary enforces the rule rather than
  grep" with the lint configuration that does enforce it.
- `architecture-building-blocks.md:259` — the same claim in the other document: "enforced by the
  crate boundary: `buildl-core` has no way to read time except through the port".

A fourth line, `architecture-building-blocks.md:339` ("A crate boundary also turns the dependency
rules into compiler errors"), is true of imports and overstated for manifests. It is listed for
the plan to judge, not mandated: it is a parenthetical inside a decision record about crate
naming, not a statement of the enforcement mechanism.

`docs/roadmap.md` §5 rows 1, 2 and 9 are closed by this chain and are deleted from §5; the chain
gains a row in §4 ("Completed intents") when its plans are done, per that section's
`Intent | Chain | Plans | Commits` shape. Every documentation section cross-reference is
load-bearing and survives these edits; every `docs/` citation added to a configuration file is
verified to point at a section that exists, which `docs/roadmap.md` §5 row 6 records as a failure
of the existing comments.

## 9. Evidence

Run on 2026-09-18 against this workspace. Tooling: cargo 1.98.1, rustc 1.98.1, clippy 0.1.98,
cargo-deny 0.20.2, cargo-make 0.37.24, rustup 1.29.1; toolchains `stable` and `1.94` installed,
the latter with the `clippy` component.

| # | Question | Command | Result |
|---|---|---|---|
| 1 | Do the current gates notice a deleted crate edge? | both dependency lines removed from `crates/buildl/Cargo.toml`; `cargo deny check bans`; `cargo make dod` | `bans ok`; `[cargo-make] INFO - Build Done in 14.05 seconds` |
| 2 | Can cargo-deny express every forbidden internal edge? | the §2.1 `deny` list; `cargo deny check bans` | `bans ok` on the correct graph |
| 3 | Does it fail on a forbidden edge? | `buildl-core` added to `buildl-cli`; `cargo deny check bans` | `error[banned]: crate 'buildl-core = 0.1.0' is explicitly banned` + inclusion tree; `bans FAILED` |
| 4 | Same for airsl outside `buildl-lua`? | `airsl` added to `buildl-core`; `cargo deny check bans` | `warning[unmatched-wrapper]` + `error[banned]`, `bans FAILED` |
| 5 | Is a ban on an absent crate inert? | `rayon`, `clap` banned; `cargo deny check bans` | `bans ok`, only `warning[unused-wrapper]` |
| 6 | Is `unused-wrapper` controllable? | `cargo deny check bans -A unused-wrapper`; `-D unused-wrapper` | `bans ok`; `bans FAILED` |
| 7 | Does a crate-local `clippy.toml` scope to its crate? | `crates/buildl-core/clippy.toml` banning `std::fs::metadata`; the identical call added to both `buildl-core` and `buildl`; `cargo clippy --workspace` | flagged in `buildl-core`; not flagged in `buildl` |
| 8 | Does it merge with a root `clippy.toml`? | root file banning `std::fs::symlink_metadata` added; both calls placed in both crates | `buildl-core` saw only its local ban, `buildl` only the root's — shadowed, not merged |
| 9 | Can a module be banned? | `{ path = "std::fs" }` under `disallowed-types` | `warning: expected a type, found a module` |
| 10 | Does an unresolvable ban fail the gate? | `{ path = "std::nonexistent::thing" }`; `cargo clippy -p buildl-core -- -D warnings` | `warning: … does not refer to a reachable function`, **exit 0**, span at `clippy.toml:3:5`. Control: a `&Vec` parameter in the same file → exit 101 |
| 11 | Is the diagnostic useful? | ban with `reason = "probe"` | span at the call site plus `= note: probe` |
| 12 | Are aliased calls caught? | `use std::fs; fs::metadata(…)` | flagged |
| 13 | Does `#[expect]` silence a ban? | `#[expect(clippy::disallowed_methods, reason = "…")]` | silenced, and no unfulfilled-expectation warning |
| 14 | Is `cargo tree` output committable? | `--format '{lib}'` vs the default `{p}` | `{lib}` → `buildl_core`, `airsl`; `{p}` → absolute paths |
| 15 | Where do the banned crates actually hang? | `cargo tree --invert <crate> --depth 1` | `toml`, `walkdir`, `tempfile`, `globset`, `mlua` each return `airsl v0.1.4` as sole direct parent; `rayon` and `clap` → `error: package ID specification 'rayon' did not match any packages`. (`serde` has seven parents and is not in the ban list) |
| 16 | Does the workspace build on the declared floor? | `cargo +1.94 check --workspace --all-targets --all-features` | `Finished dev profile`, 22.19s cold |
| 17 | Would MSRV drift be caught? | `n.bit_width()` (stable 1.97) in `buildl-core` | stable: `Finished`; 1.94: `error[E0658]: use of unstable library feature uint_bit_width` |
| 18 | Can the MSRV be derived from the manifest? | `sed -n 's/^rust-version *= *"\(.*\)"/\1/p' Cargo.toml` | `1.94` |
| 19 | Are the core dependencies `no_std`-capable? | feature tables of the vendored sources | `serde_json` has `alloc = ["serde_core/alloc"]`; `thiserror` and `sha2` are `default = ["std"]` — recorded because it is the evidence behind declining `#![no_std]` |
| 20 | What does the edge dump do under a deleted edge? | edges removed; `cargo tree -p buildl --depth 1 --edges normal,dev` | one line, the crate itself — the two dependency lines gone, which is what §2.2's diff compares against |
| 21 | Does the suppression regex cover every attribute form? | `grep -nE '#!?\[[^]]*disallowed_'` against a file holding the inner form, outer `allow`, outer `expect` and `cfg_attr` | all four matched. The narrower `#\[` form matched only two |
| 22 | Does cargo-deny accept a per-entry `reason`? | `{ crate = "walkdir", wrappers = ["airsl"], reason = "…" }` | `bans ok` |
| 23 | Are `unused-wrapper` and `unmatched-wrapper` both real lints? | `cargo deny check bans -D <name>` for each, and for `bogus-lint` | both accepted; `bogus-lint` → `error: invalid value 'bogus-lint' for '--deny <DENY>'` |
| 24 | How many free functions does `std::env` expose? | `grep -c -oE '^pub (unsafe )?fn [a-z_]+' env.rs` | `15` — the two `unsafe` ones are `set_var` and `remove_var` |
| 25 | Which `Path` methods touch the filesystem? | grep over `path.rs` | `exists`, `try_exists`, `is_file`, `is_dir`, `is_symlink`, `metadata`, `symlink_metadata`, `canonicalize`, `read_dir` |
| 26 | How many documents carry the false enforcement claim? | `grep -n "crate boundary" docs/*.md` | three, plus one parenthetical: `architecture.md:216`, `architecture-building-blocks.md:259`, `:270`'s table, and `:339` |
| 27 | Which TOML string form do the `reason` values need? | a `deny` entry with `reason = "\` + newline, then the same with `reason = """\` | single-line: `failed to parse config … invalid escape character in string: \n`; multi-line: `bans ok` |
| 28 | Does the reason reach the diagnostic? | `walkdir` banned with a wrong wrapper and a multi-line `reason` | `error[banned]: crate 'walkdir = 2.5.0' is explicitly banned`, with the reason rendered beneath the span and labelled `reason` |

| 29 | Does the assembled `guard-crate-edges` task fail on a deleted edge? | both dependency lines removed; `cargo make guard-crate-edges` | `diff -u` naming `buildl_core` and `buildl_lua` as missing under `# buildl`, then `guard: crate edges do not match crates/expected-edges.txt`; task exit 105. Green again on restore, 0.44s |
| 30 | Does the assembled `guard-core-purity` task fail on a violation? | `std::time::SystemTime::now()` in `buildl-core`; `cargo make guard-core-purity` | `error: use of a disallowed type 'std::time::SystemTime'`, with `= note: the wall clock is the Clock port's`; task exit 105 |
| 31 | Does it catch an inner-attribute suppression? | `#![allow(clippy::disallowed_types)]` added above the same call | clippy exits clean; the guard prints the `grep` hit at `probe.rs:2` then `guard: a purity ban was suppressed in source`; task exit 105 |
| 32 | Does rot detection survive a warm cargo cache? | a ban path changed to `std::fs::nonexistent_fn`; `cargo make guard-core-purity` run three times with no source change between runs | all three replayed the cached warning and failed with `guard: a ban … no longer resolves`; task exit 105 each time |
| 33 | Is `allow-invalid` rejected? | `allow-invalid = true` added to that entry | `guard: allow-invalid disables rot detection for a ban entry`; task exit 105 |
| 34 | What happens when `clippy.toml` is absent? | the file deleted; `cargo make guard-core-purity` | **before the fix**: `grep: crates/buildl-core/clippy.toml: No such file or directory`, then `Build Done`, **exit 0** — every ban silently gone. After adding the `[ ! -f ]` check: `guard: … is missing — every purity ban is gone`, task exit 105 |
| 35 | Does clippy itself point at the loophole? | the row 30 diagnostic, read in full | its last line is ``help: to override `-D warnings` add #[allow(clippy::disallowed_types)]`` |

Every probe was reverted; `git status` and `cargo make dod` were clean and green afterwards.

## 10. Non-goals

- The `buildl-core` types, error model, canonical serializer and clock port these guards protect.
  They are the sibling chain, `2026-09-18-core-foundation`. Neither chain blocks the other.
- `#![no_std]` for `buildl-core`. Considered and declined in §3.1; the evidence that it would work
  is kept in §9 row 19 so the decision can be revisited without re-deriving it.
- Enforcing adapter isolation *inside* `buildl` — that adapter modules never import each other.
  `architecture-building-blocks.md` §7 records this as review-enforced until an adapter earns its
  own crate, and no module-level tool applies to a crate with no modules.
- `docs/roadmap.md` §5 rows 6, 7 and 8: inaccurate tooling comments, the §7-versus-§4
  contradiction about whether an adapter may depend on another adapter, and the `run` versus
  `build` command-name disagreement. Prose fixes belonging to a docs sweep. The three amendments
  in §8 are in scope because they describe the mechanisms this chain builds.
- Row 10, whether the runners' rustup is new enough for `rustup toolchain install` with no
  argument. It is answered by observing the first CI run, not by building anything. The new `msrv`
  job passes an explicit argument and does not depend on the answer.
- A guard for `architecture.md` §5 rule 1 (no `HashMap` iteration reaches output). `HashMap` is
  explicitly permitted as a lookup table, so the rule is about iteration sites, which
  `disallowed-types` cannot express.
- A separate dev-dependency *policy*. `architecture-building-blocks.md` §1.2 does not distinguish
  regular from dev-dependencies, so this chain guards them identically: `--edges normal,dev`
  covers them in the golden file, and cargo-deny already includes them in its graph, marked
  `(dev)`. Whether `buildl-core`'s tests should be allowed a dependency its library may not have
  is a question for §1.2, not for this chain.
- Windows or non-unix CI coverage, publishing readiness, and any change to crate source.
