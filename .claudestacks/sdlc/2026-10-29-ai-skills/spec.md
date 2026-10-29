---
status: approved
created: 2026-10-29
built-on: ff8bfb525ac0
---

# Spec: AI skills for buildl

Two skills in the Agent Skills format teach an AI agent what no model learned in training: `buildl`,
for writing build files and reading what buildl reports, and `buildl-migrate`, for moving a
Makefile, a justfile, plain shell scripts or npm scripts to `build.lua` with a parity check whose
verdict comes from a script, not from the agent. They live in a top-level `skills/` directory, are
offered to Claude Code by a marketplace file that points at them without copying them, and are
installed by one shell script. Every other script they carry is Lua run by `airsl`. A book page
says how to install and use them. No crate changes.

## 1. Premises and their evidence

Every fact the design rests on, proven in this task. External documents were downloaded raw on
2026-10-29 and are cited as `<document>:<line>`; tool behaviour was run, and the output is quoted.

| # | Fact | Evidence |
|---|---|---|
| P1 | A skill is a folder holding `SKILL.md` with YAML frontmatter; `name` (required) is 1–64 characters, lowercase letters, digits and hyphens, no leading, trailing or doubled hyphen, equal to the folder name; `description` (required) is 1–1024 characters; `license`, `compatibility` (≤ 500), `metadata`, `allowed-tools` are optional | agentskills.io/specification.md:28-35, :58-67 |
| P2 | `allowed-tools` is experimental: "Support for this field may vary between agent implementations" | agentskills.io/specification.md (the `allowed-tools` field section) |
| P3 | `SKILL.md` should stay under 500 lines; file references go one level deep from `SKILL.md`, as relative paths | agentskills.io/specification.md:237, :250 |
| P4 | `assets/` is an optional folder for templates and data; `scripts/` holds code the agent runs; `references/` holds documents loaded on demand | agentskills.io/specification.md, *Optional directories* |
| P5 | The spec's validator is `skills-ref`, a Python package needing Python ≥ 3.11 that describes itself as "intended for demonstration purposes only"; it installs from source | agentskills/agentskills skills-ref/README.md:6, :12-18; the installed `skills_ref-0.1.0.dist-info/METADATA`: `Requires-Python: >=3.11` |
| P6 | `skills-ref` pinned to commit `69ef37e9424c0a7ea9dd2293b559e43ec8176379` installs with `pip install "git+https://github.com/agentskills/agentskills@69ef37e9424c0a7ea9dd2293b559e43ec8176379#subdirectory=skills-ref"`, accepts a valid skill and rejects a mismatched name | run: `Valid skill: mkt/skills/demo` exit 0; control: `Directory name 'wrong' must match skill name 'not-wrong'` exit 1 |
| P7 | A Claude Code marketplace entry with `"source": "./"` and a `skills` list loads only the listed folders | code.claude.com plugins/manifest-reference.md:681 ("only those subdirectories load") |
| P8 | Without `.claude-plugin/plugin.json`, the marketplace entry is the plugin's manifest | code.claude.com plugins/marketplace-reference.md, *How an entry combines with plugin.json* ("No `plugin.json`: the entry is the manifest regardless of `strict`") |
| P9 | That layout passes `claude plugin validate --strict` | run on a probe marketplace (root source, `skills: ["./skills/demo"]`), Claude Code 2.1.312: `✔ Validation passed` exit 0 |
| P10 | `claude plugin marketplace add` takes `owner/repo@ref` and `--sparse <paths...>`; `claude plugin install <plugin>@<marketplace>` defaults to scope `user` | `claude plugin marketplace add --help` (2.1.312); marketplace-reference.md:387; plugins/cli-reference.md:76, :83, :100 |
| P11 | A symlinked local source is not read | marketplace-reference.md:496 (`Local source "x" is or traverses a symlink, so <path> was not read`) |
| P12 | `buildl check` rejects a misspelt option key with exit 5 and the line | probe `input = {…}`: `error: build.lua: unknown field "input": line 2: buildl.target 'x.txt': unknown field 'input'`, exit 5 |
| P13 | `buildl check` validates declarations only: a missing input file and a program outside the ceiling both pass it | probe with `inputs = { "missing.c" }` and `run = { "cc", … }` under `proc.run = ["python3"]`: `ok: 1 declaration in 1 build file`, exit 0 |
| P14 | `buildl build` refuses a program outside the ceiling with exit 4, naming the program and the ceiling | probe: `error: build.lua: //:x.txt: program sh is outside the ceiling (proc.run allows: python3)`, exit 4 |
| P15 | `buildl doctor` lists each want against the ceiling | probe: `run sh         REFUSED  -` |
| P16 | `buildl graph` prints, by default, canonical JSON (the bytes of `.buildl/graph.json`) listing every declared target, tests included, each with its `role` (`Build` or `Test`) and its `network` (`Sealed` when it reaches no network); `buildl plan` with no target leaves tests out | book/src/appendix-01-cli.md:70; run on a copy of `examples/python`: `graph` lists `//:app.pyz` (`"role":"Build"`, `"network":"Sealed"`) and `//:unit` (`"role":"Test"`, `"network":"Sealed"`), while `plan --json`'s `targets` hold only `//:app.pyz`; `buildl plan --help`: "with none, plan and build take every target but tests" |
| P17 | Exit codes 0–7 are Success, ActionFailure, Usage, Infrastructure, Refused, Definition, CorruptState, Internal | book/src/appendix-04-exit-codes.md:8-15 |
| P18 | buildl v1 does not support one target per `.o` file; one action over a full glob of a toolchain's sources is sound | book/src/ch11-09-what-it-promises.md:22-24, :31-33 |
| P19 | airsl's modules are `json`, `regex`, `path`, `fs`, `glob`, `env`, `proc`, `time`, `hash`, `stdio`, `hook`, `ext`; there is no TOML module; `proc.run` takes only `{program, arg, …}`, with no working-directory option | airsl book/src/appendix-02-modules.md:7-147, :95 |
| P20 | airsl's `fs.walk` lists hidden entries and folders as well as files; `fs.copy` keeps the execute bit; `hash.hash_file` is SHA-256; `require` loads a sibling file under the default `confined` policy | probe (airsl 0.1.2): `[".hidden",".hidden/h","a.txt","run.sh","sub","sub/b.txt"]`; `-rwxr-xr-x … out/run.sh`; `hash_file` equals `shasum -a 256`; `require("lib")` returned the module |
| P21 | airsl's `fs.tempdir` needs a write grant on the system temp folder | probe: `fs.tempdir denied: /private/var/folders/…/T is outside the granted write roots` |
| P22 | `airsl test` runs every `*_test.lua` / `test_*.lua` under a folder; a test file returns a table of functions; a test fails by raising (`assert`) | `airsl test --help`; airsl book/src/ch05-05-testing.md:47-50 |
| P23 | `cargo install` takes several crates in one command; `airsl-cli 0.1.4` is on crates.io | `cargo install --help`: `cargo install [OPTIONS] [CRATE[@<VER>]]...`; `cargo search airsl`: `airsl-cli = "0.1.4"` |
| P24 | The repository has no git tags, locally or on GitHub; its raw files and tarball are publicly reachable, the tarball through a redirect | `git tag \| wc -l` → `0`; `curl https://api.github.com/repos/airsstack/buildl/tags` → `[]`; `https://raw.githubusercontent.com/airsstack/buildl/main/README.md` → 200; `https://github.com/airsstack/buildl/archive/main.tar.gz` → 302 to `codeload.github.com/airsstack/buildl/tar.gz/refs/heads/main` |
| P25 | CI jobs run `cargo make <task>`; repository tooling in `Makefile.toml` already uses `@shell` scripts | .github/workflows/ci.yml:67, :109; Makefile.toml:368 |
| P26 | `.claude/` holds the repository's own Claude Code settings | `git ls-files .claude` → `.claude/settings.json` |
| P27 | Under `confined`, `require` finds only modules in the script's own folder and below, takes no `..`, and `load`/`dofile` are removed; a sibling `require` works with no read grant on the script's folder | airsl book/src/appendix-03-presets.md:10, :13; ch02-00-first-project.md:138-140; probe: `airsl run --allow-read . <other>/scripts/main.lua` with `require("lib")` → `loaded lib`, exit 0 |
| P28 | airsl's `--allow-write DIR` does not grant reading; `fs.exists`, `fs.list`, `fs.walk` need a read grant | airsl book/src/appendix-01-cli.md:29; appendix-02-modules.md:58-61 |
| P29 | The workspace ceiling's keys are `proc.run`, `env.read`, `fs.read`, `fs.write`; it has no network key | book/src/appendix-02-buildl-toml.md:48-51 |
| P30 | `buildl doctor` prints the ceiling as one line per key, values joined by `, `, `-` when empty | the P15 probe: `ceiling` / `  proc.run  python3` / `  env.read  -` / `  fs.read   .` / `  fs.write  .buildl, out` |
| P31 | Exit 4 also comes from the approver: `interactive` refuses when stdin or stderr is not a terminal, with `error: no terminal to ask on; run the build in a terminal once to approve, or set [approver] default = "manifest"`; `deny` refuses every build | book/src/ch04-03-approvers.md:35-48, :70-77 |
| P32 | `build.lua` has eleven functions; `$input` is not a placeholder; `print` is removed from build files; an action's environment holds exactly the declared variables that have a value | book/src/appendix-03-lua-api.md:5, :75-77, :236; ch11-09-what-it-promises.md:52 |
| P34 | Exit 4 covers three refusals: `Refused` (the ceiling), `PluginRefused` (a plugin's own manifest, or the workspace's `[plugins.<name>]` cap) and `NotApproved` (the approver); a plugin refusal names the plugin in brackets, as `//:bye (plugin gobin@0.1.0): … is outside plugin gobin@0.1.0 (env.read allows: nothing)`, while the ceiling would allow the want | book/src/ch11-04-errors.md:182; ch08-03-how-buildl-decides.md:50-56, :66-75 |
| P35 | Exit 3 covers an infrastructure failure, a missing program, a missing blob and a failed plugin fetch | book/src/ch11-04-errors.md:181 |
| P36 | A program a dependency built is no want: it needs no `proc.run` entry | book/src/ch08-03-how-buildl-decides.md:32; ch11-01-declare-then-execute.md:306-307 |
| P37 | `$input` and other near-placeholders pass through to the command unchanged | crates/buildl-core/src/placement/expand.rs:273-279 (a test asserting `"$input"` is kept) |
| P38 | `interactive` keeps a yes per workspace location, so a copy of the workspace elsewhere is not approved; with no terminal it refuses | book/src/ch04-03-approvers.md:65-77 |
| P33 | A C program compiled one file at a time and linked is not byte-identical to the same program compiled in one command | probe (Apple clang 15.0.0, two `.c` files and a header): `cc -c` each then `cc -o` → `d86b95b9…`; `cc -o app main.c util.c` → `cabbc3d7…` |

