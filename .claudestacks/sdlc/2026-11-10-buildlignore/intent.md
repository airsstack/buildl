---
status: approved
created: 2026-11-10
---

# Intent: .buildlignore, folders and files outside the build

## Problem

A workspace has no way to say "these paths are not part of the build". `buildl.sources(pattern)`
walks every folder below the build file except the output directory and `.buildl`, so a pattern
such as `**/*` lists documentation, planning notes, editor and agent folders, and other
workspaces' scratch alongside the sources. Every file it lists becomes an input, and so a part of
every action key that reads it: an edit anywhere in those folders makes those actions dirty.

The `rust` plugin hits this on every call. Its inputs are `buildl.sources("**/*")` minus a
`target/` and a `.git/` folder, because cargo may read any file below the crate. In buildl's own
repository that pulls in `.claudestacks/`, `.claude/`, `book/`, `examples/` and `skills/`, none of
which cargo reads. In the main checkout `.claude/worktrees/*` holds full copies of the repository,
each with its own `target/`, so the walk also enters them, splits the key on whatever another
worktree holds, and can run past `[declaration] walk_entries`.

It matters now because the next step is buildl building buildl (all five crates and their
tests, on a pinned release). That was tried in a scratch copy of the repository with buildl
0.2.1 and `plugin-rust-v0.2.0`, with `.claudestacks/` removed by hand:

| Run | Result | Time |
|---|---|---|
| `buildl build` (`buildl`, `cargo-buildl`), cold | 2 built | 80.6s |
| `buildl build`, again | cached | 0.08s |
| `buildl test` (one `rust.test` per crate, `--all-features`), cold | 5 passed | 522.3s |

It works, but in the real repository it would only stay cached until the next edit to a note or a
book page. Removing folders by hand is not an answer; the workspace has to be able to say it.

## Affected systems

- `buildl-lua`: the `sources` walk (`crates/buildl-lua/src/sources.rs`, its skip list) and the
  reading of `inputs`.
- `buildl`: `watch`, which today ignores only the output directory, `.buildl` and `.git`; the
  composition that hands the walk its skip list.
- The workspace `Cargo.toml` and `deny.toml`, for any new dependency.
- The book: build files (`ch03`), watch mode (`ch10`), the Build File API (`appendix-03`, `sources`
  and `inputs`), exit codes (`appendix-04`); the `buildl` skill where it describes `sources`.

## Desired outcome

A workspace marks paths as outside the build with `.buildlignore` files, and buildl behaves as if
those paths were not there for declaring and watching:

- `sources()` never lists an ignored path and never walks into an ignored folder, so it costs no
  `walk_entries`.
- A build file that names an ignored file in `inputs` is refused when it loads, naming the file
  and the `.buildlignore` that ignores it.
- `watch` does not start a new cycle for a change under an ignored path.
- With `.claudestacks/`, `.claude/`, `book/`, `examples/` and `skills/` ignored at the root of
  buildl's repository, editing a file there leaves the `rust` plugin's targets clean.

## Constraints

Decided with the author on 2026-11-10:

- The list lives in `.buildlignore` files, not in a key of `buildl.toml`.
- They nest as `.gitignore` files do: one may sit in any folder and applies to that folder and
  everything below it.
- Full `.gitignore` syntax: globs, `!` negation, a leading `/` anchoring to the file's folder, a
  trailing `/` matching folders only. Ripgrep's `ignore` crate is the expected implementation.
- An explicit `inputs` entry naming an ignored file is refused (exit code 5), not let through.

From the repository's rules:

- The dependency rules (`book/src/ch11-03-four-crates.md`): `buildl-core` does no I/O; `buildl-lua`
  never depends on `buildl`; adapter modules in `buildl` do not import each other.
- The determinism rules (`book/src/ch11-05-determinism-rules.md`): what is ignored is decided by
  the files alone, never by walk order or the machine.
- Every new dependency is commented with its reason in the workspace `Cargo.toml` and passes
  `cargo deny check`.
- buildl is unpublished as 1.0: no compatibility switch, an old workspace without
  `.buildlignore` simply has nothing ignored.

## Non-goals

- The root `buildl.toml` and `build.lua` with which buildl builds itself: a sibling chain,
  written after a release that has `.buildlignore`, which that chain pins.
- Reading `.gitignore`: only `.buildlignore` is honoured, so the build never changes because of a
  file kept for git.
- Changes to the `rust` or `c` plugin: they keep calling `sources()` and gain the filter through
  it.
- Ignoring paths for the sandbox copy, the store, `plugins fetch`, or `clean`/`gc`: only declaring
  (`sources`, `inputs`) and `watch` are in scope.
