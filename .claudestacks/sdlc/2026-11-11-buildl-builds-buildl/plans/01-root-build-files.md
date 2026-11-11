---
status: done
created: 2026-11-11
built-on: a6e2956d9de2
---

# Root Build Files Implementation Plan

**Goal:** The pinned buildl 0.3.0 builds and tests this repository from its root.

**Architecture:** Five new root files and two `.gitignore` lines; nothing in the crates, the
plugins or `cargo make` changes (spec §9). `build.lua` declares four calls on the `rust` plugin at
`plugin-rust-v0.2.1`; `.buildlignore` keeps only what cargo reads; `.buildl-version` names the
release CI and people install.

**Tech Stack:** Lua (`build.lua`), TOML (`buildl.toml`), the `rust` plugin, buildl 0.3.0.

**Content authority:** `spec.md` §2, §3, §4 (the file), §7, §8 (the local run and the `book/`
edit).

---

## Files that change

```
.buildlignore      — new: the eight lines of spec §2, verbatim
buildl.toml        — new: [workspace], [ceiling], [plugins.rust] as `cargo buildl init` writes them,
                     rev = "plugin-rust-v0.2.1", plus [execution] timeout = "30m"
build.lua          — new: the four calls of spec §3, each with version_probe = true
buildl.lock        — new: written by `buildl plugins fetch`, committed
.buildl-version    — new: `0.3.0` and a newline
.gitignore         — two lines: /out/ and /.buildl/
```

## Order of work

### Task 1 — Write the root build files

**Files:**
- Create `.buildlignore` — exactly the eight lines in spec §2, in that order (`/*`, the four `!`
  lines, `.DS_*`, `*.rs.bk`, `mutants.out*/`).
