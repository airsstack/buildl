---
status: done
created: 2026-10-27
milestone: buildl/09
---

# Intent: Publish the four crates to crates.io

## Problem

The examples, the book, the READMEs and the AI skills of milestone buildl/09 are done, the C and C++
gap is closed by the `c` plugin, and the four crates (`buildl-core`, `buildl-lua`, `buildl`,
`buildl-cli`) already package cleanly with `cargo package --workspace`. Their names on crates.io are
still free. But none of them is published, so a user who has never read the source can only get
`buildl` by building it from git: the README, `book/src/ch01-01-installation.md`,
`book/src/ch01-04-with-an-ai-agent.md` and `skills/install.sh` all install with
`cargo install --git`, and there is no tagged release and no prebuilt binary.

## Affected systems

- The four crates' manifests and the workspace `Cargo.toml` (version and publish metadata).
- CI under `.github/workflows/` (building the prebuilt binaries for a release).
- The install instructions: `README.md`, `book/src/ch01-01-installation.md`,
  `book/src/ch01-04-with-an-ai-agent.md` and `skills/install.sh`, and the GitHub Pages site that
  serves the book.
- The `c` and `cpp` examples' READMEs, which describe the plugin tag as not yet published.
- The git repository's tags and the GitHub release page.

## Desired outcome

- `buildl-core`, `buildl-lua`, `buildl` and `buildl-cli` are published at version `0.1.0`, in the
  order `book/src/ch11-03-four-crates.md` (Publishing) sets.
- `cargo install buildl-cli --locked` from crates.io gives a working `buildl`.
- The repository has a `buildl-cli-v0.1.0` tag and a GitHub release with release notes and
  prebuilt `buildl` binaries for the supported platforms, and the `plugin-c-v0.1.0` tag that the
  book, the `c` and `cpp` examples and the `buildl` skill already pin the `c` plugin to.
- The README, the book and `skills/install.sh` install the prebuilt binary first, with no Rust
  toolchain needed; `cargo install` from crates.io comes second, and installing from git or from a
  local checkout stays documented as an alternative.

## Constraints

- Publishing can't be undone: a version on crates.io can be yanked but never deleted or replaced.
  No `cargo publish` runs without the user's explicit go-ahead at that step.
- The publish order and the four packaging rules in `book/src/ch11-03-four-crates.md` hold.
- `cargo make dod` and `cargo deny check` are green on the commit that is published and tagged.
- The binaries built for the release are the same `0.1.0` source that is on crates.io.
- No AI attribution in commits, the tag, or the release notes.

## Non-goals

- Feature work from any other milestone.
- Packaging for other distribution channels (Homebrew, apt, Nix and the like).
- Changing the version scheme or making a stability promise beyond what pre-1.0 semver implies.
