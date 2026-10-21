---
status: approved
created: 2026-10-20
built-on: ae73bcf2c139
---

# Spec: I16 input sandbox — run each action with its declared inputs only

buildl runs every action at tier 1, `bare`, where an action reads any workspace file, declared or
not, and `build` refuses the default strategy, `sandbox`. This spec adds tier 2: an `InputSandbox`
execution strategy that runs each action in a directory of its own, outside the workspace, holding
copies of exactly its declared inputs and its dependencies' outputs, with `TZ`, `LC_ALL` and
`SOURCE_DATE_EPOCH` pinned. `build` and `test` run under `bare` and `sandbox`; `os-sandbox` and
`container` stay refused.

## 1. Premises (what exists today)

Every claim below was opened or run on 2026-10-20 at commit `ef2f7c4`. Probes ran in the session
scratchpad, outside the repository; none is part of the change. Their output is quoted.

|#|Premise|Evidence|
|---|---|---|
|P1|`build` and `test` refuse every strategy but `bare`, the default `sandbox` included.|`crates/buildl/src/local_ports.rs:152` (`Wiring::runs`), called at `crates/buildl/src/workspace.rs:210` and `:312`. Run: `cargo test -p buildl --lib -- local_ports exec::bare state::lock` → `33 passed`, including `local_ports::tests::only_bare_can_be_run` (`local_ports.rs:418`).|
|P2|`LocalPorts` names one strategy type, `Bare`, and `Wiring::ports` builds it.|`local_ports.rs:58` (`type Strategy = Bare`), `:272` (`strategy: Bare::new(…)`).|
|P3|`Bare` clears each output path under `out/` in `prepare`, then runs with `current_dir` the workspace root, an empty environment plus the declared variables, and a program located against the root.|`crates/buildl/src/exec/bare.rs:60-77` (`prepare`, `clear_path` at `:74`), `:89` (`self.search.locate(&program, &self.root)`), `:102-103` (`current_dir`, `env_clear`).|
|P4|The strategy's directory lives from `prepare` to the end of the core's `run`, past `collect`; nothing else holds it.|`crates/buildl-core/src/execute/perform.rs:358` (`fn run`), `:365` (`strategy.prepare`), `:380` (`strategy.collect(&dir, …)`); `dir` is a local of that function.|
|P5|`ReadyAction` already carries the declared inputs, every dependency output path, and the output paths, all workspace-relative.|`crates/buildl-core/src/types/ready_action.rs:38-45`.|
|P6|The port's contract: `prepare` makes every path of `inputs` and `deps` readable at its relative path, a directory output whole; each output path has its parent and nothing at the path.|`crates/buildl-core/src/ports/exec_strategy.rs` (doc of `prepare`).|
|P7|Reading what an action left at a path, storing its files, is one function of the `exec` module.|`crates/buildl/src/exec/outputs.rs:36` (`found`).|
|P8|Removing whatever stands at a path, a read-only tree included, following no link, is one function of `state`.|`crates/buildl/src/state/clear.rs:25` (`clear_path`); test `:73` (`clear_path_removes_nothing_a_file_a_read_only_tree_and_a_link_alone`).|
|P9|A relative program name with a `/` is joined to the base it is located against; a bare name is searched for on `PATH`.|`crates/buildl/src/search_path.rs` (`SearchPath::locate`: `if name.contains('/') { return Some(base.join(name))… }`).|
|P10|Plan holds every want to the ceiling in one function that sees the workspace (its `execution.strategy` included) and every declared env name with its lowest-labelled target.|`crates/buildl-core/src/plan/authority.rs:70` (`check(graph, sites, workspace)`), `Sites::env` (`BTreeMap<&EnvName, &Node>`); called from `plan/assembly.rs` (`plan`: `let wanted = check(graph, &sites, &workspace)?`).|
|P11|`Isolation` is ordered `Bare < Sandbox < OsSandbox < Container`, `Sandbox` the default.|`crates/buildl-core/src/types/isolation.rs:34-50`.|
|P12|`TZ`, `LC_ALL` and `SOURCE_DATE_EPOCH` are valid `EnvName`s: ASCII letters, digits and `_`, starting with a letter or `_`.|`crates/buildl-core/src/types/env_name.rs:21-35` (`env_name_grammar`).|
|P13|Errors that mean "fix your build files" exit 5 (`Definition`).|`crates/buildl-cli/src/exit.rs:91-106`.|
|P14|`core` can digest bytes.|`crates/buildl-core/src/types/digest.rs:43` (`Digest::of`).|
|P15|Every command that takes the state lock goes through `Wiring::lock`.|`workspace.rs:184`, `:211`, `:237`, `:266`, `:313`.|
|P16|**A sandbox directory inside the workspace leaks the workspace to tools that search upward.** In a scratch workspace `ws/` holding `go.mod`, `Cargo.toml` (`[workspace]`) and `node_modules/hidden`, from `ws/.buildl/sandbox/x/`: `go env GOMOD` → `…/ws/go.mod`; `node -e 'console.log(require("hidden"))'` → `from workspace node_modules`; `bun -e …` → the same; `cargo locate-project --workspace` from a crate there → `error: current package believes it's in a workspace when it's not: … workspace: …/ws/Cargo.toml`.|Probe `probe-upward`.|
|P17|**Outside the workspace, nothing above leaks.** From a `mktemp -d "${TMPDIR}buildl-probe.XXXX"` directory: `go env GOMOD` → `/dev/null`.|Probe `probe-symlink.sh` (control).|
|P18|**A symlinked input leaks; a copied input does not.** `src/main.js` (`console.log(require("hidden"))`) placed in the outside directory: as a symlink to `ws/src/main.js`, `node src/main.js` and `bun src/main.js` print `from workspace node_modules`; as a copy, `node src/main.js` fails at `node:internal/modules/cjs/loader:1423 throw err;` (module not found).|Probe `probe-symlink.sh`.|
|P19|**Writing through a hard link overwrites the source.** A 1 GiB file, hard-linked, then `fs::write(link, b"overwritten via link")`: `source after write through hard link: 20 bytes`.|Probe `probe-copy` (Rust, `--release`).|
|P20|**`std::fs::copy` is near-instant on APFS** (measured; consistent with a clone, std's implementation not cited). `fs::copy` of a 1 GiB file: `4.791625ms`; `hard_link`: `443.25µs`. Workspace, `$TMPDIR` and `/tmp` are on one device (`stat -f %d` → `16777234` for each). Linux speed is not verified; CI runs `ubuntu-latest` and `macos-latest` (`.github/workflows/ci.yml:36`).|Probe `probe-copy`.|
|P21|**The directory a compile runs in reaches its output.** `rustc -g src/main.rs -o out.bin` twice in one directory → `identical`; once in each of two directories → `differ: char 466706, line 2014`.|Probe `probe-cwd.sh`.|
|P22|**ZIP refuses timestamps before 1980.** Python `zipfile`, `ZipInfo(date_time=(1970,1,1,0,0,0))` → `ValueError ZIP does not support timestamps before 1980`; `(1980,1,1,0,0,0)` → ok. `date -u -r 315532800` → `Tue Jan  1 00:00:00 UTC 1980`.|Probe (python3, date).|

Observed and not investigated: with the copied input, `bun src/main.js` printed `[Function: shim]`
rather than failing, so bun resolved `hidden` from somewhere outside the sandbox directory. Where it
came from is not verified; it is outside the workspace, which tier 2 does not hide (§8).

## 2. Behaviour

|Topic|Behaviour|
|---|---|
|Strategies run|`build` and `test` run under `bare` and `sandbox`; a manifest that writes no `strategy` runs `sandbox`. `os-sandbox` and `container` are refused before anything runs, with an error naming the line to write: `strategy = "sandbox"`.|
|Where an action runs|In its **exec dir**, `<scratch>/<action id>`, outside the workspace (P16, P17). `<scratch>` is `<temp>/buildl-<workspace id>`: `<temp>` is `std::env::temp_dir()`, read once when the workspace is opened; `<workspace id>` is the first 32 hex digits of `Digest::of` the workspace root's absolute path bytes. `<action id>` is the first 32 hex digits of `Digest::of` the label's text. The path is the same for a target on every run in one workspace on one machine (P21), and two targets never share one.|
|What is in it|A **copy** of each declared input and of every file of each dependency output, at its workspace-relative path, mode kept (P18, P19, P20). Nothing else of the workspace. The parent directory of each declared output exists; nothing is at the output path.|
|Program|Located against the exec dir by the existing rule (P9): a bare name on `PATH`; an absolute name is itself, run from the host as at tier 1; a relative name with a `/` inside the exec dir. A relative program the command writes out (or names through a setting) that is not already there as an input or a dependency output is the workspace file the plan fingerprinted; it is copied into the exec dir at its path before the run, so the file run is the file fingerprinted and the command's text is true in the exec dir (design §8.5). A relative program with a `..` component is refused by `prepare` (`InfraFailure { kind: Prepare }`, the diagnostic naming the program as written and saying to name it by an absolute path or declare it): copying it would write outside the exec dir.|
|Environment|The declared variables that have a value, plus `TZ=UTC`, `LC_ALL=C`, `SOURCE_DATE_EPOCH=315532800` (1980-01-01T00:00:00Z, the earliest time ZIP can record, P22). Nothing of the host's.|
|Pinned names|Under `sandbox` (or any strategy above `bare`), a target that declares `TZ`, `LC_ALL` or `SOURCE_DATE_EPOCH` in `env` fails Plan with `Error::PinnedEnv`, exit 5, naming the variable, the lowest-labelled target declaring it, and the value the sandbox sets. Under `bare` such a target plans as today.|
|`out/`|`prepare` clears each declared output path under the real `out/` first, as `Bare` does (P3), so a target whose action fails holds nothing at its output paths, the same at both tiers. The action writes only in its exec dir; the core places outputs into `out/` from the cas after they are stored, as today.|
|Clean up|The exec dir is removed when the core drops it, after `collect` (P4), whatever the result; a removal that fails is ignored. Every command that takes the state lock (P15) removes `<scratch>` itself when it takes the lock and again when it releases it, so an interrupted run's exec dirs go at the next `plan`, `build`, `test`, `clean` or `gc`, and a finished command leaves nothing in the temp directory. A failed removal at release is ignored.|
|Scratch safety|`<scratch>` is created by the first `prepare` that needs it, with mode `0700`. An existing `<scratch>` is used only when it is a directory (not a link), owned by the same user as the state directory, with mode `0700`, and does not lie inside the workspace root (a `TMPDIR` inside the workspace would bring back P16); otherwise `prepare` fails (`InfraFailure { kind: Prepare }`) naming the reason. A `<scratch>` that cannot be removed when the lock is taken (another user's, on a shared `/tmp`) fails the command as an infrastructure failure naming the path.|
|Diagnostics|No diagnostic the sandbox produces names its exec dir or `<scratch>`: each names the label, workspace-relative paths, and the program as the command wrote it. Diagnostics reach `ActionOutcome.diagnostic` and so the log (`crates/buildl-core/src/execute/perform.rs:145-151`, `crates/buildl-core/src/types/logged_run.rs:35`), and the intent forbids the sandbox's location reaching a log event. `Bare`'s `running {program.display()}` (`crates/buildl/src/exec/bare.rs:110-115`) is not copied: the sandbox's message names the program as written.|
|Cache|`strategy` stays outside the action key (architecture §6): a target cached under `bare` stays cached under `sandbox`.|
|Same results|A workspace whose every input is declared builds the same bytes under `bare` and `sandbox` only for tools that read no clock, locale or time zone and do not record the directory they run in: the working directory differs between the tiers, and a compile with debug info records it (P21). Design §12.2 says so.|

## 3. Components

### 3.1 `buildl-core`: the pinned variables and the Plan check

- `Isolation::pinned(self) -> Vec<(EnvName, EnvValue)>` in `types/isolation.rs`: empty for `Bare`;
  `LC_ALL=C`, `SOURCE_DATE_EPOCH=315532800`, `TZ=UTC` (name order) for every other strategy. The
  one place the names and values are written; the sandbox reads them from here.
- `Error::PinnedEnv { name: EnvName, value: EnvValue, site: Box<DeclarationSite> }` in `error.rs`,
  mapped to `Exit::Definition` in `buildl-cli/src/exit.rs`.
- The check runs in `plan` right after `authority::check` (P10): the first pinned name, in name
  order, that `sites.env` holds, reported with that entry's node. It is pure and tested against
  fake ports; `plan`'s documented error order gains it as step 4, after the ceiling.

### 3.2 `buildl::state::Scratch` (new file `state/scratch.rs`)

Shared by the lock path and the sandbox, so it lives in `state` (adapter modules do not import
each other). `Scratch::new(temp, root)` computes `<scratch>`; `action(&Label) -> PathBuf` the exec
dir; `claim(owner_uid)` creates or checks `<scratch>` (§2, Scratch safety) using
`DirBuilderExt::mode` and `MetadataExt::{uid, mode}`, no `unsafe`, and is safe to call from
several workers at once (an `AlreadyExists` from a racing create is followed by the same check);
`remove()` removes `<scratch>` itself with `clear_path` (P8). `Wiring` builds one `Scratch` when it
reads the workspace and hands it to `StateLock::acquire`, which calls `remove()` after taking the
lock and again in its `Drop`; `StateLock::owner()` is the uid of the lock file, handed to the
sandbox for `claim`. A removal that fails at acquire maps to `Error::Adapter` naming
`PortName::ExecStrategy`.

### 3.3 `buildl::exec::InputSandbox` (new file `exec/sandbox.rs`)

`InputSandbox::new(root, scratch, owner, timeout, search)`; `type Dir = ExecDir`, which removes its
directory with `clear_path` on `Drop`.

```text
prepare  clear_path(root/out path) for each output          (as Bare)
         scratch.claim(owner)                                (§2, Scratch safety)
         refuse a relative program with a `..` component     (§2, Program)
         clear_path(exec dir); create it
         copy root/<input> → exec/<input>                    for each input
         copy every regular file below root/<dep> → exec/…   for each dependency output
         copy root/<program> → exec/<program>                relative program not yet present
         create_dir_all(exec/<output parent>)                for each output
run      locate program against exec dir; current_dir = exec dir;
         env_clear + declared env + Isolation::Sandbox.pinned(); capture as Bare does
collect  outputs::found(exec/<output>, cas) for each output  (P7)
```

Dependency outputs are copied from the workspace's `out/`, where the core placed them before
releasing dependents (architecture §3.4, `docs/architecture.md:235`). The helpers `Bare` and
`InputSandbox` both need — turning a capture into a `RawOutcome`, building an adapter error — move
to one sibling file of `exec`, so neither strategy names the other.

### 3.4 `buildl::exec::Local` (new file `exec/local.rs`)

`enum Local { Bare(Bare), Sandbox(InputSandbox), Refused(Isolation) }` implementing
`ExecStrategy` with `type Dir = LocalDir` (`Bare(())`, `Sandbox(ExecDir)`), each method
delegating to its variant: static dispatch (rust guideline, static-dispatch). `Refused` holds a
strategy this buildl cannot run; its `prepare` returns the same refusal `Wiring::runs` returns,
so an action is never run under less isolation than the manifest asked for. A `Dir` of the other
variant is an `Error::Adapter` naming `ExecStrategy`, never a panic. `LocalPorts::Strategy`
becomes `Local`.

### 3.5 `buildl::local_ports::Wiring`

`runs()` accepts `Bare` and `Sandbox` and refuses the rest, its message naming
`strategy = "sandbox"`. `ports()` builds `Local::Bare`, `Local::Sandbox` or, for `os-sandbox`
and `container`, `Local::Refused` from the manifest's strategy; it never fails because of the
strategy. So `plan` works under every strategy, as today (`crates/buildl/tests/e2e/facade.rs:71`,
`crates/buildl-cli/tests/cli/exit_codes.rs:522`): `strategy` is no key component, so the
verdicts are the same under each, while `build` and `test` are refused by `runs()` before anything is made or run.

### 3.6 `buildl-cli`

No new flag or subcommand. The exit-code table gains `PinnedEnv` (5); the refusal's message text
changes with §3.5.

## 4. Error handling

|Failure|Reported as|
|---|---|
|A target declares a pinned name under `sandbox`|`Error::PinnedEnv`, exit 5, before anything is hashed or run|
|`<scratch>` cannot be removed when the lock is taken|`Error::Adapter { port: ExecStrategy }`, exit 3, from `Wiring::lock`|
|`<scratch>` is a link, someone else's, not `0700`, or inside the workspace; or cannot be made|`prepare` fails: `InfraFailure { kind: Prepare }` for that target|
|A relative program with a `..` component|`prepare` fails: `InfraFailure { kind: Prepare }` for that target|
|An input, a dependency output or a program cannot be copied|`prepare` fails: `InfraFailure { kind: Prepare }` for that target, as today for `Bare`|
|An action reads an undeclared workspace file|the action's own failure (exit 1), with the tool's own message; buildl names no file|
|The exec dir cannot be removed after the run|ignored; the lock's release, or the next lock, removes `<scratch>` with it|

## 5. Testing

|Layer|Covers|
|---|---|
|`buildl-core` unit|`Isolation::pinned` per variant; the Plan check: a pinned name under `sandbox` refused with the lowest-labelled site, the same name under `bare` planned, no pinned name planned.|
|`buildl-core` flows (fake ports)|`plan` returns `PinnedEnv` after `Refused` in the documented order.|
|`buildl` unit, `state/scratch.rs`, `state/lock.rs`|ids are stable and distinct per label; `claim` creates `0700`, refuses a link, a wrong mode, a `<scratch>` inside the root, and tolerates a racing create; `remove` removes a tree with read-only parts; the lock removes `<scratch>` on acquire and on drop.|
|`buildl` unit, `exec/sandbox.rs` (real file system)|an undeclared workspace file is absent; inputs and every file of a directory dependency output are present with their modes; the exec dir is outside the workspace and the same path on two runs; a relative written-out program is copied in; a `..` program is refused; a `$deps` program runs from the exec dir; env is declared + pinned only; outputs are collected; the dir is gone after drop; `prepare` clears the real `out/` paths; a missing program and a failed copy give diagnostics that do not contain the `<scratch>` path.|
|`buildl` unit, `exec/local.rs`|delegation per variant; a mismatched `Dir` is an error; `Refused`'s `prepare` returns the refusal and nothing runs.|
|`buildl` e2e (`Workspace`)|default manifest builds; an undeclared input fails under `sandbox` and builds under `bare`; the same fixture with every input declared, its actions `cp` and `cat` (which record no directory), builds byte-identical outputs under both; a target built under `bare` is `Cached` after the manifest switches to `sandbox`; an action that writes part of an output then exits non-zero, and one that times out, leave nothing at the output path under `out/`; `os-sandbox` refused by `build` and planned by `plan`; leftovers in `<scratch>` removed by the next build, and no `<scratch>` left after a build; `test` runs sandboxed; no log event of a failed sandboxed action names the `<scratch>` path.|
|`buildl-cli`|exit 5 for `PinnedEnv`; the new refusal text for `os-sandbox` from `build`, and `plan` exiting 0 under it.|

## 6. Documents

|Document|Change|
|---|---|
|`docs/design.md` §4|strategy paragraph: `bare` and `sandbox` run; `os-sandbox`/`container` refused|
|`docs/design.md` §8.5, §12.2, §12.3, §12.4|tier 2 as built: exec dir outside the workspace, copies, pinned values, the `PinnedEnv` rule; what tier 2 does not stop (absolute paths, climbing out, files outside the workspace); a target cached under `bare` stays cached under `sandbox`, and `verify` is how it is checked; outputs match across tiers only for tools that do not record their working directory (P21)|
|`docs/architecture.md` §3.4|`InputSandbox` and `Local` as built (replacing "symlinked declared inputs")|
|`docs/architecture.md` §6|records: materialise by copy, not link (P18, P19, P20); exec dir outside the workspace (P16); a stable path per target (P21); the pinned set lives in core|
|`docs/architecture.md` §8|the open question on `ExecDir` materialisation marked settled|
|`docs/design.md` §8.7|the exit-5 row names a target that declares a pinned variable|
|`docs/architecture-building-blocks.md` status line and §6|`InputSandbox` row (`:454`) as built; the status line (`:3`) no longer lists it as to come|
|`README.md`, `CLAUDE.md` Status, `crates/buildl/src/lib.rs` docs|tier 2 available; the default strategy runs|

## 7. Risks

- **Copy cost off APFS.** On a Linux filesystem without reflinks `fs::copy` copies bytes; a target
  with large inputs pays for them on every run (P20 measured macOS only). Correctness does not
  depend on it.
- **A workspace that worked under `bare` may fail under `sandbox`** — that is the point, but the
  default strategy now runs the sandbox, so an existing workspace that writes no `strategy` and has
  hidden inputs starts failing. The release note and design §4 say how to opt back: `strategy =
  "bare"`.

## 8. Non-goals

- Tiers 3 and 4: the network, files outside the workspace (`/usr`, `$HOME`, tool caches), an
  action that reaches the workspace by an absolute path or by climbing out of its exec dir.
- `buildl verify` (I17).
- A per-target strategy, or any change to the Lua declaration API.
- Stopping the processes an action starts (architecture §6).
- Naming the undeclared file an action tried to read.
- Reusing exec dirs across actions, or keeping them for debugging.
