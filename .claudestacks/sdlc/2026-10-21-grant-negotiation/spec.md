---
status: approved
created: 2026-10-21
built-on: 4455297527cf
---

# Spec: I18 grant negotiation — approver, built programs and `buildl doctor`

Delivers milestone 4 (design §13 item 4): before `build`, `test` or `verify` runs anything, the
build's complete wanted set is held to the ceiling and then put to an approver chosen in
`buildl.toml` (`manifest`, `interactive` or `deny`); a program the build itself made runs with
no `proc.run` entry; `buildl doctor` prints the ceiling, the approver, the wanted set and the run
settings; per-target narrowing is settled as declined. Everything is rooted in the intent of
this chain.

## 1. Premises (proven in this task)

|#|Claim|Evidence|
|---|---|---|
|P1|Plan holds a want to the ceiling before anything else is asked: `sites` then `check`, at `crates/buildl-core/src/plan/assembly.rs:76-80`; `check` returns the `WantedSet` (`plan/authority.rs:70`).|file:line opened|
|P2|Plan's programs are only those written out or named through a setting: `program_of` returns `None` for a first argument that still holds `$` after settings (`plan/tools.rs:18-31`).|file:line opened|
|P3|A program named through `$in`, `$out` or `$deps` is found only in `prepared` (`execute/assembly.rs:249-270`), which expands each command and calls `admit` (`execute/admission.rs:32-46`); `admit` refuses any program not in `ceiling.run`. `verify` calls the same `prepared` (`verify/assembly.rs:22,90`). Both run before the first task.|file:line opened|
|P4|Today a program the build made must be listed by hand. Go workspace (`server`, `gen`, `banner.txt` with `run = { "$deps", "$out" }`) under `proc.run = ["go"]`: `error: build.lua: //:banner.txt: program out/gen is outside the ceiling (proc.run allows: go)`, exit 4. With `proc.run = ["go", "out/gen"]`: `3 built`, then `3 cached`, then after one source edit `1 built, 2 cached`.|binary built from this branch and run, 2026-10-21|
|P5|Output paths are known before anything runs and are a pure function of the graph: `Layout::of(graph, out)` (`execute/layout.rs:34`), `layout.deps(graph, id)` (`:81`). `expand(command, settings, lists)` is pure (`execute/expand.rs:65`).|file:line opened|
|P6|The manifest reader refuses unknown keys: `Raw` is `#[serde(deny_unknown_fields)]` with tables `workspace`, `ceiling`, `declaration`, `execution` (`crates/buildl/src/manifest/schema.rs:49-64`); an `[approver]` table is refused today (`manifest/toml_manifest.rs:321-322`).|file:line opened|
|P7|`Error::Refused` maps to exit 4, "The build wants more than the workspace's ceiling allows" (`crates/buildl-cli/src/exit.rs:33,110`).|file:line opened|
|P8|`std::io::IsTerminal` reports no terminal for redirected streams: probe printed `stdin=false stderr=false` under `</dev/null`, and `stdin=true stderr=true` under `script -q /dev/null` (a pseudo-terminal). The CLI already reads `io::stderr().is_terminal()` (`crates/buildl-cli/src/main.rs:80`). Nothing reads the process's stdin today: `git grep -n stdin -- crates` finds only `crates/buildl-core/clippy.toml:77` (a ban) and `crates/buildl/src/exec/capture.rs:83` (`.stdin(Stdio::null())` for a child) — the control that the search reaches both crates.|probe compiled with rustc and run; search run|
|P9|airsl's approver cannot be used: `Approver::decide(&ApprovalRequest)` where the request holds an extension `Manifest` and a `Negotiation` (`airsl-0.1.4/src/extension/approver.rs:20-71`); airsl is a dependency of `buildl-lua` only (workspace `Cargo.toml:18-24`, `crates/buildl-lua/Cargo.toml:14`). Its model is kept: "An approver can only narrow: the host refuses any negotiation with a denial before the approver is consulted" (`approver.rs:5-7`).|registry source opened|
|P10|Other build systems run tools built in the same build without allow-listing them: Bazel genrule `tools` ("they are built using the exec configuration, since these tools are executed as part of the build", bazel.build/reference/be/general), Buck2 `$(exe …)`, Please `$(exe …)` ("Entries that are not build labels are assumed to be system-level commands", please.build/lexicon.html). Prompt-once tools remember outside the project: direnv keeps sha256 of path and content under `$XDG_DATA_HOME/direnv/allow` (github.com/direnv/direnv `internal/cmd/rc.go`: `hasher.Write([]byte(path + "\n"))` then `io.Copy(hasher, fd)`).|pages fetched 2026-10-21; quotes kept in this chain's `research-built-tools.md`|
|P11|A per-workspace name already exists: `name_of`, 32 hex digits of SHA-256 of the root path, used for the sandbox directory (`crates/buildl/src/state/scratch.rs:45,71-79`; design §12.2).|file:line opened|

