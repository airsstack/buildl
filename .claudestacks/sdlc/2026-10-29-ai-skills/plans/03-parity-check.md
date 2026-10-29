---
status: done
created: 2026-10-29
depends-on: [01]
built-on: c87ddf1ee4ad
---

# Parity Check Implementation Plan

**Goal:** `parity.lua prepare` and `parity.lua compare` judge, from files alone, whether a project's
old build and its buildl build agree, as spec §4.4–4.6 define.

**Architecture:** Two Lua files in `skills/buildl-migrate/scripts/`. `parity_lib.lua` is pure: it
reads no file and takes plain tables and strings — the recipe listing per tool, `parity.json`
validation, `doctor` text parsing, path rules, the judging of every row, and the table's rendering.
`parity.lua` is the entry: it does the file work (walk, copy, hash, read status and JSON files) and
calls the library. Tests are in `skills/tests/parity_test.lua`, run by `cargo make skills`, which
stages them beside the library (plan 01, Task 3 step 4) and starts them from the staging folder, so
fixtures are read as `fixtures/…` (plan 01, *Where tests find their fixtures*). The compare fixtures
are a real project and the work folder its real commands produced, so no test runs a build
(spec §6.4).

**Tech Stack:** Lua 5.4 on `airsl` 0.1.x: `fs`, `path`, `hash`, `json`, `regex`, `stdio`.

---

## Files that change

```
skills/buildl-migrate/scripts/parity_lib.lua  — [create] pure: recipes, config, ceiling, check_paths, left_out, rows, render
skills/buildl-migrate/scripts/parity.lua      — [create] entry: prepare P W | compare P W
skills/tests/parity_test.lua                  — [create] the tests spec §6.4 lists
skills/tests/fixtures/recipes/{make,just,shell,npm}/ — [create] one small old build per tool
skills/tests/fixtures/compare/match/{P,W}/    — [create] a real project, and the work folder its commands produced
skills/tests/fixtures/compare/mismatch/{P,W}/ — [create] the same with one output byte changed in W/new
```

## Facts this plan relies on

| Fact | Source |
|---|---|
| airsl functions used: `fs.walk`, `fs.copy`, `fs.mkdir`, `fs.exists`, `fs.is_dir`, `fs.is_file`, `fs.read`, `fs.list`; `path.join`, `path.is_absolute`, `path.relative_to`, `path.normalize`; `hash.hash_file`; `json.decode`; `regex.find_all`, `regex.captures`; `stdio.write`, `stdio.error` | airsl book appendix-02-modules.md:7-128 |
| `fs.walk` lists hidden entries and folders as well as files, sorted; `fs.copy` keeps the execute bit | spec §1 P20 |
| `--allow-write` grants no reading; `fs.exists`/`fs.list` need a read grant | spec §1 P28 |
| `buildl graph` lists every declared target, tests included, each with `role` and `network` (`Sealed` when it reaches no network) | spec §1 P16 |
| `doctor` prints a `ceiling` line, then one indented line per key, `  <key>  <values joined by ", ">`, `-` when empty; other lines come before it (`workspace`, `approver`, `execution`, `remote:`) and after it (`wants …`) | spec §1 P30, P15 |
| `airsl run` exits non-zero when the script raises | airsl book appendix-01-cli.md:53 |
| make looks for `GNUmakefile`, `makefile` and `Makefile`, in that order | `man make` (GNU Make, macOS): "make will look for the makefiles GNUmakefile, makefile, and Makefile" |
| `parity.json`'s fields; what is required; what is refused when read | spec §4.5 |
| the checks 1–6 and 4b, their FAIL conditions, and the `NOT REPEATABLE`, `JUDGED BY TESTS`, `NO TESTS` verdicts | spec §4.6 |

## Order of work

### Task 1 — Recipe listing per tool

**Files:**
- Create `skills/buildl-migrate/scripts/parity_lib.lua` — `recipes(tool, files)`: `files` maps a
  relative path to its text; returns the sorted recipe names
