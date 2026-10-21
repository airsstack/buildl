---
status: approved
created: 2026-10-21
built-on: 2de09370a800
---

# Spec: I17 `buildl verify` — re-run cached actions and name the ones that differ

A cached target is trusted to give the same bytes again, and nothing checks that it does. This
spec adds `buildl verify`. It plans the way `build` does, puts the cached targets' outputs back in
`out/`, re-runs the cached actions (all of them, or a fixed sample) in the input sandbox, and
compares each output's digest with the one its cache row records. It names every target whose
re-run differs or fails. It writes no row and no log event, and a re-run's bytes never reach
`out/`.

## 1. Premises (what exists today)

Every claim below was opened or run on 2026-10-21 at commit `30b25a5`. The probe ran in the
session scratchpad, outside the repository; its output is quoted.

|#|Premise|Evidence|
|---|---|---|
|V1|`Pipeline::build` runs load → resolve → record graph → `narrow(…, Purpose::Build)` → `plan(…, Rerun::Off, …)` → `execute`.|`crates/buildl-core/src/pipeline/driver.rs:151`.|
|V2|A planned target is `Verdict::Clean { key }` when its row stands under the key just computed. A target with a dependency that is not `Clean` is never `Clean`: it is `Conditional`, or `Dirty { Changed }` when something already known differs (`verdict.rs:116-118`). An `always = true` target is `Dirty { reason: Always }`, and one with no row is `Dirty { reason: NeverBuilt }`.|`crates/buildl-core/src/types/plan.rs` (`Verdict`); `plan/verdict.rs:131-145`, tests `an_unsettled_target_with_nothing_known_to_differ_is_conditional` (`:387`) and `everything_downstream_of_a_dirty_target_is_conditional` (`:568`); `:90-94` (Always), `:104-108` (NeverBuilt).|
|V3|A row records one digest per output, in the order the outputs were declared. Each digest is that of the output's `OutputManifest` bytes: `File(StoredFile { digest, mode })` or `Tree`. A `Tree`'s entries are sorted by path in `Tree::new`, and each entry carries `path`, `digest` and `mode`.|`types/cache_row.rs` (`CacheRow::outputs`, `produced`); `types/output_manifest.rs`; `execute/perform.rs:401` (`kept`: `cas.put(&manifest.to_bytes()?)`); `types/tree.rs:64-70`, `:24-30`.|
|V4|For a cache hit, `build` puts the outputs back under `out/` from the cas (`restore` → `place`), clearing each output path first. It first checks the row's printed output and manifests are held (`settle::restorable`); when they are not, the target re-runs instead.|`execute/record.rs:240` (`restore`), `:317` (`place`); `execute/settle.rs:74-92`, `:141-142`.|
|V5|`prepared` turns each planned node into a `Prepared { node, own, deps, action: ReadyAction }`, expanding and admitting its command and laying out its dependency output paths.|`execute/assembly.rs:227-237`, `:249-283`.|
|V6|`perform` runs one task through `prepare` → `run` → store what it printed → its exit → `collect` → `collected` (manifests) → store each manifest. It returns `Ended::Produced { outputs, printed }` or `Ended::Failed { class, diagnostic, printed }`. It writes no row and puts nothing in place.|`execute/perform.rs:313`, `:358-399`.|
|V7|The `Dispatcher` port runs work on threads with a step closure fed `Turn::{Begin, Told, Done}`. The local adapter's thread count is the manifest's `[execution] workers`.|`ports/dispatcher.rs`; `types/turn.rs:16-23`; `crates/buildl/src/local_ports.rs:257-258`; `manifest/interpret.rs:128-130`.|
|V8|**`InputSandbox::prepare` clears each declared output path in the real `out/`** before it makes the exec dir.|`crates/buildl/src/exec/sandbox.rs:93-101`; test `prepare_clears_the_real_output_paths` (`:407`).|
|V9|The sandbox copies each dependency output into the exec dir **from the workspace's `out/`** (`root.join(dep)`).|`exec/sandbox.rs:127`, `:135-138`, `:230-238`.|
|V10|`gc` keeps the blobs named by a kept row or a logged run, and removes every other blob.|`pipeline/gc.rs:83-91`; test `log_only_digests_are_kept` (`:497`, `:527`).|
|V11|With no targets named, `narrow` under `Purpose::Build` keeps build targets and what they need; a named target is kept whatever its role.|`resolve/narrow.rs:38-62`, `:95-112`; tests `:239`, `:249`, `:256`.|
|V12|`Workspace::build` runs `wiring.runs()?` (the strategy refusal), then `wiring.lock()?`, then `Pipeline::new(wiring.ports(&lock, reporter)?)`. `Wiring` builds `Local::Bare`, `Local::Sandbox(InputSandbox::new(…))` or `Local::Refused`.|`crates/buildl/src/workspace.rs:205-216`; `local_ports.rs:158`, `:212`, `:239-247`, `:275`.|
|V13|Exit codes: 0 success, 1 action failure, 2 usage, 3 infrastructure, 4 refused, 5 definition, 6 corrupt state, 7 internal. A strategy refusal is `Error::Adapter { port: Manifest }`, which exits 5.|`crates/buildl-cli/src/exit.rs:23-39`, `:86-87`; `exec/local.rs:54`.|
|V14|`build`'s arguments are the targets, `--set NAME=VALUE`, `-k/--keep-going` and `--stream`. No CLI flag takes a number today.|`crates/buildl-cli/src/cli.rs:108-114`, `:130-140`; `grep -n "u32\|usize\|NonZero\|u64\|value_parser" cli.rs` → only `:113`, the string `--set`.|
|V15|**Re-running deterministic actions in the sandbox reproduces their row digests, and a time-stamping action does not.** The `chain` fixture with `strategy = "sandbox"`, plus `stamp` (`/bin/sh -c "/bin/date +%s > $0; echo $$ >> $0"`), was built, its state and `out/` were removed, and it was built again. Rows compared by label: the six `chain` targets were identical; `//:stamp 91f136d78d52` vs `4310e33815db`.|Probe `/private/tmp/claude-501/-Users-hiraq-Projects-airsstack-buildl/def4c49c-ab29-4356-b2e1-38d88295eaca/scratchpad/probe-verify/run.sh` (deleted with the session scratchpad). Real output: `7 built` twice, then `diff` of the label → first-12-hex-digit row lists: `5c5 < //:stamp 91f136d78d52 --- > //:stamp 4310e33815db`, and `rows: 7`.|

