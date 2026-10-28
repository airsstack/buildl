---
status: approved
created: 2026-10-28
depends-on: [01]
built-on: 053dd1fe3030
---

# Caching, Sandbox and Discovery Chapters Implementation Plan

**Goal:** Chapters 5–7 of the book explain how buildl decides what to run again, where it runs
actions, and how a build learns its inputs from a tool.

**Architecture:** This plan fills stubs that plan 01 created and changes no other file. It
follows the writing rules every chapter plan shares:
- **Voice.** Plain words, in the voice of airsl's book.
- **Real output only.** Every shell transcript comes from the debug binary built at the branch head (`cargo build -p buildl-cli`), run in a scratch copy of a workspace. Nothing is typed from memory.
- **The binary decides.** Where `docs/` disagrees with the binary, the book says what the binary does.
- **Diagrams.** This plan owns the `docs/` Mermaid diagrams listed under its `## Proof`; each is carried into its chapter as a ` ```mermaid ` block, redrawn where the binary has moved on.
- **A command that disagrees with its `--help`** is raised to the user, not written around.
- **Links.** Chapters link with relative `.md` links. No `§` sign appears in the book.

**Tech Stack:** Markdown, mdBook 0.4.52, the `buildl` debug binary, Go 1.22, Rust, Python 3.

---

## Files that change

```
book/src/ch05-00-caching.md            — [modify] chapter opener
book/src/ch05-01-action-key.md         — [modify] what the key folds; plan explaining dirtiness
book/src/ch05-02-early-cutoff.md       — [modify] same bytes, clean downstream; polyglot comment edit
book/src/ch05-03-tests.md              — [modify] test, cached tests
book/src/ch05-04-clean-gc-verify.md    — [modify] clean, gc, verify
book/src/ch06-00-sandbox.md            — [modify] isolation ladder, tier 2, bare, refused tiers
book/src/ch07-00-discovery-targets.md  — [modify] discover/discovered, stdout, decode_all, depfile gap
```

## Order of work

### Task 1 — The action key and early cutoff

**Files:** Modify `book/src/ch05-00-caching.md`, `book/src/ch05-01-action-key.md`, `book/src/ch05-02-early-cutoff.md`.

**Change:**
- **Opener:** lists the chapter's sections.
- **The Action Key** covers:
  - what the SHA-256 key folds: the command, input hashes, each dependency's output hashes, env values, tool fingerprints, settings and os/arch;
  - that a key folds what a dependency produced, not the dependency's key;
  - `buildl plan` explaining a dirty target, using its `--json` and plain forms;
  - `docs/fundamental-walkthrough.md`'s concrete trace, re-run on a real target.
- **Early Cutoff** covers:
  - a dependency rebuilt to the same bytes leaves downstream targets clean;
  - a downstream target is *conditional* until the output it depends on is known;
  - `examples/polyglot`'s comment edit, which gives `1 built, 3 cached`.

Sources: `docs/design.md:412-435` (§8.3), `docs/fundamental-walkthrough.md:63-102` (§3) and `:132-156` (§5), with their Mermaid diagrams.

**Proof:**
- The `plan` transcripts before and after an edit are captured.
- The polyglot comment-edit transcript is captured, and it shows `1 built, 3 cached` as `examples/polyglot/assert.sh` asserts.
- `mdbook build book` exits 0.

**Done when:** all three hold. Commit `docs(repo): book chapter 5, the action key and early cutoff`.

### Task 2 — Tests, clean, gc, verify

**Files:** Modify `book/src/ch05-03-tests.md`, `book/src/ch05-04-clean-gc-verify.md`.

**Change:**
- **Tests:** `buildl test`, test targets cached by key, and the `tests: N passed (M cached)` line.
- **clean, gc and verify:**
  - `buildl clean` removes `out/`;
  - `buildl gc` drops the rows of undeclared targets and removes what nothing needs;
  - `buildl verify`, and `--sample N`, re-run cached actions in the sandbox and name mismatches; `verify` runs only under `strategy = "sandbox"` and records nothing.

Sources: `docs/design.md:279-390` (§7) and `:979-1000` (§12.3).

**Proof:**
- Transcripts are captured for each of the four commands on `examples/go` or `examples/python`.
- `mdbook build book` exits 0.

**Done when:** both hold. Commit `docs(repo): book chapter 5, tests, clean, gc and verify`.

### Task 3 — The sandbox

**Files:** Modify `book/src/ch06-00-sandbox.md`.

**Change:**
- The isolation ladder, with tier 2, the input sandbox, as the default. It runs in a directory outside the workspace that holds copies of the declared inputs only, with `TZ`, `LC_ALL` and `SOURCE_DATE_EPOCH` pinned.
- `strategy = "bare"` (tier 1).
- `os-sandbox` and `container` are refused; show the real error.
- Guarantee, mitigate, verify, with `verify` as the check.

Sources: `docs/design.md:947-1000` (§12.2, §12.3), with their diagrams.

**Proof:**
- A transcript shows an action that reads an undeclared file failing in the sandbox, and passing under `bare`.
- A transcript shows `strategy = "container"` being refused.
- `mdbook build book` exits 0.

**Done when:** all three hold. Commit `docs(repo): book chapter 6, the sandbox`.

### Task 4 — Discovery targets

**Files:** Modify `book/src/ch07-00-discovery-targets.md`.

**Change:**
- Why a declaration cannot run a tool.
- `buildl.discover` and `buildl.discovered`, at `crates/buildl-lua/src/module.rs:142` (the `discover` primitive) and `:211`.
- The second declaration pass, run first by `build` and `test`.
- `stdout = true`.
- `buildl.decode_all`, at `module.rs:214`.
- `examples/go-discovery` walked through, including adding a package and seeing a new test target.
- The depfile gap, and why discovery targets are buildl's answer to it.

Sources: `docs/design.md` §10's discovery text (`:663-758`), `docs/architecture.md:359-398` (§3.6), and `docs/fundamental-walkthrough.md:157-208` (§6), with its diagram.

**Proof:**
- The transcripts are captured in a scratch copy of `examples/go-discovery`.
- They include a run after adding a package, quoting the `tests:` line it prints (expected to count one more test, all but the new one cached; the capture decides).
- `mdbook build book` exits 0.

**Done when:** both hold. Commit `docs(repo): book chapter 7, discovery targets`.

## Risks

- `fundamental-walkthrough.md`'s action-key trace shows illustrative digests. The book shows a real `plan --json` instead, so the numbers will differ from the old document.
- A sandbox-escape demonstration depends on the host. Keep it to reading a file that was not declared, which the sandbox reliably leaves out.

## Proof

- `cargo make book-build` exits 0.
- The 8 `docs/` Mermaid diagrams this plan owns each appear as a ` ```mermaid ` block in the chapter named, and each such page renders `<pre class="mermaid">` in `book/book/`; open each page once in a browser (`cargo make book`) and confirm the diagram draws, not an error box: `docs/design.md:416` and `docs/fundamental-walkthrough.md:76` → `ch05-01`; `docs/fundamental-walkthrough.md:136` → `ch05-02`; `docs/design.md:949` → `ch06`; `docs/design.md:368`, `docs/architecture.md:380`, `docs/fundamental-walkthrough.md:166` and `:187` → `ch07`.
- The spec §3 rows this plan lands. Mark each in the execution record.

  | Source | Section | Chapter |
  |---|---|---|
  | design | §7 | ch05-03, ch05-04 |
  | design | §8.3 | ch05-01 |
  | design | §10 (discovery) | ch07 |
  | design | §12.2 | ch06 |
  | design | §12.3 | ch06 |
  | architecture | §3.6 | ch07 |
  | walkthrough | §3 | ch05-01 |
  | walkthrough | §5 | ch05-02 |
  | walkthrough | §6 | ch07 |
