---
status: done
created: 2026-10-28
built-on: 139359074974
---

# Short READMEs Implementation Plan

**Goal:** every README in the repository is a short, current pointer into the book, with links
that work wherever it is read.

**Architecture:** the book (`book/src/`, published at `https://airsstack.github.io/buildl/`,
`book/book.toml:16` sets `site-url = "/buildl/"`) is the authority. `README.md` becomes a
60–80 line front page; each crate README becomes a card of about 15–25 lines that crates.io
shows. Nothing in a README says what the book does not.

**Content authority:** `.claudestacks/sdlc/2026-10-27-readmes/intent.md` (spec skipped).

**Tech Stack:** Markdown; mdBook chapter URLs (`<chapter file>.md` → `<chapter file>.html`
under `https://airsstack.github.io/buildl/`).

---

## Files that change

```
README.md                      — [modify] rewritten whole: short front page          Task 1
CLAUDE.md                      — [modify] line 18, what README.md is                 Task 1
crates/buildl-core/README.md   — [modify] rewritten whole: crate card               Task 2
crates/buildl-lua/README.md    — [modify] rewritten whole: crate card               Task 2
crates/buildl/README.md        — [modify] rewritten whole: crate card               Task 2
crates/buildl-cli/README.md    — [modify] rewritten whole: crate card               Task 2
```

No README is pulled into rustdoc: `grep -rn 'include_str\|README' crates/*/src/lib.rs
crates/*/src/main.rs Makefile.toml .github/workflows/*.yml` finds nothing, so no README code
block is a doctest. Each crate's `Cargo.toml` already has `readme = "README.md"` (line 5 in all
four).

## Order of work

### Task 1 — Rewrite `README.md` short

**Files:**
- Rewrite `README.md` (283 lines today)
- Modify `CLAUDE.md:18`

**Change:** `README.md` becomes 60–80 lines, in this order:

| Section | Holds |
|---|---|
| `# buildl` + tagline | the one-line description from `crates/buildl/Cargo.toml:4` |
| pitch | one short paragraph: Lua build files evaluated on airsl, declare then execute, nothing is ambient; pre-release status in one sentence |
| why | the three properties in the book's own words (`book/src/introduction.md:14-22`: build files are safe to read, rebuilds you can explain, authority you can see), one line each |
| a taste | `examples/python/build.lua` copied verbatim (a real, CI-checked file), one line saying its `buildl.toml` sets the ceiling (linking ch04-02), then the commands `buildl build`, `buildl test`, `buildl plan` |
| install | the two ways `book/src/ch01-01-installation.md` gives: line 20 (`cargo install --git https://github.com/airsstack/buildl buildl-cli --locked`) and line 39 (`cargo make install` from a checkout); Linux and macOS (`book/src/ch11-03-four-crates.md:232`) |
| learn more | the book's front page, then a table grouping absolute links: Start (ch01, ch03), Concepts (ch04, ch05, ch06), Features (ch08, ch09 remote cache and execution, ch10), Reference (CLI, exit codes, ch11-00), Examples (`examples/`, relative is fine here: GitHub renders this file) |
| crates | a four-row table: `buildl-core`, `buildl-lua`, `buildl`, `buildl-cli`, one line each, in dependency order |
| license | Apache-2.0; part of airsstack |

Dropped: the long status paragraph, the architecture, action-key, plugins, remote and command
sections, the repository layout, the roadmap and the numbered references (all in the book;
references are `book/src/appendix-05-references.md`).

`CLAUDE.md:18` reads `README.md — condensed public summary of the book`; it becomes: a short
front page that points into the book and `examples/`, kept consistent with the book.

**Proof:**
- `wc -l README.md` prints a number from 60 to 80.
- every `https://airsstack.github.io/buildl/<name>.html` link in `README.md` names a file
  `book/src/<name>.md` that exists (check with a loop over the links).
- the `build.lua` block is byte-identical to `examples/python/build.lua` (`diff` of the
  extracted block against the file).
- the install commands match `book/src/ch01-01-installation.md:20` and `:39`.

**Done when:** the checks above pass, `cargo make dod` and `cargo deny check` are green.
Commit `docs(repo): short README.md`.

### Task 2 — Rewrite the four crate READMEs as cards

**Files:**
- Rewrite `crates/buildl-core/README.md`, `crates/buildl-lua/README.md`,
  `crates/buildl/README.md`, `crates/buildl-cli/README.md`

**Change:** each becomes a card of about 15–25 lines:

