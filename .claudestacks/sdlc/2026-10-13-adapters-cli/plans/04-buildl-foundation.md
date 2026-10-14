---
status: done
created: 2026-10-14
depends-on: [03]
---

# buildl Foundation Implementation Plan

**Goal:** The `buildl` crate becomes a skeleton that four sets of adapters can be written into at once, each in files no other set touches.

**Architecture:** `crates/buildl` gains every dependency its adapters need, a module for each of the thirteen ports it serves, the two modules those adapters share, and the gate rule that keeps them apart. `lib.rs` stays a table of contents: fifteen `pub mod` lines. Each adapter module is a directory whose `mod.rs` holds a module doc comment and nothing else, so the crate compiles and lints clean with the adapters unwritten, and whoever writes an adapter adds files inside one directory and edits that directory's `mod.rs` alone. Two modules implement no port and name no other module of the crate: `state/` (`StateLayout`, the path of each file of a state directory; `StateLock` and `LockError`, one process per state directory; `write_whole` and `Access`, a file written to a temp file and renamed into place; `clear_path`, removing whatever stands at a path) and `search_path.rs` (`SearchPath`, the one rule from a program's name to a file). They are the only code of this crate an adapter may import. A new `cargo make` task, `guard-adapter-isolation`, hangs on `clippy` beside the three existing guards and refuses any other import between modules; it is a rule check with no golden file. `deny.toml` lets `buildl` name `toml`, `tempfile` and `rayon`, and `crates/expected-edges.txt` records the five new edges of `buildl`. The composition modules, `local_ports.rs` and `workspace.rs`, and the re-exports of `lib.rs` are not part of this plan: no adapter may name them, so nothing here needs them declared.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, `tempfile` 3.27.0 and `thiserror` 2 (used here), `rayon` 1.12.0, `serde` 1 and `toml` 0.8.23 (declared here, first used by the adapters), `buildl-core` (`Directory`, `Program`, `Timestamp`), `cargo-make` 0.37, `cargo-deny` 0.20, POSIX `sh`.

**Content authority:** spec §3 (module tree, import rule), §4.5 (`SearchPath`), §5.1 (state layout), §5.2 (`state/`), §8.4 (the `pub mod` lines of `lib.rs`), §12.1 and §12.2 (the `state/` and `search_path.rs` tests), §13.1 to §13.5 (the gate), §14 (dependencies), §17 (the rows the `state/` and `search_path.rs` tests prove on Linux).

---

## Context an implementer needs

What the workspace holds when this plan starts, and what this plan builds on:

| Item | Where | Used as |
|---|---|---|
| `crates/buildl/src/lib.rs` is a crate doc comment and no item | `crates/buildl/src/lib.rs` | the file task 2 replaces |
| `crates/buildl` depends on `buildl-core` and `buildl-lua` only | `crates/buildl/Cargo.toml` | the manifest task 1 extends |
| `Directory::parse(raw) -> Result<Directory>`, `Directory::as_str(&self) -> &str`; the empty string is the workspace root | `buildl_core::Directory` | the state directory's name, `Workspace::state` |
| `Program::parse(raw) -> Result<Program>`, `Program::as_str(&self) -> &str`; any non-empty text without `NUL` and `$`, so `cc`, `/usr/bin/cc` and `scripts/gen.sh` are all programs | `buildl_core::Program` | what `SearchPath::locate` is asked for |
| `Timestamp::from_unix_nanos(u64)`, `Ord` | `buildl_core::Timestamp` | when a `StateLock` was taken |
| The thirteen port traits `Manifest`, `Host`, `Digester`, `ToolResolver`, `ContentStore`, `ActionCache`, `EventLog`, `StatCache`, `GraphRecord`, `ExecStrategy`, `Dispatcher`, `Clock`, `Reporter`, and `DeclarationSource`, each exported at the root of `buildl_core` | `crates/buildl-core/src/lib.rs` | the targets of the doc links in `lib.rs` and in each `mod.rs` |
| `[workspace.dependencies]` already names `rayon`, `toml`, `tempfile`, `serde`, `thiserror`; `Cargo.lock` already holds `tempfile` 3.27.0, `toml` 0.8.23, `serde` 1.0.229 and `thiserror` 2.0.20 through other members | `Cargo.toml`, `Cargo.lock` | task 1 raises three floors and adds five lines to `buildl` |
| `clippy`'s dependencies are `guard-core-purity`, `guard-crate-edges`, `guard-module-edges` | `Makefile.toml` | the list task 3 extends |
| `cargo make deny` is `cargo deny check -D unused-wrapper`, and CI runs that same command | `Makefile.toml`, `.github/workflows/ci.yml` | the supply-chain half of the gate |

Gate facts that bite here. Each was seen by building this plan's code in task order in a throwaway copy of the workspace (rustc 1.99.0, macOS arm64), and the quoted lines are what the tools printed.

- **A dependency nothing uses trips nothing.** After task 1 `buildl` depends on `rayon`, `serde` and `toml` with no `use` of any, and `cargo make dod` exits `0`: the workspace does not turn on `unused_crate_dependencies`.
- **The crate-edge guard and `cargo deny` both read the manifests, so the manifest, `deny.toml` and the golden file change in one commit.** With the manifest edited and nothing else, `cargo make guard-crate-edges` fails on five missing lines and `cargo deny check bans` fails with `error[banned]: crate 'tempfile = 3.27.0' is explicitly banned` and the same for `toml = 0.8.23`.
- **A wrapper for a dependency edge that does not exist yet is an error.** `cargo make deny` passes `-D unused-wrapper`. With `"buildl-cli"` among `tempfile`'s wrappers, or a `clap` entry wrapped by `buildl-cli`, before `buildl-cli` depends on either, it prints `error[unused-wrapper]: wrapper for banned crate was not encountered` and exits `2`. So this plan widens `deny.toml` for `buildl` alone. The `buildl-cli` wrapper of `tempfile`, the `clap` entry, the `clap` line of `[workspace.dependencies]` and the `# buildl-cli` section of `crates/expected-edges.txt` belong to the commit that gives `buildl-cli` those dependencies.
- **`cargo tree -d` is the duplicate check for the host, `cargo deny` the one for every platform.** `cargo tree -d` prints `warning: nothing to print.` before and after this plan. `cargo tree -d --target all` lists `syn` 2.0.119 beside `syn` 3.0.5, before this plan and after it alike: the older one is reached only through `jiff-static`, which `jiff` names under `cfg(any())`, an edge no platform enables. `cargo deny`, whose `multiple-versions` is `deny` over the whole graph, reports `bans ok`.
- **A crate-private item nothing calls is dead code, and so is a private field nothing reads.** With `write_whole` and `Access` declared `pub(crate)` and no adapter written, clippy fails with ``error: function `write_whole` is never used``, ``error: enum `Access` is never used`` and ``error: unused imports: `Access` and `write_whole` ``. They are `pub`, in the public module `state`. `clear_path` stays `pub(crate)`: taking the lock sweeps half-written files through it, which is why it lands in the task that adds the sweep. `StateLock` holds its file only to keep the lock; without a reader clippy fails with ``error: field `file` is never read``, so `StateLock` implements `Drop` and unlocks there.
- **Two of the new guard's three text checks read comment lines too.** `crate::{` and `super::super` anywhere under `crates/buildl/src`, and `super::` in any `mod.rs` or top-level file, are refused in a doc comment as in code. No doc comment in this plan writes them.
- **`super` in a top-level file is the crate root.** `search_path.rs` is one file, so its tests import `crate::search_path::SearchPath`, not `super::SearchPath`. Files inside `state/` may write `super::`.
- **rustfmt sorts `statcache` before `state`.** The `pub mod` lines of `lib.rs` are in the order rustfmt keeps.
- **Tests may not `panic!`, and `assert!(x.is_empty())` is refused.** `clippy::panic` is denied in tests as everywhere; a test that expects an error takes it with `unwrap_err()` and checks it with `matches!`. `clippy::assert_is_empty` (nursery) wants `assert_eq!` against an empty value. Every test module opens with the `#![expect(clippy::unwrap_used, …)]` the other crates' tests use.
- **A file whose rustdoc holds a fenced example is shown below inside four backticks.**
- **The filesystem tests name no platform.** They use `std::os::unix` only, never read the current directory, set no environment variable and no umask, and compare a mode with what the umask lets through. They run on both legs of the CI matrix; nothing was run on Linux while this plan was written.

What the plans after this one may rely on, exactly as this plan leaves it:

| Item | Signature |
|---|---|
| `buildl::state::StateLayout` | `#[derive(Debug, Clone, PartialEq, Eq)]`; `pub fn new(root: &Path, state: &Directory) -> Self`; `pub fn dir(&self) -> &Path`; `lock`, `cache`, `journal`, `stat_cache`, `log`, `graph`, `blobs`, `blob_staging`, each `pub fn …(&self) -> PathBuf` |
| `buildl::state::StateLock` | `#[derive(Debug)]`; `pub fn acquire(layout: StateLayout) -> Result<Self, LockError>`; `pub const fn layout(&self) -> &StateLayout`; `pub const fn acquired(&self) -> Timestamp`; unlocks on drop |
| `buildl::state::LockError` | `#[derive(Debug, thiserror::Error)] #[non_exhaustive]`; `Busy { path: PathBuf }`, rendered `<path> is in use by another buildl`; `Io { path: PathBuf, source: std::io::Error }`, rendered `<path>: <source>` |
| `buildl::state::Access` | `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum Access { Rewritable, ReadOnly, ReadOnlyRunnable }`: modes `0o644`, `0o444`, `0o555` |
| `buildl::state::write_whole` | `pub fn write_whole(staging: &Path, target: &Path, bytes: &[u8], access: Access) -> io::Result<()>` |
| `crate::state::clear_path` | `pub(crate) fn clear_path(path: &Path) -> io::Result<()>` |
| `buildl::search_path::SearchPath` | `#[derive(Debug, Clone, PartialEq, Eq, Default)]`; `pub fn from_process() -> Self`; `pub fn new(directories: impl IntoIterator<Item = PathBuf>) -> Self`; `pub fn locate(&self, program: &Program, base: &Path) -> Option<PathBuf>` |

- An adapter is written by adding files inside its own directory under `crates/buildl/src/` and editing that directory's `mod.rs`: `mod` lines and `pub use` lines below the doc comment this plan writes. Nothing else of this plan's files needs to change for it.
- `lib.rs` is finished by the composition work, which adds below the fifteen `pub mod` lines:

  ```rust
  mod local_ports;
  mod workspace;

  pub use buildl_core;
  pub use local_ports::{LocalPipeline, LocalPorts, Wiring};
  pub use workspace::Workspace;
  ```

  and extends the crate doc comment with the facade and the re-export. Neither module is declared here, because no adapter may name them and nothing in this plan uses them.

All work happens in the worktree, on its branch, never on `main`. Commits follow Conventional Commits, one per task. Every Rust block below is already in rustfmt's form. Each quoted run, the failing one and the passing one, was captured by running that command on the state its step describes. A failing run is quoted by the lines that name the cause, not in full. In a quoted test result the `filtered out` figure is what the crate holds when the tasks run in this order with nothing else added to `crates/buildl`.

## File structure

```
Cargo.toml                                — [modify] floors of rayon, toml, tempfile; four comments
Cargo.lock                                — [modify] rayon and its four packages; buildl's five dependencies
crates/buildl/Cargo.toml                  — [modify] rayon, serde, tempfile, thiserror, toml
deny.toml                                 — [modify] buildl may name toml, tempfile and rayon
crates/expected-edges.txt                 — [modify] the five new edges of buildl
Makefile.toml                             — [modify] the guard-adapter-isolation task, hung on clippy
crates/buildl/src/lib.rs                  — [modify] the crate doc; fifteen `pub mod` lines
crates/buildl/src/manifest/mod.rs         — [create] module doc: buildl.toml, the Manifest port
crates/buildl/src/host/mod.rs             — [create] module doc: platform and environment, the Host port
crates/buildl/src/digest/mod.rs           — [create] module doc: file hashing, the Digester port
crates/buildl/src/tools/mod.rs            — [create] module doc: tool fingerprints, the ToolResolver port
crates/buildl/src/cas/mod.rs              — [create] module doc: content by digest, the ContentStore port
crates/buildl/src/cache/mod.rs            — [create] module doc: rows, the ActionCache port
crates/buildl/src/log/mod.rs              — [create] module doc: the log, the EventLog port
crates/buildl/src/statcache/mod.rs        — [create] module doc: remembered digests, the StatCache port
crates/buildl/src/graph/mod.rs            — [create] module doc: the graph file, the GraphRecord port
crates/buildl/src/exec/mod.rs             — [create] module doc: running an action, the ExecStrategy port
crates/buildl/src/dispatch/mod.rs         — [create] module doc: worker threads, the Dispatcher port
crates/buildl/src/clock/mod.rs            — [create] module doc: the wall clock, the Clock port
crates/buildl/src/report/mod.rs           — [create] module doc: what a build shows, the Reporter port
crates/buildl/src/state/mod.rs            — [create] declares and re-exports the four files below
crates/buildl/src/state/layout.rs         — [create] StateLayout
crates/buildl/src/state/write.rs          — [create] Access, write_whole
crates/buildl/src/state/lock.rs           — [create] StateLock, LockError
crates/buildl/src/state/clear.rs          — [create] clear_path
crates/buildl/src/search_path.rs          — [create] SearchPath
```