## 2. The flow

```text
 Load ─► Resolve ─► narrow ─► Plan ──────────────────────────► negotiate ─► Execute / verify
                              │ settings in force                │
                              │ output layout (P5)               │ approver.approve(&wanted)
                              │ every command's program          │   Granted → go on
                              │   built → no ceiling entry       │   Denied  → Error::NotApproved
                              │   else  → held to proc.run       │
                              │ env, inputs, write dirs          │
                              └─► Plan { wanted: WantedSet } ────┘
```

- `negotiate` runs in `Pipeline::build`, `Pipeline::test` and `Pipeline::verify`, after `plan`
  and before `execute` / `verify`, so nothing has run when it refuses. `Pipeline::plan` does not
  call it: `plan` reports the wanted set and asks nothing. The report is where it is today, in
  `plan --json` (`types/plan.rs:270`); `plan`'s text output is unchanged, and `doctor` (§6) is
  the readable view.
- **No separate grants phase, no `Grants` value.** The intent's candidate direction is declined:
  failure is closed, so the intersection of the wanted set with the ceiling is either the whole
  wanted set or a refusal — there is never a narrower grant to hand on. The ceiling check stays
  in Plan; `negotiate` adds only the approver's answer. The documents drop `Grants` from the
  type tables and status lines (`architecture-building-blocks.md:3,177,431`) and design §6's
  diagram ends at the approver's answer.
- A `deny` workspace refuses a build whose targets are all cached too: the refusal is the
  workspace's, not the plan's.

## 3. Built programs and the complete wanted set

**Rule — by path, not by spelling.** A command's program is *built* when, once the command is
expanded, it is one of that target's dependency output paths or lies below one (`layout.deps`,
P5). A built program needs no `proc.run` entry: declaring the dependency in the graph is the
permission (P10). How the path was written does not matter:

|Command of a target with `deps = { "gen" }` (output `out/gen`)|Program|Built?|
|---|---|---|
|`{ "$deps", "$out" }`|`out/gen`|yes|
|`{ "out/gen", "$out" }` (written out)|`out/gen`|yes|
|`{ "$opt:tool", "$out" }` with `tool = out/gen`|`out/gen`|yes|
|`{ "$deps/bin/x" }` with a directory output `out/gen`|`out/gen/bin/x`|yes|
|`{ "out/gen" }` on a target that does **not** depend on `gen`|`out/gen`|no — held to `proc.run`|
|`{ "$in" }` with input `scripts/gen.sh`|`scripts/gen.sh`|no — held to `proc.run`|
|`{ "$out" }`|the target's own output|no — held to `proc.run` (it does not exist when the action starts)|
|`{ "go", ... }`|`go`|no — held to `proc.run`|

A program that is not built is held to `proc.run` as today.

**Plan computes every program.** Plan places the outputs of the graph it was given
(`Layout::of`, P5) and expands each command (`expand`, P5) under the settings in force, so the
program of a command named through `$in` is known at plan, not first at execute (P2, P3).
Consequences:

|Before|After|
|---|---|
|`WantedSet.programs` holds programs written out or named through a setting|it holds every program a command runs that is not built: those two kinds plus `$in`-named ones|
|`$in`-named programs first checked at execute|checked at plan, in program order with the others|
|output conflicts first reported at execute|also reported at plan, for the planned graph: `plan` is the build's dry run, so it now refuses what the build refuses|
|plan's error order (`plan/assembly.rs:55-68`): override, settings, `Refused`, `PinnedEnv`, …|override, settings, then `OutputConflict` (outputs must be placed before commands can be expanded), then `Refused`, then the rest as today|

Fingerprinting is unchanged for a program that is not built: only programs written out or named
through a setting are looked up and fingerprinted (`plan/tools.rs` `fingerprints`); a `$in`
program's digest is already its input digest. A built program is neither looked up nor
fingerprinted: its bytes are its dependency's output digest (design §12.2), and it does not
exist yet when a clean build is planned. Looked up as today, a written-out one fails a clean
build: the `chain` fixture with `greeting`'s command written `{ "out/greet", "$out" }` gives
`error: build.lua: //:greeting: program out/greet was not found`, exit 3 (binary from this
branch, 2026-10-22).

`admit` keeps its check at execute as the last guard, with the same rule: a built program is
admitted without a `proc.run` entry. One function in `buildl-core` decides "built", used by both
plan and admission, so the two cannot disagree.

## 4. The approver

**Port** (`buildl-core/src/ports/approver.rs`), modelled on airsl's (P9) but buildl's own:

```rust
pub trait Approver {
    /// Whether a build that wants `wanted` may run. Never called for a want outside the ceiling.
    fn approve(&mut self, wanted: &WantedSet) -> Result<Approval>;
}

pub enum Approval {
    Granted,
    /// Not approved: what was not approved, and why, in words for the user.
    Denied { unapproved: WantedSet, reason: DenialReason },
}
```

`Ports` gains `type Approver: Approver` and `PortSet` an `approver` field. Core logic
`grants::negotiate(&WantedSet, &mut impl Approver) -> Result<()>` turns `Denied` into
`Error::NotApproved { unapproved, reason }`. An adapter error is returned as it is, never read
as approval (fail closed).

**Modes**, from `buildl.toml`:

```toml
[approver]
default = "interactive"    # "manifest" (when absent) | "interactive" | "deny"
```

The manifest reader's `Raw` gains an `approver` table (P6); any other key in it, or any other
value, is refused with line and column like every other manifest error. The value reaches the
core `Workspace` as `approver: ApproverMode`, which no phase reads; the composition root picks
the adapter from it, as it picks the strategy from `execution.strategy`.

|Mode|Adapter (`buildl/src/approver/`)|Answer|
|---|---|---|
|`manifest`|`ManifestApprover`|always `Granted` — today's behaviour|
|`deny`|`DenyApprover`|always `Denied`, reason: the workspace's approver is `deny`|
|`interactive`|`InteractiveApprover<C: Console>`|below|

One associated type covers the three, as `Local` covers the strategies
(`crates/buildl/src/local_ports.rs:55,271-274`): `enum LocalApprover<C: Console> { Manifest,
Deny, Interactive(InteractiveApprover<C>) }`, and `LocalPorts` gains the parameter `C`. Static
dispatch throughout; no `Box<dyn>`.

**Interactive.**

```text
 remembered = read approvals file (absent → empty)
 new = wanted − remembered
 new empty?            ──yes──► Granted                  (asks nothing)
 no terminal?          ──yes──► Denied: no terminal to ask on; run the build in a terminal
                                 once to approve, or set [approver] default = "manifest"
 ask on stderr, read one line from stdin
 "y" or "yes" (any case)? ─no─► Denied: not approved
 write remembered ∪ wanted, then Granted
```

