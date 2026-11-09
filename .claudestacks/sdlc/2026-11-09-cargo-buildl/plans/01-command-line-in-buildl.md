---
status: done
created: 2026-11-09
built-on: 072796d9b51f
---

# Command Line in buildl Implementation Plan

**Goal:** Move the `buildl` command line from the `buildl-cli` binary into the `buildl` library unchanged.

**Architecture:** The files of `crates/buildl-cli/src/` except `main.rs` become the folder module `crates/buildl/src/cli/`, with `cli.rs` renamed `args.rs`. `buildl::cli` exposes `Cli`, `Command`, `Color`, `Exit`, `Terminal`, `RemoteSwitches`, `StdConsole`, `run` and a new `execute` that runs an already-parsed `Command`. `buildl-cli` keeps `main.rs`, which reads the process and calls `buildl::cli::run`, and keeps its integration tests. `cli` is a composition module of `buildl`.

**Tech Stack:** Rust 2024, clap 4 derive, cargo-make guards, cargo-deny, mdBook.

**Content authority:** spec §2, §3, §10 (the rows for `Makefile.toml` composition, `expected-edges.txt` clap, `deny.toml` clap, `ch11-03`, `ch11-04`, `ch11-06`, `ch11-07`, `appendix-04`), §11 (last row).

---

## Files that change

```
crates/buildl/src/cli/mod.rs               — create: module docs, `mod` lines and `pub use` of the public items only
crates/buildl/src/cli/args.rs              — create: the former buildl-cli/src/cli.rs (parser types), items made public where spec §3 lists them
crates/buildl/src/cli/run.rs               — create: the former run.rs, plus `execute`, which `run` calls after parsing
crates/buildl/src/cli/exit.rs              — create: the former exit.rs, `Exit` public
crates/buildl/src/cli/doctor_text.rs       — create: the former doctor_text.rs
crates/buildl/src/cli/remote_switches.rs   — create: the former remote_switches.rs, `RemoteSwitches` public
crates/buildl/src/cli/terminal.rs          — create: the former terminal.rs, `Terminal` public
crates/buildl/src/cli/console.rs           — create: the former console.rs, `StdConsole` public
crates/buildl/src/lib.rs                   — modify: `pub mod cli;` and its line in the crate docs
crates/buildl/Cargo.toml                   — modify: depend on `clap` (workspace)
crates/buildl-cli/src/main.rs              — modify: module list gone; `main` calls `buildl::cli::run`; crate docs keep the command and exit tables
crates/buildl-cli/src/{cli,run,exit,doctor_text,remote_switches,terminal,console}.rs — delete (moved)
crates/buildl-cli/Cargo.toml               — modify: drop `clap`
crates/expected-edges.txt                  — modify: `clap` under `# buildl`, not under `# buildl-cli`
Makefile.toml                              — modify: `composition="local_ports workspace cli"`
deny.toml                                  — modify: `clap` wrappers `buildl`; its reason says the command line lives in buildl
book/src/ch11-03-four-crates.md            — modify: line 14 tree, line 29 and 168 diagrams, line 193 crate row, rule 4 at line 454: the command line is in buildl, buildl-cli is `main`
book/src/ch11-04-errors.md                 — modify: line 171 path to crates/buildl/src/cli/exit.rs
book/src/ch11-07-testing-layers.md         — modify: line 27 path to crates/buildl/src/cli
book/src/appendix-04-exit-codes.md         — modify: line 4 path to crates/buildl/src/cli/exit.rs
book/src/ch11-06-decision-records.md       — modify: a record "The command line lives in buildl", reason: a second binary shares it
```

## Order of work

### Task 1 — Move the command line into `buildl::cli`

**Files:**
- Create `crates/buildl/src/cli/{mod,args,run,exit,doctor_text,remote_switches,terminal,console}.rs` — the moved files, their `#[cfg(test)]` modules moved with them; imports change from `buildl::…` to `crate::…` and `buildl_core::…`.
- Modify `crates/buildl/src/lib.rs`, `crates/buildl/Cargo.toml`, `crates/buildl-cli/src/main.rs`, `crates/buildl-cli/Cargo.toml`, `crates/expected-edges.txt`, `Makefile.toml`, `deny.toml`.
- Delete the seven moved files from `crates/buildl-cli/src/`.