Every task ends with the same gate, run from the workspace root:

```
$ cargo fmt --all -- --check
$ cargo make dod
$ cargo make deny
```

All three exit `0`. `cargo fmt --all -- --check` prints nothing. `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.` and, from task 3 on, prints `guard-adapter-isolation: OK (0 edges)` among its first lines. `cargo make deny` prints `advisories ok, bans ok, licenses ok, sources ok`. The gate was run after every task below and exited `0` each time.

### Task 1 — Add the dependencies of the `buildl` adapters

**Files:**
- Modify `Cargo.toml`
- Modify `Cargo.lock`
- Modify `crates/buildl/Cargo.toml`
- Modify `deny.toml`
- Modify `crates/expected-edges.txt`

**Steps:**

1. Raise three floors in `[workspace.dependencies]` of the root `Cargo.toml`, and say in a comment what each crate is for now. Four replacements. The `clap` line stays as it is.

   Replace

   ```toml
   # SHA-256 for the Digest type — files, outputs, action keys, log blobs. SHA-256
   # over blake3 for REAPI compatibility (decision record, architecture.md §6); the
   # stat cache makes hashing rare on warm builds anyway.
   sha2       = "0.10"
   ```

   with

   ```toml
   # SHA-256 for the Digest type — files, outputs, action keys, log blobs. SHA-256
   # over blake3 for REAPI compatibility (decision record, architecture.md §6); the
   # stat cache makes hashing rare on warm builds anyway. A dependency of
   # `buildl-core` only: an adapter that hashes a file streams it through the core's
   # `Hasher`, so every digest in the workspace comes from one implementation.
   sha2       = "0.10"
   ```

   Replace

   ```toml
   rayon      = "1"
   ```

   with

   ```toml
   rayon      = "1.12.0"
   ```

   Replace

   ```toml
   # buildl.toml (the ceiling) parsing in the `Manifest` adapter. Read-only, like
   # airsl's extension manifests: the `display` (serialization) default feature is dropped.
   toml       = { version = "0.8", default-features = false, features = ["parse"] }
   ```

   with

   ```toml
   # buildl.toml (the ceiling) parsing in the `Manifest` adapter. Read-only, like
   # airsl's extension manifests: the `display` (serialization) default feature is dropped.
   # Held on 0.8 because airsl depends on 0.8: a second `toml` tree would fail
   # cargo-deny's duplicate-version check.
   toml       = { version = "0.8.23", default-features = false, features = ["parse"] }
   ```

   Replace

   ```toml
   # Exec-dir materialisation for the tier-2 input sandbox and temp-file-then-rename
   # atomic writes in the storage adapters (architecture.md §3.5,
   # architecture-building-blocks.md §6).
   tempfile   = "3.23.0"
   ```

   with

   ```toml
   # Temp-file-then-rename writes in `buildl`'s shared `state` module, which every
   # storage adapter writes a whole file through (architecture.md §3.5,
   # architecture-building-blocks.md §6), and the temp workspaces of tests. The floor
   # is the release whose source was read for what those writes rely on:
   # `Builder::permissions` hands the mode to `open(2)`, and `persist` is a rename.
   tempfile   = "3.27.0"
   ```

2. Give `crates/buildl` its dependencies. In `crates/buildl/Cargo.toml` replace

   ```toml
   # The Lua build-file adapter, bound to the pipeline here alongside the others.
   buildl-lua  = { workspace = true }
   ```

   with

   ```toml
   # The Lua build-file adapter, bound to the pipeline here alongside the others.
   buildl-lua  = { workspace = true }
   # Hashing many input files at once. Named by the `Digester` adapter and by no
   # other module, so the executor's plain threads stay the crate's one threading model.
   rayon       = { workspace = true }
   # Derives for what this crate reads and writes in a shape of its own: the tables
   # of `buildl.toml`, a line of the cache journal and an entry of the stat cache.
   serde       = { workspace = true }
   # Temp-file-then-rename in the shared `state` module, and the temp workspaces of
   # this crate's tests.
   tempfile    = { workspace = true }
   # The error of taking the state-directory lock.
   thiserror   = { workspace = true }
   # Parsing `buildl.toml` in the `Manifest` adapter.
   toml        = { workspace = true }
   ```

   There is no `[dev-dependencies]` table: the crate's tests use `tempfile`, which is a dependency already, and `std`.

3. Resolve the lockfile and look at what entered it:

   ```
   $ cargo check -p buildl
   $ cargo tree -p buildl --depth 1 --edges normal,dev --prefix none --format "{p} {l}"
   ```

   Expected: `cargo check` ends `Finished`, and the second command prints, with the workspace's own path in place of `…`:

   ```
   buildl v0.1.0 (…/crates/buildl) Apache-2.0
   buildl-core v0.1.0 (…/crates/buildl-core) Apache-2.0
   buildl-lua v0.1.0 (…/crates/buildl-lua) Apache-2.0
   rayon v1.12.0 MIT OR Apache-2.0
   serde v1.0.229 MIT OR Apache-2.0
   tempfile v3.27.0 MIT OR Apache-2.0
   thiserror v2.0.20 MIT OR Apache-2.0
   toml v0.8.23 MIT OR Apache-2.0
   ```

   `Cargo.lock` gains fifty lines and loses none (`git diff --numstat Cargo.lock` prints `50	0	Cargo.lock`): five lines in the entry of `buildl`, which now reads

   ```toml
   [[package]]
   name = "buildl"
   version = "0.1.0"
   dependencies = [
    "buildl-core",
    "buildl-lua",
    "rayon",
    "serde",
    "tempfile",
    "thiserror",
    "toml",
   ]
   ```

   and five packages, each `MIT OR Apache-2.0` with a `rust-version` of 1.80 or lower:

   ```toml
   [[package]]
   name = "crossbeam-deque"
   version = "0.8.8"
   source = "registry+https://github.com/rust-lang/crates.io-index"
   checksum = "622f3fc73690be383c7214310406f28a90e6edeadc3cea882f9d71e495b9711a"
   dependencies = [
    "crossbeam-epoch",
    "crossbeam-utils",
   ]

   [[package]]
   name = "crossbeam-epoch"
   version = "0.9.21"
   source = "registry+https://github.com/rust-lang/crates.io-index"
   checksum = "dc74980687109a3b14c72fd458107bf0baa1da1a1a805e178d15501ba9b86d9d"
   dependencies = [
    "crossbeam-utils",
   ]

   [[package]]
   name = "crossbeam-utils"
   version = "0.8.23"
   source = "registry+https://github.com/rust-lang/crates.io-index"
   checksum = "a31eee39dddec8330830986fcd7625edb5a24ec90ea038215273bbc3adb08ac6"

   [[package]]
   name = "rayon"
   version = "1.12.0"
   source = "registry+https://github.com/rust-lang/crates.io-index"
   checksum = "fb39b166781f92d482534ef4b4b1b2568f42613b53e5b6c160e24cfbfa30926d"
   dependencies = [
    "either",
    "rayon-core",
   ]

   [[package]]
   name = "rayon-core"
   version = "1.13.0"
   source = "registry+https://github.com/rust-lang/crates.io-index"
   checksum = "22e18b0f0062d30d4230b2e85ff77fdfe4326feb054b9783a3460d8435c8ab91"
   dependencies = [
    "crossbeam-deque",
    "crossbeam-utils",
   ]
   ```

   Nothing else in the lockfile moves: `either`, which `rayon` needs, was there already. If the registry has published a newer release of one of the five since this was written and the listing above shows it, pin the versions this plan was proven with, then run the two commands of this step again:

   ```
   $ cargo update -p rayon --precise 1.12.0
   $ cargo update -p rayon-core --precise 1.13.0
   $ cargo update -p crossbeam-deque --precise 0.8.8
   $ cargo update -p crossbeam-epoch --precise 0.9.21
   $ cargo update -p crossbeam-utils --precise 0.8.23
   ```

4. See the two checks that read the manifest fail, which is why the next two steps are in this commit:

   ```
   $ cargo make guard-crate-edges
    # buildl
    buildl_core
    buildl_lua
   +rayon
   +serde
   +tempfile
   +thiserror
   +toml
    # buildl-cli
    buildl
   guard: crate edges do not match crates/expected-edges.txt
          fix the manifest, or update the golden file if the change is intended
   Error while executing command, exit code: 1
   $ cargo deny check bans
   error[banned]: crate 'tempfile = 3.27.0' is explicitly banned
   error[banned]: crate 'toml = 0.8.23' is explicitly banned
   ```

5. Let `buildl` name the three crates it is the home of. Three replacements in the `deny = [ … ]` list of `deny.toml`.

   Replace

   ```toml
       { crate = "toml", wrappers = ["airsl"], reason = """\
   ```

   with

   ```toml
       { crate = "toml", wrappers = ["airsl", "buildl"], reason = """\
   ```

   Replace

   ```toml
       { crate = "tempfile", wrappers = ["airsl", "buildl-lua"], reason = """\
           ABB §1.2: temp-file-then-rename is a storage-adapter concern; buildl-lua uses it only \
           to build symlink fixtures in its tests""" },
   ```

   with

   ```toml
       { crate = "tempfile", wrappers = ["airsl", "buildl-lua", "buildl"], reason = """\
           ABB §1.2: temp-file-then-rename is a storage concern of buildl's shared state module; \
           buildl-lua and buildl also use it for the temp workspaces of their tests""" },
   ```

   Replace

   ```toml
       { crate = "globset", wrappers = ["airsl", "buildl-lua"], reason = """\
           ABB §1.2: b.sources() globbing is a filesystem read, not core logic; buildl-lua \
           matches it host-side""" },
   ```

   with

   ```toml
       { crate = "globset", wrappers = ["airsl", "buildl-lua"], reason = """\
           ABB §1.2: b.sources() globbing is a filesystem read, not core logic; buildl-lua \
           matches it host-side""" },
       { crate = "rayon", wrappers = ["buildl"], reason = """\
           architecture.md §6: rayon is confined to the Digester adapter inside buildl""" },
   ```

6. Regenerate `crates/expected-edges.txt` with the crate-edge guard's own `cargo tree` command, one member at a time, in the guard's order:

   ```
   $ printf '# buildl-core\n' > crates/expected-edges.txt
   $ cargo tree -p buildl-core --depth 1 --edges normal,dev --prefix none --format '{lib}' | tail -n +2 >> crates/expected-edges.txt
   $ printf '# buildl-lua\n' >> crates/expected-edges.txt
   $ cargo tree -p buildl-lua --depth 1 --edges normal,dev --prefix none --format '{lib}' | tail -n +2 >> crates/expected-edges.txt
   $ printf '# buildl\n' >> crates/expected-edges.txt
   $ cargo tree -p buildl --depth 1 --edges normal,dev --prefix none --format '{lib}' | tail -n +2 >> crates/expected-edges.txt
   $ printf '# buildl-cli\n' >> crates/expected-edges.txt
   $ cargo tree -p buildl-cli --depth 1 --edges normal,dev --prefix none --format '{lib}' | tail -n +2 >> crates/expected-edges.txt
   ```

   Expected: the file now reads as below, and `git diff crates/expected-edges.txt` shows five added lines, `rayon` to `toml`, and nothing else.

   ```text
   # buildl-core
   serde
   serde_json
   sha2
   thiserror
   # buildl-lua
   airsl
   buildl_core
   globset
   walkdir
   tempfile
   # buildl
   buildl_core
   buildl_lua
   rayon
   serde
   tempfile
   thiserror
   toml
   # buildl-cli
   buildl
   ```

7. Verify:

   ```
   $ cargo make guard-crate-edges
   $ cargo tree -d
   $ cargo make deny
   ```

   Expected: the guard prints no diff and ends `[cargo-make] INFO - Build Done in … seconds.`; `cargo tree -d` prints `warning: nothing to print.`, so no crate is in the graph in two versions; `cargo make deny` prints `advisories ok, bans ok, licenses ok, sources ok`, so every new package's licence is one `deny.toml` already allows and `[licenses].allow` is untouched.

8. Run the gate (the three commands under "File structure"). Expected: all three exit `0`; `cargo make dod` reports the same tests as before this task, since no source file changed.

9. Commit:

   ```
   $ git add Cargo.toml Cargo.lock crates/buildl/Cargo.toml deny.toml crates/expected-edges.txt
   $ git commit -m "build(repo): add the dependencies of the buildl adapters"
   ```

### Task 2 — Declare a module for every port adapter

