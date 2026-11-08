---
status: done
created: 2026-11-07
built-on: 16aef0025c8b
---

# Where Actions Run Implementation Plan

**Goal:** Make the book's account of the sandbox folder hold across workspaces and machines.

**Architecture:** Four book pages change, each in its own task and commit. The remote chapter
(`ch09-00`) comes first, because its new section "Who should upload" is the anchor the other three
link to; then the promises page (`ch11-09`) loses its wrong claim and gains a limit; the sandbox
chapter (`ch06-00`) says where the folder is and what that means; the action key page (`ch05-01`)
links a dependency built elsewhere to that section. No new
console output is printed, so nothing needs capturing from a run; a picture is a Mermaid diagram.
No code, setting or test changes. `README.md`, `CLAUDE.md` and `skills/` do not repeat the wrong
claim (`grep -rn -i "sandbox's directory" README.md CLAUDE.md skills book/src` matches only
`book/src/ch11-09-what-it-promises.md:99`), so they do not change.

The folder's layout the pages describe is today's code: `crates/buildl/src/state/scratch.rs:56-65`
(`Scratch::new` joins `buildl-<name_of(workspace root)>` onto the temporary directory),
`scratch.rs:74-77` (`Scratch::action` joins `name_of(label)`), `scratch.rs:5-8` (the action's
folder name depends only on its label, so it is the same on every run),
`crates/buildl/src/local_ports.rs:104` (the temporary directory is `std::env::temp_dir()`), and
`crates/buildl/src/state/name.rs:16-22` (`name_of` is a truncated SHA-256 digest).

**Tech Stack:** Markdown, mdBook 0.4.52, mdbook-mermaid 0.16.2 (`.github/workflows/pages.yml:44`).

**Content authority:** `2026-11-06-where-actions-run/intent.md` (spec skipped), its Desired
outcome and Constraints.

---

## Files that change

```
book/src/ch11-09-what-it-promises.md   — modify: correct the sandbox-directory claim; add the workspace's folder as a limit of the ledger
book/src/ch06-00-sandbox.md            — modify: say where the action's folder is, why each workspace has its own, and what a tool that records it does
book/src/ch09-00-remote.md             — modify: add the section "Who should upload"
book/src/ch05-01-action-key.md         — modify: say a dependency built elsewhere with other bytes changes the keys above it, linking to ch09
```

## Order of work

### Task 1 — Add "Who should upload" to the remote chapter

**Files:**
- Modify `book/src/ch09-00-remote.md` — a new `### Who should upload` subsection under "Sharing a
  result", after "### Uploading" and before "## Running on the server".

**Change:** The subsection says: the key of a target holds the files its dependencies produced,
so a computer that downloads a dependency gets the same bytes as the one that built it and every
key above it matches; one builder for the shared parts (CI with `--remote-upload`, or the server
under remote execution) therefore gives the most reuse, and everyone else reads; when everyone
uploads, it still works, but two computers that each build the same unchanged dependency in their
own folders can get other bytes, and then the targets above it are built again instead of fetched;
a result is never wrong either way, only reused less. One Mermaid diagram shows a single builder
uploading and two others fetching the same bytes. No console output is added.

**Proof:**
- `section-present` — `grep -n "^### Who should upload" book/src/ch09-00-remote.md` prints one line, between the lines of `### Uploading` and `## Running on the server`.
- `diagram-renders` — `grep -c '^```mermaid' book/src/ch09-00-remote.md` prints 5 (4 before the change), and `cargo make book-build` passes.

**Done when:** `cargo make book-build` passes and the Definition of Done (`cargo make dod`,
`cargo deny check`) is green. Commit `docs(repo): say who should upload to a remote cache`.

### Task 2 — Correct the promise about the sandbox's directory

**Files:**
- Modify `book/src/ch11-09-what-it-promises.md` — the "Whether a tool is reproducible" bullet
  under "The limits of the ledger", and one new bullet in the same list.

