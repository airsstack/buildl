---
status: approved
created: 2026-10-27
milestone: buildl/09
spec: skipped
---

# Intent: README, crate READMEs and CLAUDE.md

skip: documentation only, shape decided with the author (crate READMEs a short card, `README.md` about 60 to 80 lines).

## Problem

The milestone asks for a short `README.md` that points at the book and the examples, and for
crate READMEs that match what each crate does. Neither holds today:

- `README.md` is 283 lines and retells the book chapter by chapter (architecture, the action
  key, plugins, the remote cache, commands, roadmap, references).
- The four crate READMEs, which crates.io shows as each crate's page, are long inventories of
  internal types, and are out of date: `buildl-cli`'s lists no `buildl watch` and none of the
  remote-execution flags; `buildl`'s names no `remote` or `watch` module.
- The crate READMEs link to each other by relative path (`../buildl-lua`, 8 links), which
  leads nowhere on crates.io.

`CLAUDE.md` already points at the book (commit `9c5615d`); that part is done.

## Affected systems

- `README.md`
- `crates/buildl-core/README.md`, `crates/buildl-lua/README.md`, `crates/buildl/README.md`,
  `crates/buildl-cli/README.md`
- `CLAUDE.md`, only where it describes `README.md`

## Desired outcome

- `README.md` is short: what buildl is, how to install and run it, and links to the book and
  `examples/`.
- Each crate README says what that crate is for, where it sits among the four, and links to
  the book and docs.rs, with links that work on crates.io; nothing in it is out of date.
- `CLAUDE.md`'s line about `README.md` still describes it.

## Constraints

- The book is the authority; a README restates nothing the book does not say.
- Links in crate READMEs are absolute URLs.
- Gates: `cargo make dod`, `cargo deny check`; one commit per plan task, scope `repo`.

## Non-goals

- Changing the book, the examples or any code.
- Publishing the crates (the first-publish intent).