**Files:**
- Modify `crates/buildl/src/lib.rs`
- Create `crates/buildl/src/manifest/mod.rs`
- Create `crates/buildl/src/host/mod.rs`
- Create `crates/buildl/src/digest/mod.rs`
- Create `crates/buildl/src/tools/mod.rs`
- Create `crates/buildl/src/cas/mod.rs`
- Create `crates/buildl/src/cache/mod.rs`
- Create `crates/buildl/src/log/mod.rs`
- Create `crates/buildl/src/statcache/mod.rs`
- Create `crates/buildl/src/graph/mod.rs`
- Create `crates/buildl/src/exec/mod.rs`
- Create `crates/buildl/src/dispatch/mod.rs`
- Create `crates/buildl/src/clock/mod.rs`
- Create `crates/buildl/src/report/mod.rs`

These files hold no logic, so this task has no failing-test step. What checks them is the gate: `missing_docs` wants a doc comment on every public module, and rustdoc, run with warnings as errors, resolves every link to a port.

**Steps:**

1. Replace the whole of `crates/buildl/src/lib.rs` with:

   ```rust
   //! buildl — a sandboxed, deterministic build system whose build files are
   //! written in Lua and evaluated on the airsl embedded runtime.
   //!
   //! Build files *declare* targets; buildl schedules, sandboxes, parallelizes,
   //! and caches their execution. The pipeline is
   //! Load → Resolve → Plan → Execute → Record, with each phase handing the next
   //! a serializable value.
   //!
   //! This crate is the composition root: its role is to choose a concrete
   //! implementation for every port the `buildl-core` pipeline needs.
   //!
   //! # Where things live
   //!
   //! Each port a local build is served through has a module of its own here, named after its
   //! concern.
   //!
   //! | Module | Concern | Port |
   //! |---|---|---|
   //! | [`manifest`] | `buildl.toml` and the workspace root | [`Manifest`] |
   //! | [`host`] | the platform, and the environment values a target reads | [`Host`] |
   //! | [`digest`] | hashing the files a build reads | [`Digester`] |
   //! | [`tools`] | fingerprinting the programs a command names | [`ToolResolver`] |
   //! | [`cas`] | content held by digest, and the files of the output directory | [`ContentStore`] |
   //! | [`cache`] | the row a built target leaves behind | [`ActionCache`] |
   //! | [`log`] | one line for every action that ended | [`EventLog`] |
   //! | [`statcache`] | the digest of a file that has not changed since it was hashed | [`StatCache`] |
   //! | [`graph`] | the graph of the last build | [`GraphRecord`] |
   //! | [`exec`] | running one action as a process | [`ExecStrategy`] |
   //! | [`dispatch`] | the threads actions run on | [`Dispatcher`] |
   //! | [`clock`] | the wall clock | [`Clock`] |
   //! | [`report`] | what a build shows while it runs, and every rendering of a value as text | [`Reporter`] |
   //!
   //! [`DeclarationSource`] has no module here: its adapter is the `buildl-lua` crate.
   //!
   //! No module in the table names another module in the table. What two of them would otherwise
   //! each need from the other belongs to neither, so each can be read, tested and replaced alone.
   //!
   //! # Non-responsibilities
   //!
   //! - Pipeline logic, which lives in `buildl-core`.
   //! - Lua evaluation, which lives in `buildl-lua`.
   //!
   //! This file holds only module declarations, so it carries no logic to unit-test.
   //!
   //! [`ActionCache`]: buildl_core::ActionCache
   //! [`Clock`]: buildl_core::Clock
   //! [`ContentStore`]: buildl_core::ContentStore
   //! [`DeclarationSource`]: buildl_core::DeclarationSource
   //! [`Digester`]: buildl_core::Digester
   //! [`Dispatcher`]: buildl_core::Dispatcher
   //! [`EventLog`]: buildl_core::EventLog
   //! [`ExecStrategy`]: buildl_core::ExecStrategy
   //! [`GraphRecord`]: buildl_core::GraphRecord
   //! [`Host`]: buildl_core::Host
   //! [`Manifest`]: buildl_core::Manifest
   //! [`Reporter`]: buildl_core::Reporter
   //! [`StatCache`]: buildl_core::StatCache
   //! [`ToolResolver`]: buildl_core::ToolResolver

   pub mod cache;
   pub mod cas;
   pub mod clock;
   pub mod digest;
   pub mod dispatch;
   pub mod exec;
   pub mod graph;
   pub mod host;
   pub mod log;
   pub mod manifest;
   pub mod report;
   pub mod statcache;
   pub mod tools;
   ```

2. Create the thirteen module files. Each is the whole file.

   `crates/buildl/src/manifest/mod.rs`:

   ```rust
   //! The workspace manifest: `buildl.toml`, and the directory that holds it.
   //!
   //! Its own module because `buildl.toml` is both the marker of a workspace's root and the ceiling
   //! every grant is checked against, and because reading it is the only use this crate has for a
   //! TOML parser. It is the module of the [`Manifest`](buildl_core::Manifest) port's adapter.
   //!
   //! Responsibilities: finding the workspace root from a start directory, and turning the text of
   //! `buildl.toml` into the values a pipeline is handed.
   //!
   //! Non-responsibilities: deciding what a build may do with those values, which is the
   //! pipeline's, and any file of the state directory.
   //!
   //! This file holds no logic of its own, so it carries nothing to unit-test.
   ```

   `crates/buildl/src/host/mod.rs`:

   ```rust
   //! The machine a build runs on: its platform, and the environment values a target reads.
   //!
   //! Its own module because the process environment is ambient state. Reading it here, one
   //! declared name at a time, is what lets an action key hold exactly what a target was shown. It
   //! is the module of the [`Host`](buildl_core::Host) port's adapter.
   //!
   //! Responsibilities: naming the platform this crate was compiled for, and reading one
   //! environment variable when a pipeline asks for it.
   //!
   //! Non-responsibilities: checking a name against the workspace ceiling. A pipeline asks only for
   //! names that already passed it.
   //!
   //! This file holds no logic of its own, so it carries nothing to unit-test.
   ```

   `crates/buildl/src/digest/mod.rs`:

   ```rust
   //! Hashing the files a build reads.
   //!
   //! Its own module because it hashes many files at once, and is the only module of this crate
   //! that may name rayon: everything else here runs on plain threads. It is the module of the
   //! [`Digester`](buildl_core::Digester) port's adapter.
   //!
   //! Responsibilities: saying what stands at a declared input's path, and giving the digest of
   //! each file asked for, in the order asked.
   //!
   //! Non-responsibilities: remembering a digest from one build to the next, which is the stat
   //! cache's, and the hash function itself, which is `buildl-core`'s.
   //!
   //! This file holds no logic of its own, so it carries nothing to unit-test.
   ```

   `crates/buildl/src/tools/mod.rs`:

   ```rust
   //! Fingerprinting the programs a command names.
   //!
   //! Its own module because what a program is belongs in an action key, and is answered without
   //! running anything. It is the module of the [`ToolResolver`](buildl_core::ToolResolver) port's
   //! adapter.
   //!
   //! Responsibilities: the fingerprint of the file a program name leads to.
   //!
   //! Non-responsibilities: the rule that leads from a name to a file, which is `search_path`'s so
   //! that the file fingerprinted is the file run, and running the program.
   //!
   //! This file holds no logic of its own, so it carries nothing to unit-test.
   ```

   `crates/buildl/src/cas/mod.rs`:

   ```rust
   //! Content held by its digest, and the files of the output directory.
   //!
   //! Its own module because it alone decides how bytes are kept on disk and how kept bytes become
   //! a file of the workspace. It is the module of the
   //! [`ContentStore`](buildl_core::ContentStore) port's adapter.
   //!
   //! Responsibilities: keeping bytes under their digest, saying whether a digest is held, handing
   //! held bytes back, making held content the file at an output path, and clearing an output path.
   //!
   //! Non-responsibilities: knowing which target a digest belongs to. That is a row of the action
   //! cache, which this module never reads.
   //!
   //! This file holds no logic of its own, so it carries nothing to unit-test.
   ```

   `crates/buildl/src/cache/mod.rs`:

   ```rust
   //! The row each built target leaves behind, kept from one build to the next.
   //!
   //! Its own module because a row is what makes a later build skip a target, so how rows reach the
   //! disk and survive an interrupted build is one concern, kept in one place. It is the module of
   //! the [`ActionCache`](buildl_core::ActionCache) port's adapter.
   //!
   //! Responsibilities: reading the rows an earlier build recorded, and recording a row so that it
   //! is not lost once recording has returned.
   //!
   //! Non-responsibilities: the bytes a row's digests name, which the content store holds, and
   //! deciding whether a row is still good, which is the pipeline's.
   //!
   //! This file holds no logic of its own, so it carries nothing to unit-test.
   ```

   `crates/buildl/src/log/mod.rs`:

   ```rust
   //! The log of a workspace: one line for every action that ended.
   //!
   //! Its own module because the log only grows, unlike every other file of the state directory,
   //! which is replaced whole. It is the module of the [`EventLog`](buildl_core::EventLog) port's
   //! adapter.
   //!
   //! Responsibilities: appending one event as one line.
   //!
   //! Non-responsibilities: reading the log back, and pruning it.
   //!
   //! This file holds no logic of its own, so it carries nothing to unit-test.
   ```

   `crates/buildl/src/statcache/mod.rs`:

   ```rust
   //! The digest of a file that has not changed since it was last hashed.
   //!
   //! Its own module because trusting a remembered digest is a judgement about file timestamps,
   //! and that judgement is kept apart from the hashing it saves. It is the module of the
   //! [`StatCache`](buildl_core::StatCache) port's adapter.
   //!
   //! Responsibilities: reading the entries an earlier build left, taking new ones, and writing
   //! them back.
   //!
   //! Non-responsibilities: reading a file's size or modification time, and hashing it. Both are
   //! the digester's.
   //!
   //! This file holds no logic of its own, so it carries nothing to unit-test.
   ```

   `crates/buildl/src/graph/mod.rs`:

   ```rust
   //! The graph of the last build, kept as a file.
   //!
   //! Its own module because the graph is written by a build and read by nothing in a build: it is
   //! a record for whoever inspects the workspace afterwards. It is the module of the
   //! [`GraphRecord`](buildl_core::GraphRecord) port's adapter.
   //!
   //! Responsibilities: writing the graph a build resolved.
   //!
   //! Non-responsibilities: rendering a graph for a person, which is `report`'s.
   //!
   //! This file holds no logic of its own, so it carries nothing to unit-test.
   ```

   `crates/buildl/src/exec/mod.rs`:

   ```rust
   //! Running one action as a process.
   //!
   //! Its own module because it is the only place this crate starts a process, and so the place
   //! where what an action may see and how long it may run are enforced. It is the module of the
   //! [`ExecStrategy`](buildl_core::ExecStrategy) port's adapter.
   //!
   //! Responsibilities: making the place an action runs in, running its command and capturing what
   //! it prints, and gathering what it wrote.
   //!
   //! Non-responsibilities: choosing which action runs when, which is the pipeline's, and keeping
   //! what an action wrote, which is the content store's.
   //!
   //! This file holds no logic of its own, so it carries nothing to unit-test.
   ```

   `crates/buildl/src/dispatch/mod.rs`:

   ```rust
   //! The threads actions run on.
   //!
   //! Its own module because it is the only place this crate starts a thread for an action, which
   //! keeps how many run at once in one place. It is the module of the
   //! [`Dispatcher`](buildl_core::Dispatcher) port's adapter.
   //!
   //! Responsibilities: running the tasks it is handed on a bounded number of threads, and handing
   //! each result back on the thread that asked.
   //!
   //! Non-responsibilities: knowing what a task is. The port is generic over the task and its
   //! result, and the order tasks become ready in is the pipeline's.
   //!
   //! This file holds no logic of its own, so it carries nothing to unit-test.
   ```

   `crates/buildl/src/clock/mod.rs`:

   ```rust
   //! The wall clock.
   //!
   //! Its own module because the wall clock is read in exactly one place, so that no clock value
   //! can reach an action key or an artifact unnoticed. It is the module of the
   //! [`Clock`](buildl_core::Clock) port's adapter.
   //!
   //! Responsibilities: reading the host's wall clock as a timestamp.
   //!
   //! Non-responsibilities: measuring how long something took, which is the difference of two
   //! timestamps and is the pipeline's.
   //!
   //! This file holds no logic of its own, so it carries nothing to unit-test.
   ```

   `crates/buildl/src/report/mod.rs`:

   ```rust
   //! What a build shows while it runs, and every rendering of a value as text.
   //!
   //! Its own module because every byte a person reads is formed here, so a caller that prints
   //! formats nothing itself. It is the module of the [`Reporter`](buildl_core::Reporter) port's
   //! adapter.
   //!
   //! Responsibilities: showing the progress and the result of a build, and rendering a plan, a
   //! graph and an error as text.
   //!
   //! Non-responsibilities: choosing where the text goes, and detecting a terminal. Both are the
   //! caller's.
   //!
   //! This file holds no logic of its own, so it carries nothing to unit-test.
   ```

3. Verify that the crate formats, and that its documentation builds with every link resolved:

   ```
   $ cargo fmt --all -- --check
   $ RUSTDOCFLAGS="-D warnings" cargo doc -p buildl --all-features --no-deps
   ```

   Expected: the first prints nothing; the second ends `Generated …/doc/buildl/index.html` with no warning.

