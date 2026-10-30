---
status: approved
created: 2026-10-30
---

# Intent: C and C++ support through an official plugin

## Problem

buildl says it does not support C or C++ compiled one file at a time: the book
(`ch07-00-discovery-targets.md`, `ch11-09-what-it-promises.md`), the roadmap's non-goals, and the
`buildl-migrate` skill, whose stop rule forces a migrated Makefile into one target compiling the
whole program. That shape rebuilds everything on any edit, and its binary cannot match the old
per-file build byte for byte, so parity has to fall back to the tests.

The claim is wrong. Probes on the current binary (Apple clang 15, sandbox strategy) built a C program
one object per target, with a discovery target running `cc -MM` to declare each object's headers:
it rebuilt only what an edit touched, held the result through early cutoff, and `buildl verify`
found no mismatch. What is really missing is a way to write that without hand-parsing Make-format
dependency output in every build file, and nothing in buildl offers one.

Three smaller gaps sit beside it:

- A git plugin must be the whole repository, with `extension.toml` at its root, so a plugin kept in
  a folder of this repository cannot be fetched by another workspace.
- A build file cannot tell which platform it runs on, so one that builds a shared library on both
  Linux and macOS cannot choose between their flags.
- An action key fingerprints the program it runs; `/usr/bin/cc` on macOS is a launcher that does not
  change when Xcode updates the compiler behind it, so a compiler update leaves old objects cached.

buildl is about to meet its first outside users, and C and C++ are the stacks where an incremental,
sandboxed, cached build matters most.

## Affected systems

- A new plugin folder, `plugins/c/`, published by git tag from this repository.
- `buildl-core` and `buildl`: where a git plugin's files are found (`PluginSource`, the
  `[plugins.<name>]` table, `buildl.lock`, the plugin loader in `buildl-lua`), and the `buildl`
  Lua table in `buildl-lua`.
- `examples/`: new C and C++ examples, checked by `examples/check.sh` in CI.
- The book: ch07, ch08 (plugins from git), ch11-09, the `buildl.toml` appendix.
- The roadmap's non-goals, the `buildl` and `buildl-migrate` skills and their references.
- `README.md` and `CLAUDE.md` where they repeat the claim.

## Desired outcome

- A workspace builds C and C++ one object per target with every header declared, by calling an
  official `c` plugin it fetches from this repository at a tag. An edited source recompiles that
  source; an edited header recompiles the sources that include it; with the compiler-version
  probe turned on, a compiler update recompiles everything.
- The plugin builds executables, static libraries, shared libraries and tests, on Linux and macOS,
  with gcc or clang, for C and C++ alike.
- The plugin adds no flag the target kind does not need; every other choice (`-fPIC`, optimisation,
  include and define values, rpath, soname, output names, compiler) is the engineer's. Helpers that
  return values make the common choices short to write, and the engineer calls them on purpose.
- A build file can learn its platform through the plugin, by an ordinary target it asks for.
- `examples/c` and `examples/cpp` show every plugin function and helper, and CI checks them through
  a real git fetch.
- The book, roadmap and skills say per-file C and C++ is supported through the plugin, and the
  migrate skill moves a per-file Makefile to it.

## Constraints

- Core stays free of stack knowledge. Two core changes, both stack-agnostic: where a git plugin's
  files are found, and a way for Lua to read its build file's folder, since commands run from the
  workspace root; everything about C and C++ lives in the plugin's Lua.
- No depfile is read by the executor. Header discovery is a discovery target, as the book's design
  of dynamic input discovery requires.
- One plugin, `c`, covers C and C++. It lives in `plugins/c/`, and its tags are
  `plugin-c-vX.Y.Z`, apart from buildl's own release tags.
- One discovery target per source file, so editing one source re-discovers only that source.
- The compiler-version probe is off by default, and an engineer turns it on: a target that runs on
  every build leaves the compiles that depend on it conditional in `plan`.
- The platform is never guessed: it comes from a target the build file declares.
- buildl is unpublished: nothing keeps compatibility with an earlier form of `buildl.toml` or
  `buildl.lock`.
- Candidate direction: an optional `dir` key beside `git`/`github` and `rev`; `buildl.folder()`;
  `c.binary`,
  `c.library`, `c.shared_library`, `c.test`; helpers `c.includes`, `c.defines`, `c.rpath_to`,
  `c.shared_name`, `c.depfile.parse` and `c.platform` (a discovery target running
  `cc -dumpmachine`).

## Non-goals

- Windows and MSVC: buildl itself builds on Unix only.
- Cross-compiling.
- Wrapping CMake, autotools or meson; a plain target already does that.
- A depfile read by the executor, or any other side channel into an action's inputs.
- A core fix for launcher fingerprints that applies to every stack (rustup, pyenv, asdf shims).
- Plugins for other stacks.
- Bundling plugins into the `buildl` binary.
