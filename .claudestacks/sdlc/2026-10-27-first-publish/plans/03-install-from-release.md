---
status: executing
created: 2026-10-31
depends-on: [02]
built-on: a9f5951ee52c
---

# Install From the Release Implementation Plan

**Goal:** Every install instruction buildl gives leads with the released prebuilt binary, which
needs no Rust toolchain.

**Architecture:** a short pointer script, `install/install.sh`, served from the book's Pages site,
resolves `BUILDL_VERSION` to the cargo-dist installer of one release and runs it. airsl uses the
same pointer. The README, the book, the agent-skill installer and the examples then name the
release as the published state. All of it lands as PR B, after plan 02 has made the release exist,
so nothing on `main` points at an install that is not there (spec section 7).

**Tech Stack:** POSIX `sh`, GitHub Actions (Pages), mdBook, shellcheck, cargo-make.

**Content authority:** spec sections 5 and 6 (the rows `README.md`,
`book/src/ch01-01-installation.md`, `book/src/ch01-04-with-an-ai-agent.md`, `skills/install.sh`,
the examples' READMEs and `prepare.sh`), and section 7 step 9.

---

## Files that change

```
install/install.sh                     — [create] pointer: BUILDL_VERSION → that release's buildl-cli-installer.sh, run with the given arguments
.github/workflows/pages.yml            — [modify] also deploy on install/**, and copy install/install.sh to the site root
README.md                              — [modify] Install section: prebuilt first, crates.io second, git and checkout last
book/src/ch01-00-getting-started.md    — [modify] line 6 says Installation "builds" the binary; it installs it
book/src/ch01-01-installation.md       — [modify] rewritten in airsl's order: prebuilt, crates.io, GitHub, checkout
book/src/ch01-04-with-an-ai-agent.md   — [modify] what install.sh installs and when Rust is needed
skills/install.sh                      — [modify] buildl and airsl from their prebuilt installers; cargo only for a clone or a non-main --ref
Makefile.toml                          — [modify] the skills task also shellchecks install/install.sh
examples/c/README.md                   — [modify] the tag is published; prepare.sh is how CI runs the checkout's plugin
examples/cpp/README.md                 — [modify] the same
examples/c/prepare.sh                  — [modify] header line 6 no longer says the tag does not exist
examples/cpp/prepare.sh                — [modify] the same
```

## Order of work

### Task 1 — Add the install pointer and serve it from Pages

**Files:**
- Create `install/install.sh` — airsl's `install/install.sh` (78 lines, spec section 2) with spec
  section 5's renames: `BUILDL_VERSION` (default `latest`, a leading `v` stripped, a value outside
  `[0-9A-Za-z.+-]` refused), `buildl-cli-installer.sh`, release tag `buildl-cli-v$version`,
  repository `https://github.com/airsstack/buildl`; it downloads with curl, or wget when curl is
  absent, and runs the installer with the arguments it was given; its header shows the three usage
  lines with `https://airsstack.github.io/buildl/install.sh`
- Modify `.github/workflows/pages.yml` — `install/**` joins `on.push.paths`; a step before the
  upload copies `install/install.sh` into `book/book/`; the header comment says the site also
  serves the installer pointer
- Modify `Makefile.toml` — the `skills` task's `shellcheck skills/install.sh` line also checks
  `install/install.sh`

**Change:** `curl -LsSf https://airsstack.github.io/buildl/install.sh | sh` installs the latest
release once this merges, and `BUILDL_VERSION=0.1.0` pins one.

**Proof:** written first, seen failing (no file), then passing:
- `pointer lints` — `shellcheck install/install.sh` reports nothing
- `pointer refuses a bad version` — `BUILDL_VERSION='1;2' sh install/install.sh` exits 1 and
  says the value is not a version
- `pointer refuses an unreleased version` — `BUILDL_VERSION=9.9.9 sh install/install.sh` exits 1
  and names `9.9.9` and the releases page
- `pointer installs latest` — `BUILDL_CLI_INSTALL_DIR=<tmp> BUILDL_CLI_NO_MODIFY_PATH=1 sh install/install.sh`
  installs a `buildl` under `<tmp>` that prints `buildl 0.1.0`
- `pointer installs a pinned version` — the same with `BUILDL_VERSION=v0.1.0` gives the same result

**Done when:** the five checks pass, `cargo make skills` and `cargo make dod` are green. Commit
`feat(repo): add the install pointer served from Pages`.

### Task 2 — Install the prebuilt binaries from skills/install.sh