**Change:** The code moves without any change in behaviour. `cli.rs` becomes `args.rs`, because a file `cli.rs` inside module `cli` fails clippy's `module_inception`. `execute(color, command, start, terminal, remote, stdout, stderr, console) -> Exit` is the part of `run` after parsing, and `run` calls it. `Command` and the other items spec §3 names become `pub`, re-exported from `cli/mod.rs`. So do the argument types `Command`'s variants carry: `GraphArgs`, `GraphFormat`, `QueryArgs`, `PlanArgs`, `BuildArgs`, `TestArgs`, `VerifyArgs`, `LogArgs`, `Streams`, `PluginsArgs`, `PluginsCommand`, `RemoteArgs`, `Selection` and any type they hold. A public enum cannot carry private types: `private_interfaces` and `unreachable_pub` would fail clippy `-D warnings`. `cli` joins `local_ports` and `workspace` as a composition module in `guard-adapter-isolation`.

**Proof:**
- Every unit test that moved passes in its new place.
- `execute_runs_a_parsed_command` — `execute` given `Command::Clean` in a temp workspace writes the same bytes and returns the same `Exit` as `run` given `["buildl", "clean"]`.
- `crates/buildl-cli/tests/cli/*` pass unchanged.

**Done when:** `cargo make dod` and `cargo deny check` pass. Commit `refactor(buildl): move the command line from buildl-cli into buildl::cli`.

### Task 2 — Say where the command line lives in the book

**Files:**
- Modify `book/src/ch11-03-four-crates.md`, `book/src/ch11-04-errors.md`, `book/src/ch11-07-testing-layers.md`, `book/src/appendix-04-exit-codes.md`, `book/src/ch11-06-decision-records.md`.

**Change:** The paths move to `crates/buildl/src/cli/`, and `buildl-cli` is described as `main` alone. Dependency rule 4 keeps its number and says the command line reaches core types through `buildl`, where it lives. The new decision record states the move and its reason. Mermaid blocks stay Mermaid.

**Proof:**
- `cargo make book-build` builds the book.
- `grep -rn 'buildl-cli/src/' book/src` prints nothing but `main.rs` references.

**Done when:** `cargo make book-build`, `cargo make dod` and `cargo deny check` pass. Commit `docs(repo): place the command line in buildl in the book`.

---

## Risks

- The move is wide (seven files and their tests). The riskiest step is the import rewrite. The unchanged `buildl-cli/tests` are the guard that nothing the binary does changed.
- `Cli`'s `#[command(version)]` reads `CARGO_PKG_VERSION` from the crate it compiles in, which is now `buildl`, not `buildl-cli`. `buildl --version` is unchanged because the crates are released at one version together (`CHANGELOG.md` 0.2.0: "publishes the four crates at 0.2.0"); a release that split them would change it.
- `doc -D warnings`: public items that were `pub(crate)` now need complete rustdoc. They already carry docs, but intra-doc links naming `crate::` items in the old binary have to be re-pointed.

## Proof

- `cargo make dod` (fmt, clippy with the four guards, doc, tests, doctests) and `cargo deny check` are green.
- `crates/buildl-cli/tests` are unchanged and green.

## Review findings

- 🟡 `Cargo.toml` clap comment said "a dependency of `buildl-cli` only" — fixed in `440ff9a`; `cargo fmt --all -- --check` clean.
- 🟡 `Makefile.toml` guard-adapter-isolation message named only `local_ports and workspace` — fixed in `440ff9a`; `cargo make guard-adapter-isolation` → `Build Done`.
- 🔵 `execute`'s `#[expect(too_many_arguments)]` reason was inaccurate for `color` and `command` — fixed in `440ff9a`.
- 🔵 `Streams` is public though only a `pub(crate)` method produces it — kept; the plan lists it.
- 🔵 `CLAUDE.md` still calls `buildl-cli` a "thin clap shell" — fixed in plan 05 (`7e780fa`): `buildl-cli` is "only `main`, which calls `buildl::cli::run`".
- 🔵 Task 2's commit landed before Task 1's — history only.

## Deviations

- 2026-11-09 — Task 2 (book) ran in parallel with Task 1 and was committed first (`403648d`, then `94f315b`); the two tasks' files are disjoint.
- 2026-11-09 — `Override` is public too: `Selection` and `VerifyArgs` hold it in public fields.
- 2026-11-09 — `execute_runs_a_parsed_command` was written alongside `execute`, since the move and the new function landed together; no separate red run.
