---
status: approved
created: 2026-11-11
built-on: c580a03c19bd
---

# Spec: buildl builds buildl, on a pinned release

buildl's repository gains a root `buildl.toml`, `build.lua`, `.buildlignore` and `.buildl-version`,
so that buildl 0.3.0, installed as a released binary, builds the `buildl` and `cargo-buildl`
binaries and runs the workspace's tests and doctests in its sandbox. A new CI job does the same on
Linux and macOS on every push and pull request, keeping `.buildl/` in GitHub's Actions cache. The
book and the README say how. `cargo make` and its jobs are untouched.

Probes below ran on 2026-11-11 on macOS (arm64) against a scratch copy of the repository at
`origin/main` 325aa4e (the 0.3.0 release merge), with the released `buildl 0.3.0` binary
(`buildl-cli-aarch64-apple-darwin.tar.xz`, `shasum -a 256 -c` → `OK`).

## 1. Decisions

Settled with the author, one at a time, on 2026-11-11.

| # | Question | Decision |
|---|---|---|
| D1 | Form of `.buildlignore` | Leave everything out, then bring back what cargo reads (§2) |
| D2 | Test targets | One per gate step: `tests` and `doctests` (§3) |
| D3 | Toolchain in the key | `version_probe = true` on all four calls (§3) |
| D4 | `build.lua` | Written by hand; `cargo buildl init`/`sync` not used (§3) |
| D5 | Pin | A `.buildl-version` file at the root (§4) |
| D6 | CI operating systems | `ubuntu-latest` and `macos-latest` (§5) |
| D7 | CI cache | `.buildl/` in GitHub's Actions cache (§5) |
| D8 | Cache miss | Fall back to the newest cache for the OS and pin, then `buildl gc` before saving (§5) |
| D9 | Docs | A section in `book/src/ch11-07-testing-layers.md` and one in `README.md` (§6) |
| D10 | The wrong `plugin-rust-v0.2.0` tag | Fixed first in its own chain; this chain pins `plugin-rust-v0.2.1` (§3) |
| D11 | Linux, not yet proven | Proven by the CI job's first run; if it fails there, execution stops and the failure goes to the author (§5) |
| D12 | Stray files under `crates/` | Three more `.buildlignore` lines (§2) |

## 2. `.buildlignore`

The root `.buildlignore` holds exactly:

```text
/*
!/Cargo.toml
!/Cargo.lock
!/rust-toolchain.toml
!/crates/
.DS_*
*.rs.bk
mutants.out*/
```

- **Why it is needed.** The `rust` plugin declares every target's inputs as
  `buildl.sources("**/*")` minus a `target/` and `.git/` (`plugins/rust/graph.lua:124`,
  `plugins/rust/README.md:84`). Without a `.buildlignore`, `buildl plan --json` on the scratch
  copy listed **1232** inputs for `//:buildl`: `crates` 526, `.claudestacks` 282, `book` 147,
  `examples` 145, `skills` 79, `plugins` 24, `.github` 7, `tools` 3, `ci` 2, the rest single
  files at the root.
