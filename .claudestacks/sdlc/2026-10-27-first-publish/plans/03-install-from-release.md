---
status: approved
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
