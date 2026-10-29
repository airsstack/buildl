---
name: buildl-migrate
description: Move a project's Makefile, justfile, shell build scripts or npm scripts to buildl
  (build.lua and buildl.toml), and prove with a parity check that the new build makes the same
  outputs and passes the same tests. Use when the user asks to migrate, convert or port a build
  to buildl.
license: Apache-2.0
compatibility: Requires the buildl and airsl commands on PATH.
---

# buildl-migrate

## A. What this skill does

It moves a project's build to buildl and proves the new build matches the old one. The old build is
a Makefile, a justfile, plain shell scripts, or the `scripts` in `package.json`.

| The agent writes | In the project folder `P` |
|---|---|
| the new build | `build.lua` and `buildl.toml`, under the rules of the `buildl` skill |
| the mapping the check reads | `parity.json` |

| The agent never | Why |
|---|---|
| edits or deletes the old build files | the old build must keep working |
| deletes `parity.json` | the user decides |
| deletes the work folder `W` | the user decides |

The build-file rules (targets, `run`, `env`, the ceiling, exit codes) are in the `buildl` skill. Read
it first. This skill does not repeat them.

## B. The steps

```text
 1 read the old build
 2 write build.lua and buildl.toml            (buildl skill)
 3 list what the old build uses               (old.programs, old.env, old.network)
 4 list the files an earlier run left in P    (skip)
 5 write parity.json
 6 prove it with the parity check             (C)
 7 read the table                             (D)
```

1. Read every recipe of the old build. Run nothing yet.
2. Write `build.lua` and `buildl.toml`. Run `buildl check` until it passes. Each old recipe becomes a
   `target`, or a `test` when it runs tests. A tool's features and their buildl form are in `references/`
   (see G).
3. Go through the old build recipe by recipe and list:
   - `old.programs`: every program a recipe runs
   - `old.env`: every environment variable a recipe reads
   - `old.network`: `true` when any recipe downloads or calls out, else `false`

   Keep `buildl.toml`'s `proc.run` and `env.read` inside these lists. Check 6 fails when the ceiling
   allows a program or variable the old build did not use.
4. List in `skip` the intermediate files an earlier run left in `P`. Each entry is one path relative
   to `P`, never a pattern: `*.o` skips nothing. Write each `.o` path, or the folder that holds
   them. A folder entry leaves out everything below it, and the folder itself. So skip a folder only
   when the old build does not write into it, or creates it first (`mkdir -p`). A file listed in
   `outputs` is already left out and needs no entry. If the leftovers are copied, the old build
   starts from a half-built tree and the check proves little.
5. Write `parity.json` (below).

`parity.json`:

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

| Field | Meaning |
|---|---|
| `tool` | `make`, `just`, `shell` or `npm`: how `compare` reads the old build's recipe names |
| `old.build` | the old build's command |
| `old.test` | the old build's test command. Leave it out for a project with no tests |
| `old.programs`, `old.env`, `old.network` | what the old build uses (step 3) |
| `recipes` | every old recipe, mapped to the buildl target that replaces it (`target`), or `dropped` with a reason. Never both |
| `outputs` | pairs of files to compare: `old` path and `new` path, relative to the copy |
| `skip` | further paths to leave out of the copies (step 4) |

Every field but `old.test` and `skip` is required. A bad `parity.json` is refused by both commands
with a message that names the field, and `prepare` then copies nothing. The rules live in
`parity_lib.config` (`scripts/parity_lib.lua`): read it for the edges.

An `outputs` entry may carry `"judge": "tests"` and a `"reason"`. Its row is then decided by the
tests, not by comparing bytes. Use it in two cases only, each with its reason, and each needs
`old.test`:

- the C and C++ case in E
- an archive that stamps its entries with file times (a zip, jar or tar). buildl pins
  `SOURCE_DATE_EPOCH`, so its zip entries read 1980-01-01 and the archive differs from the old
  build's even when the migration is right. Without `"judge": "tests"` check 3 fails a correct
  migration

