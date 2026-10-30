---
status: done
created: 2026-10-30
depends-on: [03, 04]
built-on: f917a9ac0d2d
---

# C and C++ Documentation Implementation Plan

**Goal:** the book, the roadmap, `CLAUDE.md` and the skills describe per-file C and C++ as built
through the `c` plugin, as spec §9 lists.

**Architecture:** one new book chapter, `ch08-04-the-c-plugin.md`, holds the plugin's user guide;
every other page that said per-file C and C++ is unsupported is rewritten to point at it. The
migrate skill moves a per-file Makefile to the plugin and keeps `"judge": "tests"` only for outputs
whose bytes differ. Docs for `dir` and `folder()` are in plans 01 and 02.

**Tech Stack:** mdBook (Mermaid for diagrams in the book), Markdown, the skills' snippet check.

---

## Files that change

```
book/src/ch08-04-the-c-plugin.md            — [create] the plugin's chapter
book/src/SUMMARY.md                         — [modify] the chapter after ch08-03 (:38)
book/src/ch07-00-discovery-targets.md       — [modify] *The depfile gap* (:226-276)
book/src/ch11-09-what-it-promises.md        — [modify] *No per-file C and C++ in this version* (:20-45)
book/src/ch01-04-with-an-ai-agent.md        — [modify] *C and C++, compiled one file at a time* (:161-190)
book/src/ch11-08-where-buildl-sits.md       — [modify] the mindmap node (:175) and the sentence (:183) on fine-grained C
.claudestacks/sdlc/roadmaps/buildl/roadmap.md — [modify] non-goals (:24-27)
CLAUDE.md                                   — [modify] the status note (:9)
skills/buildl/SKILL.md                      — [modify] the line after the language table (:152)
skills/buildl-migrate/SKILL.md              — [modify] § B (:101-105), § E *C and C++ compiled one file at a time* (:199-248)
skills/buildl-migrate/references/make.md    — [modify] *C and C++ compiled one file at a time* (:178-229)
skills/buildl-migrate/references/just.md    — [modify] *C and C++* (:153-156)
skills/buildl-migrate/references/npm.md     — [modify] *C and C++* (:139-143)
skills/buildl-migrate/references/shell.md   — [modify] *C and C++* (:156-159)
```

## Facts this plan relies on

| Fact | Source |
|---|---|
| the pages and line ranges that state the old claim | spec §9; grep over book, skills, CLAUDE.md and the roadmap at commit f0f427d |
| `-g` records the compile folder, so a buildl object differs from the old build's; without `-g` the bytes match | spec P51, P57 |
| the plugin's API, rules, passes, version probe, limits | spec §4–§7, §12 |
| the book uses Mermaid, never ASCII, for diagrams; chapter file names are cited from other files and keep their numbers | CLAUDE.md, *Response style* and *Documents and their roles* |
| the skills' snippet check runs every `lua` block in `skills/**/*.md` through `buildl check`; `lua no-check` is skipped | skills/tests/snippets_lib.lua:9-37; Makefile.toml:433-446 |

## Order of work

### Task 1 — The plugin's chapter

**Files:**
- Create `book/src/ch08-04-the-c-plugin.md` — sections: what it is and where it comes from (the `buildl.toml` of spec §4); a first binary (from `examples/c`); what one call declares (a Mermaid graph of spec §5.2); headers, `header_deps` and the rules of §5.4; the two passes and the platform helpers (§5.5, §6, from `examples/cpp`); shared libraries and tests (§5.3); the version probe and its `plan` cost; limits (one build file per project, Linux and macOS, gcc and clang, the fixed program list)
- Modify `book/src/SUMMARY.md` — `[The C and C++ Plugin](ch08-04-the-c-plugin.md)` after :38

**Change:** the book teaches the plugin.

**Proof:** `cargo make book-build` succeeds; every command and output shown is taken from running `examples/c` and `examples/cpp` with the plan-04 binary.

**Done when:** `cargo make book-build` passes. Commit `docs(repo): add the C and C++ plugin chapter`.

### Task 2 — Retract the old claim in the book, roadmap and CLAUDE.md

**Files:**
- Modify `book/src/ch07-00-discovery-targets.md` — *The depfile gap*: keep the missing-edge diagrams; replace the conclusion that per-file C is left out with how a discovery target running `-MM` before the compile closes the gap, and a link to ch08-04; keep that the executor reads no depfile
- Modify `book/src/ch11-09-what-it-promises.md` — replace *No per-file C and C++ in this version* with a section on per-file C and C++ through the plugin: its limits and the launcher fingerprint (spec P26, §5.2)
- Modify `book/src/ch01-04-with-an-ai-agent.md` — the C/C++ migration section: the agent migrates to the plugin; byte differences under `-g` are judged by tests
- Modify `book/src/ch11-08-where-buildl-sits.md` — move "Fine-grained C — per-file object graphs" (:175) out of *Deferred by decision* into the covered branch as the C plugin over discovery targets, keeping the Mermaid mindmap; rewrite :183's "with fine-grained C left out behind them" to say discovery targets also carry per-file C and C++
- Modify `.claudestacks/sdlc/roadmaps/buildl/roadmap.md` — drop the per-file non-goal (:24-26), keep the depfile one (:27)
- Modify `CLAUDE.md` — the status note (:9): per-file C and C++ is built through the `c` plugin with discovery targets

**Change:** nothing in the book, roadmap or `CLAUDE.md` says per-file C and C++ is unsupported.

**Proof:** `cargo make book-build` succeeds; `grep -rn -i "per-file C\|one file at a time\|fine-grained C\|per-file object" book/src CLAUDE.md .claudestacks/sdlc/roadmaps` finds only text describing the plugin.

