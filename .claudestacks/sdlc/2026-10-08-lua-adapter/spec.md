---
status: approved
created: 2026-10-08
---

# Spec: I-lua — evaluating a real `build.lua` through airsl

This spec delivers `buildl-lua`'s first code: `LuaSource`, the airsl-backed implementation of
`buildl-core`'s `DeclarationSource` port. It evaluates one build file per call on a fresh, curated
airsl engine. It installs the `buildl` module table under the `airsstack` root and binds the same
table to the `buildl` global. The table records each declaration call as written, flattening one
level of nested lists, and its `buildl.sources` is a host-side globbing read. Every failure maps onto
I3's closed `EvaluationFailure` set. The spec closes the `math.random` determinism hole that design
§12.1 leaves open and records the one entropy source it does not close (address-derived text). It
adds the per-file staging cap (roadmap follow-up #12), raises the airsl floor (#4), and refreshes
`buildl-lua`'s crate documentation (#3). `buildl-core` is not changed.

---

## 1. Design premises, and where each one comes from

Every claim in this section was checked in this task against the artifact named.

|Premise|Evidence|
|---|---|
|The port: `fn evaluate(&self, file: &BuildFile) -> Result<Evaluated>`; a missing file is `Evaluated::Absent`, not an error|`crates/buildl-core/src/ports/declaration_source.rs` (trait and its doc)|
|`BuildFile` wraps a `Provenance`; `file()` is the workspace-relative path, `directory()` the declaring directory|`crates/buildl-core/src/types/build_file.rs:50-75`|
|Load builds each `BuildFile` from `(directory, entry)` and stops on `Absent` with `MissingBuildFile`|`crates/buildl-core/src/load/traversal.rs:44-56`|
|Load joins each input under the declaring directory|`crates/buildl-core/src/load/traversal.rs:437-441` (test `inputs_join_under_the_declaring_directory`)|
|The staged shapes: `StagedFile { declarations, subdirs }`, `StagedDeclaration { order, item }`, `StagedTarget`'s ten fields (`inputs: Vec<Written<SourcePath>>` is flat), `StagedRule`, `StagedAlias`, `StagedSetting`, all with `pub` fields|`crates/buildl-core/src/types/build_file.rs:89-180`, `:139`|
|`Error::Evaluation { provenance, failure, diagnostic }`, rendered `{provenance}: {failure}: {diagnostic}`; `EvaluationFailure` = `Syntax`, `Runtime`, `Refused`, `LimitReached { limit }`, `UnknownField { field }`, `WrongFieldType { field }`; `EvaluationLimit` = `Instructions`, `Memory`, `Staging`|`crates/buildl-core/src/error.rs:61-69`, `:127-134`, `:152-174`|
|`EvaluationLimit::Staging`'s rustdoc reads "The cap on how many declarations one file may stage", which is a count|`crates/buildl-core/src/error.rs:132`|
|`Written<T>::new` is infallible and `as_written` returns the text. `Diagnostic::new` takes any text|`crates/buildl-core/src/types/written.rs:34-43`, `types/diagnostic.rs:23`|
|The `buildl` table is one airsl `HostModule` under the default `airsstack` root, bound to the `buildl` global during install|`docs/design.md:116` (§5)|
|`b.target`'s closed table: `rule` or `run`, `inputs`, `deps`, `outputs`, `env`, `network = true`, `always = true`|`docs/design.md:149` (§5)|
|"nested input lists flatten": `{ b.sources(...), b.sources(...), "go.mod", "go.sum" }` passed as `inputs`|`docs/design.md:314-315` (§9.1), the same shape at `:333` (§9.2) and `:344` (§9.3)|
|Declaration ceilings: `[declaration] memory = "16MB"`, `instructions = 10_000_000`|`docs/design.md:103-104` (§4)|
|Declaration is safe on untrusted input; "the declaration engine holds one grant (filesystem read on the workspace, for source globbing)"|`docs/design.md:56` (§2); the same sentence in `README.md:42`|
|The curated module set is `json`, `path`, `regex`, `hash`, `glob`; `os` is dropped|`docs/design.md:583` (§12.1 item 1)|
|`globset` and `walkdir` are catalogued for `b.sources()`, with results "sorted host-side"|`Cargo.toml:44-48`|
|`deny.toml` bans `walkdir`, `tempfile` and `globset` with `wrappers = ["airsl"]`|`deny.toml:93-98`|
|Where `b.sources()` globbing runs is an open question|`docs/architecture-building-blocks.md:372` (§11)|
|airsl requirement is `"0.1"`; `Cargo.lock` resolves `0.1.4`|`Cargo.toml:24`; `Cargo.lock` (`name = "airsl"`, `version = "0.1.4"`)|
|airsl withholds unsafe globals, then installs modules|airsl-0.1.4 `src/builder.rs:109`, `:118`|
|Each module gets a fresh table. The root table is set as a global only **after** every module installs, so `airsstack` is unreachable from `install`|airsl-0.1.4 `src/builder.rs:196-202`|
|`LanguageSurface::Minimal` loads `STRING \| TABLE \| MATH \| UTF8`|airsl-0.1.4 `src/sandbox/language_surface.rs:86`|
|`HostModule: Send + Sync` with a public `install`; airsl builds mlua with `send`|airsl-0.1.4 `src/modules/registry.rs:97-116`, `Cargo.toml:113`|
|`airsstack.path.absolute` reads the process working directory (`std::path::absolute`); it is the `path` module's one function that reads anything|airsl-0.1.4 `src/modules/path.rs:16`, `:128-133`, `:268-284`|
|walkdir follows a symlinked **root** by default (`follow_root_links: true`); `follow_root_links(bool)` turns it off|walkdir-2.5.0 `src/lib.rs:293`, `:365`|
|`Lua::inspect_stack(level, f)` and `Debug::current_line()` exist|mlua-0.12.1 `src/state.rs:1042`, `src/debug.rs:143`|
|The guard asserts every member's direct normal and dev dependencies against a golden file|`Makefile.toml:121-143`, `crates/expected-edges.txt`|
|CI runs on `ubuntu-latest` and `macos-latest` only|`.github/workflows/ci.yml:36`|
|`tempfile` is in the workspace catalog and resolved in `Cargo.lock` (3.27.0)|`Cargo.toml` (`tempfile = "3.23.0"`), `Cargo.lock`|

### 1.1 Facts established by running something

The probes were throwaway binaries against `airsl = "=0.1.4"`, `globset = "=0.4.20"` and
`walkdir = "=2.5.0"` (the versions in `Cargo.lock`), run on macOS. Unless a row says otherwise, the
engine was `Policy::confined().with_language(LanguageSurface::Minimal)` with 16 MiB memory,
10 000 000 instructions, **no fs grants**, and modules `json`, `path`, `regex`, `hash`, `glob` plus
a probe `buildl` module. Rows P9–P12 granted read on the fixture's `ws/src`.

|#|Probe|Real output|Consequence|
|---|---|---|---|
|P1|`return tostring(math.random(1, 1000000000))` on two fresh engines|`617823690`, then `969768423`|`math.random` is nondeterministic on `Minimal`. Design §12.1's "drop `os`" does not close it (D4)|
|P1′|the same after `math.random = nil` and `math.randomseed = nil` in `install`|`nil`, `nil`|stripping in `install` works|
|P2|`return tostring({})` on two fresh engines in one process, three separate runs|run 1: `0xa2f004880`, `0xa2f004880`; run 2: `0x99cc08780`, `0x99cc08740`; the reviewer's runs: `0xa3ec00900`/`0xa3ec008c0`, `0x966c08780`/`0x966c08740`|address-derived text is a second entropy source. Check's double run sees it **only sometimes** (run 1 was equal). Not closed (D12)|
|P3|`buildl == airsstack.buildl`, `buildl.json` (global bound in `install`)|`true json=nil`|one table, two names, no inheritance|
|P4|`type()` of the globals|`os=nil io=nil coroutine=nil require=nil load=nil dofile=nil debug=nil print=function`, `fs=nil proc=nil env=nil time=nil stdio=nil`, `json/path/regex/hash/glob=table`|the curated surface holds; **`print` survives** (D5)|
|P5|`return (`|`Error::Lua`, source `mlua::Error::SyntaxError`|`Syntax` is distinguishable|
|P6|`error('boom')`|`Error::Lua`, source `mlua::Error::RuntimeError`|`Runtime`|
|P7|a callback returning `mlua::Error::external(Cap)`|`Error::Lua`, source `CallbackError`; `source.downcast_ref::<Cap>()` hits; `source.chain()` (`mlua::Error::chain`) also reaches `Cap`|classification uses `mlua::Error::downcast_ref` (§8)|
|P7b|`pcall(buildl.target, 'd')` after the probe cap refused|`false / staging cap`, script continues, eval returns `Ok`|a build file can swallow a refusal, so refusals must be sticky (D6)|
|P8|`airsstack.glob.walk(<abs>, '**/*.c')` with no grant|`glob.walk denied: … none are granted`; `source.downcast_ref::<airsl::Error>()` = `Denied`|`Denied` → `Refused`; with zero grants `glob.walk` cannot read|
|P9|`glob.walk(<abs ws/src>, '**/*.c')` with the grant|`a/z.c,a.c,b.c,sub/c.c`|airsl's walk order is DFS, not sorted (one reason for D1)|
|P13/P14|`while true do end`; unbounded allocation|`Error::InstructionLimit`; `Error::MemoryLimit`|`LimitReached{Instructions|Memory}`|
|P15|100 engine builds|average `41.924µs`|a fresh engine per file costs nothing worth caching|
|W1|host walk of a tree with `a.c`, `a/z.c`, `b.c`, `sub/c.c`, `.hidden/h.c`, `a.h`, a dir symlink `link → outside/`, a file symlink `sub/alias.c → ../a.c`; `WalkDir::new(base)`, keep `file_type().is_file()`, match `**/*.c` with `literal_separator(true)`, sort|`[".hidden/h.c", "a.c", "a/z.c", "b.c", "sub/c.c"]`; both symlinks seen with `is_file=false is_symlink=true` and dropped; `outside/` not entered|regular files only, symlinks below the base never followed, byte-sorted (§6.2)|
|W2|`../**/*.c` against `a.c`|`false`|a pattern cannot match outside its base|
|W3|`GlobBuilder::new("a[")`|`Err("error parsing glob 'a[': unclosed character class; missing ']'")`|bad patterns are reportable|
|W4|`print = nil` in `install`|`type(print)` → `nil`|stripping `print` works (D5)|
|W5|`Script::from_source(…, "lib/build.lua")` raising|`lua error in lib/build.lua: runtime error: lib/build.lua:1: boom` + traceback|the chunk name may hold `/`; airsl diagnostics carry `path:line`|
|S1|from a buildl callback called on line 3 of `lib/build.lua`: `lua.inspect_stack(1, \|d\| d.current_line())`|level 0: `(None, "[C]")`; level 1: `(Some(3), "lib/build.lua")`|a refusal can name the build file's line (§7.3)|
|S2|a wrapper `HostModule` that delegates `install` to `airsl::modules::Path` and then sets `absolute = nil` on its own table; the probe first tried `lua.globals().get("airsstack")` inside `install`|wrapper: `absolute=nil normalize=function`; the global read: `FromLuaConversionError { from: "nil", to: "table" }`|stripping `path.absolute` works through a wrapper; reaching `airsstack` from `install` does not (D13)|
|C1|`cargo deny --offline check bans` on a copy with `globset`/`walkdir` (normal) and `tempfile` (dev) added to `buildl-lua` (the reviewer's run)|three `error[banned]`, `bans FAILED`|`deny.toml` needs `buildl-lua` among the wrappers|
|C2|the same copy with `"buildl-lua"` added to those three `wrappers` lists: `cargo deny --offline check`|`advisories ok, bans ok, licenses ok, sources ok`|the §10 `deny.toml` change suffices|
|C3|`cargo tree --offline -p buildl-lua --depth 1 --edges normal,dev --prefix none --format '{lib}' \| tail -n +2` on that copy|`airsl`, `buildl_core`, `globset`, `walkdir`, `tempfile`|the golden file's `# buildl-lua` block (§10)|

---

## 2. Decisions

|#|Question|Decision|Rationale|
|---|---|---|---|
|D1|Where `buildl.sources` globs|**Host-side** in `buildl-lua`, with `globset` and `walkdir`. The engine holds **zero** fs grants|`buildl.sources` becomes the only read in the literal sense (P8: `airsstack.glob.walk` is refused). Sorted output is a host guarantee (P9 shows airsl's order is DFS). Refusals name `buildl.sources`. Closes `architecture-building-blocks.md` §11's open question. Author's choice in the dialogue|
|D2|What `buildl.sources` returns|Regular files only (symlinks and directories skipped, links never followed, the base included), paths **relative to the declaring directory**, `/`-separated, byte-sorted|W1. Relative to the declaring directory because Load joins inputs under it (traversal.rs:437-441): workspace-relative output would double the prefix in subdirectories. This **departs from the intent's original "workspace-relative" wording**; the intent is amended to match|
|D3|The staging cap's measure|A **byte budget** equal to the memory ceiling, charged as the in-memory size of each staged record plus the byte length of each staged string|A count cap does not bound bytes (10 000 declarations of a 15 MB string). One knob, so Rust-side staging stays within about 1× the Lua ceiling. Author's choice. It reinterprets `EvaluationLimit::Staging`, whose rustdoc says "how many declarations" (error.rs:132). Core cannot change in this slice, so the refusal names bytes and a follow-up rewords core's doc (§12)|
|D4|`math.random` / `math.randomseed`|Removed from the `math` table in `buildl`'s `install`|P1/P1′. Lua 5.4 seeds them from entropy and airsl keeps `math` whole on every surface|
|D5|`print`|Removed in `buildl`'s `install`|P4/W4. Stdout belongs to the CLI; build files only declare. Author's choice|
|D6|A refusal swallowed by `pcall`|**Sticky**: the first refusal is recorded, every later primitive call raises again, and `evaluate` reports the first refusal even if the script carried on|P7b. Without it a `pcall` loop could drop declarations silently or slip past the staging cap|
|D7|How the script is loaded|The adapter reads the file itself and uses `Script::from_source(text, <workspace-relative path>)`|A missing file must become `Absent`, which needs the adapter's own `NotFound` check. `from_source` installs no `require` (P4; airsl `engine.rs:738`), which closes design §5's "labels, not `require`" by construction. W5: the chunk name gives `path:line` diagnostics|
|D8|Lua value shapes|String fields take Lua **strings only** (no number coercion). List fields take a sequence `1..n` whose elements are strings or **one level** of nested sequences of strings, flattened in order. Flags take booleans only. Non-UTF-8 text is `WrongFieldType`|Design §9.1–§9.3's `{ b.sources(...), "go.mod" }` requires flattening (§1). Core's lists are flat (build_file.rs:139), so the adapter flattens. One level covers every design example and cannot recurse into a self-referencing table. The rule is the same for every list field. mlua's `String: FromLua` would coerce numbers, so values are matched on `mlua::Value` variants|
|D9|Language surface|`LanguageSurface::Minimal`, plus D4, D5 and D13|Drops `os` and `coroutine` (design §12.1)|
|D10|The adapter's limits type|`DeclarationLimits` in `buildl-lua`, built from `NonZeroU64` memory bytes and instruction count. The default is design §4's values: `"16MB"` read as **16 MiB** (16 777 216 bytes), and 10 000 000|`buildl.toml` does not exist yet, and the composition root will build this from `[declaration]` later. Its parser must read `"16MB"` the same way. A `buildl-lua` type keeps airsl types out of the public API|
|D11|airsl floor|`airsl = "0.1.4"` (caret)|Every probe ran on 0.1.4. Stays on the `0.1` series|
|D12|Address-derived text (`tostring` or `string.format` of a table or function, P2)|**Not closed.** Recorded as residual nondeterminism, with a roadmap follow-up. The intent's "never tripped by entropy" is narrowed to match|Closing it means replacing `tostring` and wrapping `string.format`, whose `%s` and `%p` both reach addresses. That is more surface than this slice's goal. Check's double run detects it only when the two addresses differ (P2 run 1 did not)|
|D13|`airsstack.path.absolute`|Removed. `path` is installed through `CuratedPath`, a wrapper `HostModule` that delegates to airsl's `Path` and then sets `absolute = nil` on its table|It makes staged text depend on the invocation directory, which is ambient and stable within one process, so check cannot catch it (S2). `install` cannot reach `airsstack.path` itself (builder.rs:196-202, S2)|
|D14|A declaring directory reached through a symlink|`buildl.sources` canonicalises its base and refuses (`Refused`) unless the result lies under the root. The walk uses `follow_root_links(false)`|walkdir follows a symlinked root by default (walkdir lib.rs:293), and W1 never tested a symlinked base. The root is canonical by `LuaSource`'s contract (§3)|
|D15|`buildl.sources`' host cost|Accepted bound: each call walks the declaring subtree once. Calls are bounded by the instruction ceiling, so cost ≤ calls × subtree size, and the walk reads only names inside the user's own workspace|No ceiling covers host I/O. Capping walks adds a knob with no driver in this slice|
|D16|A non-UTF-8 file name in the walk|Refusal with the `Runtime` kind, naming the lossy path|Skipping it would drop an input silently. Passing raw bytes would fail later as `WrongFieldType` with a worse message|

---

## 3. Public API

The crate exports two types. Nothing else is `pub`.

```rust
/// Evaluates build files with airsl. One fresh engine per call.
pub struct LuaSource { root: PathBuf, limits: DeclarationLimits }

impl LuaSource {
    /// `root` must be the canonical, absolute workspace root; the composition root supplies it.
    pub fn new(root: impl Into<PathBuf>, limits: DeclarationLimits) -> Self;
}

impl buildl_core::DeclarationSource for LuaSource { /* §4 */ }

/// The ceilings the declaration phase runs under.
pub struct DeclarationLimits { memory: NonZeroU64, instructions: NonZeroU64 }

impl DeclarationLimits {
    pub const fn new(memory_bytes: NonZeroU64, instructions: NonZeroU64) -> Self;
    pub const fn memory_bytes(&self) -> NonZeroU64;
    pub const fn instructions(&self) -> NonZeroU64;
}
impl Default for DeclarationLimits { /* 16 MiB, 10_000_000 (D10) */ }
```

- `LuaSource` holds only the root and the limits, so it is `Send + Sync`, and parallel load later
  costs nothing here.
- The canonical-root contract is documented on `new` and not checked. The port has no error to
  report it with, and the composition root, which canonicalises the root, is the only caller.
- The staging budget (D3) is `limits.memory_bytes()`. It is not a separate knob.
- The memory ceiling converts to `airsl::MemoryLimit::bytes(usize)`. A value above `usize::MAX` is
  clamped to `usize::MAX`. This only matters on 32-bit targets, which CI does not build.

---

## 4. One evaluation

```text
evaluate(&BuildFile)
  1. path = root / file.file()
     read bytes ── NotFound ──────────────► Ok(Evaluated::Absent)
                ── other io error ────────► Err(Evaluation{Runtime, "<path>: <io error>"})
     UTF-8? ────── no ────────────────────► Err(Evaluation{Syntax, "not UTF-8 text"})
  2. staging = Arc<Mutex<Staging::new(budget = memory bytes)>>
     engine  = Engine{ Minimal + limits, grants none,
                       modules [json, CuratedPath, regex, hash, glob, buildl(staging, root, dir)] }
  3. result = engine.eval(Script::from_source(text, file path))
  4. drop engine; take the Staging out under the lock (mem::replace with an empty buffer)
     recorded refusal = Some(r) ──────────► Err(Evaluation{r.failure, r.diagnostic})   (D6)
     result = Err(e)    ──────────────────► Err(Evaluation{classify(e)})              (§8)
     otherwise ───────────────────────────► Ok(Evaluated::Staged(staging.into_file()))
```

- Every `Error::Evaluation` carries `file.provenance().clone()`.
- Taking the contents under the lock (`mem::replace` with an empty `Staging`) leaves no "is this the last `Arc`" question.
- An error from engine construction or `Script::from_source` is a host fault, not the file's. The
  closed set has no host-fault variant, so it is reported as `Runtime` with airsl's message.

---

## 5. The engine and the `buildl` module

### 5.1 Policy and module set

```text
Policy::confined()
  .with_language(LanguageSurface::Minimal)           D9
  .with_grants(GrantSet::declared())                 zero grants, D1
  .with_limits(ResourceLimits::new(memory, instructions))
ModuleSet (insertion order = install order)
  json · path (CuratedPath, D13) · regex · hash · glob · buildl
```

The root table stays airsl's default `airsstack` (`EngineBuilder::root_table` is not called).
`glob` stays installed: `glob.match` is pure, and `glob.walk` is refused for want of a grant (P8).

### 5.2 `buildl` install

`BuildlModule` (a `HostModule` named `buildl`) holds the `Arc<Mutex<Staging>>`, the root and the
declaring directory. `install` does the following:

1. It sets `target`, `test`, `rule`, `alias`, `option`, `subdir` and `sources` on the table airsl
   handed it.
2. It binds the same table to the global `buildl` (`lua.globals().set("buildl", table)`), which P3
   proved.
3. It removes `math.random`, `math.randomseed` (D4) and the global `print` (D5). These are Lua
   globals, so they are reachable from `install`. `airsstack` is not (S2), which is why D13 uses a
   wrapper module.

A poisoned mutex is recovered with `PoisonError::into_inner`, never unwrapped. Nothing that runs under the lock can panic under the workspace lints, so recovery is safe.

### 5.3 Amending design §12.1

Design §12.1 item 1 says dropping `os` removes `math.random`'s nondeterminism along with
`os.time`/`os.clock`. P1 disproves that: `math` is loaded whole. The amendment records three things:

- buildl strips `math.random`/`math.randomseed` and `airsstack.path.absolute` itself.
- `print` is withheld because stdout belongs to the CLI.
- Address-derived text (P2) remains. Check's double run detects it only when the two runs' addresses
  differ, and closing it is a recorded follow-up (D12).

---

## 6. The `buildl` table

Each primitive validates the **shape** of its Lua arguments (D8) and records the text as written.
It never checks a name's grammar, resolves a label or joins a path. Those belong to Load (I3 D2).

|Call|Positional parameters|Accepted option fields|Staged as|
|---|---|---|---|
|`target(name, options)`|`name`: string; `options`: table|`rule` string, `run` list, `inputs` list, `deps` list, `outputs` list, `env` list, `network` bool, `always` bool|`StagedItem::Target`, `role: Build`|
|`test(name, options)`|same|same|`StagedItem::Target`, `role: Test`|
|`rule(name, options)`|`name`, `options`|`run` list (required), `desc` string|`StagedItem::Rule`|
|`alias(name, target)`|`name`: string; `target`: string|—|`StagedItem::Alias`|
|`option(name, options)`|`name`, `options`|`default` string (required)|`StagedItem::Setting`|
|`subdir(path)`|`path`: string|—|`StagedSubdir`|
|`sources(pattern)`|`pattern`: string|—|nothing staged; returns a list (§6.2)|

### 6.1 Reading values

|Lua value|Field kind|Accepted|Otherwise|
|---|---|---|---|
|`string`|text|a Lua string that is valid UTF-8|`WrongFieldType{field}`|
|list|text list|a table whose keys are exactly `1..n`, each element either a text string or a table whose keys are exactly `1..m` and every element a text string. Nested elements are spliced in place, in order (D8)|`WrongFieldType{field}`|
|`boolean`|flag|`true`/`false`. `network`: `true`→`Declared`, `false`→`Sealed`. `always`: `true`→`Always`, `false`→`Cached`|`WrongFieldType{field}`|
|absent (`nil`)|optional field|the default: `rule`/`run`/`outputs` → `None`, lists → empty, `network` → `Sealed`, `always` → `Cached`, `desc` → `None`|required fields (`rule.run`, `option.default`, every positional) → `WrongFieldType{field}`|

- **Field names in errors.** Option fields use their key. Positional parameters use the names in
  §6's table (`name`, `options`, `target`, `path`, `pattern`). Every name fits `FieldName`'s grammar.
- **Rendering keys.** A string key renders as its text, with non-UTF-8 bytes decoded lossily
  (`String::from_utf8_lossy`). Any other key renders as `<type>`, for example `<number>`.
- **Validation order**, pinned so fixtures can assert exact failures:
  1. positional parameters, left to right;
  2. unknown keys: all keys rendered, **sorted**, and the first one outside the primitive's accepted
     set gives `UnknownField{field}`. Sorting makes the result independent of `pairs` order;
  3. accepted fields, in the order §6's table lists them.
- Extra positional arguments are ignored, following Lua convention.

### 6.2 `buildl.sources(pattern)`

```text
base = canonicalize(root / declaring dir)
       ── io error ───────────────► refusal Runtime, "buildl.sources: <path>: <io error>"
       ── not under root ─────────► refusal Refused, "buildl.sources: <root>/<dir> resolves outside the workspace"   (D14)
matcher = GlobBuilder::new(pattern).literal_separator(true).backslash_escape(true)
          ── invalid ──────────────► refusal Runtime, "buildl.sources: <globset message>"     (W3)
WalkDir::new(base).follow_root_links(false)        (links never followed: the default below the root)
  keep: file_type().is_file() && matcher.is_match(relative)            (W1)
  name not UTF-8 ─────────────────► refusal Runtime, "buildl.sources: non-UTF-8 file name <lossy>"   (D16)
  walk io error ──────────────────► refusal Runtime, "buildl.sources: <path>: <io error>"
sort by bytes ──► Lua sequence of `/`-separated paths relative to the declaring directory
```

- The matcher flags are the ones airsl's own `glob` uses (airsl-0.1.4 `src/modules/glob.rs:63-75`),
  so `*` stops at `/` and `**/` matches zero segments.
- A pattern cannot reach outside `base` (W2). Symlinks are never returned or entered (W1, D14).
- Dot-files match like any other name (W1: `.hidden/h.c`). Excluding the workspace's `out` and
  `.buildl` directories needs `buildl.toml`, which does not exist yet. This is recorded as a roadmap
  follow-up for the manifest slice.
- Every refusal here is sticky (D6).

---

## 7. Staging

`Staging` is the buffer behind `Arc<Mutex<…>>`. It has one entry point per outcome.

### 7.1 Order

There is one `DeclarationOrder` counter per file, shared by declarations and `subdir`s. It increments
on each staged call. `sources` does not stage and does not count. On counter overflow
(`checked_add`), the call is refused with `LimitReached{Staging}`.

### 7.2 Budget (D3)

Before a record is stored, its charge is added to the running total:

```text
charge(record) = size_of::<StagedDeclaration>()          (or size_of::<StagedSubdir>())
               + Σ len() of every string the record holds   (after flattening)
```

If the total would exceed the budget, the call is refused with `LimitReached{Staging}` and the record
is not stored. The diagnostic names bytes: `staging budget of <N> bytes exceeded`. Every string is
copied out of Lua after its shape is checked, so the Lua memory ceiling has already bounded each
single string.

### 7.3 Sticky failure (D6)

|State|A primitive call does|
|---|---|
|no failure|validates, charges, stages, returns|
|validation, `sources` or budget refusal|records `(failure, diagnostic)` as **the** failure, then raises its diagnostic as a Lua runtime error (`mlua::Error::RuntimeError`)|
|failure already recorded|raises the recorded failure again without staging anything|

`evaluate` checks the recorded failure **before** looking at the eval result (§4), so a
`pcall`-swallowed refusal still fails the file.

The diagnostic names the line and, once it is known, the declared name. The line is the
`current_line()` of the **nearest Lua frame** on the stack at refusal time, found by walking
`lua.inspect_stack(level, …)` upward from level 1 (S1). Level 1 alone is not enough: under `pcall`
it is `pcall` itself, a C frame with no line. The prototype showed this, which is why the
amendment was made. For example:

```text
line 12: buildl.target 'app': field 'run' must be a list of strings, found integer
```

`Error::Evaluation` adds the file in front of it (error.rs:61). When no line is available, the
`line N: ` prefix is omitted.

---

## 8. Classifying an airsl error

Only errors with no recorded refusal reach this step.

|airsl result|`EvaluationFailure`|Evidence|
|---|---|---|
|`Error::InstructionLimit`|`LimitReached{Instructions}`|P13|
|`Error::MemoryLimit`|`LimitReached{Memory}`|P14|
|`Error::Lua{source}` with `source.downcast_ref::<airsl::Error>()` = `Denied`|`Refused`|P8|
|`Error::Lua{source}` with `*source` = `mlua::Error::SyntaxError`|`Syntax`|P5|
|any other `Error::Lua`|`Runtime`|P6|
|any other `airsl::Error`|`Runtime` (host fault, §4)|—|

The `Diagnostic` is airsl's `Display` of the error, which includes the chunk path, the line and the
traceback (W5). Downcasts use `mlua::Error::downcast_ref` (P7, P8).

---

## 9. Module layout

`lib.rs` is export-only (rust-guidelines `mod-rs-export-only`). One concept per file
(`modularity`). Every logic-bearing file carries a `#[cfg(test)] mod tests` (`unit-test-mandate`).

```text
crates/buildl-lua/src/
├── lib.rs        crate docs + `pub use` of LuaSource, DeclarationLimits
├── source.rs     LuaSource; read → engine → eval → finish; impl DeclarationSource   (§4)
├── limits.rs     DeclarationLimits; Default; conversion to airsl limits             (§3)
├── engine.rs     policy + curated ModuleSet assembly                                (§5.1)
├── curated.rs    CuratedPath: airsl Path minus `absolute`                           (D13)
├── module.rs     BuildlModule: HostModule; installs primitives, global, strips      (§5.2)
├── primitives.rs per-primitive argument shapes → staged records                     (§6)
├── values.rs     Lua value readers: text, flattened text list, flag, closed table   (§6.1)
├── refusal.rs    Refusal: a refused call's kind, detail, subject; its diagnostic    (§7.3)
├── sources.rs    host-side glob walk                                                (§6.2)
├── staging.rs    Staging: order, budget, sticky failure, into StagedFile            (§7)
└── classify.rs   airsl::Error → (EvaluationFailure, Diagnostic)                     (§8)
```

Static dispatch everywhere. The only trait objects are the `Box<dyn HostModule>` values passed to
`ModuleSet::insert`, which airsl's API requires. Each carries a `// dyn:` comment naming that reason
(rust-guidelines `static-dispatch`, exception 3).

---

## 10. Dependencies

|Change|Where|
|---|---|
|`airsl = "0.1.4"`, with the reason comment kept|workspace `Cargo.toml:24` (D11, follow-up #4)|
|`globset`, `walkdir` as normal deps of `buildl-lua` (`{ workspace = true }`, each with a reason comment)|`crates/buildl-lua/Cargo.toml`|
|`tempfile` as a dev-dependency of `buildl-lua` (symlink fixtures, §11)|`crates/buildl-lua/Cargo.toml`|
|`"buildl-lua"` added to the `wrappers` of the `walkdir`, `tempfile` and `globset` bans. Their reasons are reworded to name `buildl-lua` as the sources-globbing adapter (`globset`, `walkdir`) and the fixture user (`tempfile`)|`deny.toml:93-98` (C1, C2)|
|`# buildl-lua` block: `airsl`, `buildl_core`, `globset`, `walkdir`, `tempfile`, in that order|`crates/expected-edges.txt` (C3)|
|`redundant_pub_crate = "allow"`, with a reason comment|workspace `[workspace.lints.clippy]`. This toolchain's clippy (1.98) fires the nursery lint on every `pub(crate)` item in a private module, and rustc's `unreachable_pub` (workspace `warn`) fires on `pub` there, so one of the two must give way. airsl uses `pub(crate)` in private modules (airsl-0.1.4 `src/instruction_budget.rs:43-103`). The conflict was found by the prototype run|

`globset` 0.4.20, `walkdir` 2.5.0 and `tempfile` 3.27.0 are already resolved in `Cargo.lock`. C2 ran
`cargo deny` green with these changes.

---

## 11. Testing

|Layer|Where|What|
|---|---|---|
|Unit|each `src/*.rs`|value readers (every accepted and refused shape, one-level flattening, two-level refused, sorted unknown-field pick, validation order, key rendering), staging (order, budget exactly-at and one-over, sticky re-raise, byte-naming diagnostic), classification (each row of §8), limits (default values, conversion), sources (sorting, files-only, bad pattern, non-UTF-8 name where the platform allows it), `CuratedPath` (`absolute` nil, the rest present), module (global binding, stripped names)|
|Fixtures|`crates/buildl-lua/tests/flows/`. Multi-line and multi-file cases use checked-in workspaces under `tests/fixtures/<case>/`; one-line failure and surface cases write their `build.lua` to a temporary workspace|real files → `LuaSource::evaluate` → **exact** `Evaluated` value. Cases: every primitive with all fields; defaults; flattened `inputs = { buildl.sources(...), "x" }`; `subdir`; missing file → `Absent`; each `EvaluationFailure` variant and limit; `pcall`-swallowed refusal still fails; refusal diagnostic carries the line; curated surface (absent globals and modules, `buildl == airsstack.buildl`, `buildl.json == nil`, `airsstack.path.absolute == nil`); `airsstack.glob.walk` → `Refused`|
|Determinism|same|a fixture evaluated twice is equal; `math.random`, `math.randomseed` and `print` are `nil`. Address-derived text is out of scope (D12)|
|Sources|`sources.rs` unit tests on `tempfile` trees, plus `tests/flows/sandbox.rs` for the symlinked declaring directory through `LuaSource` (`std::os::unix::fs::symlink`; CI is Unix-only, §1)|sorted output, files only, links below the base not followed, a declaring directory reached through a symlink to outside the root is `Refused` (D14), a pattern cannot escape|
|End to end|same|`buildl_core::load(&LuaSource, &EntryName)` over a multi-directory fixture workspace → exact `Vec<Declaration>`|

No test needs `buildl-core`'s `#[cfg(test)]` fakes. The flows run against the real adapter.

---

## 12. Documentation deliverables and amendments

|File|Change|
|---|---|
|`crates/buildl-lua/src/lib.rs`, `crates/buildl-lua/README.md`|describe `LuaSource`, `DeclarationLimits` and the `buildl` table; rewrite the crate docs (the export-only sentence stays: it is now true); Status no longer "no public API" (follow-up #3)|
|`README.md:42` (root)|the declaration engine holds **no** grant; `buildl.sources` reads host-side|
|`docs/design.md` §2 (`:56`)|the same|
|`docs/design.md` §5 (`:148`)|`b.sources()` reads host-side and returns regular files only, relative to the declaring directory, sorted by the host ("an airsl guarantee" corrected). List fields flatten one level|
|`docs/design.md` §12.1 (`:583`)|§5.3's amendment|
|`docs/architecture-building-blocks.md` §11 (`:372`)|open question closed: globbing is host-side in `buildl-lua`|
|`docs/architecture.md` (`:180`)|the staging buffer is `Arc<Mutex<Staging>>`, not `Arc<Mutex<StagedFile>>`|
|`docs/roadmap.md`|the I-lua row and §4 on completion; close follow-ups #4 and #12, and #3 for `buildl-lua`. New follow-ups: exclude `out`/`.buildl` from `buildl.sources` once the manifest names them (target I4); reword `EvaluationLimit::Staging`'s rustdoc to a byte budget (target: the next slice touching `buildl-core`); close address-derived text (D12; unassigned)|

Section cross-references stay intact (CLAUDE.md).

---

## 13. Definition of Done

- `cargo make dod` passes, including `guard-crate-edges` and `guard-core-purity`. So does
  `cargo deny check`.
- Every §11 row exists and passes.
- `git diff main -- crates/buildl-core` is empty.
- The §12 amendments are made.

---

## 14. Non-goals

- `buildl.toml`, the `Manifest` port, the ceiling, grants and the wanted set (I4, milestone 4).
  `DeclarationLimits` and the root arrive as constructor arguments until then.
- Excluding `out`/`.buildl` from `buildl.sources` (needs the manifest; recorded follow-up).
- Closing address-derived nondeterminism (D12; recorded follow-up).
- `LocalPorts`, the composition root, any CLI wiring.
- Parallel load.
- `buildl.use` and plugins (milestone 5).
- Any change to `buildl-core`, including new `EvaluationFailure` variants or the `Staging` rustdoc.
- Moving airsl to `0.2`.