- **What it keeps.** With the first five lines, the same command listed **529** inputs per target:
  `Cargo.lock` 1, `Cargo.toml` 1, `crates` 526, `rust-toolchain.toml` 1. The root `build.lua`,
  `buildl.toml`, `buildl.lock`, `.buildlignore` and `.buildl-version` are not inputs; buildl still
  reads the first two to load (the 0.3.0 behaviour, CHANGELOG `0.3.0`, "The root `build.lua` and
  `buildl.toml` are always read").
- **Editing an ignored path rebuilds nothing.** Probe on the §3 layout with the full store of §7:
  append to `book/src/SUMMARY.md`, write a file under `.claudestacks/`, `.claude/`, `examples/` and
  `skills/`, rewrite `README.md` → `buildl build` `4 built, 2 cached` (the four are the probes),
  `buildl test` `tests: 2 passed (2 cached)`. Control: append a line to
  `crates/buildl-core/src/lib.rs` → `buildl plan` `6 to run, 0 conditional, 0 clean`, `buildl plan
  //:tests //:doctests` `4 to run, 0 conditional, 0 clean`.
- **Stray files under `crates/`.** `!/crates/` keeps the whole folder, and buildl reads no
  `.gitignore`, so the last three lines leave out the stray files the repository's `.gitignore`
  expects (`.DS_*`, `**/*.rs.bk`, `**/mutants.out*/`, `.gitignore:25,7,14`); a later line wins over
  `!/crates/` below it. Probe, same store: write `crates/.DS_Store`, `crates/buildl/.DS_Store`,
  `crates/buildl/src/lib.rs.bk` and `crates/mutants.out.old/a` → `4 built, 2 cached`,
  `tests: 2 passed (2 cached)`. Any other file a developer leaves under `crates/` is an input and
  makes every target dirty on that machine; a clean checkout, as CI has, holds none.
- **Nothing cargo needs is left out.** The crates read no file outside `crates/`:
  `git grep -n CARGO_MANIFEST_DIR -- crates` lists six files, each joining a path below its own
  crate (`crates/buildl-cargo/tests/init.rs:40`, `crates/buildl-cargo/tests/sync.rs:49`,
  `crates/buildl-lua/tests/flows/common.rs:16`, `crates/buildl/src/remote/proto.rs:47-87`,
  `crates/buildl/src/remote/tls_fixture.rs:23`, `crates/buildl/tests/e2e/common.rs:63`), and
  `git grep -n -E 'include(_str|_bytes)?!' -- crates` names only files below `crates/` (for
  example `crates/buildl-cli/tests/cli/remote.rs:23` → `crates/buildl/tests/fixtures/tls/`), the
  repository has no `.cargo/` folder (`git ls-files | grep '\.cargo/'` → nothing) and no
  `build.rs` under `crates/` (`git ls-files | grep 'build\.rs$'` → only
  `crates/buildl-core/tests/flows/build.rs`, a test file, and two under `examples/`). The full run
  in §7 built and tested with only these 529 files in the sandbox.
- **A new root file or folder** stays out without a new line. **If cargo comes to read a new root
  file** (a `.cargo/config.toml`, a workspace-level `build.rs`), it is missing from the sandbox and
  the target fails; the fix is a `!/<path>` line. That failure is the intended signal.

## 3. `buildl.toml` and `build.lua`

`buildl.toml` holds the `[workspace]`, `[ceiling]` and `[plugins.rust]` tables `cargo buildl init`
writes (`crates/buildl-cargo/src/scaffold.rs:25-49`: `proc.run = ["sh", "rustc"]`,
`env.read = env.unkeyed = ["HOME", "PATH"]`, `fs.read = ["."]`, `fs.write = ["out", ".buildl"]`,
`network = true`), with the plugin at `rev = "plugin-rust-v0.2.1"` (D10), plus
`[execution] timeout = "30m"`: the default is `"10m"` (`book/src/appendix-02-buildl-toml.md:74`),
and the cold `tests` action took 199.2s here (§7) and is slower on a runner with fewer cores.
`buildl.lock`, written by `buildl plugins fetch`, is committed; it pins the commit the tag names.

**D10, the plugin tag.** The published `plugin-rust-v0.2.0` names the 0.1.0 plugin
(`git ls-remote --tags origin 'plugin-*'` → `plugin-rust-v0.1.0` and `plugin-rust-v0.2.0` both
`d0717135…`; `git show d0717135:plugins/rust/extension.toml` → `version = "0.1.0"`). The author
chose to fix that first, in its own chain, `2026-11-11-rust-plugin-tag`, which publishes
`plugin-rust-v0.2.1`. This chain pins that tag, so its execution starts after that tag exists.
The probes below ran on `plugin-rust-v0.2.0` (`fetched rust d0717135…`). The plugin
`plugin-rust-v0.2.1` will carry is the one in the working tree (that chain changes only its
version), and it declares the same eight commands for this `build.lua`: `buildl graph --json` with
`[plugins.rust] path = "plugins/rust"` against the same with the fetched tag, each target's `run`
listed and compared → identical (for example `//:tests` →
`["sh", "-c", "cargo test --locked --profile dev --target-dir target \"$@\"", "sh",
"--workspace", "--all-targets", "--all-features"]`).

`build.lua` is written by hand and declares four calls, each with `version_probe = true`:

| Target | Call | cargo runs |
|---|---|---|
| `buildl` | `rust.binary("buildl", { args = rust.package("buildl-cli") })` | `cargo build --locked --bin buildl --profile dev --target-dir target -p buildl-cli` |
| `cargo-buildl` | `rust.binary("cargo-buildl", { args = rust.package("buildl-cargo") })` | the same for `-p buildl-cargo` |
| `tests` | `rust.test("tests", { args = { "--workspace", "--all-targets", "--all-features" } })` | `cargo test --locked --profile dev --target-dir target --workspace --all-targets --all-features` |
| `doctests` | `rust.test("doctests", { args = { "--workspace", "--all-features", "--doc" } })` | `cargo test --locked --profile dev --target-dir target --workspace --all-features --doc` |

- **D2.** `tests` and `doctests` carry the arguments of `cargo make test` and `cargo make test-doc`
  (`Makefile.toml:93-103`), so a pass means what the gate's two test steps mean. All targets read
  the same 529 inputs (§2), so a split by crate would add cold cargo builds without letting any
  target stay clean; one per crate took 522.3s cold on 0.2.1 (intent), this layout 199.2s (§7).
- **D3.** `rust-toolchain.toml:6` tracks `channel = "stable"`, so a `rustup update` changes the
  compiler with no input change. `version_probe = true` declares `<name>.version` (`rustc -vV`,
  always run) and makes the target depend on it (`plugins/rust/graph.lua:133-136`). Probe:
  `buildl build //:buildl.version` → `rustc 1.99.0 (b940084d7 2026-09-28)` … `1 built`, the same
  as `rustc --version` in the repository. The probe runs in a sandbox without
  `rust-toolchain.toml`, so it reports rustup's default toolchain (`rustup default` →
  `stable-aarch64-apple-darwin (default)`); both are `stable` today, and on a CI runner, where
  `rustup toolchain install` installs `stable` and it is the default. On a machine whose default
  is another toolchain, an update to `stable` changes what cargo uses without changing the probe;
  the book section (§6) says so, and deleting `.buildl/`, then `buildl plugins fetch`, is the
  remedy there. Cost, as documented
  (`plugins/rust/README.md:176-178`) and seen in §7: `buildl plan` after a full build reads
  `4 to run, 2 conditional, 0 clean`, and `buildl build` counts the four probes as built.
