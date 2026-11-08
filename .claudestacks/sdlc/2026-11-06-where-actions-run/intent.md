---
status: done
created: 2026-11-06
spec: skipped
---

# Intent: The book says where actions run and who should upload

Spec skipped: a book-only change to four pages, with no code, settings or behaviour changed.

## Problem

An action in the input sandbox runs in `<temp>/buildl-<hash of the workspace's path>/<hash of the
label>`. The label part is the same on every run; the rest differs between workspaces, users and
machines. Tools that write the folder they ran in into what they produce (rustc's `include!` from
`OUT_DIR`, the macOS linker's install name, debug info, C's `__FILE__`) give other bytes in another
workspace. That is correct, and it is the intended design: each workspace, git worktree included,
keeps its own folder and never waits on or shares one with another.

The book does not say so, and says the opposite in one place:

| Page | What it says | What is true |
|---|---|---|
| `ch11-09-what-it-promises.md` (limits of the ledger) | "`buildl` pins the inputs, the time zone, the locale, the build time and the sandbox's directory" | the directory is the same on every run of one workspace only |
| `ch06-00-sandbox.md` | the folder "will have another name on your machine" | true, but not what it means for the bytes a tool writes |
| `ch09-00-remote.md` | "CI builds `main` and stores its results there" | not why one builder matters, or what happens when everyone uploads |
| `ch05-01-action-key.md` | a key folds the files a dependency produced | not that a dependency built elsewhere, with other bytes, changes every key above it |

Evidence, gathered 2026-11-06/07 while probing a Rust plugin (sibling chain
`2026-11-06-rust-plugin`), buildl 0.1.0:

- GitHub Actions runs 42221321359 and 42221611326 (`ubuntu-latest`, `ubuntu-24.04-arm`,
  `macos-latest`): the same build in another folder with another `TMPDIR` gave a different
  dependency layer and binary, while `buildl verify` in the first folder passed.
- A remote cache probe on macOS (bazel-remote 2.6.4 on localhost, three workspace folders with
  their own `TMPDIR`): one folder built with `--remote-upload`; a second fetched both targets with
  the same bytes (`deps.tar` `5259c451…`, `app` `1650bb4d…`), and after an edit to `main.rs`
  rebuilt only the app; a third built the dependency layer itself with `--no-remote`, got other
  bytes (`8a1c61ea…`), and so built the unchanged app instead of fetching it. Every result was
  correct; only reuse was lost.
- Rewriting the path per tool (`--remap-path-prefix`) does not remove it: cargo stores the flag,
  which holds the path, in its fingerprints, and the macOS linker writes the path as the install
  name of a proc-macro library.

## Affected systems

- `book/src/ch11-09-what-it-promises.md`, `ch06-00-sandbox.md`, `ch09-00-remote.md`,
  `ch05-01-action-key.md`.
- `README.md` and `CLAUDE.md`, only if they repeat the wrong claim.
- The `buildl` skill's references, only if they repeat it.

## Desired outcome

- No page says the sandbox's directory is the same everywhere: it is the same on every run of one
  workspace, and differs between workspaces and machines.
- The sandbox chapter says that a tool which writes its folder into its output gives other bytes in
  another workspace, that this is correct and not a cache error, and why each workspace keeps a
  folder of its own.
- The remote chapter says who should upload: one builder (CI, or the server under remote
  execution) gives the most reuse, because everyone then fetches the same bytes of the shared
  parts; everyone uploading still works, with less reuse; and a result is never wrong either way.
- The action key page says that a dependency built elsewhere with other bytes changes the keys
  above it, and points to the remote chapter.

## Constraints

- Book only: no change to code, settings, the action key or the sandbox's location.
- Every example the book prints is captured from a real run, as the book's other examples are.
- Diagrams in the book are Mermaid; chapter file names and numbered rules keep their names and
  numbers.
- References cite into `appendix-05-references.md`.

## Non-goals

- A fixed sandbox path shared by every workspace or machine: rejected, because worktrees would
  share and wait on one folder.
- Rewriting paths inside each tool's output (`--remap-path-prefix`, `-ffile-prefix-map`): each
  plugin's choice, not this chain's.
- Keying a target by its dependencies' keys instead of their outputs (losing early cutoff).
- The Rust plugin's own chapter (`2026-11-06-rust-plugin`).
