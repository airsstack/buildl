---
status: executing
created: 2026-10-28
depends-on: [01]
built-on: 053dd1fe3030
---

# Build Files and Permission Chapters Implementation Plan

**Goal:** Chapters 3 and 4 of the book teach how a build file declares targets and how buildl
decides what those targets may use.

**Architecture:** This plan fills stubs that plan 01 created and changes no other file. It
follows the writing rules every chapter plan shares:
- **Voice.** Plain words, in the voice of airsl's book (`/Users/hiraq/Projects/airsstack/airsl/book/src/ch03-01-nothing-by-default.md` is the model).
- **Real transcripts.** Every shell transcript comes from the debug binary built at the branch head (`cargo build -p buildl-cli`), run in a scratch copy of a workspace, never typed from memory.
- **The binary decides.** Where `docs/` and the binary disagree, the book says what the binary does.
- **Diagrams.** This plan owns the `docs/` Mermaid diagrams listed under its `## Proof`; each is carried into its chapter as a ` ```mermaid ` block, redrawn where the binary has moved on.
- **A command that disagrees with its `--help`** is raised to the user, not written around.
- **Links.** Chapters link to each other with relative `.md` links, and the book never uses `§`.

**Tech Stack:** Markdown, mdBook 0.4.52, the `buildl` debug binary, Go 1.22, Rust, Python 3 (for `examples/polyglot`).

---

## Files that change

```
book/src/ch03-00-build-files.md          — [modify] chapter opener
book/src/ch03-01-targets-and-labels.md   — [modify] target/test/rule/alias, labels, graph, query
book/src/ch03-02-inputs-outputs-deps.md  — [modify] $in/$out/$deps, sources, the input rule
book/src/ch03-03-toolbox.md              — [modify] the airsstack modules
book/src/ch03-04-many-directories.md     — [modify] subdir, cross-directory labels, polyglot
book/src/ch03-05-build-settings.md       — [modify] buildl.option and --set
book/src/ch04-00-asking-permission.md    — [modify] chapter opener
book/src/ch04-01-nothing-is-ambient.md   — [modify] the wanted set, tools as deps, untrusted clone
book/src/ch04-02-the-ceiling.md          — [modify] buildl.toml's ceiling tables
book/src/ch04-03-approvers.md            — [modify] manifest, interactive, deny; doctor
```

## Order of work

### Task 1 — Targets and labels

**Files:** Modify `book/src/ch03-00-build-files.md` and `book/src/ch03-01-targets-and-labels.md`.

**Change:**

The opener lists the sections and says what the build file holds:
- the two globals, `buildl` and `airsstack`, which are one table bound twice;
- `print`, `math.random` and `math.randomseed` removed. `crates/buildl-lua/src/module.rs:104-109` shows what is removed.

Targets and Labels covers:
- `buildl.target`, `buildl.test`, `buildl.rule` and `buildl.alias`. These are installed at `module.rs:139-152`, `:161` and `:170`. Each option they take is read from the parsing code they call, not from `docs/`.
- Labels in the form `//dir:name`.
- Relative labels.
- A directory-only `//dir` label being rejected, shown with its real error. The rule comes from `docs/design.md:1040-1044`.
- `buildl graph`.
- `buildl query 'deps(…)'` and `'rdeps(…)'`.

Sources: `docs/design.md:199-243` (§5) and `docs/architecture.md:44-183` (§2, for the label rules).

**Proof:**
- Each transcript is captured from a scratch workspace that declares the example targets.
- The rejected `//dir` label is captured with its real message and exit code.
- `mdbook build book` exits 0.

**Done when:** all of these hold. Commit `docs(repo): book chapter 3, targets and labels`.

### Task 2 — Inputs, outputs, dependencies and the toolbox

**Files:** Modify `book/src/ch03-02-inputs-outputs-deps.md` and `book/src/ch03-03-toolbox.md`.

**Change:**

Inputs, Outputs and Dependencies covers:
- `inputs`, `outputs` and `deps`, and the `$in`, `$out` and `$deps` substitutions;
- `buildl.sources`, at `module.rs:198`;
- `out/`;
- the rule that an input stays under its declaring directory, at `crates/buildl-core/src/load/traversal.rs:183-190`, shown with its real error.