4. Run the gate. Expected: all three exit `0`.

5. Commit:

   ```
   $ git add crates/buildl/src
   $ git commit -m "feat(buildl): declare a module for every port adapter"
   ```

### Task 3 — Guard the isolation of the adapter modules

**Files:**
- Modify `Makefile.toml`

**Steps:**

1. See that the task does not exist:

   ```
   $ cargo make guard-adapter-isolation
   Task "guard-adapter-isolation" not found
   ```

2. Hang the new guard on `clippy`. In `Makefile.toml`, in `[tasks.clippy]`, replace

   ```toml
   dependencies = ["guard-core-purity", "guard-crate-edges", "guard-module-edges"]
   ```

   with

   ```toml
   dependencies = [
     "guard-core-purity",
     "guard-crate-edges",
     "guard-module-edges",
     "guard-adapter-isolation",
   ]
   ```

3. Add the task. In `Makefile.toml`, directly above the line `[tasks.guard-core-purity]`, insert this block, blank line at its end included:

   ```toml
   [tasks.guard-adapter-isolation]
   category = "Gate"
   description = "Assert no adapter module of buildl names another module of the crate"
   # The compiler lets any module of a crate import any other, so nothing stops one
   # adapter from leaning on another — and adapters that lean on each other can no
   # longer be read, tested or replaced alone (architecture-building-blocks.md §7
   # rule 2). What several adapters need lives in the two shared modules, `state`
   # and `search_path`, which implement no port; adapters meet only in the
   # composition modules, `local_ports` and `workspace`.
   #
   # A rule check with no golden file: the rule is "no edge", so a committed list
   # would record nothing but the composition root's own imports.
   #
   # The listing reads the text `crate::<module>` on lines that are not comments,
   # test code included. Three spellings cross modules without that text, so each
   # is refused outright: a grouped `crate::{...}`; `super::` climbing to the crate
   # root — twice from a nested file, once from a module's own mod.rs or from a
   # module that is a single file; and `crate::<Item>`, an item named through the
   # crate root, which hides the module it lives in.
   script_runner = "@shell"
   script = '''
   src="crates/buildl/src"
   # The modules that wire adapters together, and the modules adapters may share.
   composition="local_ports workspace"
   support="state search_path"
   export LC_ALL=C

   if grep -rnE 'crate::\{|super::super' "${src}"; then
       echo "guard: the imports above cross modules in a spelling the edge listing cannot read" >&2
       echo "       write each one as crate::<module>::<item>" >&2
       exit 1
   fi

   # In a module's mod.rs, and in a module that is one file, super is the crate root.
   if grep -n 'super::' "${src}"/*.rs "${src}"/*/mod.rs; then
       echo "guard: super:: in a mod.rs or in a single-file module names the crate root" >&2
       echo "       write crate::<module>::<item>, in that file's tests too" >&2
       exit 1
   fi

   if grep -rnE 'crate::[A-Z]' "${src}" | grep -vE '^[^:]*:[0-9]+:[[:space:]]*//'; then
       echo "guard: the lines above name an item through the crate root, which hides its module" >&2
       echo "       name a buildl-core item as buildl_core::<item>, and a module's as crate::<module>::<item>" >&2
       exit 1
   fi

   edges="$(mktemp)"
   trap 'rm -f "${edges}"' EXIT
   for entry in "${src}"/*; do
       module="$(basename "${entry}" .rs)"
       if [ "${module}" = "lib" ]; then
           continue
       fi
       grep -rhv '^[[:space:]]*//' "${entry}" \
           | grep -oE 'crate::[a-z_]+' \
           | sort -u \
           | sed "s|^crate::|${module} -> |" \
           | grep -v "^${module} -> ${module}$" >> "${edges}" || true
   done

   status=0
   while read -r from arrow to; do
       case " ${composition} " in *" ${from} "*) continue ;; esac
       case " ${support} " in *" ${to} "*)
           case " ${support} " in *" ${from} "*) ;; *) continue ;; esac ;;
       esac
       echo "guard: ${from} ${arrow} ${to}: a module of buildl names a module it may not" >&2
       status=1
   done < "${edges}"

   if [ "${status}" -ne 0 ]; then
       echo "       an adapter module may name state and search_path and no other module of the crate;" >&2
       echo "       state and search_path name none; adapters meet only in local_ports and workspace" >&2
       echo "       (architecture-building-blocks §7 rule 2)" >&2
       exit 1
   fi
   echo "guard-adapter-isolation: OK ($(wc -l < "${edges}" | tr -d ' ') edges)"
   '''
   ```

   The script between `script = '''` and `'''` is POSIX `sh`, like the three guards beside it.

4. Run it on the tree:

   ```
   $ cargo make guard-adapter-isolation
   guard-adapter-isolation: OK (0 edges)
   ```

5. Prove it refuses each kind of breach. Save the script below as `guard_cases.sh` in a directory outside the repository, your scratch directory for example. It is a probe, not a file of the workspace. Each case is a throwaway copy of `crates/buildl/src` with one line appended, and the real task is run with that copy as its working directory.

   ```sh
   #!/bin/sh
   # Proves the guard-adapter-isolation task refuses each kind of breach.
   #
   #   sh guard_cases.sh <workspace root>
   #
   # Each case is a throwaway copy of the workspace's crates/buildl/src with one line appended,
   # and the task is run with that copy as its working directory. Nothing in the workspace is
   # touched.
   root="$(cd "$1" && pwd)"
   work="$(mktemp -d)"
   trap 'rm -rf "${work}"' EXIT

   run_case() { # run_case <title> <file under src, or -> <line to append>
       echo "=== $1"
       case_dir="${work}/case"
       rm -rf "${case_dir}"
       mkdir -p "${case_dir}/crates/buildl"
       cp -R "${root}/crates/buildl/src" "${case_dir}/crates/buildl/src"
       if [ "$2" != "-" ]; then
           mkdir -p "$(dirname "${case_dir}/crates/buildl/src/$2")"
           printf '%s\n' "$3" >> "${case_dir}/crates/buildl/src/$2"
       fi
       cargo make --cwd "${case_dir}" --makefile "${root}/Makefile.toml" guard-adapter-isolation \
           > "${work}/out.txt" 2>&1
       status=$?
       grep -v '^\[cargo-make\] INFO' "${work}/out.txt"
       echo "exit ${status}"
   }

   run_case "1. the tree as it is" - -
   run_case "2. an adapter names another adapter" cache/mod.rs 'use crate::cas::LocalCas;'
   run_case "3. a shared module names another module" state/layout.rs 'use crate::search_path::SearchPath;'
   run_case "4. an adapter names the composition root" report/mod.rs 'use crate::local_ports::Wiring;'
   run_case "5. an item through the crate root" cache/mod.rs 'use crate::Workspace;'
   run_case "6. super:: out of a single-file module" search_path.rs 'use super::state::StateLayout;'
   run_case "7. super:: in a mod.rs" cas/mod.rs 'use super::state::StateLayout;'
   run_case "8. a grouped import" cache/mod.rs 'use crate::{cas::LocalCas, state::StateLayout};'
   run_case "9. super::super from a nested file" cas/blob.rs 'use super::super::cache::LocalCache;'
   run_case "10. a shared module is named: allowed" cas/blob.rs 'use crate::state::StateLayout;'
   ```

   Run it with the workspace root as its argument, written out in full:

   ```
   $ sh <the directory you saved it in>/guard_cases.sh <the workspace root>
   ```

   Expected, exactly:

   ```text
   === 1. the tree as it is
   guard-adapter-isolation: OK (0 edges)
   exit 0
   === 2. an adapter names another adapter
   guard: cache -> cas: a module of buildl names a module it may not
          an adapter module may name state and search_path and no other module of the crate;
          state and search_path name none; adapters meet only in local_ports and workspace
          (architecture-building-blocks §7 rule 2)
   Error while executing command, exit code: 1
   exit 105
   === 3. a shared module names another module
   guard: state -> search_path: a module of buildl names a module it may not
          an adapter module may name state and search_path and no other module of the crate;
          state and search_path name none; adapters meet only in local_ports and workspace
          (architecture-building-blocks §7 rule 2)
   Error while executing command, exit code: 1
   exit 105
   === 4. an adapter names the composition root
   guard: report -> local_ports: a module of buildl names a module it may not
          an adapter module may name state and search_path and no other module of the crate;
          state and search_path name none; adapters meet only in local_ports and workspace
          (architecture-building-blocks §7 rule 2)
   Error while executing command, exit code: 1
   exit 105
   === 5. an item through the crate root
   crates/buildl/src/cache/mod.rs:14:use crate::Workspace;
   guard: the lines above name an item through the crate root, which hides its module
          name a buildl-core item as buildl_core::<item>, and a module's as crate::<module>::<item>
   Error while executing command, exit code: 1
   exit 105
   === 6. super:: out of a single-file module
   crates/buildl/src/search_path.rs:1:use super::state::StateLayout;
   guard: super:: in a mod.rs or in a single-file module names the crate root
          write crate::<module>::<item>, in that file's tests too
   Error while executing command, exit code: 1
   exit 105
   === 7. super:: in a mod.rs
   crates/buildl/src/cas/mod.rs:14:use super::state::StateLayout;
   guard: super:: in a mod.rs or in a single-file module names the crate root
          write crate::<module>::<item>, in that file's tests too
   Error while executing command, exit code: 1
   exit 105
   === 8. a grouped import
   crates/buildl/src/cache/mod.rs:14:use crate::{cas::LocalCas, state::StateLayout};
   guard: the imports above cross modules in a spelling the edge listing cannot read
          write each one as crate::<module>::<item>
   Error while executing command, exit code: 1
   exit 105
   === 9. super::super from a nested file
   crates/buildl/src/cas/blob.rs:1:use super::super::cache::LocalCache;
   guard: the imports above cross modules in a spelling the edge listing cannot read
          write each one as crate::<module>::<item>
   Error while executing command, exit code: 1
   exit 105
   === 10. a shared module is named: allowed
   guard-adapter-isolation: OK (1 edges)
   exit 0
   ```

   `exit 105` is cargo-make's own status for a task whose script exited `1`. Cases 2 to 4 are the edges the rule forbids; cases 5 to 9 are the spellings that would hide an edge from the listing; case 10 shows that an adapter naming a shared module is counted and allowed.

6. Run the gate. Expected: all three exit `0`, and `cargo make dod` now prints `guard-adapter-isolation: OK (0 edges)` before clippy runs.

7. Commit:

   ```
   $ git add Makefile.toml
   $ git commit -m "build(repo): guard the isolation of buildl's adapter modules"
   ```

### Task 4 — Add `StateLayout`

**Files:**
- Create `crates/buildl/src/state/mod.rs`
- Create `crates/buildl/src/state/layout.rs`
- Modify `crates/buildl/src/lib.rs`

**Steps:**

1. Declare the module. Two replacements in `crates/buildl/src/lib.rs`.

   Replace

   ```rust
   //! [`DeclarationSource`] has no module here: its adapter is the `buildl-lua` crate.
   //!
   //! No module in the table names another module in the table. What two of them would otherwise
   //! each need from the other belongs to neither, so each can be read, tested and replaced alone.
   ```

   with

   ```rust
   //! [`DeclarationSource`] has no module here: its adapter is the `buildl-lua` crate.
   //!
   //! No module in the table names another module in the table. What two of them would otherwise
   //! each need from the other belongs to neither, so each can be read, tested and replaced alone.
   //! It lives in a module that serves no port and names no module of this crate:
   //!
   //! | Module | Shared by its users |
   //! |---|---|
   //! | [`state`] | where each file of the state directory is, the lock on it, and writing a file whole |
   ```

   Replace

   ```rust
   pub mod statcache;
   pub mod tools;
   ```

   with

   ```rust
   pub mod statcache;
   pub mod state;
   pub mod tools;
   ```

2. Create `crates/buildl/src/state/mod.rs`:

   ```rust
   //! The state directory of a workspace: where its files are, who may use it, and how a file in
   //! it is written and removed.
   //!
   //! Its own module because several adapters keep a file in the state directory and no adapter
   //! may name another. What each of them would otherwise write out for itself lives here once.
   //! This module implements no port, and names no other module of this crate.
   //!
   //! Responsibilities:
   //! - [`StateLayout`] — the path of each file of a state directory.
   //!
   //! Non-responsibilities: what any of those files holds. Each file's format belongs to the module
   //! that keeps it.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to
   //! unit-test.

   mod layout;

   pub use layout::StateLayout;
   ```