## 2. Layout

```text
skills/
├── install.sh                      the one shell script; installs, nothing else (§5)
├── buildl/                         a skill (§3)
│   ├── SKILL.md
│   └── references/
│       ├── lua-api.md
│       ├── buildl-toml.md
│       ├── exit-codes.md
│       └── plugins.md
├── buildl-migrate/                 a skill (§4)
│   ├── SKILL.md
│   ├── references/
│   │   ├── make.md
│   │   ├── just.md
│   │   ├── shell.md
│   │   └── npm.md
│   └── scripts/
│       ├── parity.lua
│       └── parity_lib.lua
└── tests/                          not a skill: no SKILL.md, not listed in the marketplace (§6)
    ├── parity_test.lua
    ├── snippets.lua
    └── fixtures/

.claude-plugin/marketplace.json     the Claude Code plugin (§5)
```

- Each skill folder is self-contained: an installed copy carries everything it references (P3, P11
  rule out links and symlinks out of the folder).
- `skills/tests/` is never loaded or installed as a skill: the marketplace loads only the folders it
  lists (P7), though the plugin's sparse checkout of `skills` carries the folder's files along, and
  `install.sh --dir` copies only folders holding a `SKILL.md` (§5.4).
- `.claude/` is untouched: it is the repository's own settings (P26).
- No `assets/` folder: every language has the same shape in buildl, one target over all its
  sources (P18), so a starter build file and one line per language, inside `SKILL.md`, replace
  per-language templates.