## C. The parity check

`P` is the project folder. `W` is a work folder you name.

- Both are absolute paths.
- `W` lies outside `P`.
- `W` is empty or does not exist yet.
- Pick a `W` with no spaces in it. The commands below are POSIX shell and break on a space.

`<skill dir>` is the folder this `SKILL.md` is in. Write the script as `<skill dir>/scripts/parity.lua`.

Replace `P`, `W`, `<old.build>` and `<old.test>` with the real values, then run the three steps in
order.

Step 1, prepare. It copies `P` to `W/old-1`, `W/old-2` and `W/new`. It leaves out `.git`, `.buildl`,
`out`, `parity.json`, your `skip` list and every path in `outputs`. It reads `P` and `W`, writes only
`W`, and runs nothing.

```sh
airsl run --allow-read P --allow-read W --allow-write W <skill dir>/scripts/parity.lua prepare P W
```

Step 2, run. These are your own shell's commands. The script cannot run them for you.

```sh
cd W/old-1 && <old.build> ; echo $? > W/old-1.build.status
cd W/old-1 && <old.test>  ; echo $? > W/old-1.test.status
cd W/old-2 && <old.build> ; echo $? > W/old-2.build.status
cd W/new   && buildl build ; echo $? > W/new.build.status
cd W/new   && buildl test  ; echo $? > W/new.test.status
cd W/new   && buildl graph       > W/new.graph.json
cd W/new   && buildl doctor      > W/new.doctor.txt
```

Leave out the second line when `old.test` is not set. Run every line even when one fails: a failed
build is a row in the table, and the status file is what shows it.

Step 3, compare. It reads only, writes nothing, runs nothing, and prints the table. It exits non-zero
when any row is `FAIL`.

```sh
airsl run --allow-read P --allow-read W <skill dir>/scripts/parity.lua compare P W
```

Never change `[approver]` to make this work. What happens depends on it:

| Approver | In `W/new` | The agent |
|---|---|---|
| `manifest` | `buildl build` and `buildl test` run | nothing |
| `interactive` | refused: `W/new` is a new place nobody approved, and your shell has no terminal | asks the user to run the `W/new` lines of step 2 in their own terminal, then runs step 3 |
| `deny` | nothing runs, in any place, the user's terminal included | says the parity check cannot run under `deny`, and stops |

## D. Reading the table

The last line is `parity: PASS`, or `parity: FAIL (<n> rows)`. The verdict words and what you do:

| Verdict | Check | Meaning | The agent |
|---|---|---|---|
| `PASS` | any | the check holds | nothing |
| `FAIL` | 1 | a recipe is neither mapped nor dropped, or a mapped target is not in the graph | maps or drops the recipe in `parity.json`, or fixes the target name in `build.lua` |
| `FAIL` | 2 | a build ended with a status other than 0, or its status file is missing or holds no exit code | `buildl log <target>`, then fixes `build.lua`. If the old build failed, stops and tells the user |
| `FAIL` | 3 | old-1 and old-2 agree, and the new output differs, or a file is missing (the row names where) | fixes the new build. If the file is missing on the old side, the `old` path in `parity.json` is the mistake: fixes that |
| `FAIL` | 4 or 4b | the old build is not repeatable, or the file is judged by tests, and there is no `old.test` | tells the user nothing can judge the file. Does not invent a test |
| `FAIL` | 5 | the old tests and the buildl tests ended differently, or an output left to the tests had a failing run | fixes the new build |
| `FAIL` | 6 | the ceiling allows a program or variable the old build did not use, or a target reaches the network | narrows `buildl.toml` and the target. If the old build did use it, fixes `parity.json`'s `old.*` list |
| `NOT REPEATABLE` | 4 | old-1 and old-2 differ for this file, so the tests decide it | nothing. Check 5 judges it |
| `JUDGED BY TESTS` | 4b | the entry carries `"judge": "tests"`. The row shows its reason | nothing. Check 5 judges it |
| `NO TESTS` | 5 | the project has no `old.test` | nothing |

