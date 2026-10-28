---
status: done
created: 2026-10-28
depends-on: [02, 03, 04, 05, 06, 07]
built-on: 136d5b098d7e
---

# Repoint and Remove `docs/` Implementation Plan

**Goal:** No file outside the chain history refers to `docs/`, a spec document or a spec `§` number,
and `docs/` no longer exists.

**Architecture:** This plan runs only after every chapter has landed. Each citation is changed to
the book chapter file that now holds its content. The chapter files keep the old rule numbers:
`ch11-03-four-crates.md` keeps the dependency rules, and `ch11-05-determinism-rules.md` keeps the
determinism rules. Plan 06 fixes both. So a citation such as "rule 2" stays true. Edits to
`README.md` and `CLAUDE.md` are the smallest that remove each reference; the readmes chain rewrites
both files later. `docs/` is deleted last, after a search proves nothing points at it.

**Tech Stack:** TOML comments, Markdown, cargo-make, cargo-deny.

---

## Files that change

```
Cargo.toml                       — [modify] 7 comment lines citing architecture.md / design.md
deny.toml                        — [modify] 10 lines of ban reasons citing ABB / architecture.md
Makefile.toml                    — [modify] 3 lines citing architecture-building-blocks / architecture.md
crates/buildl-core/clippy.toml   — [modify] 9 lines citing ABB §1.2 / architecture.md §5, §6
README.md                        — [modify] the 7 citing lines (:7, :246-249, :256, :269) and :245
CLAUDE.md                        — [modify] 12 citing lines plus :92
docs/                            — [delete] the four documents
```

Per-file counts come from `grep -c` with the spec §1 pattern, run on 2026-10-28.

## Order of work

### Task 1 — Repoint the build and lint configuration

**Files:** Modify `Cargo.toml`, `deny.toml`, `Makefile.toml`, `crates/buildl-core/clippy.toml`.

**Change:** Each citation names the book chapter file instead of the old document:

| Old citation | New citation |
|---|---|
| `architecture.md §5 rule N` | `book/src/ch11-05-determinism-rules.md rule N` |
| `architecture.md §6` / `decision record §6` | `book/src/ch11-06-decision-records.md` |
| `architecture.md §4` | `book/src/ch11-04-errors.md` |
| `architecture.md §3.5` | `book/src/ch11-02-crash-safety.md` |
| `architecture-building-blocks.md §1.2`, `ABB §1.2`, `§4`, `§6`, `§7 rule N` | `book/src/ch11-03-four-crates.md` (with "dependency rule N" for `§7 rule N`) |
| bare `(design.md)` at `Cargo.toml:57` | the chapter that holds the point it backs; read the surrounding comment to pick it |

Every line is a comment or a string, so no behaviour changes. In `deny.toml`, the `reason` strings change. In `Makefile.toml`, a comment (`:216`) and the `echo` messages of two guard tasks (`:284`, `:341`) change.

**Proof:**
- `grep -n "§\|design\.md\|architecture\.md\|architecture-building-blocks\|ABB" Cargo.toml deny.toml Makefile.toml crates/buildl-core/clippy.toml` prints nothing.
- Every `book/src/…` file named in the four files exists. Check each with `ls`.
- Each `rule N` cited is rule N on that page.
- `cargo make dod` is green.
- `cargo deny check` is green.

**Done when:** all hold. Commit `docs(repo): cite the book from build and lint configuration`.

### Task 2 — Repoint `README.md` and `CLAUDE.md`

**Files:** Modify `README.md` and `CLAUDE.md`.

**Change:**

`README.md`:
- The links to `docs/*.md` (`:7` and the other citing lines) point at the matching `book/src/*.md` files.
- The tree listing's `docs/` entry and its four document entries (`:245-249`) become a `book/` entry holding `book.toml` and `src/`.
- "from [`design.md` §13](./docs/design.md)" (`:256`) becomes a link to `.claudestacks/sdlc/roadmaps/buildl/roadmap.md`.
- "open an issue against `docs/design.md` or `docs/architecture.md`" (`:269`) becomes "open an issue".
- The Bazel URL at `:284` stays.

`CLAUDE.md`:
- "Documents and their roles" (`:13-16`) names the book and its parts in place of the four documents: the user chapters, Under the Hood, and the appendix.
- `:9` cites `book/src/ch07-00-discovery-targets.md` for the depfile reasoning and `book/src/ch11-09-what-it-promises.md` for the scope note.
- `:17`: "design §13 items 1–8" becomes "the milestones `01`–`08`".
- `:20` becomes this rule: chapter files under `book/src/` are cited by name from `Cargo.toml`, `deny.toml`, `Makefile.toml` and `clippy.toml`, and numbered rules keep their numbers; keep both when restructuring the book.
- `:47` becomes: the book's references are `book/src/appendix-05-references.md`, and new content cites into that list.
- `:92`, the "Chat ASCII, document Mermaid" rule, names `book/` instead of `docs/`.
- Any other line of `CLAUDE.md` that names `docs/` or a spec `§` becomes the matching chapter file.