## 3. The `buildl` skill

### 3.1 Purpose

An agent asked to set up a project with buildl, add or change a target, explain a rebuild, or fix
a failing or refused build writes build files that `buildl check` accepts, works through buildl's
commands in order, acts on each exit code, and on a refusal explains what the build wants and asks
the user.

### 3.2 Frontmatter

```yaml
---
name: buildl
description: Write, edit and debug buildl build files (build.lua) and buildl.toml, and read
  what buildl reports. Use when a project has a build.lua or buildl.toml, when the user asks
  to build, test or set up a project with buildl, or when a buildl command fails or refuses.
license: Apache-2.0
compatibility: Requires the buildl command on PATH.
---
```

Only fields the spec defines (P1). `allowed-tools` is left out (P2).

### 3.3 What `SKILL.md` holds

**A. How buildl works.** `build.lua` only declares targets; buildl runs them. Nothing is allowed by
default: a target's command sees only the environment variables it declares in `env`
(P32); every program it runs must be in `buildl.toml`'s `[ceiling]` `proc.run`, except a program a
dependency built (P36); what it reads and writes must lie within `fs.read` and `fs.write` (P29). A
plugin's targets are held to the plugin's own manifest and the workspace's `[plugins.<name>]` cap as
well. Then the approver, `[approver]`, agrees to what the build wants. Commands hold no real paths: `$in`, `$out`,
`$deps`, `$opt:name`.

**B. The working loop.**

```text
 edit build.lua / buildl.toml
        │
        ▼
 buildl check ─── exit 5 ──► fix the file at the line the error names, check again
        │ 0        (check validates declarations only: it does not apply the ceiling — P13)
        ▼
 buildl plan      what would rebuild, and why; --json gives, per target, its verdict, the reason and
                  the key's components, plus what the build wants (`wanted`); tests only when named
        │ 0       exit 4 (ceiling or plugin refusal) / exit 5 (missing input): handled as at build
        │         (book/src/ch04-01-nothing-is-ambient.md:128-134; ch08-03-how-buildl-decides.md:142)
        ▼
 buildl build ─── exit 1 ──► buildl log <target> ──► fix the code being built
        │ 0       exit 4 ──► buildl doctor ──► explain, and ask the user (3.3 D)
        ▼
 buildl test      handled as build
```

**C. What to do for each exit code** (P17).

| Code | Name | The agent |
|---|---|---|
| 0 | Success | is done |
| 1 | ActionFailure | runs `buildl log <target>` and fixes the code being built |
| 2 | Usage | fixes the command line it typed |
| 3 | Infrastructure | reads the error: a missing program or a failed plugin fetch (P35) is reported to the user, since a retry cannot help; any other, it retries once and then reports |
| 4 | Refused | runs `buildl doctor` and tells the user which of three refusals it is (3.3 D), and asks |
| 5 | Definition | fixes `build.lua` or `buildl.toml` at the line the error names, and runs `check` again |
| 6 | CorruptState | stops and reports; does not delete `.buildl` without asking |
| 7 | Internal | stops and reports a buildl bug |

**D. The rules the agent never breaks.**

| Never | Why |
|---|---|
| add to `[ceiling]`, to a `[plugins.<name>]` cap, or to a plugin's `extension.toml`, without the user's yes to that exact change | what builds may touch is the user's decision |
| change `[approver]` to get past a refusal | approval is the user's |
| set `strategy = "bare"` to get past a sandbox failure | it drops the sandbox; the fix is a missing declared input |
| delete an old build, `out` or `.buildl` without asking | it cannot be undone |