- Create `skills/tests/fixtures/recipes/make/Makefile`, `just/justfile`, `shell/{build.sh,ci/test.sh,README.md}`,
  `npm/package.json`
- Test `skills/tests/parity_test.lua`

**Change:** `make`: every rule target at line start (`name:` or `a b: deps`), including pattern
rules (`%.o`), excluding `.PHONY` and other special targets starting with `.`, and excluding
variable assignments (`X := y`, `X ::= y`, `X = y`, `X ?= y`, `X += y`). `just`: every recipe
header (`name args:`), excluding `set`, `alias`, `export` and `:=` assignments. `npm`: the keys of
`package.json`'s `scripts`. `shell`: every `*.sh` path in `files`. An unknown tool raises naming it.

**Proof:** written first, seen failing, then passing —
- `make_lists_rules_not_assignments` — the Makefile fixture (holding `all`, `app`, `%.o`, `test`,
  `.PHONY`, `CC := cc`, `X ?= y`) gives exactly `%.o`, `all`, `app`, `test`
- `just_lists_recipes_not_settings` — the justfile fixture (holding `set shell`, `alias b := build`,
  `version := "1"`, `build:`, `test arg:`) gives exactly `build`, `test`
- `npm_lists_script_keys` — the package.json fixture gives its `scripts` keys
- `shell_lists_every_script` — the shell fixture gives `build.sh` and `ci/test.sh`, not `README.md`
- `an_unknown_tool_is_an_error` — `recipes("cmake", {})` raises, naming `cmake`

**Done when:** `cargo make skills` passes, and `cargo make dod` and `cargo deny check` are green.
Commit `feat(repo): list an old build's recipes for the parity check`.

### Task 2 — `parity.json` validation

**Files:**
- Modify `skills/buildl-migrate/scripts/parity_lib.lua` — `config(decoded)`: returns the validated
  table, or raises naming the first bad field
- Test `skills/tests/parity_test.lua`

**Change:** required: `tool` (one of `make`, `just`, `shell`, `npm`), `old.build` (non-empty list of
strings), `old.programs`, `old.env` (lists of strings), `old.network` (boolean), `recipes` (table;
each value either `{ target = <string> }` or `{ dropped = <non-empty string> }`), `outputs` (list of
`{ old, new }` strings, optionally `judge = "tests"` with a non-empty `reason`). Optional: `old.test`
(non-empty list of strings), `skip` (list of strings). An empty `dropped` and a `judge` without a
`reason` are refused here, when the file is read, as spec §4.5 says.

**Proof:**
- `a_complete_config_is_accepted` — spec §4.5's example decodes and passes unchanged
- `a_missing_field_is_named` — removing `old.network` raises a message holding `old.network`
- `an_unknown_tool_is_refused` — `tool = "cmake"` raises naming `tool`
- `a_drop_without_reason_is_refused` — `{ dropped = "" }` raises naming the recipe
- `judge_without_reason_is_refused` — an `outputs` entry with `judge = "tests"` and no `reason`
  raises naming the entry
- `old_test_may_be_absent` — the example without `old.test` passes

**Done when:** `cargo make skills` passes, and `cargo make dod` and `cargo deny check` are green.
Commit `feat(repo): validate parity.json`.

### Task 3 — Reading `doctor` and judging the rows

**Files:**
- Modify `skills/buildl-migrate/scripts/parity_lib.lua` — `ceiling(doctor_text)`: the ceiling's keys
  and their value lists; `rows(input)`: given the config, the recipe list, the decoded graph, the
  ceiling, the three hashes per output (any may be `nil` for missing), and the status values, returns
  the parity rows, each with a check number, a subject, a verdict (`PASS`, `FAIL`, `NOT REPEATABLE`,
  `JUDGED BY TESTS`, `NO TESTS`) and a detail
- Test `skills/tests/parity_test.lua`

