---
status: approved
created: 2026-11-08
depends-on: [02]
built-on: fef1a1baa97b
---

# Rust Plugin Docs Implementation Plan

**Goal:** The book, the skills and the front pages describe the rust plugin as it is built.

**Architecture:** Markdown only. A new book chapter beside the c plugin's, the limits page and the
discovery chapter's Cargo sentence; the skills and their references; `README.md`, `CLAUDE.md`,
`CHANGELOG.md` (spec §5, §6). Examples printed in the book come from `examples/rust` as plan 02
left it.

**Tech Stack:** mdBook; `cargo make book-build`, `cargo make skills`.

**Content authority:** `spec.md` §2–§6.

---

## Files that change

```
book/src/ch08-05-the-rust-plugin.md  — create: the chapter
book/src/SUMMARY.md                  — modify: list it after ch08-04
book/src/ch11-09-what-it-promises.md — modify: the limits of spec §5
book/src/ch07-00-discovery-targets.md — modify: :306-307 points to the plugin
skills/buildl/SKILL.md, skills/buildl/references/plugins.md — modify: name the plugin
skills/buildl-migrate/SKILL.md, skills/buildl-migrate/references/*.md — modify: a Cargo build maps to the plugin
README.md, CLAUDE.md, CHANGELOG.md   — modify: name the plugin
```

## Order of work

Tasks 1 and 2 touch disjoint files and may run side by side.

### Task 1 — Write the book chapter

**Files:** Create `book/src/ch08-05-the-rust-plugin.md`; modify `book/src/SUMMARY.md`,
`book/src/ch11-09-what-it-promises.md`, `book/src/ch07-00-discovery-targets.md`.

**Change:** the chapter, shaped as `ch08-04-the-c-plugin.md`: setting it up, the functions and
options, the helpers and how a build file composes them, a Cargo workspace, crate types (spec §2.2),
the programs it may run, the limits (spec §5), errors (spec §4), with `examples/rust`'s files and a
`buildl build` and `buildl test` captured from it. `SUMMARY.md` lists it after ch08-04. `ch11-09`
gains the limits of spec §5; `ch07-00:306-307` points to the chapter.

**Proof:** `cargo make book-build` passes; every example matches `plugins/rust` and `examples/rust`.

**Done when:** `cargo make book-build`, `cargo make dod` and `cargo deny check` pass. Commit `docs(repo): the rust plugin chapter`.

### Task 2 — Name the plugin in the skills and front pages

**Files:** Modify `skills/buildl/SKILL.md`, `skills/buildl/references/plugins.md`,
`skills/buildl-migrate/SKILL.md`, the `skills/buildl-migrate/references/*.md` that speak of Cargo,
`README.md`, `CLAUDE.md`, `CHANGELOG.md`.

**Change:** the `buildl` skill shows `rust.binary` and `rust.test` beside the c plugin and its
`buildl.toml`; `buildl-migrate` maps a Cargo build to the plugin; `README.md` and `CLAUDE.md` name
it where they name the c plugin; `CHANGELOG.md` notes it under Unreleased with its tag
`plugin-rust-v0.1.0` at release.

**Proof:** `cargo make skills` passes.

**Done when:** `cargo make skills`, `cargo make dod` and `cargo deny check` pass. Commit `docs(repo): name the rust plugin in the skills and front pages`.

---

## Risks

- The skills' snippet tests check build-file text; a new snippet must load.

## Proof

- `cargo make book-build` and `cargo make skills` pass.
