---
status: approved
created: 2026-11-11
depends-on: [02]
built-on: a6e2956d9de2
---

# Docs Implementation Plan

**Goal:** The book and the README say how buildl builds this repository on the pinned release.

**Architecture:** The book is the authority; the README points into it (CLAUDE.md, "Keep it
consistent with the book"). The book section describes the files plan 01 wrote and the job plan
02 added, read from those files, not restated from the spec.

**Tech Stack:** Markdown, mdBook with Mermaid.

**Content authority:** `spec.md` §6, and D3 for the rustup-default case.

---

## Files that change

```
book/src/ch11-07-testing-layers.md   — the closing sentence (:115) becomes the section "buildl builds buildl"
README.md                            — a new section "Building this repository", before "## Crates"
```

## Order of work

### Task 1 — Write the book section and the README section

**Files:**
- Modify `book/src/ch11-07-testing-layers.md` — replace the paragraph at line 115 ("The last layer
  is an aim rather than a test: `buildl`'s own gate running under `buildl`.") with
  `## buildl builds buildl`, covering, in this order:
  - the four targets and what each runs (from `build.lua`), and why `tests` and `doctests` match
    `cargo make test` and `cargo make test-doc`;
  - the root `.buildlignore` (from the file): everything left out, then what cargo reads brought
    back, and the three stray-file lines; a new root file stays out, and a new file cargo reads
    needs a `!/<path>` line;
  - the pin: `.buildl-version`, the install command with `BUILDL_VERSION`, and the version check;
  - `version_probe` and its cost in `plan` (`4 to run, 2 conditional`); and the one case the probe
    misses: a machine whose rustup default is not `stable`, where an update to `stable` changes
    cargo's compiler without changing the probe, and the remedy, deleting `.buildl/` then
    `buildl plugins fetch`;
  - the CI job `self-build`: the cache key and restore key, the second pass and its two expected
    lines, `buildl gc`, and saving only on a miss;
  - the bump after each release: `.buildl-version` moves in its own change once the release is
    published, with the build files if the release needs it, and the bump is a cache miss.
  One Mermaid diagram shows the job's flow (restore → fetch → build/test → second pass → gc →
  save on a miss).
- Modify `README.md` — a section `## Building this repository`, placed before `## Crates`:
  install the version `.buildl-version` names with the `BUILDL_VERSION` installer line, then
  `buildl plugins fetch`, `buildl build`, `buildl test`; one sentence that `cargo make` stays the
  gate; a link to the book section
  (`https://airsstack.github.io/buildl/ch11-07-testing-layers.html#buildl-builds-buildl`).

**Change:** Prose only; every command and every quoted output line is one plan 01 or plan 02
ran and recorded.

**Proof:** no test: prose. `cargo make book-build` builds, and the section's anchor
`buildl-builds-buildl` is in the built `ch11-07-testing-layers.html`.

**Done when:** `cargo make book-build` passes, and `cargo make dod` and
`cargo deny check -D unused-wrapper` are green. Commit
`docs(repo): buildl builds buildl in the book and the README`.

---

## Risks

- The sentence at `ch11-07:115` is cited nowhere outside `.claudestacks`
  (`git grep -n 'aim rather than a test' -- ':!.claudestacks'` lists only that line); if a
  citation appears before execution, it moves with the section.
- A quoted line that plan 01 or 02 recorded differently from the spec is written as recorded.

## Proof

- `git grep -n 'The last layer is an aim' -- book/` matches nothing.
- `cargo make book-build`, `cargo make dod` and `cargo deny check -D unused-wrapper` green.