**Change:** The bullet stops saying buildl pins "the sandbox's directory" everywhere; it says the
action's folder has the same path on every run of one workspace. A new bullet, "Where the
workspace is", says the folder's path differs between workspaces (a git worktree included) and
between machines whose workspace path or temporary directory differs, by design, so that no two
workspaces share or wait on one folder; a tool that writes the folder into its output (an included
file's path, the build directory in debug information) gives other
bytes in another workspace; that difference is explained, it is not a bug in buildl, and the
result is never wrong: a cache only reuses less (link to `ch09-00-remote.md#who-should-upload`).
The closing sentence ("Outside those, …") stays true and is kept.

**Proof:**
- `no-pinned-directory-claim` — `grep -n "the sandbox's directory" book/src/ch11-09-what-it-promises.md` prints nothing.
- `workspace-limit-present` — the limits list holds a bullet whose bold lead is "Where the workspace is" and which names git worktrees and machines.

**Done when:** `cargo make book-build` passes and the Definition of Done (`cargo make dod`,
`cargo deny check`) is green. Commit `docs(repo): say the sandbox folder is pinned per workspace`.

### Task 3 — Say where an action's folder is in the sandbox chapter

**Files:**
- Modify `book/src/ch06-00-sandbox.md` — "Tier 2: the input sandbox", after the paragraph on why
  the directory is outside the workspace.

**Change:** A new paragraph, or a short subsection, says the folder is a directory below the
system's temporary directory named for the workspace's path, with one folder per target named for
its label; so it is the same on every run of one workspace and differs in another workspace or on
a machine whose workspace path or temporary directory differs. It says why: each workspace, a git worktree included, keeps a folder of its own
and never shares or waits on another's. It says what follows: a tool that writes the folder it ran
in into its output gives other bytes in another workspace; this is correct and is not a cache
error; and it points to `ch09-00-remote.md#who-should-upload` for what that means for a shared
cache. The existing captured tracebacks and the sentence "The temporary folder in the traceback
will have another name on your machine" stay as they are.

**Proof:**
- `folder-paragraph-present` — the section "Tier 2: the input sandbox" names both the workspace part and the label part of the folder, and says a git worktree gets a folder of its own.
- `link-resolves` — `cargo make book-build` passes and `book/book/ch06-00-sandbox.html` contains `ch09-00-remote.html#who-should-upload`.

**Done when:** `cargo make book-build` passes and the Definition of Done (`cargo make dod`,
`cargo deny check`) is green. Commit `docs(repo): say where an action runs in the sandbox chapter`.

### Task 4 — Link a dependency built elsewhere to the remote chapter

**Files:**
- Modify `book/src/ch05-01-action-key.md` — the section "What a dependency produced, not its key",
  after the paragraph ending with the link to Early Cutoff.

**Change:** One short paragraph says that because the key holds what a dependency made, a
dependency built on another machine or in another workspace with other bytes (a tool that records
its folder is one cause) changes the keys of the targets that use it, and above them only as far
as their rebuilt outputs differ too; the result is still correct,
and `ch09-00-remote.md#who-should-upload` says how a team keeps the bytes shared.

**Proof:**
- `paragraph-present` — the section "What a dependency produced, not its key" links to `ch09-00-remote.md#who-should-upload`.
- `link-resolves` — `cargo make book-build` passes and `book/book/ch05-01-action-key.html` contains `ch09-00-remote.html#who-should-upload`.

**Done when:** `cargo make book-build` passes and the Definition of Done (`cargo make dod`,
`cargo deny check`) is green. Commit `docs(repo): link a dependency built elsewhere to who uploads`.

---

## Risks

- Task 1's heading is the anchor the other three tasks link to: if its text is not exactly
  "Who should upload", mdBook makes another id and the three links point at nothing. The plan-wide
  check below looks for the id in the built page.
- The book's chapter files and numbered rules keep their names; no file is renamed.
- Not done, on purpose: a ledger row in `ch11-05-determinism-rules.md` (the intent names four
  pages), and any new captured console example (none is needed for prose and a diagram).

## Proof

- `grep -rn -i -E "sandbox's directory|same (directory|folder|path) everywhere" README.md CLAUDE.md skills book/src`
  prints nothing.
- `cargo make book-build` passes once all four tasks are in.
- `book/book/ch09-00-remote.html` holds an element with `id="who-should-upload"`, and the three
  links to it (from `ch11-09`, `ch06-00`, `ch05-01`) are in their built pages.

## Review findings

One reviewer pass over the four pages (code 0🔴 5🟡 8🔵, plan 0🔴 1🟡 1🔵), one fix round. All fixed; re-checked
with `cargo make book-build` (`Build Done in 1.08 seconds`), `id="who-should-upload"` and
`id="where-the-folder-is"` present in the built pages, each link found once in its page, and the
plan-wide grep printing nothing (exit 1).

- 🟡 ch05-01 / ch09: "the key of every target above it" contradicted early cutoff — fixed: the keys of the targets that use it change, and above them only as far as their rebuilt outputs differ.
- 🟡 ch06 / ch11-09 / ch05-01: "differs on another machine" overclaimed — fixed: the folder changes with the workspace's path and the temporary directory (`scratch.rs:56-65`, `local_ports.rs:104`); two machines with the same of both share it.
- 🟡 ch06 / ch11-09: C's `__FILE__` as an example — fixed: removed; the c plugin passes a workspace-relative source path, so `__FILE__` holds no folder. Replaced by the build directory a compiler writes into debug information.
- 🟡 ch09: the local cache is asked before the remote — fixed: one sentence says an engineer who built a dependency keeps their own bytes while their local cache holds it.
- 🟡 plan: Tasks 2-4 Change text carried the three wrong premises — fixed: amended in this plan.
- 🔵 ch06: "both names are short digests" — fixed: `buildl-` and a digest.
- 🔵 ch06: label part "same on every run of one workspace" — fixed: depends on the label only, same everywhere.
- 🔵 ch06: "What follows is for" — fixed: "This matters for".
- 🔵 ch09: "holds the files" — fixed: "holds the digests of the files".
- 🔵 ch09 / ch05-01: links to the top of ch06 or Tier 2 — fixed: `#where-the-folder-is`.
- 🔵 ch11-09: same path "as long as the temporary directory stays the same" — fixed.
- 🔵 ch11-09: "a tool ..., such as debug information" — fixed: names rustc and a compiler.
- 🔵 ch06: Rust `include!` example not named in the plan — kept; true (rlib metadata, panic locations).

## Deviations

- 2026-11-07: the task briefer was not spawned; the plan's four tasks were already in the orchestrator's context and were passed verbatim to the coders.
- 2026-11-07: Tasks 1-4 ran as one batch: they touch disjoint files, and Tasks 2-4 need only the anchor text `#who-should-upload`, which the plan fixes. The links were checked in the built pages afterward.
- 2026-11-07: Coders did not run the gates (four parallel `book-build`s write one `book/book`); the orchestrator ran `cargo make book-build`, `cargo make dod` (`Build Done in 132.97 seconds`) and `cargo deny check` (`advisories ok, bans ok, licenses ok, sources ok`) once for the batch. The fix round changed Markdown only, which `dod` does not read, so only `book-build` was re-run.
- 2026-11-07: Three plan premises were wrong in the code (see Review findings) and were amended in the Change text of Tasks 2-4. The intent's Problem says the folder "differs between workspaces, users and machines" and lists C's `__FILE__`; both are loose rather than wrong in what the intent asks for (each workspace keeps its own folder), so no `disproves` was set.
- 2026-11-07: Nothing committed; the per-task commits are left to the author (each task touches one file).
