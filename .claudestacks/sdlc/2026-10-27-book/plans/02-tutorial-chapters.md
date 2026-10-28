---
status: executing
created: 2026-10-28
depends-on: [01]
built-on: 053dd1fe3030
---

# Tutorial Chapters Implementation Plan

**Goal:** The introduction and chapters 1–2 of the book take a new reader from installing buildl to
building and testing the Go starter.

**Architecture:** This plan fills stubs that plan 01 created and changes no other file. It follows
the writing rules every chapter plan shares:
- **Plain words.** Write in the voice of airsl's book. `/Users/hiraq/Projects/airsstack/airsl/book/src/ch01-02-hello-airsl.md` is the model to read first.
- **Real output only.** Every shell transcript comes from running the debug binary built at the branch head (`cargo build -p buildl-cli`), inside a copy of the workspace in a scratch directory, never in the repository. Nothing is typed from memory.
- **The binary decides.** Where `docs/` disagrees with what the binary does, the book says what the binary does.
- **Diagrams.** This plan owns the `docs/` Mermaid diagrams listed under its `## Proof`; each is carried into its chapter as a ` ```mermaid ` block, redrawn where the binary has moved on.
- **A command that disagrees with its `--help`** is raised to the user, not written around.
- **Links and references.** Links between chapters are relative `.md` links. No `§` sign appears in the book.

**Tech Stack:** Markdown, mdBook 0.4.52, the `buildl` debug binary, Go 1.22 (for `examples/go`).

---

## Files that change

```
book/src/introduction.md               — [modify] what buildl is; who the book is for
book/src/ch01-00-getting-started.md    — [modify] chapter opener
book/src/ch01-01-installation.md       — [modify] the install commands that work today
book/src/ch01-02-hello-buildl.md       — [modify] a one-target workspace built
book/src/ch01-03-what-did-it-do.md     — [modify] plan, log, doctor on that workspace
book/src/ch02-00-first-project.md      — [modify] examples/go walked through; rust and python starters
```

## Order of work

### Task 1 — Introduction

**Files:** Modify `book/src/introduction.md`.

**Change:** The introduction covers:
- what buildl is;
- declare, then execute: build files only declare, and the host runs, sandboxes and caches;
- nothing is ambient;
- why Lua on airsl;
- who the book is for, and how its parts are ordered, ending with Under the Hood as optional reading.

Sources: `docs/design.md:9-59` (design §1, §1.1, §2) and `README.md:1-30`.

**Proof:** `mdbook build book` exits 0. The page names no command that `buildl --help` lacks; check it against that output.

**Done when:** both hold. Commit `docs(repo): the book's introduction`.

### Task 2 — Getting Started: installation

**Files:** Modify `book/src/ch01-00-getting-started.md`, `book/src/ch01-01-installation.md`.

**Change:**
- The opener lists the chapter's three sections.
- Installation gives two ways to install:
  - `cargo install --git https://github.com/airsstack/buildl buildl-cli --locked`;
  - from a checkout, `cargo make install`, which runs `cargo install --path crates/buildl-cli --locked --force` (`Makefile.toml` `[tasks.install]`).
- It also gives:
  - the Rust needed: the `rust-version` in `Cargo.toml` and the channel in `rust-toolchain.toml`;
  - `buildl --version` to confirm.
- It names no crates.io install, because the crates are unpublished.

**Proof:**
- Run `cargo install --git https://github.com/airsstack/buildl buildl-cli --locked --root <scratch>/inst` once. Paste its last line and the output of `<scratch>/inst/bin/buildl --version` into the task's report. The spec (§8) marks this as not verified, and this run verifies it. If it fails, stop and raise it to the user. Do not write around it.
- `mdbook build book` exits 0.

**Done when:** both hold. Commit `docs(repo): book chapter 1, installation`.

### Task 3 — Getting Started: hello, and what it did

**Files:** Modify `book/src/ch01-02-hello-buildl.md`, `book/src/ch01-03-what-did-it-do.md`.

**Change:**
- Hello, buildl builds a workspace from nothing:
  - a `buildl.toml` with the smallest ceiling that lets the target run;
  - a `build.lua` declaring one target that writes a file with a program every Unix machine has.

  It shows `buildl check`, `buildl build`, the file under `out/`, and a second `buildl build` running nothing.
- What Did It Do? runs these on the same workspace:
  - `buildl plan` before and after an edit;
  - `buildl log <target>`;
  - `buildl doctor`.

  Each is explained in a sentence or two.

Sources: `docs/design.md:60-91` (§3), `:199-243` (§5) and `:279-390` (§7). These are background only: the commands and outputs come from the binary.

**Proof:**
- Every transcript is captured by running the commands, in order, in `<scratch>/hello`.
- Copying the chapter's `buildl.toml` and `build.lua` into an empty directory and running `buildl build` gives the output the chapter shows.
- `mdbook build book` exits 0.