**Files:**
- Modify `skills/install.sh`

**Change:** the `buildl` step has three routes. From a clone (`--dir` from a checkout), it runs
`cargo install --locked --path` as now. With `--ref` left at `main`, it runs the buildl pointer,
`https://airsstack.github.io/buildl/install.sh`, fetched with curl and run with `sh`. With any
other `--ref`, it runs `cargo install --locked --git … --branch <ref> buildl-cli` as now. The
`airsl` step runs `https://airsstack.github.io/airsl/install.sh` the same way. The
"install Rust from https://rustup.rs" refusal applies only when a cargo route is taken and `cargo`
is missing. The pointer routes need `curl`, and the script says so and stops when it is missing.
In `--dry-run`, each route prints the commands it would run, as the other steps do. The
`installed` summary names the route: `buildl (prebuilt, latest release)` or the existing cargo
wording. The header comment and the `--ref` help line say that `--ref` names the branch of the
skills and, when it is not `main`, of the `buildl` built from it. `--claude`, `--dir`, `--dry-run`
and exit code 2 are unchanged.

**Proof:** written first, seen failing, then passing. Each runs a copy of the script outside a
clone, with `--dry-run`, and `PATH` set to a temporary folder holding only `sh`, `curl`, `tar` and
the core utilities the script calls, so neither `buildl`, `airsl` nor `cargo` is found:
- `default ref uses the buildl pointer` — `--dir <tmp>` prints a command fetching
  `https://airsstack.github.io/buildl/install.sh`, and no `cargo install` line for buildl
- `airsl uses its pointer` — the same run prints a command fetching
  `https://airsstack.github.io/airsl/install.sh`, and no `cargo install --locked airsl-cli`
- `no Rust needed by default` — the same run exits 0 though `cargo` is absent
- `other ref builds that branch` — with `--ref topic` and `cargo` on `PATH`, the run prints
  `cargo install --locked --git https://github.com/airsstack/buildl --branch topic buildl-cli`
- `other ref needs Rust` — with `--ref topic` and no `cargo`, the run exits 1 with
  `install Rust from https://rustup.rs`
- `clone route unchanged` — the existing `cargo make skills` dry-run check (`Makefile.toml:486-500`)
  still passes; and `sh skills/install.sh --dry-run --dir <tmp>` run from the clone itself, with the
  same restricted `PATH` plus `cargo`, so `buildl` is not found, prints
  `cargo install --locked --path <clone>/crates/buildl-cli`

**Done when:** the six checks pass, `cargo make skills` and `cargo make dod` are green. Commit
`feat(repo): install prebuilt buildl and airsl from skills/install.sh`.

### Task 3 — Rewrite the install instructions

**Files:**
- Modify `README.md` — the Install section (lines 41-58) gives
  `curl -LsSf https://airsstack.github.io/buildl/install.sh | sh` first,
  `cargo install --locked buildl-cli` second, and the GitHub and checkout lines after; the AI-agent
  paragraph below it says `install.sh` installs the prebuilt `buildl` and `airsl`
- Modify `book/src/ch01-00-getting-started.md` — line 6: Installation installs `buildl` and puts it
  on your `PATH`
- Modify `book/src/ch01-01-installation.md` — rewritten in airsl's order (spec section 6):
  "macOS and Linux" with the curl command, the install folder `~/.local/bin`, a new terminal,
  `buildl --version` printing `buildl 0.1.0`, `BUILDL_CLI_NO_MODIFY_PATH=1`, and
  `BUILDL_VERSION=0.1.0`; then "From crates.io" with `cargo install --locked buildl-cli`, Rust 1.94
  or newer and a C compiler; then the existing "From GitHub" and "From a checkout" sections; then
  "Other ways to install", naming the release's archives, a `.sha256` per archive, the attestation,
  and `gh attestation verify --repo airsstack/buildl`. The sentence "not on crates.io yet, and
  there are no prebuilt binaries" goes.
- Modify `book/src/ch01-04-with-an-ai-agent.md` — the "It takes a while" paragraph and the Rust
  sentence (lines 50-62) describe Task 2's routes: prebuilt `buildl` and `airsl` by default, with
  no Rust; Cargo builds only from a clone or for a `--ref` other than `main`. The `--ref` row (line 47) of the
  options table says the same.

**Change:** a reader of the README or the book installs the prebuilt binary first. Every
statement matches what plan 02 published and what Tasks 1 and 2 do.