- *No terminal* means stdin or stderr is not a terminal (P8). The `Console` trait (in
  `buildl::approver`) wraps both streams and that check, so tests drive the adapter with a fake.
  The name `Terminal` is taken by the CLI's display settings struct
  (`crates/buildl-cli/src/terminal.rs:19`), which stays as it is; the CLI adds `StdConsole`, the
  one `Console` over the real stdin and stderr.
- The prompt lists only the new wants, in the `WantedSet`'s own order (sorted sets), so it is
  the same text on every run:

  ```text
  this build wants to
    run programs:   go, protoc
    read variables: HOME, PATH
    write into:     .buildl, out
    reach the network: //:fetch
  allow? [y/N]
  ```

  A line is left out when it lists nothing.
- *Approvals file*: `<data>/buildl/approved/<workspace id>.json`, where `<data>` is
  `$XDG_DATA_HOME`, or `$HOME/.local/share` when that is unset, and `<workspace id>` is
  `name_of` of the workspace root (P11). Outside the workspace on purpose: a repository can ship
  its own `.buildl/`, and an approval kept there would come with the code. Written with the
  canonical serializer, to a temporary file then renamed. Every failure is a refusal, never an
  approval, and nothing runs:

  |Failure|Error|Exit|
  |---|---|---|
  |the file is not what buildl writes|`Error::CorruptState` naming the file (as for the other files buildl writes, `error.rs:90`), remedy: remove it|6|
  |the file cannot be read, or written after a yes|`Error::Adapter`, port `Approver`|3|
  |neither `XDG_DATA_HOME` nor `HOME` is set|`Error::Adapter`, port `Approver`|3|
- `WantedSet` gains `Deserialize` and two pure methods in core: `covers(&self, other) -> bool`
  and `minus(&self, other) -> WantedSet`.

## 5. Errors and exit codes

|Error|When|Exit|
|---|---|---|
|`Error::Refused` (exists)|a want outside the ceiling — now including `$in`-named programs at plan|4|
|`Error::NotApproved { unapproved, reason }` (new)|the approver said no|4|
|`Error::OutputConflict` (exists)|now also at plan|5|
|`Error::CorruptState` (exists)|the approvals file is not what buildl writes|6|
|`Error::Adapter { port: Approver }` (new port name)|the approvals file cannot be read or written, or there is no `HOME`|3|

`NotApproved` prints the reason and the unapproved wants in the prompt's layout. Exit 4's doc in
`exit.rs` becomes "the build wants more than the workspace allows or than was approved".

## 6. `buildl doctor`

Loads the workspace once, resolves it, and prints to stdout. It takes no lock, writes nothing, plans nothing and hashes nothing: the wanted set comes
from a pure core function over the whole graph and the default settings, the same function plan
uses for its ceiling check (§3), here reporting instead of refusing.

```text
workspace  /path/to/myapp
approver   interactive
execution  workers 0 (one per core), timeout 10m, strategy sandbox
ceiling
  proc.run  go, protoc
  env.read  HOME, PATH
  fs.read   .
  fs.write  .buildl, out
wants                         ceiling   approved
  run go                      allowed   yes
  run curl                    REFUSED   -
  read HOME                   allowed   yes
  write out                   allowed   no
  network //:fetch            -         no
```

- `approved` is `-` for a want outside the ceiling. Otherwise: `yes` for every want under
  `manifest`, `no` for every want under `deny`, and under `interactive` whether the approvals
  file covers it.
- Wants outside the ceiling, or not approved, are **reported**, and `doctor` exits 0. What
  stops `doctor` from computing the report is an **error**, with the exit the error has
  everywhere: a load or resolve error (as `check`), an output conflict (5, outputs must be
  placed to know the programs), and the approvals-file failures of §4 (6 or 3).
- Paths are those the build would use; the workspace root is the one absolute path printed.

## 7. Per-target narrowing — settled as declined

Design §14's open question is closed: each action already receives only its own declared
variables (tiers 1 and 2) and, at tier 2, only its declared inputs (design §12.2); a grant
narrower per target matters only where an OS sandbox can enforce it (tier 3). Recorded in
design §14 with this reason.

