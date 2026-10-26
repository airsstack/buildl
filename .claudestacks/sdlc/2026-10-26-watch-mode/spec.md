---
status: approved
created: 2026-10-26
built-on: 05865c4b6c91
---

# Spec: watch mode

`buildl watch [target]...` builds once, then waits on the operating system's file events
and builds again when a change can matter, in the full-reload form design §8.6 and §13
item 7 describe: every build cycle is the `buildl build` the user would have typed. A
change that cannot affect the build — a README, an editor's swap file, the build's own
writes — costs at most one load of the build files and starts no build, writes no log
line and prints nothing.

## 1. Premises

Each claim below was proven in this design; probes were run and deleted.

|#|Claim|Evidence|
|---|---|---|
|P1|`notify` 8.2.1 is the newest stable release (a 9.0.0-rc.6 exists), MSRV 1.85, under our 1.94|`cargo info notify@8` → `version: 8.2.1 (latest 9.0.0-rc.6)`, `license: CC0-1.0`, `rust-version: 1.85`; `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/notify-8.2.1/Cargo.toml:14` `rust-version = "1.85"`, `:16` `version = "8.2.1"`|
|P2|Its whole normal-dependency tree on every target is `bitflags`, `inotify`, `inotify-sys`, `kqueue`, `kqueue-sys`, `libc`, `log`, `mio`, `notify-types`, `objc2-core-foundation`, `objc2-core-services`, `same-file`, `walkdir`, `wasi`, `winapi-util`, `windows-link`, `windows-sys`|`cargo tree -e normal --target all` in a probe crate depending on `notify = "8"`|
|P3|Under this repository's `deny.toml`, `notify` itself (`license = "CC0-1.0"`) is the only crate of that tree rejected, and `exceptions = [{ crate = "notify", allow = ["CC0-1.0"] }]` makes the check pass|`cargo deny check licenses` in the probe: `notify-8.2.1/Cargo.toml:39:12 license = "CC0-1.0" rejected: license is not explicitly allowed`; with the exception: `licenses ok`|
|P4|On macOS, events name paths in their canonical form, not as the watched root was given|probe watching `/var/folders/…/notify-probe-ws` received `/private/var/folders/…/notify-probe-ws/a.c`|
|P5|On macOS, event kinds are merged and unreliable: an in-place write to an existing `a.c` followed by an atomic save produced `Create(File)`, `Modify(Name(Any))`, `Modify(Metadata(Any))`, `Modify(Data(Content))` for `a.c`; whether a file is new cannot be read from an event|same probe|
|P6|A build in which every target is cached still appends one log line per target|`chain` fixture copied to a scratch dir: `buildl build` → `1 failed, 1 skipped, 4 built`, `log.jsonl` 6 lines; again → `4 cached`, 12 lines|
|P7|`buildl graph` takes no lock and changes nothing under `.buildl/`|`Workspace::graph`, `crates/buildl/src/workspace.rs:245`, calls `pipeline::graph` with no `self.wiring.lock()`; probe: `ls -lT .buildl` identical before and after `buildl graph`, `log.jsonl` still 12 lines|
|P8|`Workspace::build` takes and releases the state lock per call|`crates/buildl/src/workspace.rs:389`–`421`: `let lock = self.wiring.lock()?` … `drop(pipeline)`|
|P9|The pipeline's build already holds the whole graph it ran on, as `Prepared::whole`, and does not return it|`crates/buildl-core/src/pipeline/driver.rs:39`–`44`, `:413`; `Pipeline::build` returns `Result<Vec<ActionOutcome>>` (`:403`–`:410`)|
|P10|`buildl.sources` is a build file's only filesystem read, so a new file reaches the build only through a load|`crates/buildl-lua/src/engine.rs:6`–`7` ("`buildl.sources` is the only filesystem read"); design §3|
|P11|The output and state directories come from the manifest's workspace (`[workspace] out`; the state directory, `.buildl`), read when the workspace is opened|design §4 (`out = "out"`); `crates/buildl/src/local_ports.rs:253` (`StateLayout::new(self.root.as_path(), &self.workspace.state)`)|
|P12|`build`'s flags are `--keep-going`/`-k`, `--stream`, the `Selection` (targets, `--set`) and `RemoteArgs` (`--remote-upload`, `--remote-strict`, `--no-remote`)|`crates/buildl-cli/src/cli.rs:152`–`180`|
|P13|Exit code 3 is `Exit::Infrastructure`, 2 is `Exit::Usage`|`crates/buildl-cli/src/exit.rs:23`–`40`|
|P14|`buildl-core` bans `Instant` and `SystemTime` through `clippy.toml`, enforced by `cargo make guard-core-purity`|`docs/architecture.md:547`|
|P15|CI runs the gate on `ubuntu-latest` and `macos-latest`|`.github/workflows/ci.yml:35`–`37`|
|P16|An open `Workspace` reads `buildl.toml` once: `Workspace::open` calls `Wiring::read`, which parses the manifest and the workspace, and every later call uses that stored copy|`crates/buildl/src/workspace.rs:130`–`134`; `crates/buildl/src/local_ports.rs:96`–`108`|
|P17|Discovery targets are built first in every build, whatever the scope; the scoped graph is narrowed afterwards|`crates/buildl-core/src/pipeline/driver.rs:253`–`313` (`prepare`), `:413`–`:425`|
|P18|On a workspace with a discovery target, after a build, the graph `buildl graph` loads (with the stored discovery results) equals the graph the build recorded|`discovery` fixture copied to a scratch dir: `buildl build` → `2 built`; `buildl graph` stdout and `.buildl/graph.json` parsed as JSON: `equal as JSON: True` (they differ by a trailing newline only)|