Exit 4 has three causes (P34), told apart by the error line and `doctor`:

| Cause | How it shows | What the agent tells the user |
|---|---|---|
| the ceiling | `doctor` lists a want as `REFUSED` in its `ceiling` column (P15); the error names the ceiling key (P14) | the quoted line, the key and value that would allow it, and a question (below) |
| a plugin | the error names a plugin in brackets, `(plugin <name>@<version>)`, and says the want is outside that plugin (P34) | the quoted line; whether the plugin's own manifest or the workspace's `[plugins.<name>]` cap stops it; and a question about that cap — the plugin's manifest is its author's, so the agent offers no edit to it |
| the approver | every want is allowed by the ceiling and the plugins; the error is the approver's, such as `no terminal to ask on` (P31), or `[approver]` is `deny` | that the approver refused; with `interactive`, the user runs the build once in their own terminal to approve it; with `deny`, the choice of approver is the user's |

For the ceiling or a cap, the agent quotes buildl's line (P14) and asks. On the user's yes it makes
exactly that change and nothing more; without it, it changes nothing. The question has this form:

```text
buildl refused the build:
  //:x.txt: program sh is outside the ceiling (proc.run allows: python3)
To allow it, buildl.toml's [ceiling] proc.run needs "sh". Should I add it?
```

**E. Mistakes, and what buildl does with each.**

| Mistake | What happens |
|---|---|
| a misspelt option key (`input` for `inputs`) | exit 5, `unknown field 'input'`, with the line (P12) |
| `$input` for `$in` | not a placeholder (P32): the command gets the text unchanged (P37) |
| an environment variable used but not in `env` | the command does not see it (P32) |
| a program used but not in the ceiling | exit 4 at `build` (P14), not at `check` (P13) |
| printing from `build.lua` | `print` is removed (P32); `error()` shows a value |

**F. Starting a project.** One generic `build.lua` and `buildl.toml` pair, and one `run` line per
language, taken from `examples/`:

| Language | `run`, and anything else the target needs |
|---|---|
| Go | `{ "go", "build", "-o", "$out", "." }`, with `env = { "HOME" }` (examples/go/build.lua) |
| Rust | `{ "cargo", "install", "--path", ".", "--root", "$out", "--target-dir", "tt", "--no-track" }`, with `env = { "HOME", "PATH" }`: without `PATH` the build stops with ``error: linker `cc` not found`` (examples/rust/build.lua:10-11; examples/rust/README.md:10-11) |
| Python | `{ "python3", "-m", "zipapp", "app", "-o", "$out" }` (examples/python/build.lua) |

### 3.4 References

| File | Holds |
|---|---|
| `lua-api.md` | the eleven functions (P32) and their option keys, labels, placeholders, `sources` patterns |
| `buildl-toml.md` | `[workspace]`, `[ceiling]`, `[declaration]`, `[approver]`, `[execution]`, `[plugins.<name>]`, `[remote]` |
| `exit-codes.md` | codes 0–7 with the error text each prints |
| `plugins.md` | `buildl.use`, `extension.toml`, `buildl plugins fetch` |

Written by hand, short, for an agent: the book stays the full account for people. Every `lua` code
block in them is checked (§6.2).

## 4. The `buildl-migrate` skill

### 4.1 Purpose

An agent asked to move a project to buildl reads its old build — a Makefile, a justfile, plain
shell scripts, or npm `scripts` in `package.json` — writes `build.lua` and `buildl.toml`, and proves
with a script that the new build does what the old one did. The old build is never edited or
deleted.

### 4.2 Frontmatter

```yaml
---
name: buildl-migrate
description: Move a project's Makefile, justfile, shell build scripts or npm scripts to buildl
  (build.lua and buildl.toml), and prove with a parity check that the new build makes the same
  outputs and passes the same tests. Use when the user asks to migrate, convert or port a build
  to buildl.
license: Apache-2.0
compatibility: Requires the buildl and airsl commands on PATH.
---
```

### 4.3 Files

| File | Written by | What it is |
|---|---|---|
| the old build files | the user, already there | never edited, never deleted |
| `build.lua`, `buildl.toml` | the agent | the new build, written with the `buildl` skill's rules |
| `parity.json` | the agent | the mapping the check reads (4.5) |
| `scripts/parity.lua` | shipped | the check's two commands, `prepare` and `compare` |
| `scripts/parity_lib.lua` | shipped | pure functions: list a build file's recipes, hash outputs, judge each row, render the table |
| `references/{make,just,shell,npm}.md` | shipped | each tool's features and their buildl equivalent |

### 4.4 The parity check: prepare → run → compare

`P` is the project folder; `W` is a work folder the agent names. Both are absolute paths, and `W`
lies outside `P`. `S` is the installed skill's folder: SKILL.md writes the script as
`<skill dir>/scripts/parity.lua`, since the folder differs between a Claude Code install and a
`--dir` install.

