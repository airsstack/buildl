---
status: approved
created: 2026-10-19
built-on: da47e168414a
---

# Spec: clean and gc — drop the outputs, prune the store

A workspace's state only grows: every build adds blobs to the cas, rows to the action cache and
lines to the log, and a row of a target since removed from the build files stays for good. This
spec adds `buildl clean`, which removes the output directory so the next build puts every output
back from the cas, and `buildl gc`, which drops the rows of targets no longer declared and sweeps
every blob that neither a kept row nor `buildl log` still needs. Both take the workspace lock.

## 1. Premises (what exists today)

Every claim below was opened or run on 2026-10-19 at commit `bc6bea6`.

|#|Premise|Evidence|
|---|---|---|
|P1|A build after the output directory was removed puts every output back from the cas and runs nothing.|Probe: fixture `crates/buildl/tests/fixtures/chain` copied to a scratch directory; `buildl build` → `1 failed, 1 skipped, 4 built`; `rm -rf out`; `buildl build` → `1 failed, 1 skipped, 4 cached`; `out/` after the second build lists the same six files as after the first. (The one failure is the fixture's `out/greet` not found on the binary's search path, the same in both builds.)|
|P2|Taking the lock already removes what interrupted writes left: every `.tmp-` file in the state directory and everything in `cas/tmp`.|`crates/buildl/src/state/lock.rs:63-64` (`sweep(layout.dir(), is_staged)`, `sweep(&layout.blob_staging(), \|_\| true)`), `:139-143` (`is_staged`: name starts with `STAGED_PREFIX`), `crates/buildl/src/state/write.rs:20` (`STAGED_PREFIX = ".tmp-"`).|
|P3|The lock creates the state directory, and reports a held lock as `LockError::Busy`, which `Wiring::lock` turns into `Error::Adapter` naming `ActionCache`.|`state/lock.rs:49` (`create_dir_all`), `:59` (`Busy`); `crates/buildl/src/local_ports.rs:192-206` (`Wiring::lock`: ceiling check on the state directory, then `StateLock::acquire`, error mapped to `PortName::ActionCache`).|
|P4|The output directory is never the workspace root and never holds, or lies inside, the state directory.|`crates/buildl/src/manifest/interpret.rs:185-199` (`fn out`: `is_root` refused; `out.contains(state) \|\| state.contains(&out)` refused).|
|P5|A blob is `cas/<first 2 hex>/<other 62 hex>`; its executable twin is the same name with `.x`.|`crates/buildl/src/cas/blob.rs:24-33` (`BlobPath::of`).|
|P6|`ContentStore::get` returns `None` for content not held, and removes a blob, with its twin, whose bytes fail its digest.|`crates/buildl-core/src/ports/content_store.rs:42`; `crates/buildl/src/cas/local_cas.rs:242-257`.|
|P7|An output digest names an `OutputManifest`: one `StoredFile`, or a `Tree` of `TreeEntry`s, each with its own digest. `from_bytes` returns `None` for bytes that are not a manifest.|`crates/buildl-core/src/types/output_manifest.rs:27-32`, `:52`; `types/stored_file.rs:33-37`; `types/tree.rs:24-30`, `:97` (`entries`).|
|P8|A row holds one manifest digest per output and the digests of its stdout and stderr.|`crates/buildl-core/src/types/cache_row.rs:28-33`; accessors `:63` `stdout`, `:69` `stderr`, `:76` `outputs`.|
|P9|The action cache's rows are a `BTreeMap<Label, CacheRow>`; `forget` appends one journal line and is durable on return; `fold` writes `cache.json` and removes the journal.|`crates/buildl/src/cache/local_cache.rs:36-37`, `:112-125` (`fold`), `:150-` (`forget`); `crates/buildl-core/src/ports/action_cache.rs:35-48`.|
|P10|`ActionCache` has no way to list its labels; it is implemented six times.|`ports/action_cache.rs:19-48` (`row`, `record`, `forget` only). `grep -rn "impl ActionCache for" crates` → `buildl-core/tests/flows/common.rs` (`FakeCache`), `execute/record.rs` (`Rows`), `ports/bundle.rs` (doc example `Nothing`), `ports/port_set.rs` (`Stub`), `buildl/src/cache/local_cache.rs`, `buildl/src/report/plan_text.rs` (`Rows`).|
|P11|Nothing lists or removes the cas's blobs. `ContentStore` has `put`, `contains`, `get`, `clear` (an output path), `place`.|`ports/content_store.rs:24-69`. `grep -n "fn held\|fn remove\|read_dir" crates/buildl/src/cas/*.rs` → only test code: `placement.rs:88` (an assertion), `local_cas.rs:353`, `:357`, `:383` (inside `#[cfg(test)] mod tests`, which begins at `:276`; `:353` is a test helper also named `held`). Control: `grep -n "fn get" crates/buildl/src/cas/local_cas.rs` → `242`.|
|P12|The newest run of a label that stored output is chosen by `history::last_run`; a run has output when both digests are present.|`crates/buildl-core/src/history/last_run.rs:39`, `:80` (`has_output`); `pipeline/log.rs:36` (`log` uses it, then `ContentStore::get`).|
|P13|`pipeline::log` is the model for a command that takes only the ports it needs, as a free function.|`crates/buildl-core/src/pipeline/log.rs:36` (`log<H: RunHistory, C: ContentStore>`); `pipeline/mod.rs` (`pub use log::{Shown, log}`).|
|P14|The module edges `gc` needs already exist.|`crates/expected-module-edges.txt`: `pipeline -> history`, `pipeline -> load`, `pipeline -> resolve`, `pipeline -> ports`, `pipeline -> types`, `history -> types`.|
|P15|A graph's targets are its nodes, each with its label; aliases are a separate map and have no rows.|`crates/buildl-core/src/types/target_graph.rs:27-29` (`Node { label, … }`), `:117` (`nodes`), `:153` (`aliases`).|
|P16|A command that changes rows folds the cache after the pipeline and drops the adapters before the lock.|`crates/buildl/src/workspace.rs:179-195` (`Workspace::build`: `lock`, `Pipeline::new`, `build`, `cache.fold()`, `drop(pipeline)`, `concluded`).|
|P17|Removing whatever stands at a path, following no link, is one function.|`crates/buildl/src/state/clear.rs:25` (`clear_path`).|
|P18|Exit codes: `Error::Refused` → 4; `Error::Adapter` on any port but `Manifest` → 3.|`crates/buildl-cli/src/exit.rs:33` (`Refused = 4`), `:86-90`.|
|P19|A summary line counts with one helper, singular and plural.|`crates/buildl/src/report/message.rs:25` (`counted`), `:40-50` (`check_line`: `ok: {n} declarations in {m} build files`).|
|P20|A digest is read back from its hex form.|`crates/buildl-core/src/types/digest.rs:56` (`Digest::from_hex`).|
|P21|`LocalLog` is the `RunHistory` adapter and touches nothing when made.|`crates/buildl/src/log/local_log.rs:87` (`new`), `:114` (`impl RunHistory`).|
|P23|An output in `out/` is a hard link to its blob where the store links, and a copy otherwise; either way the file in `out/` outlives the removal of the blob's name in the cas.|`crates/buildl/src/cas/local_cas.rs:264-273` (`place`: `fs::hard_link(source, target)`, copy where it does not or cannot link).|
|P24|`check` loads twice and compares; `build` loads once.|`crates/buildl-core/src/pipeline/driver.rs:235-237` (`check`: `load` twice), `:118-129` (`plan`, which `build` follows: `load` once).|
|P22|The port names an adapter error can carry.|`crates/buildl-core/src/error.rs:558-` (`PortName`: `Manifest`, `Host`, `Digester`, `StatCache`, `ToolResolver`, `ActionCache`, `ContentStore`, `EventLog`, `RunHistory`, `ExecStrategy`, `Dispatcher`, …).|