- Create `buildl.toml` — the `[workspace]`, `[ceiling]` and `[plugins.rust]` tables
  `crates/buildl-cargo/src/scaffold.rs` `buildl_toml` writes (`proc.run = ["sh", "rustc"]`,
  `env.read` and `env.unkeyed` `["HOME", "PATH"]`, `fs.read = ["."]`,
  `fs.write = ["out", ".buildl"]`, the plugin's `network = true`), the plugin at
  `github = "airsstack/buildl"`, `rev = "plugin-rust-v0.2.1"`, `dir = "plugins/rust"`, and an
  `[execution]` table with `timeout = "30m"`. The workspace name is `buildl`.
- Create `build.lua` — `local rust = buildl.use("rust")`, then the four calls of the spec §3 table:
  `rust.binary("buildl", …)` on `rust.package("buildl-cli")`, `rust.binary("cargo-buildl", …)` on
  `rust.package("buildl-cargo")`, `rust.test("tests", …)` with
  `{ "--workspace", "--all-targets", "--all-features" }`, `rust.test("doctests", …)` with
  `{ "--workspace", "--all-features", "--doc" }`; each passes `version_probe = true`. A short
  comment at the top says the file is written by hand, not by `cargo buildl init`/`sync`.
- Create `.buildl-version` — `0.3.0` and a newline, nothing else (spec §4 table: no `v`, no CRLF).
- Create `buildl.lock` — by running `buildl plugins fetch`, never by hand.
- Modify `.gitignore` — add `/out/` and `/.buildl/` with a one-line comment (the two lines
  `cargo buildl init` adds, `scaffold.rs` `IGNORED`).

**Change:** The repository root is a buildl workspace. The binary used is buildl 0.3.0 installed
as spec §4 says: `curl -LsSf https://airsstack.github.io/buildl/install.sh |
BUILDL_VERSION="$(cat .buildl-version)" BUILDL_CLI_INSTALL_DIR=<scratch dir>
BUILDL_CLI_NO_MODIFY_PATH=1 sh`, and `<scratch dir>/buildl --version` prints `buildl 0.3.0`
before any other step. Every buildl command below is that binary.

**Proof:** no Rust test: build files.
- `buildl plugins fetch` → `fetched rust 43d7a542023880fe37657ac8ba0564a25146f72a`, and
  `buildl.lock` names that commit.
- `buildl check` → `ok: 8 declarations in 1 build file`.
- `buildl plan --json`: the inputs of `//:buildl` number 529, every one `Cargo.toml`,
  `Cargo.lock`, `rust-toolchain.toml` or under `crates/` (spec §2).
- `git status --short` lists none of `out/` or `.buildl/`.

**Done when:** the four checks above hold, and `cargo make dod` and
`cargo deny check -D unused-wrapper` are green. Commit
`build(repo): buildl builds buildl on the pinned 0.3.0 release`.

### Task 2 — The full local run

**Change:** none; the run of spec §7 and §8 on Task 1's files, from an empty `.buildl/` followed by
`buildl plugins fetch`, with the same binary.

**Proof:** each step's last line, as spec §7 records it:

| Step | Last line |
|---|---|
| `buildl build`, cold | `6 built` |
| `buildl test`, cold | `tests: 2 passed` |
| `buildl build`, again | `4 built, 2 cached` |
| `buildl test`, again | `tests: 2 passed (2 cached)` |
| `buildl plan` | `4 to run, 2 conditional, 0 clean`, and its lines name `plugin rust@0.2.1` |
| `out/buildl --version` | `buildl 0.3.0` |
| append a line to `book/src/SUMMARY.md`, then `buildl build` | `4 built, 2 cached` |
| the checkout with its `.buildl/` copied to a second folder (without `target/` and `out/`), then `buildl build` / `buildl test` there | `4 built, 2 cached` / `tests: 2 passed (2 cached)` |

The `book/` edit is reverted after the check; the second folder is deleted after its check. Each command runs once; the times are recorded, not
checked.

**Done when:** every row holds. No commit (no file changes).

---

## Risks

- Task 2 is the riskiest and the heaviest step: two cold workspace builds (spec §7: 107 s and
  199 s on the probe machine). It runs once; a failure is recorded with its log, not re-run to
  see.
- A file left under `crates/` by a developer (spec §2, "Any other file…") makes every target
  dirty on that machine; Task 2 runs on a checkout with no such file (`git status --short --ignored crates`
  empty, which also shows files `.gitignore` hides, such as `target` or `*.pdb`).
- The 529 count is the spec's, from `325aa4e`; this branch adds no file under `crates/` since
  (only `crates/buildl-cargo` edits in `59dbe96`), so it holds unless a file is added before
  execution.

## Proof

- Task 1 and Task 2 checks.
- `cargo make dod` and `cargo deny check -D unused-wrapper` green.

## Review findings

One reviewer pass over plans 01–03 found one item for this plan's files:

- 🔵 `.buildlignore`: no CI check catches a `.buildlignore` that stops leaving files out (the
  second pass edits no file). Declined: a guard on the input count is in neither this plan nor
  plan 02; the 529 count and the `book/` edit are proven by Task 2's run.

## Deviations

- 2026-11-11: the gates ran once for Task 1: `cargo make dod` rc 0 ("Build Done in 48.24
  seconds"), `cargo deny check -D unused-wrapper` → "advisories ok, bans ok, licenses ok, sources
  ok". The reviewer was told not to re-run them (light machine load). The work was done inline,
  not by coder agents: config files only.
- 2026-11-11, Task 1 checks, with buildl 0.3.0 installed to a scratch dir (`buildl 0.3.0`):
  `buildl plugins fetch` → `fetched rust 43d7a542023880fe37657ac8ba0564a25146f72a`, and
  `buildl.lock` names that commit; `buildl check` → `ok: 8 declarations in 1 build file`;
  `buildl plan --json` → `//:buildl` and `//:cargo-buildl` 529 inputs each (`Cargo.lock` 1,
  `Cargo.toml` 1, `crates` 526, `rust-toolchain.toml` 1); `git status --short` lists no `out/` or
  `.buildl/`, and `git status --short --ignored crates` is empty. Commit `3bdcf63`.
- 2026-11-11, Task 2 (each step once):

  | Step | Last line | Time |
  |---|---|---|
  | `buildl plugins fetch` | `fetched rust 43d7a542023880fe37657ac8ba0564a25146f72a` | 3 s |
  | `buildl build`, cold | `6 built` | 103 s |
  | `buildl test`, cold | `tests: 2 passed` | 238 s |
  | `buildl build`, again | `4 built, 2 cached` | 1 s |
  | `buildl test`, again | `tests: 2 passed (2 cached)` | 0 s |
  | `buildl plan` | `4 to run, 2 conditional, 0 clean` (6 lines name `rust@0.2.1`) | 0 s |
  | `out/buildl --version` | `buildl 0.3.0` | 2 s |
  | `book/src/SUMMARY.md` appended, `buildl build` | `4 built, 2 cached` | 0 s |
  | second folder, `buildl build` / `buildl test` | `4 built, 2 cached` / `tests: 2 passed (2 cached)` | 0 s / 0 s |
