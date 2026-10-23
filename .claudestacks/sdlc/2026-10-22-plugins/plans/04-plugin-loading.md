---
status: approved
created: 2026-10-22
depends-on: [01, 02, 03]
built-on: 5fa60e343706
---

# Plugin Loading Implementation Plan

**Goal:** A build file calls `buildl.use(name)` and gets the plugin's module, whose declarations carry the plugin's origin.

**Architecture:** All Lua work stays in `buildl-lua` (airsl's only crate). A new `plugin`
module reads `extension.toml` with airsl's `Manifest` (P1–P4) and turns it into the core
`PluginRef` before anything crosses the port; it loads the entry file as a chunk in the calling
file's engine under a per-instance environment table (mlua `Chunk::set_environment`, P7), whose
`buildl` and `airsstack.buildl` are a new primitives table stamped with that instance's origin,
and whose `require` is buildl's own, confined to the plugin directory (airsl's needs the
`Restricted` surface, P5–P6). Instances and modules are cached per origin chain for one
evaluation (spec §4). `LuaSource::new` gains the checked plugin directories, which the
composition root takes from plan 03's `Checked`.

**Tech Stack:** Rust, `buildl-lua` (airsl 0.1.4, `airsl::mlua`), `buildl`, `buildl-cli`,
`cargo make dod`, `cargo deny check`. Guideline: `claudestacks-guideline-rust:rust-guidelines`
(static dispatch; the only `dyn` stays airsl's `HostModule` seam, `engine.rs:26`).

**Content authority:** spec §4, §5 (stamping), §7 (manifest and `use` rows), §8 (`buildl-lua`
row), §9 (`buildl-lua` unit rows; e2e path plugin build).

---

## Files that change

```
crates/buildl-lua/src/plugin/mod.rs          — [create] export-only: Plugins, PluginLoader
crates/buildl-lua/src/plugin/manifest.rs     — [create] read extension.toml via airsl; refusals; → PluginRef
crates/buildl-lua/src/plugin/environment.rs  — [create] the per-instance environment table
crates/buildl-lua/src/plugin/require.rs      — [create] buildl's confined `require` for one instance
crates/buildl-lua/src/plugin/loader.rs       — [create] `use`: chain cache, cycle refusal, entry chunk load
crates/buildl-lua/src/module.rs              — [modify] primitives table built by one fn taking an Origin; install `use`; snapshot of the language globals at install
crates/buildl-lua/src/staging.rs             — [modify] `stage` takes the Origin and records it on StagedDeclaration
crates/buildl-lua/src/source.rs              — [modify] LuaSource::new gains `plugins: BTreeMap<PluginName, PathBuf>`
crates/buildl-lua/src/lib.rs                 — [modify] `mod plugin`
crates/buildl-lua/tests/flows/plugins.rs     — [create] flows over fixture plugins
crates/buildl-lua/tests/flows/main.rs        — [modify] `mod plugins;`
crates/buildl-lua/tests/fixtures/plugins/…   — [create] fixture plugins: ok, nested require, escape attempts, bad manifests, wrapper
crates/buildl/src/local_ports.rs             — [modify] Wiring::source (:157) passes `Checked` dirs to LuaSource::new (:173); test calls (:525, :541, :551)
crates/buildl/tests/e2e/plugins.rs           — [modify] a path plugin builds end to end
crates/buildl/tests/fixtures/plugin_path/…   — [create] workspace with a path plugin
crates/buildl-cli/tests/cli/output.rs        — [modify] plan, doctor and a refusal through the binary with a plugin target
```

## Order of work

### Task 1 — Stamped primitives and the origin on staged declarations

**Files:** `module.rs`, `staging.rs`.

**Change:** the code that installs `target`, `test`, `rule`, `alias`, `option`, `subdir` and
`sources` (`module.rs:64-127`) moves into one function that builds a primitives table for a
given `Origin`, sharing the evaluation's `Staging` and `Sources`. `install` builds the file's
table with `Origin::BuildFile`, binds it as `airsstack.buildl` and `buildl` as today, and records
a snapshot of the language globals (every global name and value present at the end of `install`
except `buildl`, `airsstack`, `_G` and `require`), so a build file reassigning a global later
cannot change what a plugin sees. `Staging::stage` records the origin on each
`StagedDeclaration` (plan 01 added the field and staged `BuildFile`).

**Proof:** written first, seen failing, then passing —
- `module` unit `a_table_built_for_an_origin_stamps_its_declarations`
- `module` unit `the_file_table_still_stamps_build_file` (existing flows stay green)
- `module` unit `the_language_snapshot_excludes_buildl_airsstack_and_g`

**Done when:** `cargo test -p buildl-lua` passes; `cargo make dod` green. Commit
`refactor(buildl-lua): build the primitives table for an origin`.

### Task 2 — Reading a plugin's manifest

**Files:** `plugin/mod.rs`, `plugin/manifest.rs`, `lib.rs`, fixtures.

**Change:** `read_manifest(dir, expected: &PluginName) -> Result<(PluginRef, entry)>` calls
`airsl::extension::Manifest::from_dir(dir, &Variables::default())`; refuses: a `name` not equal
to `expected`; any `fs.read`/`fs.write` (`required().fs_read()`/`fs_write()` non-empty); any
module flag (`required().modules()` non-empty); a non-empty optional block (`optional()` not
empty, P21); any set limit (`limits().memory()` or `instructions()` is `Some`). An unsupported
`api` is airsl's own refusal (P3). `proc.run` / `env.read` parse into `Program` / `EnvName`.
Every refusal is an `EvaluationFailure` of the calling file whose diagnostic names
`<plugin dir>/extension.toml` and the reason.

**Proof:** written first, seen failing, then passing (unit, fixture dirs) —
- `a_valid_manifest_becomes_a_plugin_ref` — name, version, caps
- `a_plugin_table_is_refused` (P2), `api_2_is_refused` (P3), `a_mismatched_name_is_refused`
- `fs_read_is_refused`, `fs_write_is_refused`, `a_module_flag_is_refused`,
  `a_non_empty_optional_block_is_refused`, `a_memory_limit_is_refused`,
  `an_instructions_limit_is_refused`
- `a_missing_entry_is_refused` — airsl's own check (`Manifest::validate` returns
  `ManifestInvalid` for "an entry that leaves `dir` or does not exist",
  `airsl-0.1.4/src/extension/manifest.rs:203-216`)