3. Write the failing tests. Create `crates/buildl/src/state/layout.rs` with the test module only:

   ```rust
   //! Placeholder — replaced later in this task.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::path::Path;

       use buildl_core::Directory;

       use super::StateLayout;

       fn layout() -> StateLayout {
           StateLayout::new(Path::new("/work"), &Directory::parse(".buildl").unwrap())
       }

       #[test]
       fn the_state_directory_is_joined_to_the_workspace_root() {
           assert_eq!(layout().dir(), Path::new("/work/.buildl"));
           let nested = StateLayout::new(Path::new("/work"), &Directory::parse("var/state").unwrap());
           assert_eq!(nested.dir(), Path::new("/work/var/state"));
       }

       #[test]
       fn every_file_has_its_name_directly_in_the_state_directory() {
           let layout = layout();
           assert_eq!(layout.lock(), Path::new("/work/.buildl/lock"));
           assert_eq!(layout.cache(), Path::new("/work/.buildl/cache.json"));
           assert_eq!(layout.journal(), Path::new("/work/.buildl/cache.journal"));
           assert_eq!(
               layout.stat_cache(),
               Path::new("/work/.buildl/statcache.json")
           );
           assert_eq!(layout.log(), Path::new("/work/.buildl/log.jsonl"));
           assert_eq!(layout.graph(), Path::new("/work/.buildl/graph.json"));
       }

       #[test]
       fn blobs_are_staged_inside_the_directory_that_holds_them() {
           let layout = layout();
           assert_eq!(layout.blobs(), Path::new("/work/.buildl/cas"));
           assert_eq!(layout.blob_staging(), Path::new("/work/.buildl/cas/tmp"));
       }
   }
   ```

4. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl --lib state::layout
   error[E0432]: unresolved import `super::StateLayout`
   error[E0432]: unresolved import `layout::StateLayout`
   ```

5. Replace the placeholder line at the top of `crates/buildl/src/state/layout.rs` with the implementation, leaving the test module below it unchanged. The file above its test module reads:

   ````rust
   //! Where each file of a workspace's state directory is.
   //!
   //! Its own file because the names inside the state directory are a format: every module that
   //! keeps a file there asks this type for the path, so the name of a file is written once.
   //!
   //! Responsibilities: [`StateLayout`].
   //!
   //! Non-responsibilities: making, reading or writing anything. A layout is a set of paths, and
   //! building one touches nothing on disk.

   use std::path::{Path, PathBuf};

   use buildl_core::Directory;

   /// Where each file of a workspace's state directory is.
   ///
   /// # Examples
   ///
   /// ```
   /// use std::path::Path;
   ///
   /// use buildl::state::StateLayout;
   /// use buildl_core::Directory;
   ///
   /// let layout = StateLayout::new(Path::new("/work"), &Directory::parse(".buildl")?);
   /// assert_eq!(layout.dir(), Path::new("/work/.buildl"));
   /// assert_eq!(layout.cache(), Path::new("/work/.buildl/cache.json"));
   /// assert_eq!(layout.blob_staging(), Path::new("/work/.buildl/cas/tmp"));
   /// # Ok::<(), buildl_core::Error>(())
   /// ```
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub struct StateLayout {
       dir: PathBuf,
   }

   impl StateLayout {
       /// The layout of the state directory `state` of the workspace at `root`.
       #[must_use]
       pub fn new(root: &Path, state: &Directory) -> Self {
           Self {
               dir: root.join(state.as_str()),
           }
       }

       /// The state directory itself.
       #[must_use]
       pub fn dir(&self) -> &Path {
           &self.dir
       }

       /// `lock`: the file a process locks while it uses the directory.
       #[must_use]
       pub fn lock(&self) -> PathBuf {
           self.dir.join("lock")
       }

       /// `cache.json`: the row of every target built so far.
       #[must_use]
       pub fn cache(&self) -> PathBuf {
           self.dir.join("cache.json")
       }

       /// `cache.journal`: the rows recorded since `cache.json` was last written.
       #[must_use]
       pub fn journal(&self) -> PathBuf {
           self.dir.join("cache.journal")
       }

       /// `statcache.json`: the digests of files that have not changed since they were hashed.
       #[must_use]
       pub fn stat_cache(&self) -> PathBuf {
           self.dir.join("statcache.json")
       }

       /// `log.jsonl`: one line for every action that ended.
       #[must_use]
       pub fn log(&self) -> PathBuf {
           self.dir.join("log.jsonl")
       }

       /// `graph.json`: the graph of the last build.
       #[must_use]
       pub fn graph(&self) -> PathBuf {
           self.dir.join("graph.json")
       }

       /// `cas`: the directory content is held in, by digest.
       #[must_use]
       pub fn blobs(&self) -> PathBuf {
           self.dir.join("cas")
       }

       /// `cas/tmp`: where a blob is written before it gets its name.
       #[must_use]
       pub fn blob_staging(&self) -> PathBuf {
           self.blobs().join("tmp")
       }
   }
   ````

6. Run `cargo fmt --all`, then run the tests and the documentation example and confirm they pass:

   ```
   $ cargo test -p buildl --lib state::layout
   test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   $ cargo test -p buildl --doc state::layout
   test crates/buildl/src/state/layout.rs - state::layout::StateLayout (line 19) ... ok
   test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   ```

7. Run the gate. Expected: all three exit `0`.

8. Commit:

   ```
   $ git add crates/buildl/src/lib.rs crates/buildl/src/state/mod.rs crates/buildl/src/state/layout.rs
   $ git commit -m "feat(buildl): add StateLayout, the paths of a state directory"
   ```

### Task 5 — Add `write_whole`

**Files:**
- Create `crates/buildl/src/state/write.rs`
- Modify `crates/buildl/src/state/mod.rs`

`write_whole` and `Access` are `pub`: no adapter exists yet to call them, and a crate-private function nothing calls fails the gate as dead code.

**Steps:**

1. Declare the file. Replace the whole of `crates/buildl/src/state/mod.rs` with:

   ```rust
   //! The state directory of a workspace: where its files are, who may use it, and how a file in
   //! it is written and removed.
   //!
   //! Its own module because several adapters keep a file in the state directory and no adapter
   //! may name another. What each of them would otherwise write out for itself lives here once.
   //! This module implements no port, and names no other module of this crate.
   //!
   //! Responsibilities:
   //! - [`StateLayout`] — the path of each file of a state directory.
   //! - [`write_whole`] and [`Access`] — a file written so that a reader finds it whole.
   //!
   //! Non-responsibilities: what any of those files holds. Each file's format belongs to the module
   //! that keeps it.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to
   //! unit-test.

   mod layout;
   mod write;

   pub use layout::StateLayout;
   pub use write::{Access, write_whole};
   ```

2. Write the failing tests. Create `crates/buildl/src/state/write.rs` with the test module only:

   ```rust
   //! Placeholder — replaced later in this task.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::fs::{self, OpenOptions};
       use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
       use std::path::Path;

       use super::{Access, write_whole};

       /// The permission bits of the file at `path`.
       fn mode(path: &Path) -> u32 {
           fs::metadata(path).unwrap().permissions().mode() & 0o777
       }

       /// The names in `dir`, sorted.
       fn names(dir: &Path) -> Vec<String> {
           let mut names: Vec<String> = fs::read_dir(dir)
               .unwrap()
               .map(|entry| entry.unwrap().file_name().into_string().unwrap())
               .collect();
           names.sort();
           names
       }

       #[test]
       fn write_whole_replaces_the_target_and_leaves_no_temp_file() {
           let dir = tempfile::tempdir().unwrap();
           let staging = dir.path().join("staging");
           let kept = dir.path().join("kept");
           fs::create_dir(&staging).unwrap();
           fs::create_dir(&kept).unwrap();
           let target = kept.join("blob");

           write_whole(&staging, &target, b"first", Access::ReadOnly).unwrap();
           assert_eq!(fs::read(&target).unwrap(), b"first");

           // The file being replaced is read-only: a rename needs no write access to it.
           write_whole(&staging, &target, b"second", Access::ReadOnly).unwrap();
           assert_eq!(fs::read(&target).unwrap(), b"second");
           assert_eq!(names(&staging), Vec::<String>::new());
           assert_eq!(names(&kept), ["blob"]);
       }

       #[test]
       fn write_whole_gives_the_file_its_access() {
           let dir = tempfile::tempdir().unwrap();
           // What the umask lets through, learned from a file made asking for everything.
           let probe = dir.path().join("probe");
           OpenOptions::new()
               .write(true)
               .create_new(true)
               .mode(0o777)
               .open(&probe)
               .unwrap();
           let allowed = mode(&probe);

           for (access, asked) in [
               (Access::Rewritable, 0o644),
               (Access::ReadOnly, 0o444),
               (Access::ReadOnlyRunnable, 0o555),
           ] {
               let target = dir.path().join(format!("{asked:o}"));
               write_whole(dir.path(), &target, b"bytes", access).unwrap();
               assert_eq!(mode(&target), asked & allowed, "asked for {asked:o}");
           }
       }

       #[test]
       fn write_whole_makes_a_missing_directory() {
           let dir = tempfile::tempdir().unwrap();
           let staging = dir.path().join("a").join("staging");
           let target = dir.path().join("b").join("c").join("file");

           write_whole(&staging, &target, b"bytes", Access::Rewritable).unwrap();

           assert_eq!(fs::read(&target).unwrap(), b"bytes");
           assert_eq!(names(&staging), Vec::<String>::new());
       }

       #[test]
       fn a_write_that_fails_leaves_the_old_file() {
           let dir = tempfile::tempdir().unwrap();
           let staging = dir.path().join("staging");

           // A directory with something in it cannot be renamed over.
           let occupied = dir.path().join("occupied");
           fs::create_dir(&occupied).unwrap();
           fs::write(occupied.join("old"), b"old").unwrap();
           assert!(write_whole(&staging, &occupied, b"new", Access::Rewritable).is_err());
           assert_eq!(fs::read(occupied.join("old")).unwrap(), b"old");
           assert_eq!(names(&staging), Vec::<String>::new());

           // Nor can a file be staged below a path that is a file.
           let target = dir.path().join("file");
           fs::write(&target, b"old").unwrap();
           let below_a_file = target.join("staging");
           assert!(write_whole(&below_a_file, &target, b"new", Access::Rewritable).is_err());
           assert_eq!(fs::read(&target).unwrap(), b"old");
           assert_eq!(names(dir.path()), ["file", "occupied", "staging"]);
       }
   }
   ```

3. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl --lib state::write
   error[E0432]: unresolved imports `super::Access`, `super::write_whole`
   error[E0432]: unresolved imports `write::Access`, `write::write_whole`
   ```

4. Replace the placeholder line at the top of `crates/buildl/src/state/write.rs` with the implementation, leaving the test module below it unchanged. The file above its test module reads:

   ```rust
   //! Writing a file whole: a reader finds the old bytes or the new ones, never part of either.
   //!
   //! Its own file because every file a build keeps is written this way, and the two steps that
   //! make it safe, a new file beside the old one and then a rename onto it, are easy to get half
   //! right when each module writes them out for itself.
   //!
   //! Responsibilities: [`write_whole`], and [`Access`], the mode a written file is given.
   //!
   //! Non-responsibilities: surviving the machine losing power. Nothing is synced: a rename covers
   //! a process that dies, which is what a build promises, and a sync costs many times the write.

   use std::fs::{self, Permissions};
   use std::io::{self, ErrorKind, Write};
   use std::os::unix::fs::PermissionsExt;
   use std::path::Path;

   use tempfile::{Builder, NamedTempFile};

   /// The start of the name of a file that is still being written.
   pub(super) const STAGED_PREFIX: &str = ".tmp-";

   /// What may be done with a file once it is written.
   ///
   /// Each is the mode asked for when the file is made. What the file gets is that mode less the
   /// process's umask.
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   pub enum Access {
       /// Read by anyone, written by its owner: `0o644`.
       Rewritable,
       /// Read by anyone, written by no one: `0o444`.
       ReadOnly,
       /// Read and run by anyone, written by no one: `0o555`.
       ReadOnlyRunnable,
   }

   impl Access {
       /// The mode a file with this access is made with.
       const fn mode(self) -> u32 {
           match self {
               Self::Rewritable => 0o644,
               Self::ReadOnly => 0o444,
               Self::ReadOnlyRunnable => 0o555,
           }
       }
   }

   /// Makes `bytes` the whole of `target`: written to a new file in `staging`, then renamed onto it.
   ///
   /// A `staging` directory that does not exist is made, and so is a missing parent of `target`.
   /// When the write fails, the new file is removed and `target` is left as it was.
   ///
   /// # Errors
   ///
   /// Returns the I/O error of the step that failed. `staging` and `target` must be on one
   /// filesystem: a rename across two is refused by the system, and that refusal is what is
   /// returned.
   pub fn write_whole(staging: &Path, target: &Path, bytes: &[u8], access: Access) -> io::Result<()> {
       let mut file = staged(staging, access)?;
       file.write_all(bytes)?;
       let Err(refused) = file.persist(target) else {
           return Ok(());
       };
       let missing_parent = refused.error.kind() == ErrorKind::NotFound;
       match target.parent() {
           Some(parent) if missing_parent => {
               fs::create_dir_all(parent)?;
               refused.file.persist(target)?;
               Ok(())
           }
           _ => Err(refused.error),
       }
   }

   /// A new, empty file in `staging` with the mode of `access`, making `staging` when it is missing.
   fn staged(staging: &Path, access: Access) -> io::Result<NamedTempFile> {
       let mut builder = Builder::new();
       builder
           .prefix(STAGED_PREFIX)
           .permissions(Permissions::from_mode(access.mode()));
       match builder.tempfile_in(staging) {
           Err(error) if error.kind() == ErrorKind::NotFound => {
               fs::create_dir_all(staging)?;
               builder.tempfile_in(staging)
           }
           made => made,
       }
   }
   ```

5. Run `cargo fmt --all`, then run the tests and confirm they pass:

   ```
   $ cargo test -p buildl --lib state::write
   test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 3 filtered out
   ```

6. Run the gate. Expected: all three exit `0`.

7. Commit:

   ```
   $ git add crates/buildl/src/state/mod.rs crates/buildl/src/state/write.rs
   $ git commit -m "feat(buildl): add write_whole, a file written whole"
   ```

### Task 6 — Add `StateLock`

**Files:**
- Create `crates/buildl/src/state/lock.rs`
- Modify `crates/buildl/src/state/mod.rs`

**Steps:**

1. Declare the file. Replace the whole of `crates/buildl/src/state/mod.rs` with:

   ```rust
   //! The state directory of a workspace: where its files are, who may use it, and how a file in
   //! it is written and removed.
   //!
   //! Its own module because several adapters keep a file in the state directory and no adapter
   //! may name another. What each of them would otherwise write out for itself lives here once.
   //! This module implements no port, and names no other module of this crate.
   //!
   //! Responsibilities:
   //! - [`StateLayout`] — the path of each file of a state directory.
   //! - [`StateLock`] and [`LockError`] — one process using a state directory at a time.
   //! - [`write_whole`] and [`Access`] — a file written so that a reader finds it whole.
   //!
   //! Non-responsibilities: what any of those files holds. Each file's format belongs to the module
   //! that keeps it.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to
   //! unit-test.

   mod layout;
   mod lock;
   mod write;

   pub use layout::StateLayout;
   pub use lock::{LockError, StateLock};
   pub use write::{Access, write_whole};
   ```

2. Write the failing tests. Create `crates/buildl/src/state/lock.rs` with the test module only:

   ```rust
   //! Placeholder — replaced later in this task.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::fs;
       use std::io;
       use std::path::PathBuf;
       use std::process;
       use std::time::UNIX_EPOCH;

       use buildl_core::{Directory, Timestamp};

       use super::{LockError, StateLock};
       use crate::state::StateLayout;

       /// A temporary workspace root, and the layout of its state directory.
       fn workspace() -> (tempfile::TempDir, StateLayout) {
           let dir = tempfile::tempdir().unwrap();
           let layout = StateLayout::new(dir.path(), &Directory::parse(".buildl").unwrap());
           (dir, layout)
       }

       #[test]
       fn a_second_lock_is_refused_and_a_dropped_one_is_free() {
           let (_dir, layout) = workspace();

           let held = StateLock::acquire(layout.clone()).unwrap();
           assert_eq!(held.layout(), &layout);
           let refused = StateLock::acquire(layout.clone()).unwrap_err();
           assert!(
               matches!(&refused, LockError::Busy { path } if *path == layout.lock()),
               "{refused:?}"
           );

           drop(held);
           StateLock::acquire(layout).unwrap();
       }

       #[test]
       fn taking_the_lock_makes_the_directory_and_names_the_holder() {
           let (_dir, layout) = workspace();
           assert!(!layout.dir().exists());

           let held = StateLock::acquire(layout.clone()).unwrap();
           assert_eq!(
               fs::read_to_string(layout.lock()).unwrap(),
               format!("{}\n", process::id())
           );

           // The file is kept and rewritten, never grown, by the holder after it.
           drop(held);
           fs::write(layout.lock(), b"4294967295 and more\n").unwrap();
           let _held = StateLock::acquire(layout.clone()).unwrap();
           assert_eq!(
               fs::read_to_string(layout.lock()).unwrap(),
               format!("{}\n", process::id())
           );
       }

       #[test]
       fn the_lock_stamp_is_not_before_a_file_written_earlier() {
           let (dir, layout) = workspace();
           let earlier = dir.path().join("earlier");
           fs::write(&earlier, b"bytes").unwrap();
           let written = fs::metadata(&earlier)
               .unwrap()
               .modified()
               .unwrap()
               .duration_since(UNIX_EPOCH)
               .unwrap();
           let written = Timestamp::from_unix_nanos(u64::try_from(written.as_nanos()).unwrap());

           let held = StateLock::acquire(layout).unwrap();

           assert!(held.acquired() >= written);
       }

       #[test]
       fn a_directory_that_cannot_be_made_is_an_io_error_naming_it() {
           let dir = tempfile::tempdir().unwrap();
           fs::write(dir.path().join("file"), b"bytes").unwrap();
           // The state directory would lie below a file.
           let layout = StateLayout::new(
               &dir.path().join("file"),
               &Directory::parse("state").unwrap(),
           );

           let refused = StateLock::acquire(layout.clone()).unwrap_err();
           assert!(
               matches!(&refused, LockError::Io { path, .. } if path == layout.dir()),
               "{refused:?}"
           );
       }

       #[test]
       fn each_error_names_its_path() {
           let busy = LockError::Busy {
               path: PathBuf::from("/work/.buildl/lock"),
           };
           assert_eq!(
               busy.to_string(),
               "/work/.buildl/lock is in use by another buildl"
           );

           let io = LockError::Io {
               path: PathBuf::from("/work/.buildl"),
               source: io::Error::other("refused"),
           };
           assert_eq!(io.to_string(), "/work/.buildl: refused");
       }
   }
   ```

3. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl --lib state::lock
   error[E0432]: unresolved imports `super::LockError`, `super::StateLock`
   error[E0432]: unresolved imports `lock::LockError`, `lock::StateLock`
   ```

4. Replace the placeholder line at the top of `crates/buildl/src/state/lock.rs` with the implementation, leaving the test module below it unchanged. The file above its test module reads:

   ```rust
   //! The lock one process holds while it uses a state directory.
   //!
   //! Its own file because two builds in one workspace write the same files, and everything else
   //! that keeps a file in the state directory is written as if it were alone there. Holding a
   //! [`StateLock`] is what makes that true.
   //!
   //! Responsibilities: [`StateLock`], which takes the lock and stamps when it was taken; and
   //! [`LockError`].
   //!
   //! Non-responsibilities: waiting for a lock. A directory in use is refused at once, and the
   //! caller decides what to tell the person who asked.

   use std::fs::{self, File, OpenOptions, TryLockError};
   use std::io::{self, Write};
   use std::path::{Path, PathBuf};
   use std::process;
   use std::time::UNIX_EPOCH;

   use buildl_core::Timestamp;

   use crate::state::StateLayout;

   /// Proof that this process alone uses a state directory. Dropping it frees the directory.
   ///
   /// The lock is one the system holds for the open file, so a process that dies frees it too: no
   /// stale lock is ever left behind, and the lock file itself is never removed.
   #[derive(Debug)]
   pub struct StateLock {
       file: File,
       layout: StateLayout,
       acquired: Timestamp,
   }

   impl StateLock {
       /// Takes the lock of the state directory at `layout`, making the directory when it is
       /// missing.
       ///
       /// # Errors
       ///
       /// Returns [`LockError::Busy`] when another process holds the lock, and [`LockError::Io`]
       /// when the system refuses an operation on the directory or on something in it.
       pub fn acquire(layout: StateLayout) -> Result<Self, LockError> {
           fs::create_dir_all(layout.dir()).map_err(|source| failed(layout.dir(), source))?;
           let path = layout.lock();
           let mut file = OpenOptions::new()
               .write(true)
               .create(true)
               .truncate(false)
               .open(&path)
               .map_err(|source| failed(&path, source))?;
           match file.try_lock() {
               Ok(()) => {}
               Err(TryLockError::WouldBlock) => return Err(LockError::Busy { path }),
               Err(TryLockError::Error(source)) => return Err(failed(&path, source)),
           }
           let acquired = stamp(&mut file).map_err(|source| failed(&path, source))?;
           Ok(Self {
               file,
               layout,
               acquired,
           })
       }

       /// The layout of the directory this lock holds.
       #[must_use]
       pub const fn layout(&self) -> &StateLayout {
           &self.layout
       }

       /// The filesystem's own time when the lock was taken.
       ///
       /// It is the modification time of the lock file, so it can be compared with the modification
       /// time of any other file on the same filesystem without asking a clock.
       #[must_use]
       pub const fn acquired(&self) -> Timestamp {
           self.acquired
       }
   }

   impl Drop for StateLock {
       fn drop(&mut self) {
           // Closing the file frees the lock as well. Asking for it says so, and frees the directory
           // even when a copy of the descriptor is still open somewhere.
           let _ = self.file.unlock();
       }
   }

   /// Why a state directory's lock was not taken.
   #[derive(Debug, thiserror::Error)]
   #[non_exhaustive]
   pub enum LockError {
       /// Another process holds the lock.
       #[error("{} is in use by another buildl", .path.display())]
       Busy {
           /// The lock file.
           path: PathBuf,
       },
       /// The system refused an operation on the directory or on something in it.
       #[error("{}: {source}", .path.display())]
       Io {
           /// What the operation was on.
           path: PathBuf,
           /// What the system answered.
           #[source]
           source: io::Error,
       },
   }

   /// The error of an operation on `path`.
   fn failed(path: &Path, source: io::Error) -> LockError {
       LockError::Io {
           path: path.to_path_buf(),
           source,
       }
   }

   /// Makes the lock file name this process, and returns the modification time that write gave it.
   fn stamp(file: &mut File) -> io::Result<Timestamp> {
       file.set_len(0)?;
       file.write_all(format!("{}\n", process::id()).as_bytes())?;
       let since = file
           .metadata()?
           .modified()?
           .duration_since(UNIX_EPOCH)
           .unwrap_or_default();
       Ok(Timestamp::from_unix_nanos(
           u64::try_from(since.as_nanos()).unwrap_or(u64::MAX),
       ))
   }
   ```

   The lock is `std`'s `File::try_lock`, which is `flock` on Unix: it belongs to the open file, so a second `acquire` in the same process is refused like one from another process, which is what the first test relies on.

5. Run `cargo fmt --all`, then run the tests and confirm they pass:

   ```
   $ cargo test -p buildl --lib state::lock
   test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 7 filtered out
   ```

6. Run the gate. Expected: all three exit `0`.

7. Commit:

   ```
   $ git add crates/buildl/src/state/mod.rs crates/buildl/src/state/lock.rs
   $ git commit -m "feat(buildl): add StateLock, one process per state directory"
   ```

### Task 7 — Sweep half-written files when the lock is taken

**Files:**
- Create `crates/buildl/src/state/clear.rs`
- Modify `crates/buildl/src/state/lock.rs`
- Modify `crates/buildl/src/state/mod.rs`

`clear_path` and the sweep land together: the sweep is `clear_path`'s first caller, and without a caller a crate-private function fails the gate.

**Steps:**

1. Write the failing test of the sweep. In `crates/buildl/src/state/lock.rs`, add this test at the end of the test module, after `each_error_names_its_path` and before the module's closing brace:

   ```rust
       #[test]
       fn taking_the_lock_sweeps_staging() {
           let (_dir, layout) = workspace();
           let shard = layout.blobs().join("ab");
           fs::create_dir_all(layout.blob_staging()).unwrap();
           fs::create_dir_all(&shard).unwrap();
           fs::write(layout.dir().join(".tmp-a1B2c3"), b"half").unwrap();
           fs::create_dir(layout.dir().join(".tmp-dir")).unwrap();
           fs::write(layout.blob_staging().join(".tmp-x9Y8z7"), b"half").unwrap();
           fs::write(layout.blob_staging().join("other"), b"half").unwrap();
           fs::write(layout.cache(), b"{}").unwrap();
           fs::write(layout.dir().join(".tmpfile"), b"kept").unwrap();
           fs::write(shard.join("cdef"), b"blob").unwrap();

           let _held = StateLock::acquire(layout.clone()).unwrap();

           // Gone: what was being written, in the directory itself and in the blob staging.
           assert!(!layout.dir().join(".tmp-a1B2c3").exists());
           assert!(!layout.dir().join(".tmp-dir").exists());
           assert_eq!(fs::read_dir(layout.blob_staging()).unwrap().count(), 0);
           // Kept: every other file, a name that only resembles the prefix, and the blobs.
           assert_eq!(fs::read(layout.cache()).unwrap(), b"{}");
           assert_eq!(fs::read(layout.dir().join(".tmpfile")).unwrap(), b"kept");
           assert_eq!(fs::read(shard.join("cdef")).unwrap(), b"blob");
       }
   ```

2. Run it and confirm it fails, because nothing is swept yet:

   ```
   $ cargo test -p buildl --lib state::lock
   test state::lock::tests::taking_the_lock_sweeps_staging ... FAILED
   assertion failed: !layout.dir().join(".tmp-a1B2c3").exists()
   test result: FAILED. 5 passed; 1 failed; 0 ignored; 0 measured; 7 filtered out
   ```