```text
 1 PREPARE   airsl run --allow-read P --allow-read W --allow-write W S/scripts/parity.lua prepare P W
             • refuses unless P and W are absolute, W lies outside P, and W is empty or absent
               (the read grant on W is what lets it look: a write grant does not read, P28)
             • copies P to W/old-1, W/old-2 and W/new
             • leaves out .git, .buildl, out, parity.json, parity.json's "skip" list, and every
               path in "outputs"; intermediate files an earlier run left in P (build/, dist/,
               each main.o) are left out only when the agent lists them in "skip", which SKILL.md
               tells it to do; a "skip" entry is a path relative to P, never a pattern ("*.o"
               matches nothing), and a folder's entry covers everything below it
               (parity_lib.is_left_out)
             • reads P and W, writes only W, runs nothing

 2 RUN       the agent runs these commands, fixed in SKILL.md, from its own shell:
               cd W/old-1 && <old.build> ; echo $? > W/old-1.build.status
               cd W/old-1 && <old.test>  ; echo $? > W/old-1.test.status    (when old.test is set)
               cd W/old-2 && <old.build> ; echo $? > W/old-2.build.status
               cd W/new   && buildl build ; echo $? > W/new.build.status
               cd W/new   && buildl test  ; echo $? > W/new.test.status
               cd W/new   && buildl graph       > W/new.graph.json
               cd W/new   && buildl doctor      > W/new.doctor.txt

 3 COMPARE   airsl run --allow-read P --allow-read W S/scripts/parity.lua compare P W
             • prints the parity table (4.6); exits non-zero when any row is FAIL
             • reads only; writes nothing; runs nothing
```

`parity.lua` loads `parity_lib.lua` with a sibling `require`, which needs no grant on the skill's
folder (P27).

Step 2's `buildl build` and `buildl test` in `W/new` run under the project's approver. With
`manifest`, the default, they run. With `interactive`, `W/new` is a new location, so it is not
approved, and the agent's shell has no terminal (P38): the agent asks the user to run step 2's
`W/new` commands in their own terminal, then carries on with step 3. With `deny`, every build is
refused, in the user's terminal too (book/src/ch04-03-approvers.md:23), so the check cannot run:
the agent says so and stops. In neither case does it change `[approver]` (3.3 D).

The builds run in the agent's shell, not in the script, because airsl's `proc.run` cannot start a
program in another folder (P19). `prepare` takes `W` as an argument rather than creating a temp
folder, so its one write grant names one folder (P21). The copies are made with `fs.walk` and
`fs.copy`, which list hidden entries and keep the execute bit (P20), so shell scripts in the
project still run in a copy.

### 4.5 `parity.json`

Written by the agent in `P`:

```json
{
  "tool": "make",
  "old": {
    "build":    ["make", "all"],
    "test":     ["make", "test"],
    "programs": ["go"],
    "env":      ["HOME"],
    "network":  false
  },
  "recipes": {
    "all":    { "target": "//:app" },
    "app":    { "target": "//:app" },
    "test":   { "target": "//:unit" },
    "clean":  { "dropped": "buildl clean replaces it" },
    "deploy": { "dropped": "kept in Makefile: not a build step (user's answer)" }
  },
  "outputs": [ { "old": "app", "new": "out/app" } ],
  "skip": []
}
```

An `outputs` entry may carry `"judge": "tests"` with a non-empty `"reason"`; its row is then judged
by check 5 instead of check 3 (4.6). Two cases use it: the C/C++ case of 4.7, and an archive that
stamps its entries with file times (a zip, jar or tar). buildl pins `SOURCE_DATE_EPOCH`, so such
an archive differs from the old build's even when the migration is right (plan 03 Task 5: over
`examples/python`, old-1 and old-2 agreed on `app.pyz` and buildl's, stamped 1980-01-01, differed).

| Field | Meaning |
|---|---|
| `tool` | `make`, `just`, `shell` or `npm`: how `compare` reads the old build's recipe names |
| `old.build` | the old build's command, run in step 2 |
| `old.test` | the old build's test command; optional, for a project with no tests |
| `old.programs`, `old.env`, `old.network` | the programs, environment variables and network use the old build has, which the agent reads off the old build's recipes |
| `recipes` | every old recipe, mapped to the buildl target that replaces it, or `dropped` with a reason |
| `outputs` | pairs of files to compare, old path ↔ buildl path, relative to the copy |
| `skip` | further paths to leave out of the copies |

Every field but `old.test` and `skip` is required. A `parity.json` missing one, holding an unknown
`tool`, holding a `dropped` with an empty reason, an empty `outputs` list, an `outputs` path that is
absolute or climbs out of the copy (`..`), or an `outputs` entry with `judge` and no `reason`, is
refused when it is read: both commands fail with a message naming the field, so
`prepare` copies nothing and `compare` prints no table.

### 4.6 What `compare` checks