Not verified: event behaviour on Linux (inotify) was not probed on this machine; the
adapter tests (§8) run on both CI platforms and are what proves Linux. CI does not run
Windows (P15), so no claim is made about ReadDirectoryChangesW beyond `notify` choosing
it there.

## 2. The command

```text
buildl watch [target]... [--set K=V]... [-k|--keep-going] [--stream]
             [--remote-upload] [--remote-strict] [--no-remote]
```

- The flags are `build`'s (P12), with `build`'s meaning, applied to every cycle. A
  workspace with no target named watches the whole workspace.
- Each cycle opens the workspace again — `buildl.toml` read anew (P16) — and runs
  `Workspace::build` on it with the same scope, overrides and options: a full reload
  (manifest, load, resolve, plan, approve, execute, record). A check pass (§3) opens the
  workspace again too. Watch keeps only the workspace root and the approvals directory
  between cycles, never an open `Workspace`, and adds no second way to do any phase.
- The workspace is opened once before watching starts; a manifest that cannot be found,
  read or accepted then exits as `buildl build` does. A manifest refused at a later cycle
  is that cycle's error (§6).
- The file watcher is running before the first cycle starts, so an edit made during the
  first build is seen.
- The approver is asked as `build` asks it, each cycle. An `interactive` approver's
  remembered yes means a cycle that wants nothing new asks nothing.
- Between cycles the state lock is not held (P8), so `buildl plan`, `log`, `graph` and
  `doctor` run while watch waits.
- What a cycle records — rows, log lines, cas blobs, outputs — is what `buildl build`
  would have recorded, so watch changes no key, row or output format.

## 3. When a change starts a cycle

Watch keeps a **reference**, renewed by every cycle:

- `layout` — from the last time the workspace opened: the output directory, the state
  directory (P11), and the config paths `buildl.toml`, `buildl.lock` and
  `<path>/extension.toml` of each `[plugins.<n>]` with a `path`;