## 2. Behaviour

|Topic|Behaviour|
|---|---|
|Command|`buildl verify [TARGET…] [--set NAME=VALUE…] [--sample N]`. Targets and `--set` work as for `build` (V11, V14): the named targets and what they need, or every build target when none is named. This is how the spec reads the intent's "every target when none is named" together with its "same target arguments as `build`": a test target is in scope when it is named or a build target needs it, as with `build`, and is then re-run as an action like any other.|
|Strategy|Under `sandbox` it runs. Under `bare` it is refused before the lock is taken or anything is loaded, with an error naming the line to write, `strategy = "sandbox"`, exit 5 (V13). The reason: at tier 1 a re-run writes into `out/`. `os-sandbox` and `container` are refused with `build`'s existing refusal.|
|Plan|As `build` (V1): the same keys and verdicts. Only `Clean` targets are candidates (V2). Every other target in scope is counted as **not cached**: dirty, never built, `always`, or conditional.|
|Restore|Before anything re-runs, each `Clean` target's outputs are put in place under `out/` in graph order, as `build` does for a cache hit (V4). A `Clean` target whose row's content is not held (`restorable` is `None`), or whose cached manifest does not re-encode to the row's digest (§3.3), is counted as not cached. A target whose placement fails is reported as **unplaced** with the store's error. A candidate with a direct dependency that is not cached or unplaced is **skipped**, naming that dependency, because its re-run could not find the dependency's outputs (V9).|
|Sample|With no `--sample`, every remaining candidate re-runs. With `--sample N` (N ≥ 1), the N candidates whose key digest is lowest re-run; N above the count means all of them. The choice is the same for the same keys on every run and machine, and the others are counted as **not sampled**.|
|Re-run|Each chosen action runs through `perform` (V6) in a sandbox that **leaves `out/` alone** (§3.4), on the manifest's `workers` threads (V7), with no live output. Re-runs are independent: every dependency output is already in `out/`, and nothing re-run touches it, so a target and its dependency may re-run at once.|
|Compare|`Ended::Produced`: each output's manifest is compared with the cached one, in declared order (§3.3; equal manifests are equal digests). All equal → **verified**. The first that differs → **mismatched**, naming the output and the first difference (§3.3). `Ended::Failed` → **failed**, carrying its class, its diagnostic and what it printed, read back from the cas.|
|Nothing recorded|No row is recorded or forgotten, and no log event is appended. A re-run's outputs are never placed. What a re-run stores in the cas is named by no row and no log, so the next `gc` removes it (V10).|
|Report|One line per checked, skipped or unplaced target, in label order, printed once every re-run is done, then a summary line: `N verified, N mismatched, N failed, N skipped, N not cached, N not sampled`. A mismatch line names the target, the output and the difference. A failed line is shaped like `build`'s failure, with the diagnostic and what the action printed.|
|Exit|0 when nothing mismatched, failed, was skipped or was unplaced. A mismatch exits 1. A failed re-run exits as `build` would for its class (`Exit::of_class`). An unplaced target exits 3, and so does a skipped one: its check could not be made because content the cache names is not held or not placed, which is the store's failure, not the action's. (`Exit::of_class` maps `OutcomeClass::Skipped` to 0, `exit.rs:50-52`; verify's skip does not go through it.) When several apply, the most severe wins (`Exit::severity`). A workspace with nothing to check (all not cached) exits 0 and says so in the summary.|

## 3. Components

### 3.1 `buildl-core` types (new files under `types/`)

- `Sample` — `All` or `Lowest(NonZeroUsize)`: how many candidates re-run.
- `Check` — the result for one target: `Verified`; `Mismatched { output: OutputName, difference: OutputDifference }`; `Failed { class: OutcomeClass, diagnostic: Option<Diagnostic>, stdout: Vec<u8>, stderr: Vec<u8> }`; `Skipped { because: Label }`; `Unplaced { diagnostic: Diagnostic }`.
- `OutputDifference` (not `Difference`, which `types::Difference` at `types/plan.rs:30` already names) — `Kind` (a file where the cache holds a tree, or the reverse); `File(FileChange)` for a file output, where `FileChange` is `Bytes` or `Mode`; `Entry { path: TreePath, change: EntryChange }` for a tree, where `EntryChange` is `Bytes`, `Mode`, `OnlyCached` or `OnlyRerun`. No `bool` carries any of these (strong types).
- `Verification` — `checked: Vec<(Label, Check)>` in label order, `not_cached: usize`, `not_sampled: usize`.

### 3.2 `buildl-core::verify` (new module: `verify/mod.rs` export-only, `verify/assembly.rs`, `verify/sample.rs`, `verify/difference.rs`)

- `assembly.rs`: `verify(whole, graph, plan, sample, ports) -> Result<Verification>` does §2's Restore → Sample → Re-run → Compare. It reuses `prepared` (V5), `restorable` and `restore` (V4), `perform` (V6) and `shown` (`record.rs:284`). Each becomes `pub(crate)` where it is not already, with no change in behaviour. Re-runs go through `Dispatcher::dispatch`: `Turn::Begin` hands out every chosen task, and `Done` collects each result.
- `sample.rs`: choosing the candidates from `Sample` and their keys, lowest key digest first.
- `difference.rs`: the first difference between the cached manifest and the re-run's manifest. The cached manifests are the ones `restorable` already read and parsed for the restore (`settle.rs:81-91`), passed along with the candidate; nothing reads them a second time. A tree is walked in path order (V3).
- `Pipeline::verify(entry, scope, overrides, sample)` in `pipeline/driver.rs`: load → resolve → `narrow(Purpose::Build)` → `plan(Rerun::Off)` → `verify`. It records no graph, because verify changes no state that `gc` or `log` reads.

### 3.3 First difference

```text
cached  File(a)  vs re-run File(b)   → a.digest ≠ b.digest → File(Bytes)      ; else modes differ → File(Mode)
cached  Tree     vs re-run Tree      → merge both entry lists by path; first path where
                                        only one side has it → OnlyCached / OnlyRerun
                                        digests differ       → Bytes
                                        modes differ         → Mode
cached  File     vs re-run Tree (or the reverse) → Kind
```

The walk is the comparison: an output is verified when the walk finds no difference, and
mismatched at the first one it finds. It agrees with comparing digests because the restore step
accepts a cached manifest only when its canonical bytes hash to the row's digest
(`Digest::of(manifest.to_bytes())`); a manifest that does not is counted not cached, like one
that is gone. So there is no case of "digests differ, manifests equal", and no error for it.

### 3.4 `buildl` adapter

- `InputSandbox` gains a constructor for a sandbox that does not clear or make anything under the
  real `out/` in `prepare`, and is otherwise the same: same exec dir, copies, environment, program
  rule and clean-up. The choice is held as an enum field (for example `OutPaths::Cleared` /
  `OutPaths::Untouched`), not a `bool`. `build` and `test` keep today's sandbox unchanged (V8).
- `Wiring` gains the verify wiring: `Local::Sandbox` built with the untouched variant. Under
  `bare` it returns the verify refusal (§4), and for `os-sandbox` / `container` the existing
  `Local::refusal`.
- `Workspace::verify(scope, overrides, sample) -> Result<Verification>`: verify refusal → lock →
  ports → `Pipeline::verify`, in `build`'s order (V12). The reporter is the silent one: verify
  reports through its return value.

### 3.5 `buildl-cli`

- `Command::Verify(VerifyArgs)`, where `VerifyArgs` is `Selection` plus `--sample <N>` parsed as `NonZeroUsize`. `--sample 0` and a non-number are usage errors, exit 2.
- The report lines and the summary of §2, written by a text function in `buildl/src/report/` beside `plan_text.rs` and `query_text.rs`, with `build`'s paint rules. An exit from `Verification` per §2's Exit row, reusing `Exit::of_class` and `Exit::severity`.

## 4. Error handling

|Case|Result|
|---|---|
|`strategy = "bare"`|`Error::Adapter { port: Manifest }`: `buildl.toml: [execution] strategy is "bare", and verify re-runs actions in the input sandbox only: write strategy = "sandbox" under [execution]`. Exit 5, nothing loaded or locked.|
|`os-sandbox` / `container`|`Local::refusal`, exit 5, as `build`.|
|Load / resolve / plan errors|As `build`, same exits.|
|A re-run fails (exit, signal, timeout, missing or refused output, infrastructure)|`Check::Failed`; the remaining re-runs go on (there is no stop-on-failure for verify).|
|Restoring a cached target fails|`Check::Unplaced`; its dependents are `Skipped`; nothing else stops.|
|A cached manifest is gone or unreadable|`restorable` is `None`: the target is counted not cached (§2 Restore), not an error.|

## 5. Testing

|Layer|Tests (named in the plans)|
|---|---|
|`buildl-core` unit|`sample`: lowest key digests, N above the count, the same choice whatever the input order. `difference`: every row of §3.3. `assembly` against fake ports: a clean target verified; a fake strategy that writes other bytes → mismatched naming the output; a failed re-run carries what it printed; not-cached, not-sampled and skipped counts; an unplaced restore skips its dependents; no `ActionCache::record`/`forget` and no `EventLog::append` call (fakes that fail the test when called); nothing placed for a re-run's outputs.|
|`buildl` adapter|The untouched sandbox leaves a file at an output path in `out/` alone (the counterpart of V8's test). The cleared sandbox is unchanged. `Wiring`'s verify refusal under `bare`.|
|`buildl` e2e (`tests/e2e/verify.rs`, a fixture with a deterministic and a time-stamping target)|After `build`: verify → the deterministic targets verified and `stamp` mismatched; the cache file and the log file are byte-identical before and after; `out/` matches what `build` leaves; `--sample 1` checks one target and the same one twice in a row; a target not built is counted not cached; `bare` refused, and `os-sandbox` refused with `build`'s refusal; `gc` after verify leaves the cas holding exactly the blobs it held before verify (the re-run of the time-stamping target stored new ones, and they are gone).|
|`buildl-cli`|`verify` exit 0 on a clean fixture, 1 on a mismatch, 5 under `bare` and under `container`, 2 for `--sample 0`; the mismatch line names the target and the output.|

## 6. Documents

- `docs/design.md`: status line; §4 (the commands list at `:130`, which gains `verify` among the commands that run actions and are refused above tier 2); §7 (the commands table, a `verify` row); §8.7 (verify's exits); §12.2 (`:694`, `verify` as built); §12.3 (`:716`, "re-runs a sample" → what is built: all, or `--sample N` lowest key digests); §13 item 3 (`:747`).
- `docs/architecture.md`: status line; §3's per-command table, with `verify` as plan + re-run without record; a §6 decision record "verify leaves `out/` to the cache, and its sandbox does not clear output paths" (V8, V9).
- `docs/architecture-building-blocks.md`: status line, the core module rows (`verify`), the e2e test table.
- `README.md` status paragraph and "Running it"; `CLAUDE.md` Status: milestone 3's `verify` built.
- The crate docs: `buildl-core/src/lib.rs` (the module list gains `verify`), `buildl/src/lib.rs` (`Workspace`'s commands), `buildl-cli`'s command help.

## 7. Risks

- **Restoring before re-running rewrites `out/`.** This is what `build` does for a cache hit
  (V4), so each cached target's outputs end as a `build` would leave them. Dirty and conditional
  targets are not built, so their paths keep whatever was there. A user who edited a cached
  target's output by hand loses the edit, as with `build`.
- **The sample is lowest key digest, not random.** The same keys always check the same targets.
  A target is checked again by a different sample only once its key changes. This is the price of
  determinism (architecture §5), and `verify` with no `--sample` checks everything.
- **A re-run is as slow as a build of the same targets.** Nothing makes verify cheaper than
  rebuilding; `--sample` is the way to bound it.
- Whether `perform`, `prepared` and `restore` can be shared without changing their behaviour is
  for the plan to settle against their tests: they become `pub(crate)` and nothing else.

## 8. Non-goals

- Repairing on a mismatch: no row forgotten, no rebuild, no mark.
- Verifying under `os-sandbox`, `container`, or remotely.
- A byte-level diff, or naming more than the first difference.
- Live output, progress lines while re-running, or stop-on-first-failure for `verify`.
- Running a test target's checks as tests: a test target in scope is re-run as an action like any
  other.