| # | Check | How | FAIL when |
|---|---|---|---|
| 1 | Every recipe accounted for, and every mapping real | reads recipe names from the old build itself — never from the agent's own list — then looks up each mapped `target` in `new.graph.json`'s `targets`, which hold tests too (P16). Per tool: `make` reads the first of `GNUmakefile`, `makefile`, `Makefile` (make's own order), and its recipes are its rule targets, pattern rules (`%.o`) included, special targets (`.PHONY` and others starting with `.`) and variable assignments not; `just` reads `justfile`, and its recipes are its recipe headers, not its `set`, `alias`, `export` or `:=` lines; `npm` reads `package.json`'s `scripts` keys; `shell` takes every `*.sh` file in `P` outside the left-out paths, keyed in `recipes` by its path relative to `P` | a recipe is neither mapped nor `dropped`, or a mapped target does not exist |
| 2 | The builds ran | the `*.build.status` files | a status file is missing; the old build failed, leaving nothing to compare against; or `buildl build` failed |
| 3 | Same outputs | SHA-256 of `old-1/<old>`, `old-2/<old>` and `new/<new>` per `outputs` pair | old-1 and old-2 agree and buildl's differs, or a file is missing |
| 4 | Old build not repeatable | old-1 and old-2 differ for a file | when there is no `old.test`: nothing could judge the file. Otherwise never: the row reads `NOT REPEATABLE`, and check 5 judges that file |
| 4b | Judged by tests | an `outputs` entry with `"judge": "tests"` | when there is no `old.test`, as in check 4. Otherwise never: the row reads `JUDGED BY TESTS` with the entry's reason, and check 5 judges that file |
| 5 | Same test outcome | `old-1.test.status` against `new.test.status`; with no `old.test`, the row reads `NO TESTS` | one passed and the other failed; or an output is left to the tests (check 4 or 4b) and either run failed |
| 6 | No wider permissions | the ceiling, read from `new.doctor.txt` (P30), and each target's `network` in `new.graph.json` (P16), against `old.programs`, `old.env` and `old.network` | the ceiling's `proc.run` or `env.read` holds something the old build does not use, or a target's `network` is not `Sealed` while `old.network` is false |

Check 6 reads the ceiling from `doctor`'s output, and not from `buildl.toml`, because airsl has no
TOML reader (P19). The workspace ceiling has no network key (P29), so network use is judged from
each declared target's `network` in the graph, tests included. `fs.read` and `fs.write` are not
compared: they name folders of the workspace (P29), which the old build, run in that workspace,
already reached.

Check 5 compares the outcome of the whole test run: whether the old tests and the buildl tests each
passed. It does not count individual tests, since make, just, shell and npm report them in no
common form; that each old test recipe has a buildl test target is check 1's.

### 4.7 Stop rules in `SKILL.md`

**C and C++ compiled one file at a time.** When the old build compiles each `.c` or `.cc` to its own
`.o` and links them (`%.o: %.c`), the agent writes no per-`.o` targets: buildl v1 does not support
them, because a compiler reads headers nobody declared (P18). It offers one target compiling the
whole program in one command, with every source and every header declared as an input —
`run = { "cc", …, "-o", "$out", sources }`, the sources named in `run` rather than as `$in`, since
`$in` holds the headers too and a compiler given a header with `-o` refuses (plan 04: clang,
"cannot specify -o when generating multiple output files") — so no header is undeclared, and says
what it costs: any edit rebuilds the whole program. `sources` paths are relative to the build file,
so a bare `sources` in `run` is right in the root `build.lua`. Such a binary differs from the per-file build's byte for byte
(P33), so on acceptance the agent marks that output `"judge": "tests"` with the reason, the parity
table shows it as `JUDGED BY TESTS`, and the test outcome decides it (4.6 check 4b). A project with
no test command can therefore not pass parity this way (check 4b), and the offer says so before the
user answers. If the user declines, it stops with nothing changed.

**A recipe that is not a build step.** A recipe that deploys, tags, pushes or otherwise acts outside
building files (`deploy`, `release`) is not dropped silently and not forced into a target. The agent
lists such recipes and asks whether to keep them in the old build or drop them, and records the
answer as `dropped` with its reason, so check 1 still adds up.

### 4.8 What is left afterwards

The old build files, `parity.json` and `W` are left in place; deleting any of them is the user's
decision.

## 5. Install script and Claude Code plugin

### 5.1 `.claude-plugin/marketplace.json`

```json
{
  "name": "buildl",
  "owner": { "name": "airsstack" },
  "description": "Skills for buildl: write and debug build files, and migrate builds to buildl.",
  "plugins": [
    {
      "name": "buildl",
      "source": "./",
      "description": "The buildl and buildl-migrate skills.",
      "skills": ["./skills/buildl", "./skills/buildl-migrate"]
    }
  ]
}
```

The repository root is the plugin, so the skills are used where they are (P7); only the two listed
folders load (P7); there is no `plugin.json` (P8); this shape passes the strict validator (P9).

### 5.2 Running `install.sh`

```sh
# from GitHub: download, read, run
curl -fsSLO https://raw.githubusercontent.com/airsstack/buildl/main/skills/install.sh
sh install.sh --claude

# from a clone
sh skills/install.sh --dir ~/.codex/skills
```

The intent's "one command" is `sh install.sh`; downloading it is a separate step on purpose, so the
user can read the script before running it.

### 5.3 Options

| Option | Meaning | Default |
|---|---|---|
| `--claude` | install the skills as a Claude Code plugin | |
| `--dir P` | copy each skill folder into `P`, the folder another agent reads skills from | |
| `--ref R` | the git branch to install from | `main` (the repository has no tags, P24) |
| `--dry-run` | print every command and run none | off |

Exactly one of `--claude` and `--dir` is required.

### 5.4 Steps

```text
 1. arguments    not exactly one of --claude / --dir, or an empty P ──► usage on stderr, exit 2
                 --claude and claude missing ──► "install Claude Code first", exit 1
                 (checked here, before any slow cargo install)
 2. cargo        needed only when buildl or airsl is missing;
                 then missing ──► "install Rust from https://rustup.rs", exit 1
 3. buildl       on PATH ──► skipped
                 missing, --dir from a clone ──► cargo install --locked --path <clone>/crates/buildl-cli
                 missing, otherwise ──► cargo install --locked --git https://github.com/airsstack/buildl
                               --branch R buildl-cli
 4. airsl        on PATH ──► skipped
                 missing ──► cargo install --locked airsl-cli          (P23)
 5a. --claude    claude plugin marketplace add airsstack/buildl@R --sparse .claude-plugin skills
                 claude plugin install buildl@buildl                      (P10)
 5b. --dir P     a clone: the script's own folder is named skills and its parent holds
                 .claude-plugin/marketplace.json ──► copy from that folder
                 otherwise ──► curl -fsSL https://github.com/airsstack/buildl/archive/R.tar.gz
                               (-L follows the redirect, P24), unpack it in a temporary folder
                 the source folder and P are the same folder ──► refused, exit 1, nothing removed
                 for each of buildl and buildl-migrate: replace P/<name> with the source's copy
 6. report       what was installed, and where
```

A folder named `skills` holding `buildl/SKILL.md` is not proof of a clone: an agent's skills folder
where buildl was installed before looks the same, and taking it as the source removed the user's own
skill there before the copy failed (plan 05 review, reproduced in a scratch folder). Only the two
named skills are copied, so another agent's skills next to the script are never taken.

- POSIX `sh`, no `sudo`; each command is printed before it runs.
- A command already on `PATH` is not replaced: the script does not upgrade a buildl the user
  installed on purpose.
- `--dir` replaces only `P/<name>` for each skill, leaving the user's other skills alone, so a
  second run gives the same result.
- `--scope` is not offered; `claude plugin install` defaults to `user` (P10).
- buildl is installed from git at `R`, before and after the publish alike, or from the clone the
  skills are copied from, so the binary and the skills always come from the same source.

## 6. Checks

### 6.1 Tasks

| Task | Runs | Where |
|---|---|---|
| `cargo make skills` | 6.2 – 6.6 | a new CI job, `skills`, on `ubuntu-latest` (P25) |
| `cargo make skills-claude` | `claude plugin validate --strict .` | the developer's machine only: not in CI, not in `dod` |

Neither is a step of `dod`: like `examples`, they need tools beside the Rust toolchain (`airsl`,
Python, `shellcheck`, `claude`).

The `skills` CI job follows the `examples` job's steps (ci.yml:70-109: checkout, Rust toolchain,
cargo cache, cargo-make), and adds: `actions/setup-python` with 3.12 and the pinned `skills-ref`
install (P6); `cargo install --locked airsl-cli` (P23); `sudo apt-get update` then `sudo apt-get install -y shellcheck`. The
task itself builds the `buildl` binary with `cargo build -p buildl-cli` and uses it from the target
directory, as the `examples` task does (Makefile.toml:370-372).