| Part | Holds |
|---|---|
| `# <crate>` | the crate's `description` from its `Cargo.toml:4` |
| what it is | two to four sentences on the crate's job, matching the book's four-crates page (`book/src/ch11-03-four-crates.md`) |
| where it sits | the publish order `buildl-core`, `buildl-lua`, `buildl`, `buildl-cli`, said to be publish order, this crate marked, and the crates it depends on by name (`buildl` depends on both `buildl-core` and `buildl-lua`, `crates/buildl/Cargo.toml:14`) |
| links | absolute: the book, `ch11-03-four-crates.html`, this crate's `https://docs.rs/<crate>` (not `buildl-cli`: binary only, `doc = false`, docs.rs has nothing to show), the repository |
| `buildl-cli` only | `cargo install` line as in Task 1, the command list as the names only (`check`, `graph`, `query`, `plan`, `build`, `test`, `verify`, `watch`, `log`, `clean`, `gc`, `doctor`, `plugins fetch`, from `book/src/appendix-01-cli.md`), and a link to the exit-codes page |
| license | Apache-2.0 |

No type inventory (rustdoc on docs.rs holds it), no relative link: the 10 links of the form
`](../…)` today (on 8 lines) lead nowhere on crates.io.

**Proof:**
- `grep -n '](' crates/*/README.md | grep -v '](https://'` prints nothing: every link is absolute.
- `wc -l crates/*/README.md` prints 15–25 for each.
- every book link names a file under `book/src/` that exists (same loop as Task 1).
- `buildl-cli`'s command list equals the subcommands `book/src/appendix-01-cli.md` documents
  (compare the two lists; `watch` and `plugins fetch` included).

**Done when:** the checks above pass, `cargo make dod` and `cargo deny check` are green.
Commit `docs(repo): crate READMEs as short cards`.

## Risks

- **Line budget.** The mandated sections come to about 96 lines laid out loosely. Task 1 keeps
  them tight (the book links inline in one paragraph, the why points as bare lines). If it still
  runs over 80, cut in this order: the console block after the taste, then the `buildl.toml`
  block (keep one line saying the ceiling lives there, linking ch04).
- **The book's site is not live yet.** The `https://airsstack.github.io/buildl/` links work
  once the repository's Pages source is "GitHub Actions" and `pages.yml` runs on `main`. The
  links are right by construction (each names an existing chapter file); the site going live
  is the author's step, not this plan's.
- **docs.rs links** resolve only after the crates are published (the first-publish intent).
  They are the stable URLs crates.io readers expect, so they are written now.
- **Install wording** follows the book's pre-publish `cargo install --git` line; when the
  crates are published, the book and the README change together.

## Proof

- `cargo make dod` exit 0, `cargo deny check` ok, once per task.
- `cargo make book-build` exit 0 (the book is untouched; confirms nothing else moved).
- `git diff --stat <commit before Task 1> HEAD -- book/ examples/ crates/*/src` prints nothing.
- `book/src/appendix-01-cli.md` documents exactly the 13 commands Task 2 lists (its `##`
  headings, lines 51–247).

## Review findings

One `claudestacks:reviewer` pass over `bc4ec83..d527241`, one fix round.

| # | Tier | Finding | Disposition |
|---|---|---|---|
| 1 | 🟡 blocking | `buildl-core` card said all three crates depend on it; `buildl-cli` reaches it only through `buildl` | fixed |
| 2 | 🔵 | `buildl` card listed plugins as an adapter; the book calls it a module with no port | fixed |
| 3 | 🔵 | ch09 link labelled "remote cache"; the chapter also covers remote execution | fixed: "Remote cache and execution" |
| 4 | 🟡 | `buildl-cli` card lacked the ch11-03 and docs.rs links | ch11-03 added; docs.rs left out, the crate is binary only (`doc = false`); Task 2 amended |
| 5 | 🔵 | Task 1 still named the `buildl.toml` block cut under Risks | Task 1 amended |

The author also asked for the Learn more links as a grouped table instead of one paragraph;
Task 1 amended.

## Probe results

None needed: every claim was checked against `book/src/` and the manifests by the plan's own
checks (line counts, chapter files exist, `build.lua` byte-identical, no relative link).

## Deviations

- `README.md` ran 91 lines with the `buildl.toml` block; the block became one line under the
  Risks cut order. Final length 75.
- Gates ran once for both tasks (Markdown only, read by no build step): `cargo make dod` exit 0,
  `cargo deny check` ok.