- `built` — what the cycle's build ran on, or absent when the cycle ended before a graph
  existed (a manifest, load, resolve or discovery-stage failure):
  - `graph` — the whole workspace graph the build loaded (P9), compared by value;
  - `inputs` — the declared inputs of the targets the scope selected and what they need,
    and of every discovery target of the whole graph and what it needs, since every build
    runs those whatever the scope (P17).

A **batch** is the set of workspace-relative paths that changed (§5). Its reaction,
first match wins:

```text
 react(batch, reference)
   drop paths under layout.out/, layout.state/, .git/
   nothing left                                      ─► Ignore
   built absent                                      ─► Build
   batch ∩ built.inputs ≠ ∅                          ─► Build
   batch ∩ layout.config ≠ ∅                         ─► Build
   otherwise                                         ─► Check

 Check:  graph = graph of the workspace opened again  (no lock, writes nothing — P7)
         error or graph ≠ built.graph      ─► Build
         graph = built.graph               ─► Ignore: no output, no log line
```

Why this shape:

- The directories are dropped here, in `react`, from the layout the last open read, so a
  changed `[workspace] out` moves the ignored directory with it. The build's own writes
  during a cycle drain into a batch that is empty once dropped: Ignore.

- Kinds are not used (P5): a path counts the same whether its event says create,
  modify, rename or remove.
- A new file can matter only through a load (P10), and a check pass is exactly a load:
  a file a build file's `buildl.sources` now matches changes the graph's inputs, so it
  builds; a README or swap file does not, so it is ignored. No swap-file list is kept.
- A build-file edit that changes nothing declared (a comment) ends in Ignore: the build
  files are read again, and since what they declare is unchanged, a build would run no
  action. This is the intent's "a change to a build file starts a cycle" read as "the
  build files are read again, and a build follows when what they declare changed".
- The check pass compares like with like for a discovery workspace: after a build, the
  graph loaded with the stored discovery results equals the build's own (P18).
- `buildl.toml`, `buildl.lock` and a workspace plugin's `extension.toml` hold what the
  graph does not (ceiling, approver, execution settings, plugin caps), so they always
  build.
- An input of a target outside the scope is not in `inputs`; its change goes to a check
  pass, finds the graph unchanged, and is ignored.
- Taking the reference from the cycle's own build, never from a separate load before or
  after it, means a file created while the build ran is either seen by that build or
  differs from its graph at the next check pass. Watch can build once too often, never
  miss a change.

## 4. Components

```text
 buildl-cli   watch subcommand          parse, wire, print "watch:" lines, exit codes
     │
 buildl       Workspace::watch          composes: NotifyChanges + cycles over
     │                                  Workspace::build / Workspace::graph
     │        watch/ (adapter module)   NotifyChanges: notify, filter, relativize, debounce
     │
 buildl-core  watch/                    ChangeBatch, Reference, Reaction
                                        port Changes, port Cycles
                                        react(), watch() loop
              pipeline                  build entry point that also returns the graph it ran on
```

|Unit|Crate|Responsibility|
|---|---|---|
|`ChangeBatch`|core|a non-empty `BTreeSet` of workspace-relative paths (a newtype; sorted, so nothing depends on event order)|
|`Reference`|core|`layout` (out and state directories, config paths) and optional `built` (whole `TargetGraph`, `BTreeSet` of inputs), as §3 defines|
|`Reaction`|core|`Build`, `Check`, `Ignore`|
|`Changes` port|core|`fn next(&mut self) -> Result<ChangeBatch>`: blocks until a debounced batch exists; an error ends watching|
|`Cycles` port|core|one build cycle — workspace opened again, then built — returning its outcomes or error with the new `layout` and `built`; one check pass — workspace opened again, then loaded — returning the graph|
|`react`|core|the §3 table, pure, the directory drop included|
|`watch` loop|core|first cycle, then `next` → `react` → check pass if needed → cycle; tells the reporter what started each cycle; returns only when `Changes` fails|
|build with graph|core|beside `Pipeline::build`, returning the outcomes with the whole graph and the input set of §3 (P9, P17); `Pipeline::build` and `Workspace::build` keep their signatures|
|`NotifyChanges`|buildl, new `watch` adapter module|§5; imports no other adapter module (`cargo make guard-adapter-isolation`); takes only the root|
|`Workspace::watch`|buildl|keeps the root and approvals directory of `self`; implements `Cycles` by opening a new `Workspace` for each cycle and check pass (P16); starts `NotifyChanges` and then runs the core loop|
|`watch` subcommand|buildl-cli|`WatchArgs` flattening `BuildArgs`; renders cycle errors as `build` renders them; prints §7|