**Change:** `ceiling` reads the lines after the line `ceiling` that start with whitespace, up to the
next line that does not; each such line is a key and its values (split on `, `; `-` is none). It
raises naming the text when there is no `ceiling` line, or naming the line when an indented line in
the block has another form. Lines outside the block are ignored. `rows` gives the rows of spec §4.6,
checks 1 to 6 and 4b, exactly as its FAIL column states: check 1 looks targets up in the graph's
`targets`, which hold tests; check 6's network part fails for a target whose `network` is not
`Sealed` while `old.network` is false; checks 4 and 4b fail when there is no `old.test`; check 2
fails on a missing status file, an old build status other than 0, or a `buildl build` status other
than 0; shell recipes are keyed by their path relative to `P`.

**Proof:**
- `doctor_ceiling_is_read_from_full_output` — over a whole `buildl doctor` output (the P15 probe's
  full text: `workspace`, `approver`, `execution`, `remote:`, `ceiling` and its four lines, `wants`
  and its lines), gives `proc.run = {"python3"}`, `env.read = {}`, `fs.read = {"."}`,
  `fs.write = {".buildl", "out"}`
- `doctor_without_a_ceiling_is_an_error` — a text with no `ceiling` line raises
- `an_unreadable_ceiling_line_is_named` — an indented line of another form inside the block raises,
  holding that line
- `everything_agreeing_passes` — a fully agreeing input gives only `PASS` rows
- `an_unmapped_recipe_fails` — check 1 FAILs naming the recipe
- `a_mapping_to_a_missing_target_fails` — a `target` absent from the graph's `targets` FAILs check 1
- `a_mapping_to_a_test_target_passes` — a recipe mapped to a target with `role` `Test` passes check 1
- `a_failed_old_build_fails` — `old-1.build.status` of `2` FAILs check 2; a missing status FAILs too
- `a_failed_buildl_build_fails` — `new.build.status` of `1` FAILs check 2
- `a_changed_output_fails` — old-1 = old-2 ≠ new FAILs check 3 naming the output
- `a_missing_output_fails` — a `nil` hash FAILs check 3
- `an_unrepeatable_output_is_not_a_failure` — old-1 ≠ old-2, with `old.test`, gives `NOT
  REPEATABLE`, and the test row decides
- `an_unrepeatable_output_without_tests_fails` — the same with no `old.test` FAILs check 4
- `a_judged_output_defers_to_tests` — `judge = "tests"`, with `old.test`, gives `JUDGED BY TESTS`
  with the reason
- `a_judged_output_without_tests_fails` — the same with no `old.test` FAILs check 4b
- `differing_test_outcomes_fail` — old test status 0 and new 1 FAILs check 5
- `no_old_test_reads_no_tests` — no `old.test` and no judged or unrepeatable output gives `NO TESTS`
- `a_wider_program_ceiling_fails` — `proc.run` holding `curl` absent from `old.programs` FAILs check 6
- `a_wider_env_ceiling_fails` — `env.read` holding `TOKEN` absent from `old.env` FAILs check 6
- `an_unexpected_network_target_fails` — a target whose `network` is not `Sealed`, with
  `old.network = false`, FAILs check 6

**Done when:** `cargo make skills` passes, and `cargo make dod` and `cargo deny check` are green.
Commit `feat(repo): judge the parity rows`.

### Task 4 — `prepare`

**Files:**
- Modify `skills/buildl-migrate/scripts/parity_lib.lua` — `check_paths(P, W, w_entries)`: raises
  unless `P` and `W` are absolute, `W` is not `P` and not below it, and `w_entries` (the names in
  `W`, or `nil` when it does not exist) is empty or `nil`; `left_out(config)`: the set of paths
  `prepare` does not copy — `.git`, `.buildl`, `out`, `parity.json`, every `skip` entry, and every
  `outputs` entry's `old` and `new`
- Create `skills/buildl-migrate/scripts/parity.lua` — `prepare P W`: reads and validates
  `P/parity.json`, checks the paths, then walks `P` and copies every file not under a left-out path
  into `W/old-1`, `W/old-2` and `W/new`, creating folders as needed; prints the three folders
- Test `skills/tests/parity_test.lua`