3. Declare the file that will hold `clear_path`. Replace the whole of `crates/buildl/src/state/mod.rs` with:

   ```rust
   //! The state directory of a workspace: where its files are, who may use it, and how a file in
   //! it is written and removed.
   //!
   //! Its own module because several adapters keep a file in the state directory and no adapter
   //! may name another. What each of them would otherwise write out for itself lives here once.
   //! This module implements no port, and names no other module of this crate.
   //!
   //! Responsibilities:
   //! - [`StateLayout`] — the path of each file of a state directory.
   //! - [`StateLock`] and [`LockError`] — one process using a state directory at a time.
   //! - [`write_whole`] and [`Access`] — a file written so that a reader finds it whole.
   //! - `clear_path` — removing whatever stands at a path.
   //!
   //! Non-responsibilities: what any of those files holds. Each file's format belongs to the module
   //! that keeps it.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to
   //! unit-test.

   mod clear;
   mod layout;
   mod lock;
   mod write;

   pub(crate) use clear::clear_path;
   pub use layout::StateLayout;
   pub use lock::{LockError, StateLock};
   pub use write::{Access, write_whole};
   ```

4. Write the failing tests of `clear_path`. Create `crates/buildl/src/state/clear.rs` with the test module only:

   ```rust
   //! Placeholder — replaced later in this task.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::fs::{self, Permissions};
       use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
       use std::path::Path;

       use super::clear_path;

       /// A file at `path` holding `bytes`, which nobody may write to.
       fn read_only(path: &Path, bytes: &[u8]) {
           fs::write(path, bytes).unwrap();
           fs::set_permissions(path, Permissions::from_mode(0o444)).unwrap();
       }

       #[test]
       fn clear_path_removes_nothing_a_file_a_read_only_tree_and_a_link_alone() {
           let dir = tempfile::tempdir().unwrap();
           let root = dir.path();

           // Nothing there, and nothing below a path that is a file.
           clear_path(&root.join("absent")).unwrap();
           read_only(&root.join("file"), b"bytes");
           clear_path(&root.join("file").join("below")).unwrap();
           assert!(root.join("file").exists());

           // A file nobody may write to.
           clear_path(&root.join("file")).unwrap();
           assert!(!root.join("file").exists());

           // A directory of such files.
           let tree = root.join("tree");
           fs::create_dir_all(tree.join("inner")).unwrap();
           read_only(&tree.join("a"), b"a");
           read_only(&tree.join("inner").join("b"), b"b");
           clear_path(&tree).unwrap();
           assert!(!tree.exists());

           // A link to a directory: the link goes, what it points at stays.
           let kept = root.join("kept");
           fs::create_dir(&kept).unwrap();
           fs::write(kept.join("inside"), b"inside").unwrap();
           symlink(&kept, root.join("link")).unwrap();
           clear_path(&root.join("link")).unwrap();
           assert!(fs::symlink_metadata(root.join("link")).is_err());
           assert_eq!(fs::read(kept.join("inside")).unwrap(), b"inside");

           // A link to nothing.
           symlink(root.join("nowhere"), root.join("dangling")).unwrap();
           clear_path(&root.join("dangling")).unwrap();
           assert!(fs::symlink_metadata(root.join("dangling")).is_err());
       }

       #[test]
       fn clear_path_unlinks_and_never_writes_through_a_hard_link() {
           let dir = tempfile::tempdir().unwrap();
           let blob = dir.path().join("blob");
           let placed = dir.path().join("placed");
           read_only(&blob, b"bytes");
           fs::hard_link(&blob, &placed).unwrap();
           assert_eq!(fs::metadata(&blob).unwrap().nlink(), 2);

           clear_path(&placed).unwrap();

           assert!(!placed.exists());
           assert_eq!(fs::read(&blob).unwrap(), b"bytes");
           let left = fs::metadata(&blob).unwrap();
           assert_eq!(left.nlink(), 1);
           assert_eq!(left.permissions().mode() & 0o777, 0o444);
       }
   }
   ```

5. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl --lib state::clear
   error[E0432]: unresolved import `super::clear_path`
   error[E0432]: unresolved import `clear::clear_path`
   ```

6. Replace the placeholder line at the top of `crates/buildl/src/state/clear.rs` with the implementation, leaving the test module below it unchanged. The file above its test module reads:

   ```rust
   //! Removing whatever stands at a path.
   //!
   //! Its own file because removing is asked for by path, and the right call depends on what is
   //! found there: `remove_file` fails on a directory and `remove_dir_all` fails on a file, each
   //! with a different error on each system. One function looks first, so no caller guesses.
   //!
   //! Responsibilities: [`clear_path`].
   //!
   //! Non-responsibilities: deciding what may be removed. A caller hands over a path it owns.

   use std::fs;
   use std::io::{self, ErrorKind};
   use std::path::Path;

   /// Removes whatever is at `path`, following no symbolic link. Nothing being there is not an
   /// error.
   ///
   /// A directory is removed with everything below it. A symbolic link is removed and what it
   /// points at is left alone, a link to a directory included. A file is unlinked and never opened,
   /// so the bytes stay whole for any other name they have.
   ///
   /// # Errors
   ///
   /// Returns the I/O error of looking at `path` or of removing what is there.
   pub(crate) fn clear_path(path: &Path) -> io::Result<()> {
       let found = match fs::symlink_metadata(path) {
           Ok(found) => found,
           // A parent that is a file holds nothing either.
           Err(error) if matches!(error.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) => {
               return Ok(());
           }
           Err(error) => return Err(error),
       };
       if found.is_dir() {
           fs::remove_dir_all(path)
       } else {
           fs::remove_file(path)
       }
   }
   ```

7. Run the module's tests. `clear_path`'s two pass and the sweep's still fails; the compiler warns that the re-export is unused, which the next step ends:

   ```
   $ cargo test -p buildl --lib state::
   warning: unused import: `clear::clear_path`
   test state::clear::tests::clear_path_removes_nothing_a_file_a_read_only_tree_and_a_link_alone ... ok
   test state::clear::tests::clear_path_unlinks_and_never_writes_through_a_hard_link ... ok
   test state::lock::tests::taking_the_lock_sweeps_staging ... FAILED
   test result: FAILED. 14 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
   ```

8. Sweep when the lock is taken. In `crates/buildl/src/state/lock.rs`, replace everything above the line `#[cfg(test)]` with the text below, leaving the test module unchanged. Against what task 6 wrote it changes the module doc's Responsibilities sentence, three `use` lines (`std::ffi::OsStr`, `ErrorKind`, and the two names from `crate::state`), the doc comment and the body of `acquire` (one paragraph, two `sweep` calls), and adds the functions `is_staged` and `sweep` at the end.

   ```rust
   //! The lock one process holds while it uses a state directory.
   //!
   //! Its own file because two builds in one workspace write the same files, and everything else
   //! that keeps a file in the state directory is written as if it were alone there. Holding a
   //! [`StateLock`] is what makes that true.
   //!
   //! Responsibilities: [`StateLock`], which takes the lock, stamps when it was taken, and sweeps
   //! what an interrupted run left half-written; and [`LockError`].
   //!
   //! Non-responsibilities: waiting for a lock. A directory in use is refused at once, and the
   //! caller decides what to tell the person who asked.

   use std::ffi::OsStr;
   use std::fs::{self, File, OpenOptions, TryLockError};
   use std::io::{self, ErrorKind, Write};
   use std::path::{Path, PathBuf};
   use std::process;
   use std::time::UNIX_EPOCH;

   use buildl_core::Timestamp;

   use crate::state::write::STAGED_PREFIX;
   use crate::state::{StateLayout, clear_path};

   /// Proof that this process alone uses a state directory. Dropping it frees the directory.
   ///
   /// The lock is one the system holds for the open file, so a process that dies frees it too: no
   /// stale lock is ever left behind, and the lock file itself is never removed.
   #[derive(Debug)]
   pub struct StateLock {
       file: File,
       layout: StateLayout,
       acquired: Timestamp,
   }

   impl StateLock {
       /// Takes the lock of the state directory at `layout`, making the directory when it is
       /// missing.
       ///
       /// Once the lock is held, whatever an interrupted run left half-written is removed: every
       /// entry of the state directory whose name starts with `.tmp-`, and every entry of the blob
       /// staging directory.
       ///
       /// # Errors
       ///
       /// Returns [`LockError::Busy`] when another process holds the lock, and [`LockError::Io`]
       /// when the system refuses an operation on the directory or on something in it.
       pub fn acquire(layout: StateLayout) -> Result<Self, LockError> {
           fs::create_dir_all(layout.dir()).map_err(|source| failed(layout.dir(), source))?;
           let path = layout.lock();
           let mut file = OpenOptions::new()
               .write(true)
               .create(true)
               .truncate(false)
               .open(&path)
               .map_err(|source| failed(&path, source))?;
           match file.try_lock() {
               Ok(()) => {}
               Err(TryLockError::WouldBlock) => return Err(LockError::Busy { path }),
               Err(TryLockError::Error(source)) => return Err(failed(&path, source)),
           }
           let acquired = stamp(&mut file).map_err(|source| failed(&path, source))?;
           sweep(layout.dir(), is_staged)?;
           sweep(&layout.blob_staging(), |_| true)?;
           Ok(Self {
               file,
               layout,
               acquired,
           })
       }

       /// The layout of the directory this lock holds.
       #[must_use]
       pub const fn layout(&self) -> &StateLayout {
           &self.layout
       }

       /// The filesystem's own time when the lock was taken.
       ///
       /// It is the modification time of the lock file, so it can be compared with the modification
       /// time of any other file on the same filesystem without asking a clock.
       #[must_use]
       pub const fn acquired(&self) -> Timestamp {
           self.acquired
       }
   }

   impl Drop for StateLock {
       fn drop(&mut self) {
           // Closing the file frees the lock as well. Asking for it says so, and frees the directory
           // even when a copy of the descriptor is still open somewhere.
           let _ = self.file.unlock();
       }
   }

   /// Why a state directory's lock was not taken.
   #[derive(Debug, thiserror::Error)]
   #[non_exhaustive]
   pub enum LockError {
       /// Another process holds the lock.
       #[error("{} is in use by another buildl", .path.display())]
       Busy {
           /// The lock file.
           path: PathBuf,
       },
       /// The system refused an operation on the directory or on something in it.
       #[error("{}: {source}", .path.display())]
       Io {
           /// What the operation was on.
           path: PathBuf,
           /// What the system answered.
           #[source]
           source: io::Error,
       },
   }

   /// The error of an operation on `path`.
   fn failed(path: &Path, source: io::Error) -> LockError {
       LockError::Io {
           path: path.to_path_buf(),
           source,
       }
   }

   /// Makes the lock file name this process, and returns the modification time that write gave it.
   fn stamp(file: &mut File) -> io::Result<Timestamp> {
       file.set_len(0)?;
       file.write_all(format!("{}\n", process::id()).as_bytes())?;
       let since = file
           .metadata()?
           .modified()?
           .duration_since(UNIX_EPOCH)
           .unwrap_or_default();
       Ok(Timestamp::from_unix_nanos(
           u64::try_from(since.as_nanos()).unwrap_or(u64::MAX),
       ))
   }

   /// Whether `name` is that of a file that was still being written.
   fn is_staged(name: &OsStr) -> bool {
       name.as_encoded_bytes()
           .starts_with(STAGED_PREFIX.as_bytes())
   }

   /// Removes every entry of `dir` whose name `swept` accepts. A `dir` that does not exist holds
   /// nothing to remove.
   fn sweep(dir: &Path, swept: impl Fn(&OsStr) -> bool) -> Result<(), LockError> {
       let entries = match fs::read_dir(dir) {
           Ok(entries) => entries,
           Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
           Err(source) => return Err(failed(dir, source)),
       };
       for entry in entries {
           let entry = entry.map_err(|source| failed(dir, source))?;
           if swept(&entry.file_name()) {
               let path = entry.path();
               clear_path(&path).map_err(|source| failed(&path, source))?;
           }
       }
       Ok(())
   }
   ```

9. Run `cargo fmt --all`, then run the module's tests and confirm they pass:

   ```
   $ cargo test -p buildl --lib state::
   test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   ```

10. Run the gate. Expected: all three exit `0`.

11. Commit:

    ```
    $ git add crates/buildl/src/state/mod.rs crates/buildl/src/state/clear.rs crates/buildl/src/state/lock.rs
    $ git commit -m "feat(buildl): sweep half-written files when the lock is taken"
    ```

### Task 8 — Add `SearchPath`

**Files:**
- Create `crates/buildl/src/search_path.rs`
- Modify `crates/buildl/src/lib.rs`

**Steps:**

1. Declare the module. Two replacements in `crates/buildl/src/lib.rs`.

   Replace

   ```rust
   //! | [`state`] | where each file of the state directory is, the lock on it, and writing a file whole |
   ```

   with

   ```rust
   //! | [`state`] | where each file of the state directory is, the lock on it, and writing a file whole |
   //! | [`search_path`] | the rule that leads from a program's name to a file |
   ```

   Replace

   ```rust
   pub mod report;
   pub mod statcache;
   ```

   with

   ```rust
   pub mod report;
   pub mod search_path;
   pub mod statcache;
   ```