## 2. The commands

```text
buildl clean
buildl gc
```

|Aspect|`clean`|`gc`|
|---|---|---|
|Arguments|none|none|
|Reads|`buildl.toml`|`buildl.toml`, every build file, `cache.json` + journal, `log.jsonl`, the cas|
|Writes|removes the output directory|forgets rows (journal, then `cache.json` on fold); removes blobs and twins|
|Lock|taken (P3); busy → exit 3|taken (P3); busy → exit 3|
|stdout|nothing|nothing|
|stderr, on success|`clean: removed out` or `clean: out holds nothing` (the manifest's output directory name)|`gc: dropped 1 row, removed 5 blobs (2048 bytes)` (`counted`, P19). The bytes are those of the store entries removed, not disk reclaimed: a blob still hard-linked from `out/` (P23) frees nothing until that link goes too.|
|Exit|0; 4 when the ceiling does not allow writing the output directory or the state directory (P3); 3 busy lock, or a removal that fails (`Error::Adapter` naming `PortName::ContentStore`, the port whose `clear` does the same job)|0; 4 when the ceiling does not allow writing the state directory (P3); a load or resolve error exits as it does for `build`; 3 busy lock or I/O|

Both make `.buildl/lock` when the state directory is missing, as `plan` does (P3).

## 3. Components

### 3.1 `Sweep` port (new file `ports/sweep.rs`) and `PortName::Sweep`

```rust
/// Lists and removes what a content store holds. Kept apart from `ContentStore`, so a build,
/// which holds a `ContentStore`, cannot remove what it stored.
pub trait Sweep {
    /// Every digest the store holds a blob or a twin for, in digest order.
    fn held(&self) -> Result<BTreeSet<Digest>>;
    /// Removes the blob and the twin held under `digest`; returns the bytes they took.
    /// Nothing held is `Ok(0)`.
    fn remove(&self, digest: &Digest) -> Result<u64>;
}
```

Errors are `Error::Adapter` naming `PortName::Sweep` (new variant, P22). The `Ports` bundle does
not gain it: like `RunHistory`, it is handed only to the one command that needs it (P13).

### 3.2 `ActionCache::labels`

```rust
/// Every label with a row, in label order.
fn labels(&self) -> Vec<Label>;
```

Added to the trait (P10); each of the six implementations returns its keys.

### 3.3 `history::logged_outputs` (pure)

`fn logged_outputs(lines: &[LogLine]) -> BTreeSet<Digest>`: for each label with a run that stored
output, the stdout and stderr digests of its newest such run — the run `log` shows (P12), by the
same `has_output` rule. Damaged lines and lines without both digests contribute nothing.

### 3.4 `pipeline::gc` and `Collected` (new file `pipeline/gc.rs`)

```rust
pub struct Collected { pub rows_dropped: usize, pub blobs_removed: usize, pub bytes_removed: u64 }

pub fn gc<S, A, C, W, H>(source: &S, entry: &EntryName, cache: &mut A, store: &C,
                         sweep: &W, history: &H) -> Result<Collected>
where S: DeclarationSource, A: ActionCache, C: ContentStore, W: Sweep, H: RunHistory;
```

```text
 1 graph   = resolve(load(source, entry))           error → return; nothing forgotten or removed
 2 for label in cache.labels() not a node of graph: cache.forget(label)       rows_dropped
 3 marked  = for each remaining row:
               each output digest; store.get(it) → OutputManifest::from_bytes →
                 File: its digest · Tree: every entry's digest
               row.stdout, row.stderr
           ∪ logged_outputs(history.lines())
 4 for digest in sweep.held() − marked: bytes_removed += sweep.remove(digest)   blobs_removed
```

|Rule|Reason|
|---|---|
|Step 1 loads once, as `build` does (P24).|A build file that fails to load stops `gc` with the error `check` gives for it. A nondeterministic build file is `check`'s to catch, by its second load; `gc` keeps the rows of whatever this one load declares.|
|Rows are forgotten (step 2) before anything is removed (step 4).|A kept row never names a removed blob (design §8.5). An interrupted `gc` leaves extra blobs, never a dangling row.|
|A manifest digest is marked even when its own blob is missing, damaged or not a manifest; the files it would name are then not marked.|`get` removes a damaged blob (P6); the row then lacks what a cache hit needs, and the next build runs that target again (P1's restore path needs the manifest).|
|Any port error in steps 2–3 returns before step 4.|No blob is removed against a partial mark.|
|An error in step 4 returns at once.|What was removed is reported by nothing; the next `gc` finds the rest.|

Module edges: none new (P14). `pipeline/mod.rs` re-exports `Collected` and `gc`.

### 3.5 `LocalCas: Sweep`

`held` reads each two-hex-character directory under `cas/` (P5); a name that is the other 62 hex
characters, with or without `.x`, is a digest (P20). Every other name — `tmp`, a foreign file —
is ignored and left. A missing `cas/` holds nothing. `remove` deletes both names through
`clear_path` (P17) and returns the sum of their sizes; empty shard directories are left.

### 3.6 `LocalCache::labels`

The keys of its map (P9).

### 3.7 `Workspace::clean` and `Workspace::gc`

```text
clean: out = workspace.out
       ceiling.write holds no dir containing out  → Error::Refused { Want::Write(out) }, before the lock
       lock (P3, which also sweeps leftovers, P2)
       existed = out is present; clear_path(root/out)
         failure → Error::Adapter { port: PortName::ContentStore, … }
       → Cleaned::Removed | Cleaned::Absent

gc:    lock; LocalCache::open; LocalCas; LocalLog
       pipeline::gc(source, entry, &mut cache, &cas, &cas, &log)
       cache.fold() — as build does (P16); drop adapters, then the lock
```

`clean` has no core logic: design §7 lists no phase for it, so it lives in the facade.
`buildl::report::message` gains `clean_line(&Cleaned, &Directory)` and `gc_line(&Collected)`.

### 3.8 `buildl-cli`

`Command::Clean` and `Command::Gc` (no arguments), dispatched in `run.rs` like `check`: call the
facade, write the line to stderr, exit 0. Errors map through the existing `exit.rs` rules (P18).

## 4. Testing

|Layer|Proves|
|---|---|
|`history` unit|`logged_outputs` takes each label's newest run with both digests; skipped, `null` and damaged lines add nothing; an older run's digests are not taken.|
|`pipeline::gc` unit, fake ports|undeclared rows forgotten, declared kept; every `forget` happens before the first `remove`; a kept row's manifests, the files they name, stdout and stderr are kept; log-only digests kept; a missing or non-manifest manifest keeps the row and marks only the manifest digest; a load error forgets and removes nothing; a store error in marking removes nothing; counts and bytes as summed.|
|`buildl-core` flows (`tests/flows`)|`FakeCache` and a fake sweep run a whole `gc` after a `build` over the fake ports.|
|`LocalCas` adapter|`held` lists blob-only, twin-only and both, ignores `tmp` and foreign names, empty for a missing `cas/`; `remove` deletes both names and returns their bytes; 0 when absent.|
|`LocalCache` adapter|`labels` after open, record and forget.|
|e2e via `Workspace`|build; remove a target from a build file; `gc` drops its row and its only-used blobs, and `log` of it still reads; a second `gc` removes 0; outputs in `out/` still read after `gc`; a file left in `cas/tmp` is gone; `clean` then `build` → every target cached (P1); `clean` with no `out/` → `Absent`; a broken build file → `gc` fails and the cas is unchanged.|
|CLI binary|both stderr lines, empty stdout; exit 0; 3 with the lock held; 4 for `clean` when the ceiling leaves out the output directory.|

## 5. Documents

|File|Change|
|---|---|
|`docs/design.md` §7|the `clean` / `gc` row describes them as built; §8.5 notes `gc` forgets rows before removing blobs.|
|`docs/architecture.md` §3, §3.5|the phase table gains `gc`; §3.5's "`gc` is designed and not built" paragraph becomes the built design (`Sweep`, the mark, the order).|
|`docs/architecture.md:3`|the Status paragraph names `clean` and `gc` among the built commands.|
|`docs/architecture-building-blocks.md`|the port table gains `Sweep` and `ActionCache::labels`; the command list gains `clean` and `gc`; the paragraph at `:425` naming the pipeline's free functions gains `pipeline::gc`.|
|`README.md`|command table row marked available; status line lists `clean` and `gc`.|
|`CLAUDE.md`|Status names `clean` and `gc` as built.|

## 6. Non-goals

- Removing one target's outputs, or resetting `.buildl/` (removing the directory does that).
- Bounding or rotating `log.jsonl`; pruning `statcache.json`.
- Reference counting, keeping blobs by age or size: mark and sweep only.
- A `gc` that keeps rows without loading the build files.
- Removing empty shard directories or files in `cas/` that are not named by a digest.