Guideline rules applied: generics over `dyn` for both ports (static dispatch);
newtypes for the batch and reference (strong types); the debounce timer, the only time
value, lives in the adapter (P14); `watch/mod.rs` files hold only module docs and
re-exports; each logic-bearing file has its own unit tests.

## 5. The file-event adapter

- **Watching.** `notify::recommended_watcher` (FSEvents on macOS, inotify on Linux),
  `RecursiveMode::Recursive` on the workspace root, canonicalized once at start (P4),
  started before the first cycle.
- **Relativizing.** An event path is made relative to the canonical root; a path outside
  the root is dropped. Dropping the output, state and `.git` directories is `react`'s
  (§3), since the output directory can change between cycles (P11, P16).
- **Debouncing.** After the first kept path, keep receiving with
  `mpsc::Receiver::recv_timeout(50 ms)` until a window passes with no kept path; the
  batch is everything received. Events arriving while a cycle runs queue in the channel
  and drain into the next batch, so one cycle runs at a time and any number of changes
  during it become one batch.
- **Idle cost.** `next` blocks in `recv`; no timer runs while nothing changes.
- **Failure.** An `Err` from `notify`, or a closed channel, is returned from `next` as an
  infrastructure error naming the root.
- **Dependency.** `notify = "8.2.1"` with default features, in `buildl` only, its reason
  commented in the workspace `Cargo.toml`; `deny.toml` gets the single-crate exception of
  P3, with a comment, and `CC0-1.0` stays off the general allow list. No debouncer crate.

## 6. Failures

|Case|Handling|Watch|
|---|---|---|
|actions fail|the build's own blocks and summary|continues|
|cycle error: manifest refused, load, resolve, discovery, refusal, approver no, lock busy, remote under `--remote-strict`|rendered as `build` renders it; `built` absent if no graph existed, otherwise the new one; `layout` kept from the last open that succeeded|continues|
|check pass error|reaction becomes Build, whose cycle reports it|continues|
|usage error before the first cycle (clap, `--set`, remote flag conflicts), or the first open of the workspace failing|as `build`|exits as `build` would (2, or the manifest's code)|
|`Changes` error|`watch: <error>`|exits 3 (P13)|
|Ctrl-C|no handler installed; the process ends as `buildl build` does; design §8.5's crash safety covers the interrupted cycle|ends|

## 7. Output

All on stderr, as `build`'s (design §8.7); nothing on stdout.

```text
watch: building
  …build's blocks and summary line…
watch: waiting for changes
watch: lib/text.c changed
  …
watch: waiting for changes
watch: app/build.lua and 2 more changed
```

The path named is the first of the batch in path order. A check pass that ends in Ignore
prints nothing.

## 8. Testing

|Layer|Where|Proves|
|---|---|---|
|core unit|`buildl-core` `watch/`|`react` for every row of §3, the directory drop under a changed out directory included; the loop over fake `Changes`/`Cycles`: Ignore runs no cycle, a failed cycle keeps watching with `built` absent and `layout` kept, a check-pass error builds, a `Changes` error ends the loop with that error, and the reporter is told the first path and count|
|core unit|`pipeline`|the new build entry point returns the same outcomes as `Pipeline::build` plus the graph; its input set holds a discovery target's inputs when the scope does not reach it; absent when Load fails|
|adapter|`buildl` `watch/`|on a temp dir: a write yields a batch with its relative path; a path outside the root yields none; a burst of writes yields one batch; writes made while the consumer is not calling `next` (as during a cycle) drain into one batch; a root given through a symlinked path (macOS `/var`) still yields relative paths. Every wait is a bounded `recv_timeout`; none relies on a fixed sleep being long enough|
|e2e|`crates/buildl/tests/e2e/watch.rs`|`Workspace::watch` on fixtures with a test `Changes`, one row of the intent's Desired-outcome table each: Start (first cycle's outcomes equal a plain `buildl build`'s); Source change (only its target rebuilds); Build-definition change (a build-file edit adding a target builds it; a `buildl.toml` edit to `[workspace] out` is read again and the next cycle writes the new directory, whose writes start no cycle); New file (a file matched by `buildl.sources` is built); Ignored change (a README edit adds no log line; a build-file comment edit runs no cycle); Unchanged bytes (a save of identical bytes runs no action); Bursts (several changes delivered during one cycle make one more cycle); discovery (an edit to a discovery target's input outside the scope runs a cycle; a README edit on the discovery fixture adds no log line, settling §10's equality risk); Workspace lock (`Workspace::plan` and `Workspace::log` succeed between cycles); Keys (after the same edits, a watch's rows, `out/` and cas equal those of plain `buildl build`s)|
|CLI|`buildl-cli`|`watch` parses every flag of P12; the binary on a fixture prints `watch: waiting for changes`, rebuilds after a write, and on SIGINT ends with no summary line; no test hangs|