`compare` can also stop with an error and print no table. The message names the cause:

| The message says | Cause | The agent |
|---|---|---|
| it cannot read `new.graph.json` or `new.doctor.txt` | that step-2 line was not run, or ran somewhere else | runs the line in `W/new` and runs step 3 again |
| the doctor output has no `ceiling` line, or no `proc.run` or `env.read` line | `buildl doctor` failed and wrote an error into the file | runs `buildl doctor` in `W/new`, reads its error, fixes the new build |
| there is no `Makefile`, `justfile` or `package.json` in the project folder | `tool` in `parity.json` does not match the old build, or the file is not named as `compare` reads it (see the reference for the tool) | fixes `tool`, or tells the user the file name |

A `FAIL` is fixed in the new build. Never edit `parity.json` to make a row pass: no dropping a recipe
that does build, no widening `old.*`, no `"judge": "tests"` to hide a byte difference. The one
legitimate fix in `parity.json` is a mistake in what you wrote about the old build.

Check 5 compares only whether the old test run and the buildl test run each passed. It does not count
single tests. Whether each old test recipe has a buildl test target is check 1.

## E. Two cases that stop and ask

An archive that stamps file times is a third case with its own answer: see `"judge": "tests"` in B.

### C and C++ compiled one file at a time

The old build compiles each `.c` or `.cc` to its own `.o` and links them (`%.o: %.c`). buildl v1 has
no per-`.o` targets, because a compiler reads headers nobody declared. Do not write them. Stop and
offer the user one target that compiles the whole program in one command, with every source and
every header declared as an input:

```lua
local b = buildl

local sources = b.sources("src/*.c")
local headers = b.sources("src/*.h")

b.target("app", {
  inputs = { sources, headers },
  run = { "cc", "-o", "$out", sources },
})
```

Name the sources in `run`, not `$in`: `$in` hands the headers to the compiler too, and a compiler
given a header and `-o` refuses. The headers stay inputs, so the sandbox holds them and the key
covers them.

This block is for the build file in the project's root folder. `sources` names start at the folder
of the build file, but a path in `run` starts at the workspace root. In a build file in a folder,
such as `src/build.lua`, put the folder in front of each name in `run`:

```lua no-check
-- in src/build.lua: run paths start at the workspace root
local named = {}
for _, f in ipairs(sources) do
  named[#named + 1] = "src/" .. f
end
```

Tell the user these two things before they answer:

- any edit rebuilds the whole program
- the binary differs from the per-file build's byte for byte, so its output is marked
  `"judge": "tests"` and the tests decide it. A project with no test command cannot pass parity
  this way

Check 1 lists the old `%.o: %.c` recipe. If the user accepts, map the recipes that build the
program (`app`) to this target, and drop the per-file one: `"dropped": "per-file objects: one
whole-program target replaces them"`.

If the user accepts, mark that output in `parity.json`:

```json
{ "old": "app", "new": "out/app", "judge": "tests", "reason": "whole-program build differs from the per-file link byte for byte" }
```

If the user declines, stop with nothing changed.

### A recipe that is not a build step

A recipe that deploys, tags, pushes or otherwise acts outside building files (`deploy`, `release`)
is not dropped silently and not forced into a target. List such recipes and ask whether to keep them
in the old build or drop them. Record the answer as `dropped` with its reason, so check 1 still
adds up.

## F. What is left afterwards

The old build files, `parity.json` and `W` stay where they are. Deleting any of them is the user's
decision. Tell the user their paths and the table's last line.

## G. References

| File | Read it when |
|---|---|
| `references/make.md` | the old build is a Makefile |
| `references/just.md` | the old build is a justfile |
| `references/shell.md` | the old build is shell scripts |
| `references/npm.md` | the old build is npm `scripts` in `package.json` |

Each lists the tool's features and their buildl form, and which recipe forms `compare` reads for that
tool. Other forms are not seen by check 1.