The Toolbox covers the `airsstack` modules a build file gets: `json`, `path`, `regex`, `hash` and `glob`. Check that list against what the build-file environment actually installs before writing. Give each module one short example taken from a working build file.

Sources: `docs/design.md:60-91` (§3) and `:199-243` (§5).

**Proof:**
- Every transcript is captured.
- The toolbox's module list matches what a build file sees. Confirm it by running a build file that prints `airsstack`'s keys through an error message, or by reading `crates/buildl-lua/src` and citing `file:line`.
- `mdbook build book` exits 0.

**Done when:** all of these hold. Commit `docs(repo): book chapter 3, inputs, outputs and the toolbox`.

### Task 3 — Many directories and build settings

**Files:** Modify `book/src/ch03-04-many-directories.md` and `book/src/ch03-05-build-settings.md`.

**Change:**

Many Directories:
- `buildl.subdir`, at `module.rs:188`. It is explicit only: a build file is evaluated only because another named its directory (`docs/design.md:1040-1044`).
- Labels across directories.
- `examples/polyglot` walked through: the schema generator, the three programs, and `buildl query 'rdeps(//schema:gen)'`.

Build Settings:
- `buildl.option`, at `module.rs:179`, and `--set NAME=VALUE`.
- One worked example that changes an option and shows `plan` naming it.

Sources: `docs/design.md:615-629` (§9.5) and `:797-817` (§10.2).

**Proof:**
- The polyglot transcripts are captured in a scratch copy of `examples/polyglot`.
- The build-settings example is captured.
- `mdbook build book` exits 0.

**Done when:** all of these hold. Commit `docs(repo): book chapter 3, directories and settings`.

### Task 4 — Nothing is ambient

**Files:** Modify `book/src/ch04-00-asking-permission.md` and `book/src/ch04-01-nothing-is-ambient.md`.

**Change:**

The opener lists the sections.

Nothing Is Ambient covers:
- the wanted set: authority is derived from the build graph and intersected with the ceiling, never requested per file;
- tools are dependencies too: a program that a dependency built runs with no `proc.run` entry;
- the untrusted-clone use case.

Sources: `docs/design.md:244-278` (§6), `:630-662` (§9.6), and `docs/fundamental-walkthrough.md:103-131` (§4), along with their Mermaid diagrams.

**Proof:**
- A transcript shows a build refused because its program is outside the ceiling, with exit code 4.
- A transcript shows a program built by a dependency running without a `proc.run` entry.
- `mdbook build book` exits 0.

**Done when:** all of these hold. Commit `docs(repo): book chapter 4, nothing is ambient`.

### Task 5 — The ceiling and approvers

**Files:** Modify `book/src/ch04-02-the-ceiling.md` and `book/src/ch04-03-approvers.md`.

**Change:**

`buildl.toml`, the Ceiling covers every table that bounds a build. Read the tables and keys from the schema `buildl.toml` is parsed into, `crates/buildl/src/manifest/schema.rs:63` (`Raw`, `deny_unknown_fields`) and its tables at `:91-221`, not from `TomlManifest`, which holds interpreted values; `docs/design.md:92-198` (§4) is background only. Give each table one example. This page explains; the exhaustive key list is appendix B (plan 07).

Approvers covers:
- `[approver]` with `manifest` (the default), `interactive` and `deny`;
- that `interactive` asks once on a terminal and remembers a yes outside the workspace;
- `buildl doctor` printing the ceiling, the approver, the execution settings and every want.

Sources: `docs/design.md:244-278` (§6).

**Proof:**
- Transcripts are captured for `deny` refusing and for `doctor`.
- `interactive` cannot be captured without a terminal, so describe it from `crates/buildl/src/approver/`, cited by `file:line`.
- `mdbook build book` exits 0.

**Done when:** all of these hold. Commit `docs(repo): book chapter 4, the ceiling and approvers`.

## Risks

- Some option names in `docs/design.md` §5 may have changed since it was written. The parsing code decides, and Tasks 1 and 2 read it.
- The `interactive` approver cannot be shown with real output in a non-terminal capture. Task 5 describes it from the source and does not show a transcript.

## Proof

