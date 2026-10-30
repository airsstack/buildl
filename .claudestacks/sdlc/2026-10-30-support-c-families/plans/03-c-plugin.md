---
status: approved
created: 2026-10-30
depends-on: [02]
built-on: edb4e55efd6a
---

# The `c` Plugin Implementation Plan

**Goal:** the `c` plugin under `plugins/c/` declares per-file C and C++ targets, with every header
found by one discovery target per source, as spec §4–§7 define.

**Architecture:** five Lua modules in one plugin folder. `depfile.lua`, `flags.lua` and
`options.lua` are pure and use no `buildl`; `graph.lua` declares the targets of one call through a
`buildl` table it is handed; `api.lua`'s `new(buildl)` builds one instance of the public table over
a given `buildl`, and `main.lua`, the entry, returns `api.new(buildl)`. A test makes a fresh
instance per case by calling `api.new` with its own fake, since `require` caches modules. Tests run under `airsl test` in a new `cargo make plugins` task, with a
fake `buildl` for `graph.lua` (spec §4).

**Tech Stack:** Lua 5.4 as buildl's plugin runtime (airsl); `airsl test` for the tests; cargo-make;
GitHub Actions.

---

## Files that change

```
plugins/c/extension.toml       — [create] name "c", version "0.1.0", entry "main.lua", api 1, capabilities of spec §4
plugins/c/depfile.lua          — [create] parse(text), normalise(path); pure
plugins/c/flags.lua            — [create] includes, defines, shared_name, soname, rpath, platform_of(triple); pure
plugins/c/options.lua          — [create] check(kind, name, options) → filled options, or a raised error; pure
plugins/c/graph.lua            — [create] declare(buildl, kind, name, options): the targets of spec §5.2–§5.4
plugins/c/api.lua              — [create] new(buildl): builds one instance of the public table over the given `buildl`, with its own c.platform state
plugins/c/main.lua             — [create] the entry: returns api.new(buildl)
plugins/c/README.md            — [create] buildl.toml tables, API, §5.4–§5.5 rules, version probe, fixed program list
plugins/c/tests/depfile_test.lua — [create]
plugins/c/tests/flags_test.lua   — [create]
plugins/c/tests/options_test.lua — [create]
plugins/c/tests/fake_buildl.lua  — [create] a fake buildl that records declarations and serves discovery results
plugins/c/tests/graph_test.lua   — [create]
plugins/c/tests/api_test.lua     — [create]
Makefile.toml                  — [modify] new [tasks.plugins]
.github/workflows/ci.yml       — [modify] the skills job (it installs airsl, :144-145) runs `cargo make plugins`
```

## Facts this plan relies on

