---
status: done
created: 2026-10-28
depends-on: [01]
built-on: 136d5b098d7e
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

## Review findings

- wrong fact — `ch05-03:3-4` said a test target has no outputs; `b.test` accepts `outputs`
  (`TEST_FIELDS`, `crates/buildl-lua/src/primitives.rs`). Now "no outputs unless it names some".
  Fixed.
- wrong fact — `ch07-00:206` listed `verify` among commands that never run a discovery target,
  but `buildl verify //:pkgs` re-runs it. Now "never run a discovery target to get the targets it
  declares". Fixed.
- nit — `ch05-01:148,164` "the digest of each output" read as a file hash; now says it covers the
  name, a hash of the bytes and the executable bit (`Produced::digest`). Fixed.
- nit — `ch05-02:127` "other columns" → "other programs". Fixed.
- nit — `sed -i ''` is the macOS form; `ch05-01` now says so once for the chapter. Fixed.
- nit — `ch05-04:58` "same inputs give the same command" → "the same key, and so a cache hit".
  Fixed.
- nit — the `ch07` diagram carried from `docs/architecture.md:380` used internal terms
  ("negotiate", "record graph", `DiscoveryChanged`); redrawn in plain words with exit codes 3 and
  5 (`crates/buildl-cli/src/exit.rs:31,35`). Fixed.

## Probe results

- Plain `plan` after editing `main.go`: `run    //:hello: input main.go changed`; `plan --json`:
  `"reason": {"Changed":[{"Input":"main.go"}]}`.
- `examples/polyglot` comment edit: `plan` → `1 to run, 3 conditional, 0 clean` with `maybe` rows;
  `build` → `1 built, 3 cached` (as `examples/polyglot/assert.sh` asserts). Re-run independently by
  the reviewer: same.
- `examples/go`: `tests: 1 passed`, then `tests: 1 passed (1 cached)`; `clean: removed out`;
  `gc: dropped 1 row, removed 4 blobs (4059395 bytes)`; `verify` → `1 verified, 0 mismatched`; a
  stamp target → `MISMATCH //:stamp.txt: output stamp.txt differs: bytes`; `--sample 1` →
  `1 not sampled`.
- Sandbox: an undeclared `notes.txt` read fails with `FileNotFoundError` in the sandbox and gives
  `1 built` under `bare`. `strategy = "container"` / `"os-sandbox"`: `strategy is "container", and
  this version of buildl runs "bare" and "sandbox" only: write strategy = "sandbox" under
  [execution]`, exit 5.
- `examples/go-discovery` after adding package `shout`: `tests: 4 passed (3 cached)`, as expected.
- Diagrams: ch05-01 ×2, ch05-02 ×1, ch06 ×1, ch07 ×4 each render `<pre class="mermaid">`; opened
  in Chrome from a local build, every one is an SVG with no error box (nodes 10/11, 5, 4,
  5/11/13/6).
- Spec §3 rows landed: design §7 → ch05-03, ch05-04; §8.3 → ch05-01; §10 (discovery) → ch07;
  §12.2, §12.3 → ch06; architecture §3.6 → ch07; walkthrough §3 → ch05-01, §5 → ch05-02,
  §6 → ch07.

## Deviations

- 2026-10-28 — transcripts are captured under `/tmp/buildl-book/` with `HOME` and `TMPDIR` set
  there, so the book shows no local user or session path. The neutral `HOME` holds links to the
  real Go SDK and Rust toolchains.
- 2026-10-28 — diagrams redrawn: walkthrough:76 for `main.go` and `//:hello`; walkthrough:136 as
  the real polyglot graph; design:949 with the strategy names and which are refused today;
  architecture:380 in plain words (review).
- 2026-10-28 — `ch05-01` (219 lines), `ch06` (217) and `ch07` (276) run past the ~150-line guide,
  mostly from the full `plan --json` and ch07's four diagrams.
- 2026-10-28 — CI run 40698998488 (after these chapters) failed `Examples (ubuntu-latest)`:
  `MISMATCH //:app.pyz: output app.pyz differs: bytes`. Cause: `fs::copy` stamps the copy time on
  Linux (macOS keeps the source's), and `zipapp` records file and directory times. By the user's
  choice the sandbox now gives every copied file and directory `Isolation::BUILD_EPOCH`
  (`1903a28`), and `ch06` says so (`668b9eb`). Proven: `copies_carry_the_pinned_build_time` failed
  first (`tools/run.sh modified`), then passed; `cargo make dod` exit 0, 2830 passed; the Python
  example's archive entries read `(1980, 1, 1, 0, 0, 0)` and `verify` reports 0 mismatched. A
  review pass found directories uncovered and a tar claim too broad; both fixed.
- 2026-10-28 — CI run 40705859653 failed `Definition of Done (ubuntu-latest)`:
  `copies_carry_the_pinned_build_time` saw `tools accessed` `1793197624` against `315532800`.
  On Linux, reading a directory whose access time is older than its change time moves the access
  time (relatime), so the sandbox's own walk moved it. The sandbox now pins modification times
  only (`6e5cb12`); `cargo make dod` exit 0, 2830 passed. CI run 40708080656 at `6e5cb12`: all
  10 jobs green, `Book`, both `Examples` and both `Definition of Done` included.