**Done when:** all hold. Commit `docs(repo): book chapter 1, a first build`.

### Task 4 — A First Project

**Files:** Modify `book/src/ch02-00-first-project.md`.

**Change:** Walk through `examples/go` (its `README.md` and `assert.sh` list what it shows), in this order:
1. Read `buildl.toml` and `build.lua`.
2. Run `buildl build`, then the program.
3. Run `buildl test`.
4. Edit a source file and run `buildl plan`, which names the edited input.
5. Run `buildl test` twice; the second is served from the cache.
6. Run `buildl log`.

Close with `examples/rust` and `examples/python` as the same project in two more languages. Show Rust's `cargo install --root $out` form (from `examples/rust/build.lua`), not `docs/design.md`'s `cargo build --release` form, which does not build (`.claudestacks/sdlc/2026-10-27-examples/spec.md` §1).

Sources: `docs/design.md:551-567` and `:587-601` (§9, §9.1, §9.3), and `examples/{go,rust,python}`.

**Proof:**
- Every transcript is captured in `<scratch>/go`, a copy of `examples/go` with `out/` and `.buildl/` removed.
- `mdbook build book` exits 0.

**Done when:** both hold. Commit `docs(repo): book chapter 2, a first project`.

## Risks

- `cargo install --git` builds the whole workspace from GitHub, which takes several minutes. It runs once, in Task 2, and nowhere else.
- Output includes timings and temporary paths that change from run to run. Where they appear, keep them as captured, and say in the text that they vary.

## Proof

- `cargo make book-build` exits 0.
- The 1 `docs/` Mermaid diagrams this plan owns each appear as a ` ```mermaid ` block in the chapter named, and each such page renders `<pre class="mermaid">` in `book/book/`; open each page once in a browser (`cargo make book`) and confirm the diagram draws, not an error box: `docs/design.md:37` → `introduction.md`.
- The spec §3 rows this plan lands are: design §1 and §1.1 (introduction), §2 (introduction), §3 (ch01-02), §7 (ch01-03), §9, §9.1 and §9.3 (ch02). Mark each one in the execution record.

## Review findings

- risk — `ch01-03`'s `doctor` transcript showed the session's scratch path (local user name and
  session id) — `book/src/ch01-03-what-did-it-do.md:79`. Re-captured in `/tmp/buildl-book/hello`:
  `workspace  /private/tmp/buildl-book/hello`, the rest identical, exit 0. Fixed.
- nit — "Every `buildl` command runs the same pipeline" overstated (`log`, `clean`, `gc`,
  `plugins` read no build file); now "Every command that reads the build files". Fixed.
- nit — `ch02-00:77` said a command sees only its listed variables; now adds the variables buildl
  pins, linking The Sandbox. Fixed.
- nit — `ch01-01:4` "a couple of minutes"; now "several minutes" (the measured install took
  2m02s). Fixed.
- nit — Task 2 cited `Makefile.toml:385-389`, since moved; now cites `[tasks.install]`. Fixed.

## Probe results

- Task 2: `cargo install --git https://github.com/airsstack/buildl buildl-cli --locked --root
  <scratch>/inst` exit 0 in 2m02s: ``Installed package `buildl-cli v0.1.0
  (https://github.com/airsstack/buildl#d99e72c9)` (executable `buildl`)``; `inst/bin/buildl
  --version` → `buildl 0.1.0`. Spec §8's "not verified" install line is now verified.
- Task 3 smallest ceiling: removing each line makes `build` exit 4 naming it — `program sort is
  outside the ceiling`, `input names.in is outside the ceiling`, `directory .buildl is outside the
  ceiling`. A fresh copy of the page's files: `ok: 1 declaration in 1 build file`, `1 built`,
  `ada mia zoe`, `1 cached`. Re-run independently by the reviewer: same output.
- Introduction's "a build file that loops forever is stopped": `error: build.lua: instruction
  ceiling reached: script `build.lua` exceeded its instruction limit of 10000000`, exit 5.
- `introduction.html` renders one `<pre class="mermaid">`; opened in Chrome, the diagram draws
  with all 7 nodes and no error box.
- Spec §3 rows landed: design §1, §1.1, §2 → `introduction.md`; §3 → `ch01-02`; §7 → `ch01-03`;
  §9, §9.1, §9.3 → `ch02-00`.

## Deviations

- 2026-10-28 — the introduction's diagram (from `docs/design.md:37`) labels the execute step "run
  each action in a sandbox", not "run compilers": the binary sandboxes by default.
- 2026-10-28 — transcripts that print a workspace path are captured under `/tmp/buildl-book/`, so
  the book shows no local user or session path.