## 8. Code shape (guideline-driven)

|Crate|Change|
|---|---|
|`buildl-core`|`ports/approver.rs` (trait, `Approval`, `DenialReason` newtype); `grants` module (`negotiate`, the "built" rule, the pure wanted-set function); `types/authority.rs` (`covers`, `minus`, `Deserialize`); `types/workspace.rs` (`ApproverMode`); plan uses the new wanted-set function; `admit` uses the "built" rule; `Pipeline::build/test/verify` call `negotiate`; `Error::NotApproved`; a `pipeline::doctor` free function over `DeclarationSource`, `Dispatcher`, `Manifest`|
|`buildl`|`approver/` module (`ManifestApprover`, `DenyApprover`, `InteractiveApprover`, `LocalApprover`, `Console` trait, the approvals file); manifest `[approver]`; `LocalPorts<R, C>` gains the approver; `Workspace::doctor`; `Workspace::build/test/verify` take a `&mut impl Console` as they take a reporter|
|`buildl-cli`|`doctor` command and its text; `StdConsole` over real stdin/stderr; exit mapping for `NotApproved`|

Rules followed: static dispatch (generic `Console`, `LocalApprover` enum as the associated
`Approver` type, no `Box<dyn>`); newtypes for `DenialReason` and `ApproverMode` (no bare `String`/`bool` flags);
`mod.rs` export-only; colocated unit tests in every logic file; module docs naming
responsibilities, like the existing files; the `approver` adapter imports no other adapter module
(`cargo make guard-adapter-isolation`) — `name_of` moves from `state/scratch.rs` to `state` so
both use it from there.

## 9. Testing

|Layer|Proves|
|---|---|
|core unit|"built" rule, one test per row of the §3 table; `covers`/`minus`; `negotiate` with fake approvers (granted, denied, adapter error ⇒ error)|
|core flows (fake ports)|build/test/verify call the approver once, after plan, before any task; denied ⇒ no action runs, no row, no log line; `plan` never calls it; P4's Go shape plans and builds under `proc.run = ["go"]`; a `$in` program outside the ceiling is refused at plan; plan reports an output conflict|
|`buildl` e2e|each mode through `Workspace` with a fake `Console`: yes ⇒ file written and a second build asks nothing; growth ⇒ asks only the new wants; no ⇒ refused; no terminal ⇒ refused; corrupt file ⇒ `CorruptState`; unwritable file after yes ⇒ refused and nothing runs; `[approver]` bad key/value ⇒ manifest error with line:col; approvals file lands outside the workspace|
|CLI|`buildl doctor` text (golden); `interactive` under a piped stdin refuses with exit 4 and the remedy text; `deny` exits 4; the `chain` fixture's `proc.run` drops `out/greet` and still builds|

`cargo make dod` and `cargo deny check` green before every commit.

## 10. Documents

design §4 (`[approver]` table; `proc.run` holds host programs, built programs need none), §6
(the negotiation as built, prompt, approvals file), §7 (`doctor` row, built commands sentence),
§12.2 (tier 1 paragraph on `$deps`-named programs), §13 item 4 ("follows airsl's model"), §14
(narrowing settled); `architecture.md` (grants step, `NotApproved`, decision record for the
approvals location); `architecture-building-blocks.md` §5 (grants row built), §6 (`Approver`
row built), the `Ports` listing; roadmap `04-grant-negotiation.md` wording; `README.md`;
`CLAUDE.md` Status; crate READMEs. Fixtures' `buildl.toml` files drop `out/...` entries.

## 11. Non-goals

- Per-target grants; enforcing grants beyond tiers 1 and 2; the network beyond listing it.
- airsl's extension manifest parser and `ext.on`.
- Plugin attribution of wants (milestone 5).
- An approver over a network or a GUI; a `--yes` flag; revoking an approval from the CLI (the
  file can be removed by hand).
- Changing the action key, cache, cas or log formats.