Nothing else changes in either file.

**Proof:**
- `grep -n "§\|\(^\|[^/]\)docs/\|design\.md\|architecture\.md\|architecture-building-blocks\|fundamental-walkthrough" README.md CLAUDE.md` prints nothing.
- Every relative link added resolves to an existing file. Check each with `ls`.

**Done when:** both hold. Commit `docs(repo): point README.md and CLAUDE.md at the book`.

### Task 3 — Remove `docs/`

**Files:** Delete `docs/architecture-building-blocks.md`, `docs/architecture.md`, `docs/design.md`, `docs/fundamental-walkthrough.md`.

**Change:** First confirm that the execution records of plans 02–07 mark every row of the spec §3 table. Then `git rm -r docs`.

**Proof:**
- The spec §6 check finds only `crates/buildl-core/src/types/changed_path.rs`:

  ```
  grep -rIl "§\|\(^\|[^/]\)docs/\|design\.md\|architecture\.md\|architecture-building-blocks\|fundamental-walkthrough\|ABB" --exclude-dir=target --exclude-dir=.git --exclude-dir=.claudestacks --exclude-dir=.claude --exclude-dir=docs --exclude-dir=book .
  ```

  On 2026-10-28, before any change, the same command found exactly `Cargo.toml`, `CLAUDE.md`, `crates/buildl-core/clippy.toml`, `crates/buildl-core/src/types/changed_path.rs`, `deny.toml`, `Makefile.toml` and `README.md`. That run is the control.
- The same pattern over the book's sources, `grep -rIl "§\|\(^\|[^/]\)docs/\|design\.md\|architecture\.md\|architecture-building-blocks\|fundamental-walkthrough\|ABB" book/src`, prints nothing. `book/` is kept out of the repo-wide search because the vendored `book/mermaid.min.js` holds a `§` (`.replace(/§§§/g,",")`).
- `grep -c '```mermaid' book/src/*.md` totals at least 38, one per diagram of `docs/` (plans 02–06 list them).
- `test ! -e docs` succeeds.
- `cargo make book-build` exits 0.

**Done when:** all hold. Commit `docs(repo): remove docs/, now the book`.

## Risks

- A chapter plan may have moved a numbered rule, which would make a "rule N" citation false. Task 1 checks each cited rule against its page.
- `CLAUDE.md` is the project's own instruction file. Its edits stay within what this plan names, and the readmes chain does the rewrite.
- `cargo make dod` reruns the whole workspace gate for comment-only edits. It runs once, in Task 1, as the repository requires before a commit that touches Rust-adjacent configuration.

## Proof

- The spec §6 search finds only `changed_path.rs`.
- `cargo make dod`, `cargo deny check` and `cargo make book-build` are each green once.
- One CI run on the branch (`gh workflow run ci.yml --ref worktree-prep-publish`) shows every job green, the `Book` job included.

## Review findings

- wrong citation — `Cargo.toml:57` cited `ch11-05` for sorted `b.sources()` results; ch11-05 does
  not say it. Now cites `book/src/appendix-03-lua-api.md` ("relative to the folder, sorted").
  Fixed.
- nit — `CLAUDE.md:18` still called `README.md` a summary of "the design and architecture
  documents"; now "the book". Fixed.
- nit — `CLAUDE.md`'s book description left out the introduction and ch02 "A First Project".
  Fixed.
- nit — the longer citations pushed `Cargo.toml:30,44`, `clippy.toml:4` and `Makefile.toml:219`
  past the surrounding comment width; rewrapped. Fixed.

## Probe results

- Task 1 grep over `Cargo.toml`, `deny.toml`, `Makefile.toml`, `crates/buildl-core/clippy.toml`
  prints nothing; the five `book/src/` files they cite exist; every "rule N" / "dependency rule N"
  matches its page (ch11-03 dependency rules 1–5, ch11-05 determinism rules 1–5, unchanged order).
- `cargo make dod` exit 0 and `cargo deny check` ok, after Task 1 and again after the review fix.
- Task 2 grep over `README.md` and `CLAUDE.md` prints nothing; all added links resolve.
- Before `git rm`, every spec §3 row was found marked in the records of plans 02–07.
- Task 3: repo-wide search finds only `crates/buildl-core/src/types/changed_path.rs`; the
  `book/src` search prints nothing; 38 ` ```mermaid ` blocks in `book/src`; `test ! -e docs`
  succeeds; `cargo make book-build` exit 0.
- No other path in the repo (workflows, Makefile tasks, crate manifests, rustdoc) referred to
  `docs/` (reviewer).

## Deviations

- 2026-10-28 — `CLAUDE.md:18` (the `README.md` line) changed though Task 2 did not name it: it
  pointed at the deleted documents.
- 2026-10-28 — CI run 40717680408 at `c5776e1`: all 10 jobs green, `Book` included.