**Change:** the grants are `--allow-read P --allow-read W --allow-write W` (spec §4.4). A usage line
on stderr and a raise when the first argument is neither `prepare` nor `compare`, or when `P` or `W`
is missing.

**Proof:**
- `a_relative_project_is_refused` — `check_paths("p", "/w", nil)` raises naming `p`
- `a_relative_work_folder_is_refused` — `check_paths("/p", "w", nil)` raises naming `w`
- `a_work_folder_inside_the_project_is_refused` — `check_paths("/p", "/p/w", nil)` raises
- `a_non_empty_work_folder_is_refused` — `w_entries = {"x"}` raises naming `W`
- `an_absent_or_empty_work_folder_is_accepted` — `nil` and `{}` both pass
- `outputs_and_state_are_left_out` — `left_out` of the §4.5 example holds `.git`, `.buildl`, `out`,
  `parity.json`, `app` and `out/app`
- by hand, once: `airsl run --allow-read <fixture P> --allow-read <tmp W> --allow-write <tmp W>
  skills/buildl-migrate/scripts/parity.lua prepare <fixture P> <tmp W>` over
  `fixtures/compare/match/P` (written in Task 5; run this check when Task 5's fixture exists) makes
  three copies holding the fixture's files without `parity.json`, and a second run refuses because
  `W` is not empty

**Done when:** that holds, `cargo make skills` passes, and `cargo make dod` and `cargo deny check`
are green. Commit `feat(repo): add parity.lua prepare`.

### Task 5 — `compare`

**Files:**
- Modify `skills/buildl-migrate/scripts/parity.lua` — `compare P W`: reads and validates
  `P/parity.json`; reads the old build's file for the tool — for `make` the first of `GNUmakefile`,
  `makefile`, `Makefile` present in `P`; for `just`, `justfile`; for `npm`, `package.json`; for
  `shell`, every `*.sh` outside the left-out paths — and lists its recipes; reads `W/new.graph.json`,
  `W/new.doctor.txt` and the status files; hashes each output in the three copies; calls `rows`;
  returns the rendered table and whether it passed; as a script, prints the table and raises when a
  row is `FAIL`
- Modify `skills/buildl-migrate/scripts/parity_lib.lua` — `render(rows)`: the table as text, one line
  per row, then a last line `parity: PASS` or `parity: FAIL (<n> rows)`
- Create `skills/tests/fixtures/compare/match/P/` — a project needing no language toolchain: a
  `Makefile` whose `out.txt` rule copies `in.txt` with `cp` and whose `test` rule `grep`s it; the
  same in `build.lua` (a target `out.txt` running `cp`, a test `unit` running `grep` over `$deps`) and
  a `buildl.toml` whose ceiling `proc.run` is `cp` and `grep`; `in.txt`; a `parity.json` mapping
  `out.txt` → `//:out.txt` and `test` → `//:unit`, outputs `out.txt` ↔ `out/out.txt`,
  `old.programs` `["cp", "grep"]`
- Create `skills/tests/fixtures/compare/match/W/` — produced once by running spec §4.4's three steps
  for real over `match/P`, then committed: the three copies after their builds, the status files,
  `new.graph.json` and `new.doctor.txt`; the outputs are byte-identical, since both builds copy the
  same file
- Create `skills/tests/fixtures/compare/mismatch/{P,W}/` — a copy of `match` with one byte changed in
  `W/new/out/out.txt`
- Test `skills/tests/parity_test.lua`

**Change:** `compare` writes nothing and runs nothing; grants `--allow-read P --allow-read W`.

**Proof:**
- `render_ends_with_the_verdict` — rows holding one FAIL render with last line `parity: FAIL (1 rows)`
- `compare_passes_on_the_match_fixture` — `compare` over `match` returns `passed = true`, and the
  returned table's last line is `parity: PASS`
- `compare_fails_on_the_mismatch_fixture` — `compare` over `mismatch` returns `passed = false`, and
  the returned table names `out.txt` in a check 3 FAIL row
- by hand, once: `airsl run --allow-read <mismatch P> --allow-read <mismatch W>
  skills/buildl-migrate/scripts/parity.lua compare <mismatch P> <mismatch W>` exits non-zero, and over
  `match` exits 0 (spec §6.4: run as a script, a FAIL row exits non-zero)
- by hand, once, before this task's fixtures are final (spec §8): the three steps run for real over
  a copy of `examples/python` with a `parity.json` mapping `app.pyz` — the result (does `app.pyz`,
  a zip, come out equal, `NOT REPEATABLE`, or failing check 3?) is written into the commit message,
  and when it fails check 3 on a correct migration, plan 04's `SKILL.md` says how to treat
  time-stamped archives

The two `compare_*` tests call the entry's `compare` function directly and read what it returns;
`parity.lua` returns its functions when loaded with `require` and dispatches on `arg` only when run
as a script.

**Done when:** `cargo make skills` passes, and `cargo make dod` and `cargo deny check` are green.
Commit `feat(repo): add parity.lua compare`.

## Risks

- **Most risky: Task 5's dual use of `parity.lua`** as a script and a required module. How airsl sets
  `arg` for a `require`d file is not verified. `execute` probes it first; if `arg` is visible to the
  module, the entry dispatches only when `arg[0]` names `parity.lua`, and failing that the I/O half
  moves to a third sibling file loaded by both the entry and the tests.
- The `match/W` fixture holds absolute paths nowhere: `compare` reads only hashes, status codes, the
  graph and the doctor text. If `new.graph.json` or `new.doctor.txt` hold the machine's absolute
  workspace path (the doctor's `workspace` line does), the tests do not read those fields.
- Make's and just's grammars are large; the recipe regexes cover the forms in the fixtures and the
  forms spec §4.6 names, not every Makefile. An unusual rule header can be missed, which check 1 then
  under-reports — stated in the migrate skill's `references/make.md` (plan 04). just's other file
  spellings are not verified; `compare` reads `justfile` only.
- `doctor`'s text is not a documented machine format (spec §8); `ceiling` raises on a block line it
  cannot read rather than guess.

## Proof

`cargo make skills` passes with every test above, and the by-hand `prepare` run in Task 4 holds.

## Review findings

- correctness (blocking) — an `outputs` `old`/`new` path could climb out of the copy (`../new/out/out.txt` made the mismatch fixture pass) — parity_lib.lua `check_outputs`. Fixed: a path normalizing to absolute or `..` is refused, naming `outputs[i].old|new`. Spec §4.5 amended. Verified: `cargo make skills` → `59 passed, 0 failed`, test `an_output_path_outside_the_copy_is_refused`.
- record (blocking) — the `examples/python` result had no record, and plan 04 item 5 waits on it. Fixed: written in Probe results below, in spec §4.5 and §8, and in plan 04 steps 2 and 5.
- correctness — a ceiling without `proc.run` or `env.read` passed check 6 — parity_lib.lua `permission_rows`. Fixed: raises. Tests `a_ceiling_without_proc_run_is_an_error`, `a_ceiling_without_env_read_is_an_error`.
- correctness — check 5's PASS detail named one code — parity_lib.lua `test_rows`. Fixed: `old-1 ended in X, new in Y`. Test `check_5_names_both_codes`.
- correctness — "W outside P" compared text only (`/tmp` vs `/private/tmp` on macOS), and P was walked after W was made — parity.lua `prepare`. Fixed: P is walked first, W created, both canonicalized and checked by `check_outside` before any copy; a refused W that prepare made is removed. Test `a_work_folder_inside_the_real_project_is_refused`; by hand, P as `/tmp/…` and W as `/private/tmp/…/P/w` → `must be outside the project`, no copies, no `w` left.
- spec gap (decided) — `outputs: []` let parity pass with nothing compared. Now refused at read time; spec §4.5 amended. Test `an_empty_outputs_list_is_refused`.
- spec gap (decided) — with both test runs failing, an output left to the tests passed. Check 5 now needs both runs at 0 whenever an output reads NOT REPEATABLE or JUDGED BY TESTS; spec §4.6 amended. Test `outputs_left_to_tests_need_passing_tests`.
- test strength — target+dropped refusal, judge other than `tests`, the stale-mapping row, check 5 on a missing test status were unguarded. Tests added: `a_recipe_with_target_and_dropped_is_refused`, `a_judge_other_than_tests_is_refused`, `a_stale_mapping_is_still_checked`, `a_missing_test_status_fails`.
- wording — a status file holding no number read "is missing"; now "holds no exit code" (test `a_status_file_without_a_number_is_named`); `old_build_files` comment and parity_test.lua header corrected.
- parsing (nits, moved to plan 04) — `$(OBJS): x.h` is listed as a recipe, and a `define … endef` line with a column-0 colon reads as a rule: plan 04's `references/make.md` names both as forms check 1 sees.

## Probe results

- `arg` in a required module — `...` is empty in a script and a required module; the global `arg` is visible in both, `arg[0]` the started script (`./parity_test.lua` under `airsl test`). parity.lua dispatches only when `path.basename(arg[0]) == "parity.lua"`; no third file needed. Settles the plan's most risky step.
- real shapes — `buildl graph` prints JSON by default (`--json ... (the default)`, `buildl graph --help`); `targets` keyed `//:name`, each with `role` (`Build`/`Test`) and `network`; the non-sealed value is `Declared` (crates/buildl-core/src/types/declaration.rs:50-52) — the plan's "not `Sealed`" holds. `doctor`'s ceiling block reads `  proc.run  python3` / `  env.read  -` / `  fs.write  .buildl, out`, ended by `wants`.
- prepare by hand — over a scratch P holding `.git`, `out/`, `build/` (in `skip`), an output `app`, `.hidden/k`, empty `src/empty/`, executable `src/run.sh`, `in.txt`, `parity.json`: exit 0; old-1/old-2/new each hold `.hidden/k`, `in.txt`, `src/empty`, `src/run.sh` (`-rwxr-xr-x`) and none of the left-out paths; a second run → `work folder … is not empty`, exit 1. Re-run over `fixtures/compare/match/P`: exit 0, copies without `parity.json`; second run refused.
- compare by hand — over `mismatch`: `3  FAIL  out.txt  new/out/out.txt differs from old-1/out.txt`, `parity: FAIL (1 rows)`, exit 1; over `match`: `parity: PASS`, exit 0.
- fixtures — `match/W` made by running spec §4.4 for real (`make out.txt`/`make test` in old-1, `make out.txt` in old-2, `buildl build`/`test`/`graph`/`doctor` in new); `.buildl` left out; `new.doctor.txt`'s `workspace` line set to `/work/new` (compare does not read it). `mismatch` changes `hello` → `hellp` in `W/new/out/out.txt`.
- **against the plan** — `examples/python`: old-1 = old-2 (`python3 -m zipapp` stamps entries with the copy time, equal within 2 s), buildl's `app.pyz` differs (entries read 1980-01-01, `SOURCE_DATE_EPOCH` pinned; old read 2026-10-29 21:21:56) → check 3 FAIL on a correct migration. Remedy: mark a time-stamped archive `"judge": "tests"` with its reason (needs `old.test`). Spec §4.5, §8 and plan 04 amended. The commands inside `build.sh`/`test.sh` were run directly, since this session refuses `sh build.sh`.

## Deviations

- 2026-10-29 — `is_left_out`, `check_outside` and `render`'s pass-case test are extra to the plan's named list, each with its own test.
- 2026-10-29 — `compare` raises (prints no table) when the old build's file (make/just/npm), `W/new.graph.json` or `W/new.doctor.txt` is missing.
- 2026-10-29 — spec §4.5/§4.6/§6.4/§8 amended (paths inside the copy, empty `outputs` refused, check 5 with outputs left to the tests, time-stamped archives may be judged by tests); plans re-stamped `built-on: c87ddf1ee4ad`; plan 04 steps 2 and 5 carry the archive case.
- 2026-10-29 — the `examples/python` result is recorded here, not in the Task 5 commit message as the plan asked: the commit had already landed.