- **D4.** `cargo buildl init` on a scratch copy wrote one `rust.test` per crate with neither
  `--all-features` nor `version_probe`, plus the two binaries, inside the section `sync`
  rewrites (probe output: `rust.test("buildl-test", …)` … `rust.test("buildl-lua-test", …)`), which
  contradicts D2 and D3.
- **Adding a crate** that ships a binary needs a new `rust.binary` line; a crate without one is
  covered by `--workspace`.

`.gitignore` gains `/out/` and `/.buildl/`, the two lines `cargo buildl init` adds
(`crates/buildl-cargo/src/scaffold.rs:14`).

## 4. The pin, `.buildl-version`

- **Content.** One line, the version without a `v`: `0.3.0` followed by a newline. buildl itself
  never reads the file; CI and people do.
- **Install.** `curl -LsSf https://airsstack.github.io/buildl/install.sh | BUILDL_VERSION=<content> sh`,
  the installer the book documents (`book/src/ch01-01-installation.md:28-32`). It fetches
  `releases/download/buildl-cli-v<version>/buildl-cli-installer.sh` (`install/install.sh:45-58`);
  that installer carries the sha256 of each archive and checks it
  (`buildl-cli-installer.sh` of 0.3.0, lines 220-271, `_checksum_style="sha256"`). Probe with
  `BUILDL_VERSION=0.3.0 BUILDL_CLI_INSTALL_DIR=<dir> BUILDL_CLI_NO_MODIFY_PATH=1` →
  `everything's installed!`, `<dir>/buildl --version` → `buildl 0.3.0`.
- **The check.** After installing, CI fails unless `buildl --version` prints exactly
  `buildl <content>`. The installer alone does not refuse a bad pin — probe, one run per content:

  | `.buildl-version` | Installer | `buildl --version` | Check |
  |---|---|---|---|
  | `0.3.0` | `fetching the installer for buildl 0.3.0` … `everything's installed!` | `buildl 0.3.0` | pass |
  | empty | `fetching the installer for the latest release` … installed | `buildl 0.3.0` | fail (`buildl ` ≠) |
  | `latest` | `fetching the installer for the latest release` … installed | `buildl 0.3.0` | fail |
  | `v0.3.0` | `fetching the installer for buildl 0.3.0` … installed | `buildl 0.3.0` | fail (the file must be plain `X.Y.Z`) |
  | `0.3.0` + CRLF | `error: BUILDL_VERSION='0.3.0' is not a version` | none | fail |
  | `9.9.9` | `error: could not download the installer for buildl 9.9.9; is 9.9.9 a released version?` | none | fail |

- **Bumping.** The pin can name only a published release (row `9.9.9`), so the bump comes in its
  own change after a release is tagged: `.buildl-version` moves to the new version, and the build
  files change in the same change if the release needs it. The bump is a cache miss (§5).