**Proof:**
- `book builds` — `cargo make book-build` exits 0
- `no stale claim` — `grep -rn "not on crates.io yet" README.md book/src` finds nothing
- `prebuilt leads` — in `README.md` and `ch01-01-installation.md` the first install command is the
  curl line
- `git route is the alternative` — `grep -rn "cargo install --git" README.md book/src` matches only
  README's GitHub line after the curl and crates.io lines, ch01-01's "From GitHub" section and
  ch01-04's `--ref` description

**Done when:** the four checks pass and `cargo make dod` is green. Commit
`docs(repo): install the prebuilt binary first`.

### Task 4 — Say the plugin tag is published

**Files:**
- Modify `examples/c/README.md` — the "Run it" section, lines 57-73: the published tag is the normal route;
  `prepare.sh` is how `check.sh` runs the checkout's own plugin instead
- Modify `examples/cpp/README.md` — the "Run it" section, lines 76-90: the same
- Modify `examples/c/prepare.sh` and `examples/cpp/prepare.sh` — header line 6, "The published tag
  does not exist before the plugin is released", becomes a statement that CI runs the checkout's
  plugin rather than the published tag

**Change:** comments and READMEs only. `prepare.sh`'s behaviour is unchanged.

**Proof:**
- `examples still pass` — `sh examples/check.sh examples/c` and `sh examples/check.sh examples/cpp`
  pass with this checkout's `buildl` as `BUILDL`
- `no stale wording` — `grep -rn "does not exist before\|Before the tag exists" examples` finds
  nothing

**Done when:** both checks pass and `cargo make dod` is green. Commit
`docs(repo): the c plugin tag is published`.

### Task 5 — Open PR B and check the live install

**Change:** push the branch and open PR B against `main`, its body naming the release it points at.
Its checks include the release workflow's pull-request run, which builds and smoke-tests the
archives again. The author merges it. The Pages workflow then deploys the book with the pointer at
the site root.

**Proof:**
- `pr checks green` — `gh pr checks <PR B>` shows every job passing
- `pages serves the pointer` — `curl -sS -o /dev/null -w "%{http_code}" -L https://airsstack.github.io/buildl/install.sh`
  prints `200`
- `live install` — `curl -LsSf https://airsstack.github.io/buildl/install.sh | BUILDL_CLI_INSTALL_DIR=<tmp> BUILDL_CLI_NO_MODIFY_PATH=1 sh`
  installs a `buildl` under `<tmp>` that prints `buildl 0.1.0`

**Done when:** the three checks pass after the author has merged PR B. No commit of its own.

---

## Risks

- **The pointer runs whatever the latest release's installer is.** That is the design (spec section
  5): the short URL never goes stale, and `BUILDL_VERSION` pins a release.
- **`skills/install.sh` changes for every agent user.** Task 2's proofs cover each route, and the
  existing `skills` checks hold the unchanged options.
- **A Pages deploy replaces the live site.** `pages.yml` already never cancels a deploy halfway
  (its `concurrency` block), and the pointer is copied into the same artifact as the book.
- Not done here: a PowerShell pointer, since buildl does not run on Windows, and any change to
  `examples/*/prepare.sh`'s behaviour.

## Proof

- `cargo make dod`, `cargo make skills` and `cargo make book-build` green on PR B's head.
- `curl -LsSf https://airsstack.github.io/buildl/install.sh | sh` installs `buildl 0.1.0`.
- `grep` finds no install instruction that leads with git or claims buildl is unpublished.

## Review findings

One reviewer pass over Tasks 1-4 (2026-10-31); it re-ran `cargo make dod` (exit 0) and named no
finding blocking. One fix round closed C1-C9:

| # | Tier | Finding | Disposition |
|---|---|---|---|
| C1 | 🟡 | nothing guarded the prebuilt routes, the curl refusal or `BUILDL_VERSION` refusal | fixed: the `skills` task dry-runs a copy of `skills/install.sh` outside a clone on a `PATH` without cargo (both pointers printed, no `cargo install`), the same without curl (exit 1), and `BUILDL_VERSION='1;2' sh install/install.sh` (exit 1); all network-free |
| C2 | 🔵 | the dry run printed `curl … \| sh`, which the code deliberately avoids | fixed: it prints `curl -fsSL URL, then sh -c on the whole download` |
| C3 | 🔵 | `install/install.sh` usage named the deprecated `--no-modify-path` | fixed: `BUILDL_CLI_NO_MODIFY_PATH=1` |
| C4 | 🔵 | ch01-01 said the installer checks the `.sha256` file | fixed: it checks a checksum built into the installer |
| C5 | 🔵 | ch01-01's no-PATH-change example was easy to misapply with `curl \| sh` | fixed: `curl … \| BUILDL_CLI_NO_MODIFY_PATH=1 sh` |
| C6 | 🔵 | the `rust-toolchain.toml` sentence sat under "From GitHub" | fixed: moved to "From a checkout" |
| C7 | 🔵 | ch01-04's curl/Rust sentence was not exact | fixed to match the script |
| C8 | 🔵 | README's agent paragraph omitted the clone and non-`main` `--ref` cases | fixed |
| C9 | 🔵 | ragged wrap in `examples/c/README.md` | fixed |
| S1 | 🔵 | spec section 6's ch01-04 row says Rust only "when installing from a clone" | recorded under Deviations |