### 6.2 Snippets

Every fenced block whose info string is exactly `lua` in `skills/**/*.md`, outside the test inputs
in `skills/tests/fixtures/`, is a build file and is
checked. A block fenced `lua no-check` is skipped — a plugin's own module code, a build file that
needs a plugin installed or a child build file (`buildl.subdir`), or an example of a mistake shown
on purpose — and the text beside it says why.

```text
 airsl run --allow-read skills --allow-write W skills/tests/snippets.lua W
     └─► W/<file>-<n>/build.lua  +  a fixed buildl.toml        (one folder per block)
 for each folder: cd it && buildl check                       (the task's shell loop)
     └─► any exit other than 0 fails the task, naming the file and the block
```

The extraction is Lua; the loop is in the task's `@shell` script, which already is how
`Makefile.toml` runs repository tooling (P25), because `proc.run` cannot start `buildl` in another
folder (P19). `check` validates declarations without the ceiling (P13), so the fixed `buildl.toml`
need not list the snippet's programs.

### 6.3 Spec conformance

`skills-ref validate skills/buildl skills/buildl-migrate`, installed in the CI job with Python 3.12
from the pinned commit (P5, P6).

### 6.4 Parity scripts

Under `confined`, a test in `skills/tests/` cannot `require` the library in
`skills/buildl-migrate/scripts/` (P27). The task therefore stages them together: it copies
the contents of `skills/tests/` and `skills/buildl-migrate/scripts/*.lua` into the root of one
temporary folder `T`, then runs `cd T && airsl test --allow-read . .`, so every `require` is a
sibling one (P27), tests read their fixtures as `fixtures/…`, and the tests keep the default policy. `parity_test.lua` (P22) holds:

| Test | Proves |
|---|---|
| recipe listing, per tool, over `fixtures/{make,just,shell,npm}` | check 1 reads every recipe and nothing else (`X := y` is not a recipe); a shell script left out of `recipes` is found |
| a `recipes` entry naming a target absent from `new.graph.json`; one naming a test target present there | check 1 fails; passes |
| a ceiling line in `new.doctor.txt` holding a program `old.programs` lacks; one holding a variable `old.env` lacks; a target whose `network` is not `Sealed` with `old.network` false | check 6 fails |
| a `doctor` text with no ceiling block, or an unreadable line inside it | `compare` fails, naming the line, rather than pass |
| a missing status file; an old build status other than 0; a `buildl build` status other than 0 | check 2 fails |
| one test run passing and the other failing; an output left to the tests with a failing run | check 5 fails |
| `compare` run as a script over a fixture with a FAIL row | it exits non-zero |
| `prepare` given a relative `W`, a `W` inside `P`, or a non-empty `W` | it refuses before copying |
| `compare` over a fixture work folder with prepared copies, outputs and status files | a true match passes every row |
| the same fixture with one output byte changed | check 3 fails, naming the file |
| old-1 and old-2 differing | check 4 reads `NOT REPEATABLE`, and check 5 decides |
| an output marked `"judge": "tests"` with a reason | check 4b reads `JUDGED BY TESTS` |
| a recipe missing from `recipes` | check 1 fails |
| a `dropped` with an empty reason; `judge` with no `reason`; an empty `outputs`; an `outputs` path that is absolute or starts with `..` | refused when `parity.json` is read |
| an output `NOT REPEATABLE` or `JUDGED BY TESTS` with no `old.test` | check 4 or 4b fails |
| a malformed `parity.json` | the message names the field |