## 5. The CI job

A new job in `.github/workflows/ci.yml`, beside the existing ones, on the workflow's existing
triggers (`push` to `main`, `pull_request`, `workflow_dispatch`, `.github/workflows/ci.yml:3-7`):
the intent's "every push and pull request" read as the `dod` job reads it, every pull request and
every push to `main`.

```text
matrix: ubuntu-latest, macos-latest            timeout-minutes: 60
  checkout
  rustup toolchain install                      (as the dod job)
  install buildl $(cat .buildl-version) → check `buildl --version` (§4)
  restore .buildl/  key  = buildl-<os>-<pin>-<sha256 of rustc -vV>-<hash of the inputs>
                    else = newest key starting buildl-<os>-<pin>-
  buildl plugins fetch
  buildl build   → must exit 0
  buildl test    → must exit 0
  buildl build   → must end "4 built, 2 cached"        (second pass)
  buildl test    → must end "tests: 2 passed (2 cached)"
  buildl gc
  (end of job) save .buildl/ under key, only on a miss and only if the job passed
```

- **D6.** The matrix is the `dod` job's (`.github/workflows/ci.yml:36`). macOS is proven (§7).
  Linux is not: buildl's tests have not run inside buildl's sandbox on Linux. A run in a local
  Linux container was started and stopped at the author's word, for the machine's load (it had
  reached `buildl build`, cold, after `ok: 8 declarations in 1 build file`).