**Done when:** both checks pass, `cargo make dod` and `cargo deny check` are green. Commit
`docs(repo): per-file C and C++ is built through the plugin`.

### Task 3 — The skills

**Files:**
- Modify `skills/buildl/SKILL.md` — :152 says one target per toolchain, except C and C++, which use the `c` plugin, with a `lua` example of `c.binary` (checked by the snippet check, so it uses only what `buildl check` accepts without a fetched plugin, or is marked `lua no-check`)
- Modify `skills/buildl-migrate/SKILL.md` — § E: a per-file C/C++ build migrates to the `c` plugin, one object per source, and the old per-file targets map to the plugin's object targets; the whole-program offer is removed; § B (:101-105): `"judge": "tests"` stays for C and C++ outputs whose bytes differ, as with `-g`
- Modify `skills/buildl-migrate/references/make.md` — the same migration in Make terms, with the `-g` case
- Modify `skills/buildl-migrate/references/just.md`, `npm.md`, `shell.md` — their *C and C++* sections point at the plugin and make.md

**Change:** the skills guide an agent to the plugin.

**Proof:** `cargo make skills` passes (format check, Lua tests including the parity tests, snippet check, installer checks).

**Done when:** `cargo make skills` and `cargo make dod` pass. Commit `docs(repo): skills migrate per-file C and C++ to the plugin`.

## Risks

- A `lua` snippet using `buildl.use("c")` fails the snippet check without a fetched plugin; Task 3 marks such blocks `lua no-check` or keeps them to plain declarations.
- The book's chapter shows outputs; they must be copied from real runs of plan 04's examples, not written by hand.
- Not done here: `README.md` holds no per-file note (spec §9 table); `dir` and `folder()` docs (plans 01, 02).

## Proof

`cargo make book-build`, `cargo make skills` and `cargo make dod` green after Task 3.

## Review findings

Task 1 (ch08-04):
- accuracy — the output under "with `version_probe = true`" was the second build's (`1 built, 8 cached`), read as the first; the first prints `2 built, 7 cached` — fixed: both runs shown, matching the captures
- accuracy — the opening said every output came from the examples; the first program and the no-`sh` case came from a small two-source project, the error and probe outputs from edited copies — fixed
- diagrams — `app.version.c` drawn outside the first pass; the sequence diagram's "-MM once each" and "three last steps" — fixed
- accuracy — the `<name>` target row for `c.test` and the archive step; the "anything else" header row; "Every step that links … and each archive's"; the error table missing three §7 cases; "buildl builds on Unix only" as the reason for Linux and macOS — fixed
- outputs — the platform-in-cxxflags block was cut without saying so — fixed: whole real output
- correctness (found in the evidence, outside the chapter) — a plugin fetched from git lost its file name in Lua tracebacks (`.buildl/plugins/c/ce310d…3/:24: in upvalue 'relocate'`): Lua keeps only the start of a `=` chunk name, 59 bytes, and the store path alone fills them — fixed in crates/buildl-lua/src/module.rs: a longer name keeps its end (`...<tail>`); the new long-store-path flow test failed first with `.buildl/plugins/p/0123…4567/:3: boom`, then passed; `cargo make dod` exit 0, `cargo deny check` ok; commit `111eb1d`

Task 2 (book pages, roadmap, CLAUDE.md):
- accuracy — ch07-00 said an undeclared header becomes an input (under the sandbox every header is declared through `headers`); ch11-09 said "needs a plugin" (it needs discovery targets), read "does not cover … Linux and macOS", put the header rule on the source's folder (it is the build file's folder and `headers`), and said `plan` lists probe-dependent compiles conditional "until the probe has run" (always, spec P21) — fixed; the same probe wording in plugins/c/README.md — fixed
- accuracy — the `[plugins.c]` cap left out of "allowed by the ceiling"; "Without -g the bytes match" narrowed to what P51 and P57 prove; "project headers"; "as every example in this book does"; macOS-only launcher; plain-words nits — fixed

Task 3 (skills):
- correctness — the migrate skill put `cc` in the ceiling, failing its own check 6 for a Makefile that runs `gcc` — fixed: name the compiler the old build runs and set the call's `cc`/`cxx`
- completeness — a node-gyp header outside the workspace needs `-isystem` — fixed in make.md and npm.md
- cross-reference — the skills named the chapter before it existed — closed by Task 1 (same title)

## Probe results

- every command output in ch08-04 is from a run of target/debug/buildl on macOS (Apple clang): examples/c and examples/cpp copied and prepared through their `prepare.sh`; a small two-source project; edited copies for the error and version-probe outputs; the reviewer checked each console block against those captures
- a header left out of `headers` fails at the `-MM` discovery under the sandbox (the compiler cannot find it), before the plugin's own check; the chapter shows that failure and lists the plugin's message in its rules table
- `cargo make book-build` green after each round; `cargo make skills` green after Task 3 and its fix round; the plan's grep over book/src, CLAUDE.md and the roadmaps finds only text describing the plugin

## Deviations

- 2026-10-30 — plan, Task 1: the first program and the case without `sh` come from a small two-source project rather than examples/c, since the chapter starts from `proc.run = ["cc"]` (spec §4) and examples/c needs `sh`; the version-probe and error outputs come from edited copies of the examples
- 2026-10-30 — plan, Task 2: the dangling link to ch08-04 was written before Task 1 and committed after it, in plan order; `mdbook` does not fail on it
- 2026-10-30 — outside the plan: the traceback chunk-name fix in crates/buildl-lua (Review findings, Task 1) was closed in this run rather than carried forward
