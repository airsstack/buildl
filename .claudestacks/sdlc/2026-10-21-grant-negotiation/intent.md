---
status: approved
created: 2026-10-21
milestone: buildl/04
---

# Intent: I18 grant negotiation — approver and `buildl doctor`

## Problem

buildl checks what a build wants against the ceiling in `buildl.toml`, and nothing else
decides whether the build may run.

- **The person who wrote `buildl.toml` is not always the person running the build.** In a
  cloned repository the ceiling came with the code: it can allow `curl`, `docker` or any
  program, and the build runs it without the user ever seeing the list. Design §6 promises a
  precise prompt ("this build wants to run `cc`, `ld`, `docker`; allow?") and an approver with
  three modes, manifest, interactive and deny. None exists, and an `[approver]` table in
  `buildl.toml` is refused as an unknown key (design §4).
- **Nothing shows a workspace's authority.** Design §7 lists `buildl doctor` to print the
  effective ceiling, the grants and the run settings. It does not exist.
- **A program the build makes must be named by hand in the ceiling.** A step that runs
  another target's output names it through `$deps` (`run = { "$deps", "$out" }`), so the
  build file never writes the path. The ceiling still must: `proc.run = ["go", "out/gen"]`,
  or the build is refused (`program out/gen is outside the ceiling (proc.run allows: go)`,
  exit 4, seen on a Go workspace on 2026-10-21). Bazel, Buck2, Please, Pants, Nix, CMake and
  Cargo all run tools built in the same build, and none asks for them to be allow-listed:
  being declared in the graph is the permission.
- **Per-target grant narrowing is an open question** (design §14), and milestone 4 is where
  it is to be settled.
- **The roadmap's wording does not fit.** Milestone 4 and design §13 item 4 say buildl
  "consumes the airsl extension host's ceiling + `Approver`". airsl's are built for one Lua
  extension's request (an extension manifest and a Lua policy), not a build's programs,
  variables and directories, and airsl may be used by `buildl-lua` only
  (architecture-building-blocks §7). buildl follows airsl's model instead: requests are
  intersected with the ceiling, never unioned; the approver can only narrow; failure is
  closed.

The one slice of milestone 4 (design §13 item 4).

Agreed with the user, and overridable:

- **`manifest` is the default** and behaves as buildl does today: a build inside the ceiling
  runs.
- **Interactive asks once.** An approval is remembered for the user, outside the workspace,
  and the user is asked again only when a build wants something the remembered approval does
  not cover.
- **No terminal, no approval.** Interactive mode with no terminal to ask on refuses; it never
  approves by default. CI uses `manifest`.
- **Built programs need no ceiling entry.** A program named through `$deps` — another
  target's output — runs without a `proc.run` entry; `proc.run` lists host tools only. A
  program named through `$in` (a declared input file) is held to `proc.run` as today.
- **Per-target narrowing is declined.** Each action already gets only its own declared
  variables and input files (tiers 1 and 2); narrower grants per target only matter once an
  OS sandbox can enforce them (tier 3, design §12.2).

## Affected systems

- `buildl-core` — the approval step between the ceiling check and Execute, and its port;
  the program check for a command whose program is named through `$deps`.
- `buildl` — the approver adapters, the `[approver]` table of the manifest reader, the
  remembered approval outside the workspace, and `Workspace` gaining `doctor`.
- `buildl-cli` — the `doctor` command, the interactive prompt, the refusal messages and
  their exit codes.
- `docs/design.md` §4, §6, §7, §12.2, §13 item 4, §14; `docs/architecture.md`;
  `docs/architecture-building-blocks.md`; `.claudestacks/sdlc/roadmaps/buildl/04-grant-negotiation.md`;
  `README.md`; `CLAUDE.md` Status; the crate docs.

## Desired outcome

|Topic|Outcome|
|---|---|
|Flow|wanted set → inside the ceiling? → approver says yes? → run. A no at either step refuses before anything runs, naming what was wanted and what was refused.|
|Modes|`buildl.toml` takes `[approver] default = "manifest" \| "interactive" \| "deny"`; no table means `manifest`. Any other key or value is refused as the manifest reader refuses others today.|
|`manifest`|approves every build inside the ceiling (today's behaviour).|
|`interactive`|shows the wanted set and asks the user to allow it. A yes is remembered for the user, outside the workspace — a repository can ship its own `.buildl/`, so an approval kept there could come with the code; a later build whose wanted set is covered by it is not asked again; one that wants more is asked again, about the new wants.|
|No terminal|`interactive` refuses, and the error says how to approve.|
|`deny`|refuses `build`, `test` and `verify`.|
|Which commands ask|`build`, `test` and `verify`. `plan` reports the wanted set and asks nothing; `check`, `graph`, `query`, `log`, `clean` and `gc` are unchanged.|
|`buildl doctor`|prints the effective ceiling, the approver mode, what the workspace's build wants, and the `[execution]` settings (`workers`, `timeout`, `strategy`).|
|Built programs|a program named through `$deps` runs with no `proc.run` entry; `proc.run` names host tools only, and no build file or manifest writes an output path by hand. The approval prompt lists the host tools.|
|Per-target narrowing|settled as declined, with the reason, in design §14.|
|Wording|design §13 item 4 and roadmap milestone 4 say buildl follows airsl's model rather than consuming airsl's types.|
|Tests|core tests against fake ports, end-to-end tests through `Workspace` and CLI tests of the binary cover each row above.|
|Documents|the design, both architecture documents, README and `CLAUDE.md` describe grant negotiation and `doctor` as built.|

## Constraints

- airsl stays a dependency of `buildl-lua` only; no airsl type crosses into another crate.
- The approver can only narrow: nothing it says lets a build exceed the ceiling.
- Fail closed: no answer, no terminal, or an unreadable remembered approval is a refusal,
  never an approval.
- The action key, cache, cas and log formats do not change, and approval is no part of the
  key.
- The determinism rules of `architecture.md` §5 hold: the wanted set, the prompt and
  `doctor`'s output are the same on every run; no `HashMap` iteration reaches output.
- The structural rules hold: the logic is in `buildl-core` and testable against fake ports;
  adapter modules of `buildl` do not import each other (`cargo make guard-adapter-isolation`).
- `cargo make dod` and `cargo deny check` green before every commit; work on this branch,
  never `main`.
- Everything this slice finds is closed in this slice; nothing is handed to a later one.

## Non-goals

- Per-target grants, and enforcing grants beyond what tiers 1 and 2 already do (the network
  included): tier 3.
- airsl's extension manifest parser and `ext.on` dispatcher.
- Plugin attribution of wants (design §10): milestone 5.
- An approver that asks over a network or a GUI.

Candidate direction: the grants step moves out of Plan into its own phase, as
architecture-building-blocks §5 already describes.