Stop's crash safety (an interrupted cycle leaves the cache as an interrupted `buildl
build` does) is not tested again here: a cycle is `Workspace::build`, whose interruption
is covered by the existing `crates/buildl/tests/e2e/recovery.rs`, which tests each state
an interrupted build leaves behind (`recovery.rs:4`).

`cargo make dod` and `cargo deny check` green before each commit.

## 9. Documents

- `docs/design.md`: §7 table (watch built), §8.6 (the check pass, the reference, the
  ignored directories; "events invalidate stat-cache entries" replaced by what is built:
  the stat cache needs no invalidation, since a changed file's stat differs), §13 item 7
  marked built.
- `docs/architecture.md`: status line; the watch loop's place beside the pipeline; §6
  decision records: `notify` and its CC0 exception, the check pass over kind-based
  rules, the lock taken per cycle.
- `docs/architecture-building-blocks.md`: watch as an embedder of the composition root;
  `notify` in the dependency table.
- `README.md` (command table, status), `CLAUDE.md` Status,
  `.claudestacks/sdlc/roadmaps/buildl/07-watch-mode.md` left to the roll-up, the crate docs.

## 10. Risks

- **inotify watch limit.** A Linux tree larger than `fs.inotify.max_user_watches` fails to
  watch; this surfaces as the `Changes` error (exit 3, naming the root), not a silent gap.
  No mitigation in this slice.
- **Event latency.** FSEvents' delivery latency was not measured; the 50 ms window is added
  to it. The tests bound waits, so a slow delivery fails a test rather than hanging it.
- **Graph equality across discovery.** P18 shows one fixture's two graphs equal as JSON,
  which leaves out `Node::narrowing`. Should a check pass ever find them unequal, every
  check pass would build: correct, but a README save would then add log lines. The
  discovery e2e row of §8 fails in that case.

## 11. Non-goals

- A persistent Lua engine or `ext.on` dispatch.
- Keeping the graph between cycles to skip Load.
- Polling, for filesystems that send no events.
- Keeping the stat cache warm between separate `buildl` runs (design §11.1).
- `buildl test` in a loop.
- Windows-specific handling: CI runs no Windows job (P15), so nothing here is proven
  there.
- Remote execution (milestone 08).