The fixtures hold already-built work folders, so the tests run no build and need no toolchain.

### 6.5 Installer

`shellcheck skills/install.sh`, and `sh skills/install.sh --dry-run --dir <temp>` exiting 0.

### 6.6 Marketplace

`claude plugin validate --strict .` checks the marketplace manifest only: a `skills` entry naming a
folder that does not exist still passes (Claude Code 2.1.312, plan 05: `./skills/missing` added,
`✔ Validation passed`, exit 0, `"contents": []`). So `skills/tests/marketplace.lua`, run by
`cargo make skills` as `airsl run --allow-read . skills/tests/marketplace.lua`, reads
`.claude-plugin/marketplace.json` with the `json` module and fails, naming the path, when a listed
skill folder holds no `SKILL.md` or a `skills/*/` folder holding one is not listed.

## 7. Documentation

| File | Change |
|---|---|
| `book/src/ch01-04-with-an-ai-agent.md` | new page: installing with `install.sh` (§5.2), what each skill does, the agent's loop and the rule that a refusal is answered by asking (§3.3), migrating with the parity check (§4.4), and using the skills from another agent with `--dir`; diagrams in Mermaid |
| `book/src/SUMMARY.md` | the page under Getting Started, after *What Did It Do?* |
| `book/src/ch01-00-getting-started.md` | its list of the section's pages gains the new page |
| `book/src/ch01-01-installation.md` | one line under *Check that it worked*, pointing at the new page |
| `README.md` | an *AI agents* row in the *Learn more* table, and the install command |
| `CLAUDE.md` | *Documents and their roles* names `skills/` and `.claude-plugin/`; *Status* gains one sentence on the skills |

No chapter is renumbered: the `ch11-*` file names are cited across the repository and keep them.

## 8. Risks

| Risk | Carried by |
|---|---|
| `skills-ref` calls itself a demonstration (P5); its rules could part from the spec's | the pin (P6): a rule change reaches CI only when the pin moves |
| marketplace manifest mistakes are caught only where `cargo make skills-claude` is run, and that validator does not check the listed skill paths | the task, run before a commit that touches `.claude-plugin/` or `skills/`; the paths are checked in CI by 6.6 |
| the status files in step 2 are written by commands the agent runs, so an agent could write them by hand | `SKILL.md` fixes the commands; the hashes in check 3 are computed by the script from the copies |
| `old.programs`, `old.env` and `old.network` are the agent's reading of the old build, so check 6 is only as right as that reading | `SKILL.md` has the agent list them recipe by recipe, and the user sees them in the parity table |
| `doctor`'s text is not a documented machine format, so a change to it breaks check 6 | the parity tests' fixture holds `doctor` output; `compare` fails, naming the line it could not read, rather than pass |
| `"judge": "tests"` widens parity beyond outputs the old build does not reproduce: an agent could mark any output that fails check 3 | the check cannot tell an honest mark from a dishonest one; `SKILL.md` limits it to the C/C++ case of 4.7 and time-stamped archives (4.5), check 5 then needs both test runs to pass, and the table shows every judged output with its reason, for the user to read |
| byte equality may break on a correct migration: toolchains that embed paths make old-1 ≠ old-2 (`NOT REPEATABLE`, judged by tests), and archives that embed file times (a zip, as `examples/python` builds) can make old-1 = old-2 while buildl's differs (check 3 FAIL) | settled by plan 03 Task 5's run over `examples/python`: old-1 = old-2, buildl's `app.pyz` differs (zip times pinned to 1980 by `SOURCE_DATE_EPOCH`), check 3 FAIL; the skill tells the agent to mark such an archive `"judge": "tests"` (4.5) |
| the `--claude` install was never run end to end: P9 validates the manifest only, and a second run against an existing marketplace and plugin is unproven | plan 05 runs it once by hand on the pushed branch and records the result; CI does not run it (§9) |
| how well agents follow the skills is not measured | out of scope (§9) |
| `install.sh` builds buildl from git with `cargo install`, which takes minutes on a machine with nothing cached, for `--claude` and `--dir` alike | stated in the book page |

## 9. Non-goals

- A change to airsl, such as a working-directory option for `proc.run`.
- A buildl command that runs the skills' scripts.
- Migration from CMake, Bazel, Gradle or other build systems.
- Agent evaluations, such as `claude plugin eval`.
- Packaging for agent tools other than Claude Code beyond the Agent Skills format.
- Running `claude` in CI.
- Installing from a git tag, or matching the skills to an installed buildl's version.