- **D11, if Linux fails.** The job's first run on `ubuntu-latest` is the proof. If it fails there,
  execution stops at that task and the failure, with its log, goes to the author: no
  `continue-on-error`, no dropping `ubuntu-latest` from the matrix, no change to buildl in this
  chain (intent: "A gap found in buildl while doing it is brought to the author, not patched
  here").
- **Timeout.** 60 minutes: the cold run took 107.3s + 199.2s on the probe machine (§7); runners
  have fewer cores, and the `dod` job, one cold workspace test, has 45 (`ci.yml:38`). Runner time,
  memory and disk for two cold workspace builds at once are not measured; a run that hits the
  timeout, or runs out of memory or disk, fails the job and is handled as D11.
- **Installing.** The step runs the installer with `BUILDL_VERSION` from the file and
  `BUILDL_CLI_NO_MODIFY_PATH=1`, so it installs to the default `$HOME/.local/bin`
  (`buildl-cli-installer.sh` of 0.3.0, lines 764-772); it checks the version through that full
  path in the same step, then appends `$HOME/.local/bin` to `$GITHUB_PATH` itself, so later
  steps run `buildl` by name and nothing about `PATH` is left to the installer.
- **The hashed inputs** are `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `crates/**`,
  `build.lua`, `buildl.toml`, `buildl.lock`, `.buildlignore` and `.buildl-version`. The key decides
  only what is restored; buildl decides what is clean from its own keys, so a file missing from
  the hash costs a skipped save, never a wrong result.
- **The action** is `actions/cache@v5`, the current major (`git ls-remote
  https://github.com/actions/cache 'refs/tags/v5*'` → newest `v5.1.0`, `caa29612…`; its README,
  line 21: "`actions/cache@v5` runs on the Node.js 24 runtime"). The quotes below are from copies
  fetched on 2026-11-11: GitHub's `content/actions/reference/workflows-and-actions/dependency-caching.md`
  (github/docs `main`) and `src/saveImpl.ts`, `src/restoreImpl.ts` (actions/cache `main`).
- **Fallback (D8).** On a miss, `actions/cache` restores the newest cache whose key starts with a
  restore key ("If there are multiple partial matches for a restore key, the action returns the
  most recently created cache", GitHub docs `dependency-caching.md:174`). The restore key holds the
  OS and the pin, so a store written by another buildl release is never restored.
- **Stale store and `gc`.** Probe on a one-file workspace (`buildl.target("copy", { run =
  { "cp", "in.txt", "$out" } … })`): `1 built` (3 blobs) → change `in.txt` → `1 built` (5 blobs)
  → `gc: dropped 0 rows, removed 2 blobs (105 bytes)` (3 blobs) → `1 cached`. A restored old
  store is rebuilt where its inputs changed, and `gc` leaves only what the current results need.
- **Saving.** A hit on the exact key saves nothing (`actions/cache` `src/saveImpl.ts:50-52`:
  "Cache hit occurred on the primary key …, not saving cache."); a miss saves a new cache "if the
  job completes successfully" (`dependency-caching.md:32`), so a failed run leaves no cache.
- **When the cache service fails**, the job goes on. Restore: the toolkit catches every error
  but a `ValidationError` and logs it, "warn on cache restore failure and continue build", as a
  miss (actions/toolkit `packages/cache/src/cache.ts:285-302`, fetched 2026-11-11). Save:
  `saveImpl.ts:75-76` catches and calls `utils.logWarning`. A failed restore is a cold run, a
  failed save leaves the next run cold; neither changes what buildl decides. A
  `ValidationError` (a malformed key or path) fails the step (`restoreImpl.ts:87-88`
  `core.setFailed`), which the key's fixed shape avoids.
- **Keys across machines.** The intent's determinism constraint: the store from one folder is
  served whole in another (the probe below), and the plugin marks `HOME` and `PATH` unkeyed
  (`plan`: `unkeyed variables: HOME, PATH`), so no machine path enters a key
  (`book/src/ch11-05-determinism-rules.md`).
- **Restoring into another folder works.** Probe: the store after the full run, copied with the
  workspace to a second folder → `4 built, 2 cached` and `tests: 2 passed (2 cached)` (§7).
- **Size.** `du -sh .buildl` after the full run → `236M` (`.buildl/cas` `235M`); one cache per
  OS and key. The repository limit is 10 GB, beyond which the oldest entries are evicted, and an
  entry unused for 7 days is removed (`dependency-caching.md:356`).
- **The plugin** lives under `.buildl/plugins/` and is gone in a fresh checkout; without
  `buildl plugins fetch`, `buildl check` fails: `error: plugin rust is not fetched:
  …/.buildl/plugins/rust/d0717135… is not there; run \`buildl plugins fetch\`` (probe, exit 5).

## 6. Docs

- **`book/src/ch11-07-testing-layers.md`.** Line 115, "The last layer is an aim rather than a
  test: `buildl`'s own gate running under `buildl`.", is replaced by a section, "buildl builds
  buildl": the four targets, the `.buildlignore`, the pin and its check, the CI job and its cache,
  and the bump after each release, and the one case the probe misses: a machine whose rustup
  default is not `stable` (§3, D3). Diagrams in Mermaid, as the book uses.
- **`README.md`**, a section "Building this repository": install the pinned version from
  `.buildl-version`, run `buildl plugins fetch`, `buildl build`, `buildl test`; it links the book
  section. The book stays the authority (CLAUDE.md, "Keep it consistent with the book").

## 7. The full run

On the scratch copy with §2-§4 in place, from an empty `.buildl/`:

| Step | Last line | Time |
|---|---|---|
| `buildl plugins fetch` | `fetched rust d0717135ccee72166e79640e9b4b4fcaa46cfd2a` | — |
| `buildl check` | `ok: 8 declarations in 1 build file` | — |
| `buildl build`, cold | `6 built` (2 binaries, 4 probes; each cargo `Finished … in 1m 39s`) | 107.32s |
| `buildl test`, cold | `tests: 2 passed` | 199.20s |
| `buildl build`, again | `4 built, 2 cached` | 0.49s |
| `buildl test`, again | `tests: 2 passed (2 cached)` | 0.12s |
| `buildl plan` | `4 to run, 2 conditional, 0 clean` | — |
| `out/buildl --version` | `buildl 0.3.0` | — |
| same store, another folder: `build` / `test` | `4 built, 2 cached` / `tests: 2 passed (2 cached)` | 0.48s / 0.12s |

## 8. Testing

There is no Rust code to test. The proof is the run itself:

- Locally, the §7 sequence on the repository with `plugin-rust-v0.2.1` fetched, with the same
  last lines and `buildl plan` naming the plugin `rust@0.2.1`.
- In CI, the job of §5 on both operating systems, green on its first run (a miss) and on a re-run
  of the same commit (a hit: restore, `4 built, 2 cached`, nothing saved).
- An edit to a file under `book/` leaves `buildl build` at `4 built, 2 cached`.

## 9. Non-goals

- Replacing `cargo make` or any of its CI jobs (intent).
- Changes to the buildl crates, the `rust` or `c` plugin, or `cargo-buildl` (intent).
- A buildl built from the working tree (intent).
- A wrapper script that installs the pin by itself (D5: not chosen, a hidden download).
- A remote cache server (`[remote]`): the cache lives in GitHub's Actions cache (D7).
- `fmt`, `clippy`, `doc`, `deny`, the guards, `examples`, `plugins`, `skills` and the book build
  under buildl (intent).
