---
status: done
created: 2026-11-07
depends-on: [06]
built-on: f3bc0d9fa2e7
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
book/src/appendix-04-exit-codes.md    — E1-E10 under 4 and 5
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
`"key"` or `"unkeyed"`", with the mark table of spec §2; E1-E10 listed under exit 4 and 5, each
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

- `git grep -n -E 'env *= *\{ *"' -- book/src` prints only `appendix-03-lua-api.md`'s example of the refused list form.
- `cargo make book-build` passes.

## Review findings

One reviewer pass over Tasks 1-3 (`13-reviewer-p07`, blocking: ch11-05:85, ch09-00:205, ch11-09:104), one fix round (`14-coder-p07-fix`, `cargo make book-build` green, `git grep -n -i 'hiraq\|/Users/' -- book/src` prints nothing):

- 🟡 ch11-05 rule 6 said "nothing runs" on a match, but a discovery's output is checked after it ran — fixed: "nothing more runs".
- 🟡 ch09-00 said "Two more things" before three bullets — fixed: three.
- 🟡 ch11-09 said a plugin's version check covers machine folders — fixed: the clause is dropped.
- 🔵 appendix-04's E7 console block left out what the discovery printed — fixed: shown as `buildl build 2>&1 | tail -n 2`, as ch07-00 does.
- 🔵 ch06-00 said the mark decides only key membership — fixed: it also decides whether the value is sent to a remote server.
- 🔵 the sandbox folder had three names — fixed: "the action's folder" in ch07-00, ch11-05, ch11-09 and appendix-04.
- 🔵 reflowed lines past the wrap width in eight pages — fixed.
- 🔵 appendix-02's `Line` column cites `schema.rs` line numbers — accepted: the appendix's convention; every cited line checked correct.
- ❓ ch08-04 named the published tag `plugin-c-v0.1.0`, whose `graph.lua:191,201` holds `env = { "PATH" }`, refused by this buildl — resolved with the author: the c plugin is 0.2.0 (see Deviations).
- Gates at the end: `sh examples/check.sh examples/c` and `examples/cpp` ok; `cargo make skills` 62 passed; `cargo make book-build` done; `cargo make dod` exit 0; `cargo deny check` "advisories ok, bans ok, licenses ok, sources ok".

## Deviations

- 2026-11-08 — the c plugin is bumped to 0.2.0 (author's choice): `plugins/c/extension.toml` `version = "0.2.0"`, and every `plugin-c-v0.1.0` / `c@0.1.0` in `book/src/ch08-04`, `examples/{c,cpp}` (`buildl.toml`, `README.md`, `prepare.sh`), `plugins/c/README.md` and `skills/buildl/SKILL.md` now names 0.2.0; `CHANGELOG.md` notes it under Unreleased. The tag `plugin-c-v0.2.0` is pushed when this branch reaches `main`, at release. The `c@0.2.0` lines in ch08-04's captures were edited, not re-run: the plugin version is not in the action key (`action_key.rs` has no plugin field), so a run prints the same lines with the new version.
- 2026-11-08 — Task 2's grep proof prints `appendix-03-lua-api.md:119-120`, which show the refused list form on purpose; the Proof line is amended.
- 2026-11-08 — Task 3's ch05-01 captures could not use a fake `HOME` (setting it was refused by the capture shell); they run on `examples/go` with `HOME` unkeyed, so only its name shows. E7 there names a temporary sandbox path under `/tmp/buildl-book`.
- 2026-11-08 — appendix-04's E7 reads the home folder (`/home/alice`) as the longest match, not `/home/alice/shop` as spec §7's example; the book prints the captured text.
