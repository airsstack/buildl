---
status: done
created: 2026-10-29
milestone: buildl/09
---

# Intent: AI skills for buildl

## Problem

buildl is new, so no AI agent has learned its build-file API, its `buildl.toml` ceiling or its
exit codes. An agent asked to write a `build.lua` guesses an API that does not exist, and an
agent that meets a refusal (exit code 4) is tempted to widen the ceiling, which undoes the one
thing buildl exists to give. Someone adopting buildl after the first publish also has a build
already — a Makefile, a justfile, shell scripts, npm scripts — and no safe way to move it to
`build.lua` and know nothing was lost. buildl is about to meet its first outside users, so the
gap matters now.

## Affected systems

- A new top-level `skills/` directory, and a new `.claude-plugin/marketplace.json`.
- The book: a new page, `book/src/ch01-04-with-an-ai-agent.md`, with `SUMMARY.md` and a line in
  `ch01-01-installation.md`.
- `README.md` and `CLAUDE.md`.
- CI, which checks the skills.
- No crate changes: the skills are not part of any crate and are not published to crates.io.

## Desired outcome

- An agent with the `buildl` skill writes build files that pass `buildl check`, works through
  check → plan → build, reads exit codes and `plan --json`, and on a refusal explains what the
  build wants and asks the user — it never widens `buildl.toml` itself.
- An agent with the `buildl-migrate` skill moves a Makefile, a justfile, plain shell scripts or
  npm scripts to `build.lua`, and a parity check shows, per output, that the old build and the
  buildl build agree: same targets (or each one dropped with its reason), byte-identical outputs
  (outputs the old build does not reproduce itself are judged by the tests), the same tests with
  the same results, and a ceiling that grants nothing the old build did not use. The old build
  is never edited or deleted; removing it is the user's call once parity passes.
- The same skills work in Claude Code, as a plugin, and in any agent tool that reads the Agent
  Skills format.
- One command installs `buildl`, `airsl` and the skills; the book says how to install and use
  them.

## Constraints

- Skills follow the Agent Skills specification (agentskills.io/specification) and live in a
  top-level `skills/` directory: `skills/buildl/` and `skills/buildl-migrate/`.
- The Claude Code plugin is `.claude-plugin/marketplace.json` pointing at the skills in
  `skills/`, with no second copy of them; `.claude/` stays the repository's own settings.
- Every script the skills carry is Lua run by `airsl`; no shell, no Python. The one exception is
  the installer, `skills/install.sh`, which does nothing but install: the `buildl` and `airsl`
  commands, then the skills, as the Claude Code plugin or into a folder another agent reads.
- The parity check runs as prepare → run → compare: the Lua scripts make the copies and compare
  the results and never start a build; the builds are run by the agent with the commands the
  skill fixes.
- buildl v1 does not build C/C++ one file at a time. At a per-file C/C++ build, the migration
  offers one target compiling the whole program, saying what it costs, and stops, saying why, if
  the user declines.
- The book page sits under Getting Started as `ch01-04`; no chapter is renumbered.
- buildl is unpublished: nothing here keeps compatibility with an earlier form.
- Candidate direction: `skills/install.sh` installs the commands with
  `cargo install --locked buildl-cli airsl-cli`.

## Non-goals

- A change to airsl, such as a working-directory option for `proc.run`.
- A buildl command that runs the skills' scripts itself.
- Migration from CMake, Bazel, Gradle or other build systems.
- Agent evaluations run by `claude plugin eval`.
- Packaging for agent tools other than Claude Code beyond the Agent Skills format itself.