| Fact | Source |
|---|---|
| `-MM` rule form: escapes `\ `, `\`-newline continuation, absolute paths for absolute `-I`, `..` printed unresolved, `-MT` sets the target | spec P38, P39, P41, P57 |
| an `-isystem` folder's headers are left out of `-MM` | spec P40 |
| commands run from the workspace root; `buildl.folder()` (plan 02) gives the build file's folder | spec P12, §3 |
| a test needs its shared libraries as direct deps; `$deps` keeps declared order; `sh -c 'exec "$0"' $deps` runs only the first | spec P18, P19, P20 |
| gcc links only with `PATH`; compiles, discovery and `ar` need no environment | spec P52, P54 |
| an always-run target leaves dependents `Conditional` | spec P21 |
| discovery targets must be the same on both passes | spec P28 |
| `-dumpmachine` triples: `arm64-apple-darwin25.3.0`, `aarch64-linux-gnu`, `aarch64-unknown-linux-gnu` | spec P45, P53 |
| macOS install name defaults to the `-o` path; `-install_name @rpath/…`; Linux `-soname`; `$ORIGIN` / `@loader_path` rpaths | spec P48, P49, P50, P55 |
| a plugin's program must be in its manifest | spec P17 |
| a target name holds only ASCII letters, digits, `.`, `-`, `_` | spec P59 |
| each build file has its own plugin instance | spec P14 |
| `airsl test` runs `*_test.lua` files, each returning a table of test functions that fail by raising; `require` finds sibling modules under the default policy; `cargo make skills` stages tests and libraries in one temp folder and runs `airsl test --allow-read . .` | ai-skills spec P22, P27 (.claudestacks/sdlc/2026-10-29-ai-skills/spec.md:45, :50); Makefile.toml:422-431 |

## Order of work

### Task 1 — `depfile.lua` and the `plugins` task

**Files:**
- Create `plugins/c/depfile.lua` — `parse(text)` returns rules `{ target, prerequisites }` per spec §5.4; `normalise(path)` drops `.` segments, lets `..` remove the segment before it, and raises naming the path when it climbs above the start
- Create `plugins/c/tests/depfile_test.lua`
- Modify `Makefile.toml` — `[tasks.plugins]`, category "Plugins", an `@shell` script that checks `airsl` is on `PATH` (the message the skills task uses), copies `plugins/c/*.lua` and `plugins/c/tests/*.lua` into one temp folder, and runs `airsl test --allow-read . .` there; not a step of `dod`
- Modify `.github/workflows/ci.yml` — a `cargo make plugins` step in the `skills` job after `cargo make skills`

**Change:** Make-format discovery output is read into rules, and paths are normalised.

**Proof:** written first, seen failing, then passing — in `depfile_test.lua`:
- `one_rule_is_read` — `obj: src/a.c include/a.h` gives target `obj`, prerequisites `src/a.c`, `include/a.h`
- `a_continuation_is_joined` — a rule broken by `\` and a newline reads as one
- `an_escaped_space_is_kept_in_the_path` — `my\ dir/sp.h` gives `my dir/sp.h`
- `several_rules_are_kept_in_order` — two rules give two entries in input order
- `normalise_drops_dot_and_resolves_dotdot` — `cpp/src/../include/u.h` gives `cpp/include/u.h`; `./a/./b.h` gives `a/b.h`
- `normalise_refuses_a_climb_above_the_root` — `../x.h` raises naming `../x.h`

**Done when:** `cargo make plugins` passes, `cargo make dod` and `cargo deny check` are green.
Commit `feat(repo): add the c plugin's depfile reader and the plugins task`.

### Task 2 — `flags.lua`

**Files:**
- Create `plugins/c/flags.lua` — `includes(folder, dirs)`, `defines(map)`, `shared_name(name, platform)`, `soname(file, platform)`, `rpath(platform)`, `platform_of(triple)`, each as spec §6 defines; `folder` is passed in, so the module stays pure
- Create `plugins/c/tests/flags_test.lua`

**Change:** the helpers' values exist as pure functions.

**Proof:** written first, seen failing, then passing —
- `includes_at_the_root` — folder `""`, `{ "include" }` gives `{ "-Iinclude" }`
- `includes_below_the_root` — folder `"cpp"` gives `{ "-Icpp/include" }`
- `defines_are_sorted_and_typed` — `{ B = "1", A = true, C = false, D = 2 }` gives `{ "-DA", "-DB=1", "-DD=2" }`
- `shared_name_per_platform` — `libutil.so` for linux, `libutil.dylib` for macos, `nil` for `nil`
- `soname_per_platform` — linux `{ "-Wl,-soname,libutil.so" }`; macos `{ "-install_name", "@rpath/libutil.dylib" }`; `nil` gives `{}`
- `rpath_per_platform` — linux `{ "-Wl,-rpath,$ORIGIN" }`; macos `{ "-Wl,-rpath,@loader_path" }`; `nil` gives `{}`
- `an_unknown_platform_is_refused` — `"windows"` raises naming it, in each of the three
- `platform_of_reads_triples` — `arm64-apple-darwin25.3.0` → `macos`; `aarch64-linux-gnu` and `x86_64-pc-linux-gnu` → `linux`; `riscv64-unknown-elf` raises naming it

**Done when:** `cargo make plugins` passes, `cargo make dod` and `cargo deny check` are green.
Commit `feat(repo): add the c plugin's flag helpers`.

### Task 3 — `options.lua`

**Files:**
- Create `plugins/c/options.lua` — `check(kind, name, options)` for `kind` in `binary`, `library`, `shared_library`, `test`: refuses an unknown option, a wrong type, a missing or empty `srcs`, a source extension outside `.c .cc .cpp .cxx`, and `ldflags`/`ldlibs`/`linker`/`deps` on `library`; fills the defaults of spec §5.1 (`cc`, `cxx`, `linker` by language, `output`, `version_probe = false`, empty lists); returns the filled table, each source tagged C or C++
- Create `plugins/c/tests/options_test.lua`

**Change:** a call's options are checked and filled before any target is declared.

**Proof:** written first, seen failing, then passing — one test per row of spec §7's first two lines and §5.1's refusals:
- `an_unknown_option_is_refused` — `{ srcs = { "a.c" }, cflag = {} }` raises naming `cflag`
- `srcs_is_required_and_not_empty` — missing and `{}` both raise naming `srcs`
- `a_wrong_type_is_refused` — `cflags = "-O2"` raises naming `cflags`
- `an_unknown_extension_is_refused` — `a.f90` raises naming it
- `library_refuses_link_options` — `ldflags`, `ldlibs`, `linker`, `deps` each raise on `library`
- `defaults_are_filled` — `cc` `"cc"`, `cxx` `"c++"`, `version_probe` `false`, `output` the name (`<name>.bin` for `test`)
- `linker_follows_the_language` — all `.c` gives `cc`; one `.cpp` gives `c++`; an explicit `linker` is kept

**Done when:** `cargo make plugins` passes, `cargo make dod` and `cargo deny check` are green.
Commit `feat(repo): add the c plugin's option checks`.

### Task 4 — `graph.lua`

**Files:**
- Create `plugins/c/graph.lua` — `declare(buildl, kind, name, options)` declares, through the given table, the targets of spec §5.2 for one checked call: per source a discovery target named `<name>.<path with / as .>.deps` running `{ compiler, flags…, "-MM", "-MT", "obj", <folder>/<src> }` with `inputs { src, headers… }`, `deps { header_deps… }`, `stdout = true`; when every source has a result (`buildl.discovered`), an object target per source placed by spec §5.4 (parse, keep `obj`, normalise, the four rows, the `-isystem` hint), then the last step of spec §5.3 (`env = { "PATH" }` on links; `ar rcs $out $deps` for `library`; the binary and the `buildl.test` with the two run forms for `test`); with `version_probe`, one probe per language the call compiles — `<name>.version.c` running `{ cc, "--version" }`, `<name>.version.cpp` running `{ cxx, "--version" }` — `always = true`, `stdout = true`, a dependency of each compile of that language (the name carries the language because a target name cannot hold `+`, spec P59)
- Create `plugins/c/tests/fake_buildl.lua` — `new(folder, results)`: a fake `buildl` with `target`, `test`, `discover`, `discovered`, `folder` that records each declaration and returns canned discovery text
- Create `plugins/c/tests/graph_test.lua`

**Change:** one call declares discovery targets on the first pass, and objects and the last step once discovery has run.

**Proof:** written first, seen failing, then passing —
- `first_pass_declares_only_discovery` — no results: one `.deps` target per source, nothing else
- `discovery_commands_are_the_same_on_both_passes` — the discovery declarations of pass 1 and pass 2 are equal
- `an_object_takes_the_headers_it_read` — result `obj: cpp/src/a.c cpp/include/a.h`, folder `cpp`, `headers = { "include/a.h" }` gives inputs `{ "src/a.c", "include/a.h" }` and run ending `-c -o $out cpp/src/a.c`
- `a_dotdot_header_is_normalised` — `cpp/src/../include/a.h` places as `include/a.h`
- `an_absolute_header_is_refused_with_the_isystem_hint` — the error names the path and `-isystem`
- `a_header_outside_headers_is_refused` — without `header_deps`, the error names the path and "one build file per project"
- `a_dependency_output_is_left_to_header_deps` — with `header_deps`, `out/config.h` is not an input and the object depends on the `header_deps`
- `a_result_without_obj_is_refused` — names the source
- `cxx_sources_use_cxx_and_cxxflags` — a `.cpp` source's discovery and compile use `c++` and `cxxflags`
- `names_keep_the_extension` — `src/main.c` and `src/main.cc` give `app.src.main.c.o` and `app.src.main.cc.o`
- `binary_links_objects_then_deps_with_path` — `deps { objects…, ":lib" }`, run `{ linker, ldflags…, "-o", "$out", "$deps", ldlibs… }`, `env { "PATH" }`, `outputs { output }`
- `shared_library_adds_only_shared` — the run holds `-shared` and no other flag the call did not give
- `library_archives_without_env` — run `{ "ar", "rcs", "$out", "$deps" }`, no `env`
- `test_without_deps_runs_the_binary` — `buildl.test` with `deps { ":<name>.bin" }`, run `{ "$deps" }`
- `test_with_deps_runs_only_the_binary` — `deps { ":<name>.bin", ":lib" }`, run `{ "sh", "-c", 'exec "$0"', "$deps" }`
- `version_probe_is_off_by_default` — no `.version.` target
- `version_probe_adds_a_dependency_of_each_compile` — with one `.c` and one `.cpp` source: `<name>.version.c` (`always = true`, running `cc`) is in the C object's `deps`, `<name>.version.cpp` (running `c++`) in the C++ object's, and no declared name holds `+`

**Done when:** `cargo make plugins` passes, `cargo make dod` and `cargo deny check` are green.
Commit `feat(repo): add the c plugin's target graph`.

### Task 5 — The entry, the manifest and the README

**Files:**
- Create `plugins/c/extension.toml` — spec §4: `proc.run = ["cc", "c++", "gcc", "g++", "clang", "clang++", "ar", "sh"]`, `env.read = ["PATH"]`
- Create `plugins/c/api.lua` — `new(buildl)` returns a table: `binary`, `library`, `shared_library`, `test` (each `options.check`, then `graph.declare(buildl, …)`); `includes(dirs)` passing `buildl.folder()`; `defines`, `shared_name`, `soname`, `rpath` from `flags`; `depfile = { parse = … }`; `platform(options?)` declaring `:c.platform` (`{ cc, "-dumpmachine" }`, `stdout = true`) on its first call in that instance, returning `flags.platform_of` of the discovered text or `nil`, and raising when a later call names another `cc`
- Create `plugins/c/main.lua` — returns `require("api").new(buildl)`
- Create `plugins/c/README.md` — the user's `buildl.toml` of spec §4, the API and helpers, §5.4 and §5.5, the version probe and its `plan` cost, the fixed program list and the `path`-plugin copy for other tools
- Create `plugins/c/tests/api_test.lua` — each case calls `api.new` with a fresh fake `buildl` (the fake of `graph_test.lua`, kept in a shared test module `plugins/c/tests/fake_buildl.lua`)

**Change:** the plugin is loadable and its public table is complete.

**Proof:** written first, seen failing, then passing —
- `platform_is_nil_before_discovery` — no result: `nil`, and `:c.platform` declared once
- `platform_is_declared_once_per_instance` — two calls declare one target and return the same value
- `platform_with_another_cc_is_refused` — a second call with `cc = "clang"` raises naming `clang`
- `platform_reads_the_triple` — result `aarch64-linux-gnu\n` gives `linux`
- `the_public_table_holds_every_function` — the entry's table has exactly `binary`, `library`, `shared_library`, `test`, `platform`, `includes`, `defines`, `shared_name`, `soname`, `rpath`, `depfile`

**Done when:** `cargo make plugins` passes, `cargo make dod` and `cargo deny check` are green.
Commit `feat(repo): add the c plugin's entry and manifest`.

## Risks

- **Task 4 is the risky step**: it carries every rule of spec §5.2–§5.4, and a discovery command that differs between passes is refused by buildl (exit 5). `discovery_commands_are_the_same_on_both_passes` pins it.
- The fake `buildl` can drift from the real one; plan 04 runs the plugin against the real binary on Linux and macOS.
- Not done here: no example and no book chapter (plans 04, 05); no `env` option (spec §12).

## Proof

`cargo make plugins` and `cargo make dod` green after Task 5; the CI `skills` job runs `cargo make
plugins`.