2. Write the failing tests. Create `crates/buildl/src/search_path.rs` with the test module only. The module is one file, so its tests name the type through the crate, not through `super`:

   ```rust
   //! Placeholder — replaced later in this task.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::fs::{self, Permissions};
       use std::os::unix::fs::{PermissionsExt, symlink};
       use std::path::{Path, PathBuf};

       use buildl_core::Program;

       use crate::search_path::SearchPath;

       /// A file at `path` with permission bits `mode`.
       fn file(path: &Path, mode: u32) {
           fs::create_dir_all(path.parent().unwrap()).unwrap();
           fs::write(path, b"#!/bin/sh\n").unwrap();
           fs::set_permissions(path, Permissions::from_mode(mode)).unwrap();
       }

       fn program(name: &str) -> Program {
           Program::parse(name).unwrap()
       }

       /// A canonical temporary directory.
       fn root() -> (tempfile::TempDir, PathBuf) {
           let dir = tempfile::tempdir().unwrap();
           let root = fs::canonicalize(dir.path()).unwrap();
           (dir, root)
       }

       #[test]
       fn the_first_executable_file_in_order_is_found() {
           let (_dir, root) = root();
           let (first, second) = (root.join("first"), root.join("second"));
           file(&second.join("only-second"), 0o755);
           file(&first.join("both"), 0o755);
           file(&second.join("both"), 0o755);
           let search = SearchPath::new([root.join("no-such-dir"), first.clone(), second.clone()]);

           assert_eq!(
               search.locate(&program("both"), &root),
               Some(first.join("both"))
           );
           assert_eq!(
               search.locate(&program("only-second"), &root),
               Some(second.join("only-second"))
           );
           assert_eq!(search.locate(&program("neither"), &root), None);
           assert_eq!(SearchPath::default().locate(&program("both"), &root), None);
       }

       #[test]
       fn a_directory_a_file_without_the_bit_and_a_dangling_link_are_passed_over() {
           let (_dir, root) = root();
           let (first, second) = (root.join("first"), root.join("second"));
           fs::create_dir_all(first.join("adir")).unwrap();
           file(&first.join("noexec"), 0o644);
           symlink(first.join("nowhere"), first.join("dangling")).unwrap();
           file(&second.join("real-tool"), 0o755);
           symlink(second.join("real-tool"), first.join("linked")).unwrap();
           for name in ["adir", "noexec", "dangling"] {
               file(&second.join(name), 0o755);
           }
           let search = SearchPath::new([first.clone(), second.clone()]);

           for name in ["adir", "noexec", "dangling"] {
               assert_eq!(
                   search.locate(&program(name), &root),
                   Some(second.join(name)),
                   "{name}"
               );
           }
           // A link to an executable file is taken, and returned as the link.
           assert_eq!(
               search.locate(&program("linked"), &root),
               Some(first.join("linked"))
           );
           // With nothing behind them, the three are not found at all.
           let alone = SearchPath::new([first]);
           for name in ["adir", "noexec", "dangling"] {
               assert_eq!(alone.locate(&program(name), &root), None, "{name}");
           }
       }

       #[test]
       fn a_name_with_a_slash_is_not_searched() {
           let (_dir, root) = root();
           let bin = root.join("bin");
           file(&bin.join("gen.sh"), 0o755);
           file(&root.join("scripts").join("gen.sh"), 0o755);
           file(&root.join("scripts").join("plain.txt"), 0o644);
           let search = SearchPath::new([bin.clone()]);

           // Relative: joined to the base, whatever the search path holds.
           assert_eq!(
               search.locate(&program("scripts/gen.sh"), &root),
               Some(root.join("scripts").join("gen.sh"))
           );
           assert_eq!(search.locate(&program("./gen.sh"), &root), None);
           assert_eq!(search.locate(&program("scripts/plain.txt"), &root), None);
           assert_eq!(search.locate(&program("scripts/absent"), &root), None);

           // Absolute: itself, whatever the base is.
           let absolute = bin.join("gen.sh");
           assert_eq!(
               search.locate(&program(absolute.to_str().unwrap()), Path::new("/")),
               Some(absolute)
           );
           let missing = root.join("scripts").join("absent");
           assert_eq!(
               search.locate(&program(missing.to_str().unwrap()), &root),
               None
           );
       }

       #[test]
       fn a_relative_directory_is_left_out() {
           let (_dir, root) = root();
           file(&root.join("bin").join("tool"), 0o755);

           let search = SearchPath::new([
               PathBuf::new(),
               PathBuf::from("bin"),
               PathBuf::from("./bin"),
               root.join("bin"),
           ]);

           assert_eq!(search, SearchPath::new([root.join("bin")]));
           assert_eq!(
               SearchPath::new([PathBuf::new(), PathBuf::from("bin")]),
               SearchPath::default()
           );
           assert_eq!(
               search.locate(&program("tool"), &root),
               Some(root.join("bin").join("tool"))
           );
       }

       #[test]
       fn the_search_path_of_this_process_finds_a_system_tool() {
           let found = SearchPath::from_process()
               .locate(&program("sh"), Path::new("/"))
               .unwrap();

           assert!(found.is_absolute());
           assert_eq!(found.file_name().unwrap(), "sh");
       }
   }
   ```

3. Run them and confirm they fail to build:

   ```
   $ cargo test -p buildl --lib search_path
   error[E0432]: unresolved import `crate::search_path::SearchPath`
   ```

4. Replace the placeholder line at the top of `crates/buildl/src/search_path.rs` with the implementation, leaving the test module below it unchanged. The file above its test module reads:

   ````rust
   //! The rule that leads from a program's name to the file it runs.
   //!
   //! Its own module because two adapters need the rule and neither may name the other: the one
   //! that fingerprints a tool and the one that runs it. Both are handed one [`SearchPath`] value,
   //! so the file that is run is the file that was fingerprinted. This module implements no port,
   //! and names no other module of this crate.
   //!
   //! Responsibilities: [`SearchPath`].
   //!
   //! Non-responsibilities: reading or running the file a name leads to, and deciding which
   //! programs a workspace allows.

   use std::env;
   use std::fs;
   use std::os::unix::fs::PermissionsExt;
   use std::path::{Path, PathBuf};

   use buildl_core::Program;

   /// The directories a bare program name is looked for in, in order.
   ///
   /// # Examples
   ///
   /// ```
   /// use std::path::{Path, PathBuf};
   ///
   /// use buildl::search_path::SearchPath;
   /// use buildl_core::Program;
   ///
   /// let search = SearchPath::new([PathBuf::from("/no/such/directory")]);
   /// assert_eq!(search.locate(&Program::parse("cc")?, Path::new("/work")), None);
   /// # Ok::<(), buildl_core::Error>(())
   /// ```
   #[derive(Debug, Clone, PartialEq, Eq, Default)]
   pub struct SearchPath(Vec<PathBuf>);

   impl SearchPath {
       /// The `PATH` of this process, read once.
       ///
       /// A `PATH` that is unset or empty gives a search path that finds no bare name.
       #[must_use]
       pub fn from_process() -> Self {
           env::var_os("PATH").map_or_else(Self::default, |path| Self::new(env::split_paths(&path)))
       }

       /// These directories, in order. One that is not absolute is left out.
       #[must_use]
       pub fn new(directories: impl IntoIterator<Item = PathBuf>) -> Self {
           Self(
               directories
                   .into_iter()
                   .filter(|directory| directory.is_absolute())
                   .collect(),
           )
       }

       /// The file `program` names, for a command run in `base`.
       ///
       /// A name that holds a `/` is a path and is not searched for: an absolute one is itself, and
       /// a relative one is joined to `base`. A bare name is looked for in each directory in order.
       /// What is found is a regular file with an execute bit, links followed; a directory of that
       /// name, a file with no execute bit and a link to nothing are passed over.
       ///
       /// The path returned is the one that was tried, not its canonical form, so a program that
       /// behaves by the name it was called with still sees that name.
       #[must_use]
       pub fn locate(&self, program: &Program, base: &Path) -> Option<PathBuf> {
           let name = program.as_str();
           if name.contains('/') {
               // `join` returns an absolute name unchanged.
               return Some(base.join(name)).filter(|path| runnable(path));
           }
           self.0
               .iter()
               .map(|directory| directory.join(name))
               .find(|path| runnable(path))
       }
   }

   /// Whether `path` leads, through any links, to a regular file someone may execute.
   ///
   /// The test reads the mode bits. A file this process may not run is still found, and fails when
   /// it is started.
   fn runnable(path: &Path) -> bool {
       fs::metadata(path).is_ok_and(|found| found.is_file() && found.permissions().mode() & 0o111 != 0)
   }
   ````

5. Run `cargo fmt --all`, then run the tests and the documentation examples and confirm they pass:

   ```
   $ cargo test -p buildl --lib search_path
   test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 15 filtered out
   $ cargo test -p buildl --doc
   test crates/buildl/src/search_path.rs - search_path::SearchPath (line 24) ... ok
   test crates/buildl/src/state/layout.rs - state::layout::StateLayout (line 19) ... ok
   test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   ```

6. Confirm the guard still passes with both shared modules in place, each naming only itself:

   ```
   $ cargo make guard-adapter-isolation
   guard-adapter-isolation: OK (0 edges)
   ```

7. Run the gate. Expected: all three exit `0`, and `cargo make dod` reports `20 passed` for the unit tests of `buildl` and `2 passed` for its documentation examples.

8. Commit:

   ```
   $ git add crates/buildl/src/lib.rs crates/buildl/src/search_path.rs
   $ git commit -m "feat(buildl): add SearchPath, the lookup of a program by name"
   ```

---

## Verification summary (plan-level)

- **The gate.** `cargo fmt --all -- --check`, `cargo make dod` and `cargo make deny` exit `0` after every task. After task 8 `cargo make dod` reports, for `buildl`, `20 passed` unit tests and `2 passed` documentation examples, and prints `guard-adapter-isolation: OK (0 edges)`; the other crates' tests are untouched.
- **The floor.** `cargo +1.94 check --workspace --all-targets --all-features`, the command of the CI's minimum-version job, ends `Finished` on rustc 1.94.1 with this plan applied.
- **Dependencies.** `crates/buildl` depends on `rayon` 1.12.0, `serde` 1.0.229, `tempfile` 3.27.0, `thiserror` 2.0.20 and `toml` 0.8.23, each `{ workspace = true }`, each with its reason in a comment. `cargo tree -d` prints `warning: nothing to print.`; `cargo make deny` prints `advisories ok, bans ok, licenses ok, sources ok` with `[licenses].allow` untouched.
- **Golden files.** `crates/expected-edges.txt` is the output of the crate-edge guard's own command and differs from before by the five new lines of `# buildl`; `crates/expected-module-edges.txt` is not touched, and `cargo make guard-module-edges` passes.
- **The guard, on this tree.** `cargo make guard-adapter-isolation` passes with 0 edges, and refuses each of the eight breaches of task 3's probe.
- **The guard, on the finished tree.** The same task was run, while this plan was written, over a mock of the whole module tree the design describes: 50 files in the 18 entries of `src/`, every module with its designed files and `use` lines, `local_ports.rs` and `workspace.rs` included. It printed `guard-adapter-isolation: OK (24 edges)`: eight from adapters to the two shared modules (`cache`, `cas`, `exec`, `graph`, `log`, `statcache` to `state`; `exec`, `tools` to `search_path`), fourteen from `local_ports`, two from `workspace`. On that mock it refused `cache -> cas`, `state -> search_path`, `report -> local_ports`, `use crate::Workspace;`, `use super::state::StateLayout;` in `search_path.rs`, and a grouped `use crate::{…};`. The script's text ran with the same results under `dash`, the `sh` of the Ubuntu runner.
- **CI.** `.github/workflows/ci.yml` is not changed and needs no change: its `dod` job runs `cargo make dod`, which runs the new guard through `clippy`'s dependencies on both runners; its `deny` job runs the command of `cargo make deny`; its `msrv` job runs the command of "The floor" above. The guard uses `grep`, `sed`, `sort`, `basename`, `mktemp`, `wc` and `tr`, the tools `guard-module-edges` already runs there.
- **Linux.** The `state/` and `search_path.rs` tests are the proof, on the `ubuntu-latest` leg of `cargo make dod`, of what was seen on macOS only: a rename over a read-only file and the modes `tempfile` gives (`write_whole_replaces_the_target_and_leaves_no_temp_file`, `write_whole_gives_the_file_its_access`), removal of a link, a read-only tree and a dangling link (`clear_path_removes_nothing_a_file_a_read_only_tree_and_a_link_alone`), a second lock refused and a dropped one freed (`a_second_lock_is_refused_and_a_dropped_one_is_free`), the lock file's time ordering with an earlier file's (`the_lock_stamp_is_not_before_a_file_written_earlier`), and the lookup rule (the `search_path` tests).
- **Disjoint files for what follows.** After this plan an adapter is written inside one directory of `crates/buildl/src/` and that directory's `mod.rs`; `lib.rs`, `Cargo.toml`, `crates/buildl/Cargo.toml`, `deny.toml`, `Makefile.toml`, `crates/expected-edges.txt`, `state/` and `search_path.rs` need no further change for any adapter.
