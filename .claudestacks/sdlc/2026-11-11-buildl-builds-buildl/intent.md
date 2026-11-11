---
status: done
created: 2026-11-11
---

# Intent: buildl builds buildl, on a pinned release

## Problem

buildl's own repository is built only by cargo and `cargo make`. buildl is never run on the one
workspace its authors work in every day, so a release is not proven on a real Rust project before
users meet it, and the repository shows no example of buildl building a Cargo workspace of this
size.

It was tried in a scratch copy on 2026-11-10, with buildl 0.2.1, `plugin-rust-v0.2.0` and the
files `cargo buildl init` writes, `.claudestacks/` removed by hand:

| Run | Result | Time |
|---|---|---|
| `buildl build` (`buildl`, `cargo-buildl`), cold | 2 built | 80.6s |
| `buildl build`, again | cached | 0.08s |
| `buildl test` (one `rust.test` per crate, `--all-features`), cold | 5 passed | 522.3s |

It worked, but in the real repository every target read every file, so an edit to a note or a
book page made all of them dirty. buildl 0.3.0, released 2026-11-10 (tag `buildl-cli-v0.3.0`),
adds `.buildlignore`, which removes that obstacle.

## Affected systems

- The repository root: new `buildl.toml`, `build.lua`, `.buildlignore`, `buildl.lock`, and
  `.gitignore` entries for `out/` and `.buildl/`.
- The place that records which buildl release builds the repository (the pin).
- `.github/workflows/ci.yml`: new jobs beside the `cargo make` ones.
- `README.md` and the book, where they describe how the repository is built.

## Desired outcome

From a clean checkout, with the pinned buildl release installed:

- `buildl build` produces the `buildl` and `cargo-buildl` binaries in `out/`.
- `buildl test` runs the tests of all five crates and passes.
- Run again with nothing changed, both come from the cache.
- Editing a file that cargo does not read (`.claudestacks/`, `.claude/`, `book/`, `examples/`,
  `skills/`) leaves every target clean.
- The repository records the pinned release, and moving to a new release is one known change.
- CI runs the same on every push and pull request: new jobs install the pinned release and run
  `buildl build` and `buildl test`, and a failure fails the run.

## Constraints

Decided with the author:

- The repository is built with a **pinned released** buildl binary, never one built from the
  working tree. Each new release bumps the pin and adapts the build files in the same change.
  The first pin is 0.3.0.
- `cargo make` stays as it is; this does not replace it (2026-11-10).
- CI gains jobs for this (2026-11-11); the existing `cargo make` jobs stay.
- Folders outside the build are left out with `.buildlignore`, as built in the
  `2026-11-10-buildlignore` chain.
- Cargo builds go through the `rust` plugin as released; the plugin is a thin cargo wrapper and
  gains no machinery for this.

From the repository's rules:

- No change to the buildl crates for this work. A gap found in buildl while doing it is brought
  to the author, not patched here.
- The determinism rules (`book/src/ch11-05-determinism-rules.md`): the action keys must not split
  by machine (no home, workspace or sandbox folder in a keyed value).

## Non-goals

- Replacing `cargo make`: `fmt`, `clippy`, `doc`, doctests, `deny`, the guards, `examples`,
  `plugins`, `skills` and the book build stay its tasks.
- Building with a buildl built from the working tree.
- Changes to the `rust` or `c` plugin, or to `cargo-buildl`.
