---
status: done
created: 2026-11-09
depends-on: [01]
built-on: 072796d9b51f
---

# cargo-buildl Pass-Through Implementation Plan

**Goal:** Ship the `buildl-cargo` crate whose `cargo-buildl` binary runs every buildl command as `cargo buildl <command>`.

**Architecture:** A fifth workspace member, `crates/buildl-cargo`, with `[[bin]] name = "cargo-buildl"`. Its parser, named `cargo buildl`, flattens `buildl::cli::Command` and carries the global `--color`. `main` drops argv[1] when it is exactly `buildl`, reads the process exactly as `buildl-cli`'s `main` does, and calls `buildl::cli::execute`. `init` and `sync` are added by plans 03 and 04.

**Tech Stack:** Rust 2024, clap 4 derive, cargo-make guards, cargo-deny, cargo-dist metadata, mdBook.

**Content authority:** spec §2 (the `buildl-cargo` row), §4, §9 (exit 2), §10 (the rows for `guard-crate-edges`, `expected-edges.txt`, `deny.toml`, `ch11-03` five crates), §11 (argv tests).

---

## Files that change

```
Cargo.toml                          — modify: add crates/buildl-cargo to members; buildl-cargo path dependency if the workspace lists members' paths
crates/buildl-cargo/Cargo.toml      — create: package buildl-cargo, [[bin]] cargo-buildl, deps buildl + clap, dev-dep tempfile, [package.metadata.dist] dist = false, [lints] workspace = true
crates/buildl-cargo/README.md       — create: what the crate is, `cargo install buildl-cargo`
crates/buildl-cargo/src/main.rs     — create: crate docs and `main`, which reads the process and hands on
crates/buildl-cargo/src/args.rs     — create: `CargoCli`, `CargoCommand` (flattened buildl `Command`), `arguments` (the argv[1] rule)
crates/buildl-cargo/tests/pass_through.rs — create: runs the built binary in a temp workspace
crates/expected-edges.txt           — modify: a `# buildl-cargo` block
Makefile.toml                       — modify: guard-crate-edges member list adds buildl-cargo
deny.toml                           — modify: `buildl`, `clap` and `tempfile` wrappers add buildl-cargo; reasons updated
book/src/ch11-03-four-crates.md     — modify: five crates — opening sentence, tree, diagrams, crate table row, publish order P5, publishing rule 2 ("no fifth member") and rule 4 reworded
```

## Order of work

### Task 1 — Add the crate and its argument rule

**Files:**
- Create `crates/buildl-cargo/{Cargo.toml,README.md,src/main.rs,src/args.rs}`.
- Modify `Cargo.toml`, `crates/expected-edges.txt`, `Makefile.toml`, `deny.toml`.

**Change:**
- `arguments(args)` returns the argument list with the second item removed when it is exactly `buildl`, and unchanged otherwise.
- `CargoCli` is the clap parser named `cargo buildl`, with `version` from `buildl-cargo` and the global `--color`.
- `CargoCommand` flattens `buildl::cli::Command`.
- `main` builds `Terminal` (stderr `is_terminal`, `TERM`, `NO_COLOR`) and `RemoteSwitches` (`BUILDL_REMOTE_UPLOAD`, `BUILDL_REMOTE_STRICT`, `BUILDL_REMOTE_EXECUTE`) as `buildl-cli` does, parses, and calls `buildl::cli::execute`. A parse error is clap's, with exit code 2.

**Proof:**
- `buildl_is_dropped_only_as_the_first_argument` — `["cargo-buildl", "buildl", "build"]` gives `["cargo-buildl", "build"]`; `["cargo-buildl", "build", "buildl"]` is unchanged.
- `a_buildl_command_parses` — `build -k --set a=b` and `test --stop` parse into the flattened `Command`.
- `a_usage_error_names_cargo_buildl` — an unknown command's error text holds `cargo buildl` and maps to exit 2.

**Done when:** `cargo make dod` and `cargo deny check` pass. Commit `feat(buildl-cargo): add the cargo-buildl binary that runs buildl commands`.

### Task 2 — Prove the pass-through on the built binary

**Files:**
- Create `crates/buildl-cargo/tests/pass_through.rs`.

**Change:** Integration tests that run `env!("CARGO_BIN_EXE_cargo-buildl")` in a `tempfile` workspace. The workspace holds a `buildl.toml` that the test writes itself, so no file outside the package is read.

**Proof:**
- `clean_passes_through` — `cargo-buildl buildl clean` exits 0 and prints buildl's `clean: out holds nothing`.
- `a_usage_error_exits_2` — `cargo-buildl buildl bogus` exits 2.
- `help_comes_from_cargo_buildl` — `cargo-buildl buildl --help` exits 0 and lists `build` and `test`.

**Done when:** `cargo make dod` and `cargo deny check` pass. Commit `test(buildl-cargo): run buildl commands through cargo-buildl`.

### Task 3 — Make the book's crate chapter count five crates

**Files:**
- Modify `book/src/ch11-03-four-crates.md`.

**Change:** The chapter describes five crates. `buildl-cargo` gets a row in the crate table (depends on `buildl` and `clap`; must not depend on `buildl-core` or `buildl-lua` directly), a node in each diagram, and the fifth place in the publish order. Publishing rule 2 no longer says "no fifth member". Rule 4 covers both binaries. The file name and every rule number stay, because other files cite them. Mermaid stays Mermaid.

**Proof:**
- `cargo make book-build` builds the book.

**Done when:** `cargo make book-build`, `cargo make dod` and `cargo deny check` pass. Commit `docs(repo): describe buildl-cargo as the fifth crate`.

---

## Risks

- `deny.toml` ban reasons cite `ch11-03` rule numbers. Task 3 must keep those numbers, or the reasons point at the wrong rule.
- cargo-dist picks up any package with a binary. `dist = false` in Task 1 keeps the release workflow as it is; leaving it out would add `cargo-buildl` to every release build.

## Proof

- `cargo make dod` (the guards included) and `cargo deny check` are green.
- `cargo run -p buildl-cargo -- buildl clean` in a buildl workspace prints buildl's `clean:` line.

## Review findings

- 🔵 `help_comes_from_cargo_buildl` checked `help.contains("build")`, which "cargo buildl" satisfies alone — fixed: the test now looks for a help line starting with `build ` / `test `.
- 🔵 `Makefile.toml` `publish-dry-run` said "four crates" — fixed: five, with `buildl-cargo` last.
- 🔵 ch11-03's H1 "Four Crates, Ports and Adapters" contradicted the five-crate page — fixed: "Five Crates, Ports and Adapters", with the link text in `SUMMARY.md`, `ch11-00` and `ch11-06`; the file name stays; `cargo make book-build` → `Build Done`.
- 🔵 ch11-03 C4 level 2: the cargo-buildl process had no edge to `out/` — fixed: `links named outputs`.
- 🔵 Dependency rule 4 named `buildl::buildl_core::TargetName` before any `buildl-cargo` code used it — true since plan 03's `633598f` (`crates/buildl-cargo/src/generate.rs:10`).
- 🔵 No guard fails if `dist = false` is removed — declined: the plan names this as a risk, and no guard was planned.
- 🔵 Usage errors raised after parsing (duplicate `--set`, `Cli::command().error` in `buildl::cli::run`) print `Usage: buildl …` under `cargo buildl`; the exit code is 2 — declined: the text comes from buildl's own command line, which this add-on does not change.

## Deviations

- 2026-11-09 — Task 3 (book) ran in parallel with plan 01 Task 1 and was committed before Task 1 (`1e68573`, then `4fa9d31`, `73e17ec`); the tasks' files are disjoint.
- 2026-11-09 — `clean: out holds nothing` goes to stderr; `clean_passes_through` asserts stderr.