**Done when:** `cargo test -p buildl-lua plugin::manifest` passes; `cargo make dod` green. Commit
`feat(buildl-lua): read a plugin's extension.toml`.

### Task 3 — `buildl.use`, the environment and `require`

**Files:** `plugin/environment.rs`, `plugin/require.rs`, `plugin/loader.rs`, `module.rs`,
`source.rs`, `tests/flows/plugins.rs`, `tests/flows/main.rs`, fixtures.

**Change:** `LuaSource::new(root, limits, skipped, plugins)`. The stamped table gains `use`:
- name not in `plugins` → refusal naming the file and the name; chain already holding the name
  → refusal (`use` cycle);
- chain = caller's chain + `PluginRef` (from Task 2); an instance for that chain already loaded
  in this evaluation is returned as is;
- environment (spec §4 table): `buildl` = a primitives table stamped `Origin::Plugins(chain)`
  (Task 1) with its own `use`; `airsstack` = a new table holding the curated modules taken from
  the file's `airsstack` (`json`, `path`, `regex`, `hash`, `glob`) and `buildl` = the stamped
  table; `require` = this instance's loader; `_G` = the environment; plus the language snapshot
  (Task 1); nothing else;
- `require("a.b")`: each dot-separated part non-empty, no `/`, `\` or `..`; path
  `<dir>/a/b.lua` canonicalized and required to lie under the canonical plugin dir; a module
  being loaded requiring itself (directly or through a chain) is refused; result cached per
  instance; the module chunk runs under the same environment;
- the entry file runs as a chunk named `<plugin dir relative to the workspace root>/<entry>`;
  its return value is `use`'s result.
`buildl.sources` on a stamped table walks the calling file's directory (shared `Sources`).

**Proof:** written first, seen failing, then passing (`tests/flows/plugins.rs`) —
- `use_returns_the_plugin_module_and_its_targets_carry_its_origin`
- `a_nested_require_loads_once`
- `require_with_dotdot_slash_or_empty_part_is_refused`, `require_through_a_link_out_is_refused`,
  `a_self_require_is_refused`
- `airsstack_buildl_in_a_plugin_is_the_stamped_table` — a plugin declaring through
  `airsstack.buildl.target` gets origin `[p]`
- `g_buildl_in_a_plugin_is_the_stamped_table`, `a_file_global_is_absent_in_a_plugin` — the build
  file sets `x = buildl` before `use`; the plugin sees `x == nil`
- `go_direct_and_through_ourgo_are_separate_instances` — `[go]` and `[ourgo, go]` both appear
- `an_unknown_plugin_is_refused`, `a_use_cycle_is_refused`
- `plugin_lua_error_names_the_plugin_file_and_line`
- `print_and_math_random_are_absent_in_a_plugin`
- `sources_in_a_plugin_walks_the_calling_file_s_directory` — `lib/build.lua` uses a plugin
  calling `buildl.sources("*.c")`; the list is `lib`'s files, relative to `lib`

**Done when:** `cargo test -p buildl-lua` passes; `cargo make dod` green. Commit
`feat(buildl-lua): buildl.use loads a plugin under its own environment`.

### Task 4 — Wire plugins into the workspace, end to end

**Files:** `buildl/src/local_ports.rs`, `buildl/tests/e2e/plugins.rs`,
`buildl/tests/fixtures/plugin_path/…`, `buildl-cli/tests/cli/output.rs`.

**Change:** `Wiring::source` (fallible since plan 03) passes `Checked`'s directories to `LuaSource::new`; test
constructions (`local_ports.rs:525,541,551`) pass an empty map.

**Proof:** written first, seen failing, then passing —
- e2e `a_path_plugin_builds_and_caches` — fixture `plugin_path` with `[plugins.hello] path`,
  a plugin generating a target that runs a host tool allowed by manifest, cap and ceiling;
  `build` builds, second `build` is all cached
- e2e `a_plugin_target_outside_its_cap_is_refused_before_anything_runs` (exit class 4)
- CLI `plan_names_the_plugin_behind_a_want` — `buildl plan` stdout holds
  `declared by plugin hello@0.1.0 for //:greet`
- CLI `doctor_shows_plugin_attribution_and_exits_0`
- CLI `graph_json_shows_the_plugin_chain`

**Done when:** `cargo test --workspace` passes; `cargo make dod` and `cargo deny check` green.
Commit `feat(buildl): load path plugins in every command`.

## Risks

- Most risky: Task 3's environment. Any route from plugin code to the file's unstamped table
  defeats narrowing; the named escape tests are the guard. A route not covered: a build file that
  stores its `buildl` in a shared library table (`string`, `table`) — that is the build file's
  explicit act, as passing it as an argument is (spec §4).
- Copying the curated modules into a new `airsstack` table assumes they are plain tables whose
  functions do not depend on the table they sit in; a probe in `execute` confirms
  `airsstack.json.decode` works from the copy.

## Proof

`cargo make dod`, `cargo deny check` green; a workspace with a path plugin builds, caches,
plans with attribution, and refuses a plugin target outside its cap with exit 4.