- `cargo make book-build` exits 0.
- The 3 `docs/` Mermaid diagrams this plan owns each appear as a ` ```mermaid ` block in the chapter named, and each such page renders `<pre class="mermaid">` in `book/book/`; open each page once in a browser (`cargo make book`) and confirm the diagram draws, not an error box: `docs/design.md:248` → `ch04-01`, `docs/design.md:617` → `ch03-04`, `docs/fundamental-walkthrough.md:109` → `ch04-01`.
- These spec §3 rows land in this plan; mark each one in the execution record:
  - `docs/design.md`: §3 (ch03-02), §4 (ch04-02), §5 (ch03-01 to ch03-03), §6 (ch04-01, ch04-03), §9.5 (ch03-04), §9.6 (ch04-01), §10.2 (ch03-05), and §14's three settled rules (ch03-01, ch03-04; the plugin API rule goes to plan 05).
  - `docs/fundamental-walkthrough.md`: §4 (ch04-01).

## Review findings

- close-out — the execution record was missing; written here. Fixed.
- nit — `ch03-01:171` and `ch04-01:35` ran past 100 characters; wrapped. Fixed.
- nit — `ch03-03:109` stray blank line inside a console block; removed. Fixed.
- nit — `ch04-03:66` `<workspace>.json` read as the workspace's name; the file is named from the
  root path's bytes (`crates/buildl/src/approver/approvals.rs:38-40`); now "a JSON file named for
  the workspace's location". Fixed.
- nit — `ch04-01:19`, `ch04-02:68` said a build writes into `out`; now the output directory,
  `out` unless `[workspace] out` names another. Fixed.
- nit — `ch03-01:123` "canonical JSON" → "JSON with sorted keys, the same bytes every time". Fixed.

## Probe results

- `//lib` as a dep: `error: build.lua: declaration #2: invalid dep: invalid label: "//lib" — must
  hold ':' and a target name`, exit 5. Re-run by the reviewer: same.
- Input rule: `"lib/../README" — must not contain a '.' or '..' segment`, exit 5.
- Toolbox: a build file raising `airsstack`'s sorted keys gives `buildl, glob, hash, json, path,
  regex; one table: true`; source `crates/buildl-lua/src/engine.rs:29-34`. `glob.walk` and
  `hash.hash_file` are refused ("none are granted").
- `examples/polyglot` copy: `rdeps(//schema:gen)` gives the three programs; `build` → `4 built`.
  An unnamed `extra/build.lua` is not read.
- `plan --set greeting=goodbye` → `run //:message: setting greeting changed`; an undeclared `--set`
  exits 2.
- `curl` outside the ceiling: `program curl is outside the ceiling (proc.run allows: go)`, exit 4
  from `plan` and `build`. A program a dependency built runs with only `go` in `proc.run`:
  `2 built`. Re-run by the reviewer: same.
- `[approver] kind = "deny"`: `build` exit 4; `doctor` exit 0. `interactive` with no terminal: exit
  4, "no terminal to ask on"; the rest described from `crates/buildl/src/approver/` and
  `crates/buildl-cli/src/console.rs`.
- Diagrams: `ch03-04` ×1, `ch04-01` ×2 render `<pre class="mermaid">`; opened in Chrome from a
  local build, each is an SVG with no error box (nodes 7, 5/12).
- Spec §3 rows landed: design §3 → ch03-02; §4 → ch04-02; §5 → ch03-01 to ch03-03; §6 → ch04-01,
  ch04-03; §9.5 → ch03-04; §9.6 → ch04-01; §10.2 → ch03-05; §14's label rule → ch03-01 and subdir
  rule → ch03-04; walkthrough §4 → ch04-01.

## Deviations

- 2026-10-28 — the binary differs from `docs/`, and the book follows the binary: `graph` prints
  JSON by default; a rule's `desc` shows only in the live status line; going back to a setting's
  default runs the target again; refusals name the file and target, with no line number.
- 2026-10-28 — `docs/fundamental-walkthrough.md:109`'s diagram is redrawn with the binary's real
  refusal message and exit 4, and without the unused approver participant.
- 2026-10-28 — transcripts that print paths are captured under `/tmp/buildl-book/` with `TMPDIR`
  set there, so the book shows no local user or session path.