## Probe results

- 2026-10-31: `https://airsstack.github.io/airsl/install.sh` returns HTTP 200; airsl's
  `install/install.sh` is 78 lines; its `pages.yml:49` copies `install.sh` and `install.ps1` into
  `book/book/` (buildl copies only `install.sh`: no Windows target).
- Task 1: `shellcheck install/install.sh` clean; `BUILDL_VERSION='1;2'` exit 1 (`is not a
  version`); `BUILDL_VERSION=9.9.9` exit 1 naming `9.9.9` and the releases page; latest and
  `BUILDL_VERSION=v0.1.0`, each into a temp dir with `BUILDL_CLI_NO_MODIFY_PATH=1`, install a
  `buildl` printing `buildl 0.1.0`.
- Task 2: copy outside a clone, `PATH` of sh, curl, tar and core utilities: default ref prints both
  pointers and no `cargo install`; the unchanged script exits 1 with `install Rust from
  https://rustup.rs` (seen failing first); `--ref topic` with cargo prints `cargo install --locked
  --git https://github.com/airsstack/buildl --branch topic buildl-cli`, without cargo exits 1; the
  clone run prints `cargo install --locked --path <clone>/crates/buildl-cli`.
- Task 3: `cargo make book-build` exit 0; `grep -rn "not on crates.io yet" README.md book/src`
  finds nothing; `cargo install --git` only at README's GitHub line and ch01-01's "From GitHub";
  the first install command in README and ch01-01 is the curl line.
- Task 4: `sh examples/check.sh examples/c` and `examples/cpp` ok; the old wording grep finds
  nothing.
- Batch: `cargo make skills`, `cargo make dod`, `cargo deny check` each exit 0; after the fix round
  `cargo make skills` and `cargo make book-build` exit 0.

## Deviations

- 2026-10-31 (the spec's, S1): spec section 6's ch01-04 row says Rust is needed "only when
  installing from a clone"; the same table's `skills/install.sh` row and this plan's Task 2 also
  build with cargo for a `--ref` other than `main`. Built as the plan says; the docs name both
  cases. The spec's other row and the plan agree, so no claim was disproven by a run.
- 2026-10-31 (the plan's): `cargo make skills` now also covers Task 2's prebuilt routes and Task 1's
  version refusal (review finding C1); the plan named those checks as manual runs only.
- 2026-10-31 (found by PR B's CI, fixed in this slice on the author's word): run 41166268707, job
  `Examples (macos-latest)`, failed at `examples/cpp/assert.sh` running `./out/hello`: `dyld: Library
  not loaded: @rpath/libgreet.dylib … Referenced from: …/.buildl/cas/d1/…x`. `out/hello` was a hard
  link to its executable twin in the store (link count 2), and the c plugin links with
  `-Wl,-rpath,@loader_path`, so when macOS resolved the program to the twin's name it searched the
  store's folder. Intermittent: the same job passed on `main` and on PR #23, and no local ordering
  (nine tried, plus six `examples/check.sh examples/cpp` runs) reproduced it. The author chose copies
  for every runnable output on every OS: `LocalCas` now copies a `FileMode::Executable` file into
  `out/` from the twin (`fs::copy`, staged in the store's staging folder, renamed into place; staged
  beside the target only across file systems); plain files keep hard links. One review pass (2 🟡
  blocking doc findings, 3 🟡, 5 🔵, 1 ❓ answered "the run bit stays the boundary"), one fix round;
  `cargo make dod` green and `examples/check.sh` ok for `c` and `cpp`. The change is crate source
  after `0.1.0`, so it ships in the next release; `CHANGELOG.md` records it under Unreleased.
