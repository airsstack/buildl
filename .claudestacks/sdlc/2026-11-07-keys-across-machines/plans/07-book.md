---
status: approved
created: 2026-11-07
depends-on: [06]
built-on: 55ab2847b726
---

# Book Implementation Plan

**Goal:** The book describes key and unkeyed variables, the machine-path refusal and the remote timeout as the code now behaves.

**Architecture:** Markdown only. Reference pages first (appendices), then the chapters that teach
`env`, then the chapters that explain keys, sharing and promises. Every example the book prints
that shows `env`, a listing or a refusal is captured again from a real run of the binary these
plans built (spec §8, intent constraint).

**Tech Stack:** mdBook 0.4.52, mdbook-mermaid 0.16.2; `cargo make book-build`.

**Content authority:** `spec.md` §2-§8.

---

## Files that change

```
book/src/appendix-02-buildl-toml.md   — env.unkeyed in [ceiling] and [plugins.<name>]
book/src/appendix-03-lua-api.md       — the env row (:92): a table of marks
book/src/appendix-04-exit-codes.md    — E1-E9 under 4 and 5
book/src/ch01-03-what-did-it-do.md, ch02-00-first-project.md, ch03-01-targets-and-labels.md (:72),
ch03-04-many-directories.md           — the map form in examples and tables
book/src/ch04-01-nothing-is-ambient.md, ch04-02-the-ceiling.md, ch04-03-approvers.md — the two listing lines; env.unkeyed
book/src/ch05-01-action-key.md (:150), ch05-02-early-cutoff.md, ch05-03-tests.md — the key holds key values and unkeyed names
book/src/ch06-00-sandbox.md (:200)    — the command gets both marks' values
book/src/ch07-00-discovery-targets.md — a discovery output with a machine path is refused
book/src/ch08-01-writing-a-plugin.md, ch08-02-plugins-from-git.md, ch08-03-how-buildl-decides.md,
ch08-04-the-c-plugin.md               — env.unkeyed in the manifest and the plugin table
book/src/ch09-00-remote.md            — "Who should upload": sharing between users needs unkeyed; the server's own environment; the timeout is in the remote key
book/src/ch11-05-determinism-rules.md — marks held sorted; this machine's folders never reach a key
book/src/ch11-09-what-it-promises.md  — the timeout row: in the remote key, not the local one
```

## Order of work

### Task 1 — Update the reference appendices

**Files:** `appendix-02-buildl-toml.md`, `appendix-03-lua-api.md`, `appendix-04-exit-codes.md`.

**Change:** `env.unkeyed` rows with type, default and meaning for `[ceiling]` and
`[plugins.<name>]`; the `env` row of `target`, `test`, `discover` becomes "table from name to
`"key"` or `"unkeyed"`", with the mark table of spec §2; E1-E9 listed under exit 4 and 5, each
with its message captured from a run.

**Proof:** `cargo make book-build` passes; `grep -n 'env.unkeyed' book/src/appendix-02-buildl-toml.md`
prints the two rows.

**Done when:** `cargo make book-build`, `cargo make dod` and `cargo deny check` pass. Commit `docs(repo): reference key and unkeyed variables`.

### Task 2 — Move the teaching chapters to the map form

**Files:** `ch01-03`, `ch02-00`, `ch03-01`, `ch03-04`, `ch04-01`, `ch04-02`, `ch04-03`, `ch05-02`,
`ch05-03`, `ch06-00`, `ch08-01`, `ch08-02`, `ch08-03`, `ch08-04`.

**Change:** every `env = { "X" }` becomes the map form; every captured `plan`, approver and `doctor`
output that lists variables is captured again and shows `unkeyed variables:` where it applies;
`ch04-02` and `ch08-03` explain `env.unkeyed` narrowing `env.read` at each level; `ch06-00:200`
says the command gets both marks' values.

**Proof:** `git grep -n -E 'env *= *\{ *"' -- book/src` prints nothing; `cargo make book-build` passes.

**Done when:** `cargo make book-build`, `cargo make dod` and `cargo deny check` pass. Commit `docs(repo): declare variables with marks in the book`.

### Task 3 — Explain keys, sharing and the refusal

**Files:** `ch05-01`, `ch07-00`, `ch09-00`, `ch11-05`, `ch11-09`.

**Change:** `ch05-01:150`'s `env` row says the key holds `key` values and `unkeyed` names, with a
captured `plan` explaining a mark change; `ch07-00` shows E7 from a run; `ch09-00`'s "Who should
upload" says users and CI share only when variables that find programs are `unkeyed`, that a
server runs with its own environment, and that the timeout is in the remote key; `ch11-05` adds
the rule that this machine's folders never reach a key; `ch11-09`'s settings row says the timeout
is in the remote key, not the local one.

**Proof:** `cargo make book-build` passes; `grep -n 'unkeyed' book/src/ch09-00-remote.md` prints the
new sentences.

**Done when:** `cargo make book-build`, `cargo make dod` and `cargo deny check` pass. Commit `docs(repo): explain keys across machines in the book`.

---

## Risks

- Captures need the binary from plans 01-06; a capture run on another machine's paths would itself
  show a machine folder, so captures are made in `/tmp/buildl-book` as the book's others are.
- Chapter file names and numbered rules keep their names and numbers.

## Proof

- `git grep -n -E 'env *= *\{ *"' -- book/src` prints nothing.
- `cargo make book-build` passes.
