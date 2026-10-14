---
status: done
created: 2026-10-14
depends-on: [05, 06, 07, 08]
---

# buildl Composition Implementation Plan

**Goal:** The `buildl` crate gains its composition root, so that a caller runs every command on a real workspace through one facade, `Workspace`.

**Architecture:** Two modules of `crates/buildl` name more than one adapter, and nothing else in the crate does. `local_ports.rs` holds `LocalPorts<R>`, the bundle that chooses one adapter for each of the fourteen ports with the reporter left to the caller; `LocalPipeline<R>`, the pipeline over that bundle; and `Wiring`, a workspace whose manifest has been read. `Wiring` makes everything else from the manifest: the source of build files (`source`), the answer to whether the workspace's strategy can be run (`runs`), the lock on the state directory, held to the ceiling before the directory is made (`lock`), and one instance of every adapter (`ports`). `workspace.rs` holds `Workspace`, the facade: `open`, `root`, `label` and one method per command. `check` and `graph` make a source and nothing else, so they take no lock and open no state file. `plan` and `build` take the lock for the length of the call and wire every port inside it. `build` first asks whether the strategy can be run, lends the caller's reporter to the pipeline as `&mut R`, and folds the action cache's journal after the pipeline returns, whatever it returned; a private function, `concluded`, chooses between the build's result and the fold's. `lib.rs` declares the two modules privately and re-exports their four types and the whole of `buildl-core`, so the binary and an embedder depend on this crate alone.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor; `buildl-core` (`Pipeline`, `PortSet`, `Ports`, `pipeline::check`, `pipeline::graph`, the domain values), `buildl-lua` (`LuaSource`, `DeclarationLimits`), the thirteen adapter modules and the two shared modules of `crates/buildl`; `tempfile` 3.27.0 in tests; the system tools `cp` and `false`, run by argument vector in tests; `cargo-make` 0.37, `cargo-deny` 0.20.

**Content authority:** spec §8.1 (`LocalPorts`, `LocalPipeline`, `Wiring`), §8.2 (the `Workspace` facade, `concluded`, the lock), §8.3 (a strategy that cannot be run), §8.4 (`lib.rs`), §2.7 (the pipeline signatures the facade calls), §3 (what a composition module may name), §11 (the rows raised by `Wiring::runs`, `Wiring::lock` and `Workspace::label`), §12.2 (the unit tests of `local_ports.rs` and `workspace.rs`), §13.2 (the spellings the adapter guard refuses), §15.4 (the rustdoc of `crates/buildl/src/lib.rs`).

---

## Context an implementer needs

What the workspace holds when this plan starts. Every item exists already; this plan changes none of them.

| Item | Signature | Used as |
|---|---|---|
| `buildl_core::Ports` | a trait of fourteen associated types: `Source`, `Manifest`, `Host`, `Digester`, `StatCache`, `Tools`, `Cache`, `Cas`, `Log`, `Strategy`, `Dispatcher`, `Clock`, `Graph`, `Reporter` | what `LocalPorts<R>` implements |
| `buildl_core::PortSet<P: Ports>` | a struct of fourteen public fields: `source`, `manifest`, `host`, `digester`, `stat_cache`, `tools`, `cache`, `cas`, `log`, `strategy`, `dispatcher`, `clock`, `graph`, `reporter` | what `Wiring::ports` returns |
| `buildl_core::Pipeline<P>` | `new(ports: PortSet<P>) -> Self`; `ports_mut(&mut self) -> &mut PortSet<P>`; `plan(&mut self, &EntryName, &Scope, &BTreeMap<SettingName, SettingValue>) -> Result<Plan>`; `build(&mut self, &EntryName, &Scope, &BTreeMap<SettingName, SettingValue>, OnFailure, LiveOutput) -> Result<Vec<ActionOutcome>>` | what `plan` and `build` run |
| `buildl_core::pipeline::{check, graph}` | `check<S: DeclarationSource>(&S, &EntryName) -> Result<Vec<Declaration>>`; `graph<S: DeclarationSource>(&S, &EntryName) -> Result<TargetGraph>` | what `check` and `graph` run |
| `buildl_core::Reporter` | five methods, `begun`, `started`, `printed`, `finished`, `summary`; `&mut R` is a `Reporter` when `R` is | the reporter a build is lent |
| `buildl_core::Workspace` | public fields `out: Directory`, `state: Directory`, `ceiling: Ceiling`, `execution: Execution`; `Execution` holds `workers: Workers`, `timeout: Timeout`, `strategy: Isolation` | the manifest's value, kept in `Wiring` |
| `buildl_core::Isolation` | `Bare`, `Sandbox` (the default), `OsSandbox`, `Container`; `Display` writes `bare`, `sandbox`, `os-sandbox`, `container` | what `runs` reads |
| `buildl_core::Error` | `Adapter { port: PortName, diagnostic: Diagnostic }`, shown as `<port>: <diagnostic>`; `Refused { want: Want, site: Option<Box<DeclarationSite>>, ceiling: Box<Ceiling> }`; `InvalidName { kind, value, reason }`; `CorruptState { file, source }`. It is not `PartialEq` | the errors raised and asserted here |
| `buildl_core::Label` | `parse(&str) -> Result<Self>`; `resolve(raw: &str, base: &Directory) -> Result<Self>`, which takes `//dir:name`, `:name` and `name` | what `label` returns |
| `buildl_core::Directory` | `parse(impl Into<String>) -> Result<Self>`; ASCII letters, digits, `.`, `-`, `_` and `/` only; the empty text is the workspace root; `contains(&self, &Self) -> bool` | the ceiling test, and the base of a relative target |
| `buildl_lua::LuaSource` | `new(root: impl Into<PathBuf>, limits: DeclarationLimits, skipped: impl IntoIterator<Item = Directory>) -> Self`; `PartialEq` | the `Source` port |
| `buildl_lua::DeclarationLimits` | `Default`; `new(memory_bytes: NonZeroU64, instructions: NonZeroU64)`; `with_walk_entries(self, NonZeroU64)`; `memory_bytes()`, `instructions()`, `walk_entries()` | the ceilings build files are evaluated under |
| `crate::manifest::WorkspaceRoot` | `find(start: &Path) -> Result<Self>`; `as_path(&self) -> &Path`, canonical and absolute; `within(&self) -> &Path`, the start relative to the root | the root, and where a command was run |
| `crate::manifest::TomlManifest` | `read(&WorkspaceRoot) -> Result<Self>`; `entry(&self) -> &EntryName`; `declaration(&self) -> DeclarationSettings`, whose three public fields are `Option<NonZeroU64>`; implements `Manifest`; `Clone`, `PartialEq` | the `Manifest` port |
| `crate::state::StateLock` | `acquire(StateLayout) -> Result<Self, LockError>`, which makes the state directory; `layout(&self) -> &StateLayout`; `acquired(&self) -> Timestamp`; unlocks when dropped. `LockError` is shown as `<path> is in use by another buildl` or `<path>: <io error>` | the lock `plan` and `build` hold |
| `crate::state::StateLayout` | `new(root: &Path, state: &Directory) -> Self`; `dir()`, `cache()` | where the state directory is |
| `crate::search_path::SearchPath` | `from_process() -> Self`; `Clone` | read once, handed to two adapters |
| the adapters | `LocalHost::new() -> Result<Self>`; `FileDigester::new(root)`; `LocalStatCache::open(&StateLayout, Timestamp) -> Result<Self>`; `PathTools::new(root, SearchPath)`; `LocalCache::open(&StateLayout) -> Result<Self>` and `fold(&mut self) -> Result<()>`; `LocalCas::new(&Path, &StateLayout)`; `LocalLog::new(&StateLayout)`; `Bare::new(PathBuf, Timeout, SearchPath)`; `ThreadPool::new(Workers)`; `SystemClock`, a unit struct; `GraphFile::new(&StateLayout)`; `report::Silent`, a unit struct that implements `Reporter` | one per port |
| `crates/buildl/src/lib.rs` | a crate doc comment and fifteen `pub mod` lines, the last three `pub mod statcache;`, `pub mod state;`, `pub mod tools;` | the file tasks 1, 5, 6 and 12 edit |

Gate facts that bite here. Each was seen by building this plan's code in task order in a throwaway copy of the workspace that held the finished adapters (rustc 1.99.0, macOS arm64), and the quoted lines are what the tools printed.

- **Both modules are one file each, so `super` is the crate root.** `cargo make guard-adapter-isolation` refuses `super::` anywhere in `local_ports.rs` and `workspace.rs`, in a comment as in code. Their tests import through the crate: `use crate::local_ports::Wiring;`, `use crate::workspace::Workspace;`.
- **The guard also refuses a grouped `crate::{…}` and an item named through the crate root.** Every import of this crate is written `crate::<module>::<item>`, one module per line; a `buildl-core` item is written `buildl_core::<Item>`.
- **A private field nothing reads is dead code.** With `workspace` declared in task 1, before `source` reads it, clippy fails with ``error: field `workspace` is never read``. So `Wiring` gets `workspace` in task 2 and, by the same rule, `search` in task 5: each field arrives with its first reader.
- **A function call inside `unwrap_or` is refused.** `settings.memory_bytes.unwrap_or(defaults.memory_bytes())` fails the gate with ``error: function call inside of `unwrap_or` `` (`clippy::or_fun_call`). The three defaults are taken with `unwrap_or_else`.
- **A rustdoc link needs its target in scope.** Where a doc comment names an item the file does not import, the link carries the path: `` [`Error::Adapter`](buildl_core::Error::Adapter) ``.
- **`Error` cannot be compared.** A test takes an error with `unwrap_err()` and checks it with `matches!`, or by its text. `clippy::panic` is denied in tests as everywhere.
- **Every test module opens with the `#![expect(clippy::unwrap_used, …)]`** the other modules' tests use.
- **Whether `statcache.json` is written depends on time.** The stat cache keeps an entry only for a file last changed more than two seconds before the lock was taken, so a test that writes its inputs and builds at once finds no `statcache.json`, and a slow machine may find one. The tests list the state directory through a helper, `kept`, that leaves that one name out.
- **The tests run real builds with `cp` and `false`,** found on the test process's `PATH` and run by argument vector. No test runs a shell, sets an environment variable or reads the current directory; every workspace is a temp directory passed as a value.
- **One worker makes the order exact.** The test manifest of `workspace.rs` says `workers = 1`, and ready targets are handed out in label order, so in the test with a failing action `//:a-good` is built before `//:bad` fails.
- **A file whose rustdoc holds a fenced example is shown below inside four backticks.**

How the steps are worded. "In `<file>`, replace … with …" gives the exact old text and the exact new text; the old text occurs once in the file. "Add before the first test of the `tests` module" means directly above the first `#[test]` line of the file, followed by one empty line. "Add at the end of the `tests` module, before its closing brace" means after the last test, with one empty line between. Every Rust block is already in rustfmt's form.

What the plans after this one may rely on, exactly as this plan leaves it:

| Item | Signature |
|---|---|
| `buildl::Workspace` | `#[derive(Debug)]`; `pub fn open(start: impl AsRef<Path>) -> Result<Self>`; `pub fn root(&self) -> &Path`; `pub fn label(&self, raw: &str) -> Result<Label>`; `pub fn check(&self) -> Result<Vec<Declaration>>`; `pub fn graph(&self) -> Result<TargetGraph>`; `pub fn plan(&self, scope: &Scope, overrides: &BTreeMap<SettingName, SettingValue>) -> Result<Plan>`; `pub fn build<R: Reporter>(&self, scope: &Scope, overrides: &BTreeMap<SettingName, SettingValue>, on_failure: OnFailure, live: LiveOutput, reporter: &mut R) -> Result<Vec<ActionOutcome>>` |
| `buildl::Wiring` | `#[derive(Debug)]`; `pub fn read(start: &Path) -> Result<Self>`; `pub const fn root(&self) -> &WorkspaceRoot`; `pub const fn entry(&self) -> &EntryName`; `pub fn source(&self) -> LuaSource`; `pub fn runs(&self) -> Result<()>`; `pub fn lock(&self) -> Result<StateLock>`; `pub fn ports<R: Reporter>(&self, lock: &StateLock, reporter: R) -> Result<PortSet<LocalPorts<R>>>` |
| `buildl::LocalPorts<R>` | `impl<R: Reporter> Ports`, never made as a value |
| `buildl::LocalPipeline<R>` | `Pipeline<LocalPorts<R>>` |
| `buildl::buildl_core` | the whole of `buildl-core`, as `use buildl::buildl_core::{Error, Scope, …};` |
| a strategy other than `bare` | `build` returns `Error::Adapter { port: PortName::Manifest, .. }`, shown as `manifest: buildl.toml: [execution] strategy is "sandbox" (the default when it is not written), and this version of buildl runs "bare" only: write strategy = "bare" under [execution]`; for `os-sandbox` and `container` the same line with that word and without the parenthesis. `check`, `graph` and `plan` work under any strategy |
| a state directory outside `fs.write` | `plan` and `build` return `Error::Refused { want: Want::Write(..), site: None, .. }`, shown as `directory .buildl is outside the ceiling (fs.write allows: out)`, and no directory is made |
| a workspace another buildl holds | `plan` and `build` return `Error::Adapter { port: PortName::ActionCache, .. }`, shown as `action cache: <root>/.buildl/lock is in use by another buildl`; `check` and `graph` succeed |
| after any `build` that reached the pipeline | `.buildl/cache.journal` is absent and `.buildl/cache.json` holds every row recorded; `.buildl/graph.json` holds the canonical bytes of the whole graph |

All work happens in the worktree, on its branch, never on `main`. Commits follow Conventional Commits, one per task. Each quoted run, the failing one and the passing one, was captured by running that command on the state its step describes. A failing run is quoted by the lines that name the cause, not in full. In a quoted test result the `filtered out` figure, the count of `buildl`'s tests and the guard's edge count are what the crate holds when the plans before this one were applied as written and the tasks below run in this order.

## File structure

```
crates/buildl/src/local_ports.rs   — [create] LocalPorts, LocalPipeline, Wiring
crates/buildl/src/workspace.rs     — [create] Workspace, the facade; concluded
crates/buildl/src/lib.rs           — [modify] declares the two modules; the re-exports; the crate doc
```

Every task ends with the same gate, run from the workspace root:

```
$ cargo fmt --all -- --check
$ cargo make dod
$ cargo make deny
```

All three exit `0`. `cargo fmt --all -- --check` prints nothing. `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`; each task quotes the guard's line and the count of `buildl`'s unit tests it prints. `cargo make deny` prints `advisories ok, bans ok, licenses ok, sources ok`. The gate was run after every task below and exited `0` each time.

### Task 1 — Read a workspace's manifest into its wiring

**Files:**
- Create `crates/buildl/src/local_ports.rs`
- Modify `crates/buildl/src/lib.rs`

**Steps:**

1. Declare the module and export its type. In `crates/buildl/src/lib.rs`, replace

   ```rust
   pub mod statcache;
   pub mod state;
   pub mod tools;
   ```

   with

   ```rust
   pub mod statcache;
   pub mod state;
   pub mod tools;

   mod local_ports;

   pub use local_ports::Wiring;
   ```

2. Write the failing tests. Create `crates/buildl/src/local_ports.rs` with the test module only, under a one-line module doc that step 4 replaces. The module is one file, so its tests name the type through the crate, never through `super`:

   ```rust
   //! The wiring of a workspace.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::fs;
       use std::path::{Path, PathBuf};

       use buildl_core::{Error, PortName};

       use crate::local_ports::Wiring;

       /// A temporary workspace whose `buildl.toml` holds `manifest`, and its canonical root.
       fn workspace(manifest: &str) -> (tempfile::TempDir, PathBuf) {
           let dir = tempfile::tempdir().unwrap();
           let root = fs::canonicalize(dir.path()).unwrap();
           fs::write(root.join("buildl.toml"), manifest).unwrap();
           (dir, root)
       }

       /// The names of everything in `dir`, sorted.
       fn names(dir: &Path) -> Vec<String> {
           let mut names: Vec<String> = fs::read_dir(dir)
               .unwrap()
               .map(|entry| entry.unwrap().file_name().into_string().unwrap())
               .collect();
           names.sort();
           names
       }

       #[test]
       fn reading_finds_the_root_above_the_start_and_makes_nothing() {
           let (_dir, root) = workspace("[workspace]\nentry = \"BUILD.lua\"\n");
           fs::create_dir_all(root.join("lib/text")).unwrap();

           let wiring = Wiring::read(&root.join("lib/text")).unwrap();

           assert_eq!(wiring.root().as_path(), root);
           assert_eq!(wiring.root().within(), Path::new("lib/text"));
           assert_eq!(wiring.entry().as_str(), "BUILD.lua");
           assert_eq!(names(&root), ["buildl.toml", "lib"]);
       }

       #[test]
       fn a_manifest_that_is_refused_fails_the_reading_whatever_else_the_workspace_holds() {
           let (_dir, root) = workspace("[ceiling]\nfs.wirte = [\"out\"]\n");
           // A build file that could not be evaluated, and a state directory that could not be
           // used: neither is looked at.
           fs::write(root.join("build.lua"), "this is not Lua").unwrap();
           fs::write(root.join(".buildl"), "not a directory").unwrap();

           let error = Wiring::read(&root).unwrap_err();

           assert!(
               matches!(
                   &error,
                   Error::Adapter {
                       port: PortName::Manifest,
                       ..
                   }
               ),
               "{error:?}"
           );
           assert_eq!(
               error.to_string(),
               "manifest: buildl.toml:2:4: unknown field `wirte`, expected `read` or `write`"
           );
       }
   }
   ```

3. Run them and see them fail:

   ```
   $ cargo test -p buildl --lib local_ports::
   ```

   Expected: the crate does not build, because the type does not exist yet.

   ```text
   error[E0432]: unresolved import `local_ports::Wiring`
   error[E0432]: unresolved import `crate::local_ports::Wiring`
   ```

4. Write the wiring. In `crates/buildl/src/local_ports.rs`, replace

   ```rust
   //! The wiring of a workspace.
   ```

   with

   ```rust
   //! The wiring of a workspace: what its adapters are made from.
   //!
   //! Its own module because the adapters have to meet somewhere: every other module of this crate
   //! serves one port and names no other, so what the adapters of one workspace are made from is
   //! held here, in one place.
   //!
   //! Responsibilities: [`Wiring`], a workspace whose manifest has been read.
   //!
   //! Non-responsibilities: running a command, which is the caller's; and anything an adapter does
   //! once it is made.

   use std::path::Path;

   use buildl_core::{EntryName, Result};

   use crate::manifest::{TomlManifest, WorkspaceRoot};

   /// A workspace whose manifest has been read: everything a port is made from. It holds no lock
   /// and has written nothing.
   #[derive(Debug)]
   pub struct Wiring {
       root: WorkspaceRoot,
       manifest: TomlManifest,
   }

   impl Wiring {
       /// Finds the workspace at or above `start` and reads its manifest.
       ///
       /// The manifest is the first thing read and the only one: no build file is opened, and no
       /// directory is made.
       ///
       /// # Errors
       ///
       /// Returns [`Error::Adapter`](buildl_core::Error::Adapter), naming
       /// [`PortName::Manifest`](buildl_core::PortName::Manifest), when no directory at or above
       /// `start` holds a manifest, and when the manifest cannot be read or is refused.
       pub fn read(start: &Path) -> Result<Self> {
           let root = WorkspaceRoot::find(start)?;
           let manifest = TomlManifest::read(&root)?;
           Ok(Self { root, manifest })
       }

       /// The workspace root, and where the search for it began.
       #[must_use]
       pub const fn root(&self) -> &WorkspaceRoot {
           &self.root
       }

       /// The file name a build file has in every directory.
       #[must_use]
       pub const fn entry(&self) -> &EntryName {
           self.manifest.entry()
       }
   }
   ```

5. Run the tests and see them pass:

   ```
   $ cargo test -p buildl --lib local_ports::
   ```

   Expected:

   ```text
   test local_ports::tests::a_manifest_that_is_refused_fails_the_reading_whatever_else_the_workspace_holds ... ok
   test local_ports::tests::reading_finds_the_root_above_the_start_and_makes_nothing ... ok
   test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 213 filtered out
   ```

6. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo make deny
   ```

   Expected: all three exit `0`, and among the lines printed:

   ```text
   guard-adapter-isolation: OK (9 edges)
   test result: ok. 215 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   advisories ok, bans ok, licenses ok, sources ok
   ```

7. Commit:

   ```
   $ git add crates/buildl/src/local_ports.rs crates/buildl/src/lib.rs
   $ git commit -m "feat(buildl): read a workspace's manifest into its wiring"
   ```

### Task 2 — Make the source of build files from the manifest

**Files:**
- Modify `crates/buildl/src/local_ports.rs`

**Steps:**

1. Write the failing test. In `crates/buildl/src/local_ports.rs`, replace

   ```rust
       use std::fs;
       use std::path::{Path, PathBuf};

       use buildl_core::{Error, PortName};

       use crate::local_ports::Wiring;
   ```

   with

   ```rust
       use core::num::NonZeroU64;
       use std::fs;
       use std::path::{Path, PathBuf};

       use buildl_core::{Directory, Error, PortName};
       use buildl_lua::{DeclarationLimits, LuaSource};

       use crate::local_ports::Wiring;
   ```

   Then add at the end of the `tests` module of `crates/buildl/src/local_ports.rs`, before its closing brace:

   ```rust
       #[test]
       fn the_source_takes_each_ceiling_the_manifest_sets_and_the_default_for_the_rest() {
           let skipped = [
               Directory::parse("dist").unwrap(),
               Directory::parse(".buildl").unwrap(),
           ];
           let defaults = DeclarationLimits::default();

           let (_dir, root) = workspace(
               "[workspace]\nout = \"dist\"\n\n[declaration]\nmemory = \"2MB\"\ninstructions = 5000\nwalk_entries = 70\n",
           );
           let every = Wiring::read(&root).unwrap();
           assert_eq!(
               every.source(),
               LuaSource::new(
                   &root,
                   DeclarationLimits::new(
                       NonZeroU64::new(2 * 1024 * 1024).unwrap(),
                       NonZeroU64::new(5000).unwrap()
                   )
                   .with_walk_entries(NonZeroU64::new(70).unwrap()),
                   skipped.clone(),
               )
           );

           let (_dir, root) =
               workspace("[workspace]\nout = \"dist\"\n\n[declaration]\ninstructions = 5000\n");
           let one = Wiring::read(&root).unwrap();
           assert_eq!(
               one.source(),
               LuaSource::new(
                   &root,
                   DeclarationLimits::new(defaults.memory_bytes(), NonZeroU64::new(5000).unwrap())
                       .with_walk_entries(defaults.walk_entries()),
                   skipped.clone(),
               )
           );

           let (_dir, root) = workspace("[workspace]\nout = \"dist\"\n");
           let none = Wiring::read(&root).unwrap();
           assert_eq!(none.source(), LuaSource::new(&root, defaults, skipped));
       }
   ```

2. Run it and see it fail:

   ```
   $ cargo test -p buildl --lib local_ports::
   ```

   Expected: the test module does not build.

   ```text
   error[E0599]: no method named `source` found for struct `Wiring` in the current scope
   ```

3. Keep the workspace the manifest describes, and make the source from it. Four replacements in `crates/buildl/src/local_ports.rs`.

   In `crates/buildl/src/local_ports.rs`, replace

   ```rust
   use buildl_core::{EntryName, Result};

   use crate::manifest::{TomlManifest, WorkspaceRoot};
   ```

   with

   ```rust
   use buildl_core::{EntryName, Manifest, Result};
   use buildl_lua::{DeclarationLimits, LuaSource};

   use crate::manifest::{TomlManifest, WorkspaceRoot};
   ```

   In `crates/buildl/src/local_ports.rs`, replace

   ```rust
       root: WorkspaceRoot,
       manifest: TomlManifest,
   }
   ```

   with

   ```rust
       root: WorkspaceRoot,
       manifest: TomlManifest,
       workspace: buildl_core::Workspace,
   }
   ```

   In `crates/buildl/src/local_ports.rs`, replace

   ```rust
           let manifest = TomlManifest::read(&root)?;
           Ok(Self { root, manifest })
   ```

   with

   ```rust
           let manifest = TomlManifest::read(&root)?;
           let workspace = manifest.workspace()?;
           Ok(Self {
               root,
               manifest,
               workspace,
           })
   ```

   In `crates/buildl/src/local_ports.rs`, replace

   ```rust
       }
   }

   #[cfg(test)]
   ```

   with

   ```rust
       }

       /// The source of build files: all that checking them and resolving their graph need.
       ///
       /// Build files are evaluated under the ceilings the manifest sets, each one it leaves out
       /// taking its default, and their source listings stay out of the output and the state
       /// directory.
       #[must_use]
       pub fn source(&self) -> LuaSource {
           let defaults = DeclarationLimits::default();
           let settings = self.manifest.declaration();
           let limits = DeclarationLimits::new(
               settings
                   .memory_bytes
                   .unwrap_or_else(|| defaults.memory_bytes()),
               settings
                   .instructions
                   .unwrap_or_else(|| defaults.instructions()),
           )
           .with_walk_entries(
               settings
                   .walk_entries
                   .unwrap_or_else(|| defaults.walk_entries()),
           );
           LuaSource::new(
               self.root.as_path(),
               limits,
               [self.workspace.out.clone(), self.workspace.state.clone()],
           )
       }
   }

   #[cfg(test)]
   ```

4. Run the tests and see them pass:

   ```
   $ cargo test -p buildl --lib local_ports::
   ```

   Expected:

   ```text
   test local_ports::tests::the_source_takes_each_ceiling_the_manifest_sets_and_the_default_for_the_rest ... ok
   test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 213 filtered out
   ```

5. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo make deny
   ```

   Expected: all three exit `0`, and among the lines printed:

   ```text
   guard-adapter-isolation: OK (9 edges)
   test result: ok. 216 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   advisories ok, bans ok, licenses ok, sources ok
   ```

6. Commit:

   ```
   $ git add crates/buildl/src/local_ports.rs
   $ git commit -m "feat(buildl): make the source of build files from the manifest"
   ```

### Task 3 — Say whether the workspace's strategy can be run

**Files:**
- Modify `crates/buildl/src/local_ports.rs`

**Steps:**

1. Write the failing test. Add at the end of the `tests` module of `crates/buildl/src/local_ports.rs`, before its closing brace:

   ```rust
       #[test]
       fn only_bare_can_be_run() {
           let (_dir, root) = workspace("[execution]\nstrategy = \"bare\"\n");
           Wiring::read(&root).unwrap().runs().unwrap();

           for manifest in ["", "[execution]\nstrategy = \"sandbox\"\n"] {
               let (_dir, root) = workspace(manifest);
               let error = Wiring::read(&root).unwrap().runs().unwrap_err();
               assert!(
                   matches!(
                       &error,
                       Error::Adapter {
                           port: PortName::Manifest,
                           ..
                       }
                   ),
                   "{error:?}"
               );
               assert_eq!(
                   error.to_string(),
                   "manifest: buildl.toml: [execution] strategy is \"sandbox\" (the default when it \
                    is not written), and this version of buildl runs \"bare\" only: write strategy \
                    = \"bare\" under [execution]",
                   "{manifest:?}"
               );
           }

           for strategy in ["os-sandbox", "container"] {
               let (_dir, root) = workspace(&format!("[execution]\nstrategy = \"{strategy}\"\n"));
               let error = Wiring::read(&root).unwrap().runs().unwrap_err();
               assert_eq!(
                   error.to_string(),
                   format!(
                       "manifest: buildl.toml: [execution] strategy is \"{strategy}\", and this \
                        version of buildl runs \"bare\" only: write strategy = \"bare\" under \
                        [execution]"
                   )
               );
           }
       }
   ```

2. Run it and see it fail:

   ```
   $ cargo test -p buildl --lib local_ports::
   ```

   Expected: the test module does not build.

   ```text
   error[E0599]: no method named `runs` found for struct `Wiring` in the current scope
   ```

3. Write `runs`. Three replacements in `crates/buildl/src/local_ports.rs`.

   In `crates/buildl/src/local_ports.rs`, replace

   ```rust
   use buildl_core::{EntryName, Manifest, Result};
   ```

   with

   ```rust
   use buildl_core::{Diagnostic, EntryName, Error, Isolation, Manifest, PortName, Result};
   ```

   In `crates/buildl/src/local_ports.rs`, replace

   ```rust
   use crate::manifest::{TomlManifest, WorkspaceRoot};

   /// A workspace whose manifest has been read: everything a port is made from. It holds no lock
   ```

   with

   ```rust
   use crate::manifest::{TomlManifest, WorkspaceRoot};

   /// The file a workspace's manifest is read from, as a refusal names it.
   const MANIFEST_FILE: &str = "buildl.toml";

   /// A workspace whose manifest has been read: everything a port is made from. It holds no lock
   ```

   In `crates/buildl/src/local_ports.rs`, replace

   ```rust
       }
   }

   #[cfg(test)]
   ```

   with

   ```rust
       }

       /// Whether this version of buildl can run actions under the workspace's strategy.
       ///
       /// It runs `bare` and nothing else. A workspace that asks for more isolation is refused
       /// instead of being built under less, so a build never drops a guarantee its manifest
       /// asked for.
       ///
       /// # Errors
       ///
       /// Returns [`Error::Adapter`], naming [`PortName::Manifest`], for every strategy but
       /// [`Isolation::Bare`].
       pub fn runs(&self) -> Result<()> {
           let strategy = self.workspace.execution.strategy;
           if strategy == Isolation::Bare {
               return Ok(());
           }
           let unwritten = if strategy == Isolation::default() {
               " (the default when it is not written)"
           } else {
               ""
           };
           Err(Error::Adapter {
               port: PortName::Manifest,
               diagnostic: Diagnostic::new(format!(
                   "{MANIFEST_FILE}: [execution] strategy is \"{strategy}\"{unwritten}, and this \
                    version of buildl runs \"{bare}\" only: write strategy = \"{bare}\" under \
                    [execution]",
                   bare = Isolation::Bare,
               )),
           })
       }
   }

   #[cfg(test)]
   ```

4. Run the tests and see them pass:

   ```
   $ cargo test -p buildl --lib local_ports::
   ```

   Expected:

   ```text
   test local_ports::tests::only_bare_can_be_run ... ok
   test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 213 filtered out
   ```

5. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo make deny
   ```

   Expected: all three exit `0`, and among the lines printed:

   ```text
   guard-adapter-isolation: OK (9 edges)
   test result: ok. 217 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   advisories ok, bans ok, licenses ok, sources ok
   ```

6. Commit:

   ```
   $ git add crates/buildl/src/local_ports.rs
   $ git commit -m "feat(buildl): say whether a workspace's strategy can be run"
   ```

### Task 4 — Take the state directory's lock inside the ceiling

**Files:**
- Modify `crates/buildl/src/local_ports.rs`

**Steps:**

1. Write the failing tests. In `crates/buildl/src/local_ports.rs`, replace

   ```rust
       use buildl_core::{Directory, Error, PortName};
       use buildl_lua::{DeclarationLimits, LuaSource};
   ```

   with

   ```rust
       use buildl_core::{Directory, Error, PortName, Want};
       use buildl_lua::{DeclarationLimits, LuaSource};
   ```

   Add before the first test of the `tests` module of `crates/buildl/src/local_ports.rs`:

   ```rust
       /// A manifest that allows a build to write its outputs and its state, and to run `cp`.
       const BARE: &str = "[ceiling]\nproc.run = [\"cp\"]\nfs.read = [\".\"]\nfs.write = [\"out\", \".buildl\"]\n\n[execution]\nstrategy = \"bare\"\n";
   ```

   Then add at the end of the `tests` module of `crates/buildl/src/local_ports.rs`, before its closing brace:

   ```rust
       #[test]
       fn a_state_directory_outside_the_ceiling_is_refused() {
           let (_dir, root) = workspace("[ceiling]\nfs.write = [\"out\"]\n");
           let wiring = Wiring::read(&root).unwrap();

           let error = wiring.lock().unwrap_err();

           assert!(
               matches!(
                   &error,
                   Error::Refused { want: Want::Write(directory), site: None, ceiling }
                       if directory.as_str() == ".buildl" && **ceiling == wiring.workspace.ceiling
               ),
               "{error:?}"
           );
           assert_eq!(
               error.to_string(),
               "directory .buildl is outside the ceiling (fs.write allows: out)"
           );
           // Refused before anything was made.
           assert_eq!(names(&root), ["buildl.toml"]);
       }

       #[test]
       fn a_state_directory_inside_an_allowed_directory_is_locked_and_made() {
           for manifest in [
               "[ceiling]\nfs.write = [\".buildl\"]\n",
               "[ceiling]\nfs.write = [\".\"]\n",
           ] {
               let (_dir, root) = workspace(manifest);
               let wiring = Wiring::read(&root).unwrap();

               let lock = wiring.lock().unwrap();

               assert_eq!(lock.layout().dir(), root.join(".buildl"), "{manifest:?}");
               assert!(root.join(".buildl/lock").is_file(), "{manifest:?}");
           }
       }

       #[test]
       fn a_busy_lock_is_the_action_caches_adapter_error() {
           let (_dir, root) = workspace(BARE);
           let wiring = Wiring::read(&root).unwrap();
           let held = wiring.lock().unwrap();

           let error = wiring.lock().unwrap_err();

           assert!(
               matches!(
                   &error,
                   Error::Adapter {
                       port: PortName::ActionCache,
                       ..
                   }
               ),
               "{error:?}"
           );
           assert_eq!(
               error.to_string(),
               format!(
                   "action cache: {} is in use by another buildl",
                   root.join(".buildl/lock").display()
               )
           );

           // Free again as soon as the holder lets go.
           drop(held);
           wiring.lock().unwrap();
       }

       #[test]
       fn a_state_directory_that_cannot_be_made_is_the_action_caches_adapter_error() {
           let (_dir, root) = workspace(BARE);
           fs::write(root.join(".buildl"), "not a directory").unwrap();

           let error = Wiring::read(&root).unwrap().lock().unwrap_err();

           assert!(
               matches!(
                   &error,
                   Error::Adapter {
                       port: PortName::ActionCache,
                       ..
                   }
               ),
               "{error:?}"
           );
           assert!(
               error.to_string().starts_with(&format!(
                   "action cache: {}: ",
                   root.join(".buildl").display()
               )),
               "{error}"
           );
       }
   ```

2. Run them and see them fail:

   ```
   $ cargo test -p buildl --lib local_ports::
   ```

   Expected: the test module does not build.

   ```text
   error[E0599]: no method named `lock` found for struct `Wiring` in the current scope
   ```

3. Write `lock`. Three replacements in `crates/buildl/src/local_ports.rs`.

   In `crates/buildl/src/local_ports.rs`, replace

   ```rust
   use buildl_core::{Diagnostic, EntryName, Error, Isolation, Manifest, PortName, Result};
   ```

   with

   ```rust
   use buildl_core::{Diagnostic, EntryName, Error, Isolation, Manifest, PortName, Result, Want};
   ```

   In `crates/buildl/src/local_ports.rs`, replace

   ```rust
   use crate::manifest::{TomlManifest, WorkspaceRoot};

   /// The file a workspace's manifest is read from, as a refusal names it.
   ```

   with

   ```rust
   use crate::manifest::{TomlManifest, WorkspaceRoot};
   use crate::state::{StateLayout, StateLock};

   /// The file a workspace's manifest is read from, as a refusal names it.
   ```

   In `crates/buildl/src/local_ports.rs`, replace

   ```rust
       }
   }

   #[cfg(test)]
   ```

   with

   ```rust
       }

       /// Takes the state directory for this process, after holding it to the ceiling.
       ///
       /// The directory is made when it is missing, but only once the workspace is seen to allow
       /// writing there: a state directory the ceiling leaves out is refused before anything is
       /// made.
       ///
       /// # Errors
       ///
       /// Returns [`Error::Refused`] when the state directory lies in no directory the workspace
       /// allows a build to write into. Returns [`Error::Adapter`], naming
       /// [`PortName::ActionCache`], when another process holds the directory, and when the
       /// directory or its lock file cannot be made.
       pub fn lock(&self) -> Result<StateLock> {
           let state = &self.workspace.state;
           let ceiling = &self.workspace.ceiling;
           if !ceiling.write.iter().any(|allowed| allowed.contains(state)) {
               return Err(Error::Refused {
                   want: Want::Write(state.clone()),
                   site: None,
                   ceiling: Box::new(ceiling.clone()),
               });
           }
           StateLock::acquire(StateLayout::new(self.root.as_path(), state)).map_err(|error| {
               Error::Adapter {
                   port: PortName::ActionCache,
                   diagnostic: Diagnostic::new(error.to_string()),
               }
           })
       }
   }

   #[cfg(test)]
   ```

4. Run the tests and see them pass:

   ```
   $ cargo test -p buildl --lib local_ports::
   ```

   Expected:

   ```text
   test local_ports::tests::a_busy_lock_is_the_action_caches_adapter_error ... ok
   test local_ports::tests::a_state_directory_inside_an_allowed_directory_is_locked_and_made ... ok
   test local_ports::tests::a_state_directory_outside_the_ceiling_is_refused ... ok
   test local_ports::tests::a_state_directory_that_cannot_be_made_is_the_action_caches_adapter_error ... ok
   test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 213 filtered out
   ```

5. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo make deny
   ```

   Expected: all three exit `0`, and among the lines printed:

   ```text
   guard-adapter-isolation: OK (10 edges)
   test result: ok. 221 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   advisories ok, bans ok, licenses ok, sources ok
   ```

6. Commit:

   ```
   $ git add crates/buildl/src/local_ports.rs
   $ git commit -m "feat(buildl): take the state directory's lock inside the ceiling"
   ```

### Task 5 — Choose an adapter for every port and make them all

**Files:**
- Modify `crates/buildl/src/local_ports.rs`
- Modify `crates/buildl/src/lib.rs`

**Steps:**

1. Write the failing tests. In `crates/buildl/src/local_ports.rs`, replace

   ```rust
       use core::num::NonZeroU64;
       use std::fs;
       use std::path::{Path, PathBuf};

       use buildl_core::{Directory, Error, PortName, Want};
       use buildl_lua::{DeclarationLimits, LuaSource};

       use crate::local_ports::Wiring;
   ```

   with

   ```rust
       use core::num::NonZeroU64;
       use std::collections::BTreeMap;
       use std::fs;
       use std::path::{Path, PathBuf};

       use buildl_core::{
           Directory, Error, Pipeline, PortName, Scope, Timeout, Verdict, Want, Workers,
       };
       use buildl_lua::{DeclarationLimits, LuaSource};

       use crate::cas::LocalCas;
       use crate::digest::FileDigester;
       use crate::dispatch::ThreadPool;
       use crate::exec::Bare;
       use crate::graph::GraphFile;
       use crate::host::LocalHost;
       use crate::local_ports::{LocalPipeline, Wiring};
       use crate::log::LocalLog;
       use crate::manifest::TomlManifest;
       use crate::report::Silent;
       use crate::tools::PathTools;
   ```

   Then add at the end of the `tests` module of `crates/buildl/src/local_ports.rs`, before its closing brace:

   ```rust
       #[test]
       fn every_port_is_made_from_the_manifest_the_root_and_the_lock() {
           let (_dir, root) = workspace(&format!("{BARE}workers = 3\ntimeout = \"7s\"\n"));
           let wiring = Wiring::read(&root).unwrap();
           let lock = wiring.lock().unwrap();
           let layout = lock.layout();
           let search = wiring.search.clone();

           let ports = wiring.ports(&lock, Silent).unwrap();

           assert_eq!(ports.source, wiring.source());
           assert_eq!(ports.manifest, TomlManifest::read(wiring.root()).unwrap());
           assert_eq!(ports.host, LocalHost::new().unwrap());
           assert_eq!(ports.digester, FileDigester::new(&root));
           assert_eq!(ports.tools, PathTools::new(&root, search.clone()));
           // The adapters below cannot be compared, so each is held to the rendering of one made
           // here from the values it must have been handed.
           let timeout = Timeout::parse("7s").unwrap();
           assert_eq!(
               format!("{:?}", ports.strategy),
               format!("{:?}", Bare::new(root.clone(), timeout, search))
           );
           assert_eq!(
               format!("{:?}", ports.dispatcher),
               format!("{:?}", ThreadPool::new(Workers::of(3)))
           );
           assert_eq!(
               format!("{:?}", ports.cas),
               format!("{:?}", LocalCas::new(&root, layout))
           );
           assert_eq!(
               format!("{:?}", ports.log),
               format!("{:?}", LocalLog::new(layout))
           );
           assert_eq!(
               format!("{:?}", ports.graph),
               format!("{:?}", GraphFile::new(layout))
           );
       }

       #[test]
       fn a_cache_that_is_not_canonical_stops_the_wiring_with_its_own_error() {
           let (_dir, root) = workspace(BARE);
           let wiring = Wiring::read(&root).unwrap();
           let lock = wiring.lock().unwrap();
           fs::write(lock.layout().cache(), "{} ").unwrap();

           let error = wiring.ports(&lock, Silent).unwrap_err();

           assert!(
               matches!(&error, Error::CorruptState { file, .. } if *file == root.join(".buildl/cache.json")),
               "{error:?}"
           );
       }

       #[test]
       fn a_pipeline_over_the_ports_plans_a_workspace() {
           let (_dir, root) = workspace(BARE);
           fs::write(root.join("a.txt"), "a\n").unwrap();
           fs::write(
               root.join("build.lua"),
               "buildl.target(\"copy\", { inputs = { \"a.txt\" }, run = { \"cp\", \"$in\", \"$out\" } })\n",
           )
           .unwrap();
           let wiring = Wiring::read(&root).unwrap();
           let lock = wiring.lock().unwrap();

           let mut pipeline: LocalPipeline<Silent> =
               Pipeline::new(wiring.ports(&lock, Silent).unwrap());
           let plan = pipeline
               .plan(wiring.entry(), &Scope::Workspace, &BTreeMap::new())
               .unwrap();

           assert_eq!(plan.len(), 1);
           assert!(
               matches!(plan.verdicts(), [Verdict::Dirty { .. }]),
               "{:?}",
               plan.verdicts()
           );
       }
   ```

2. Run them and see them fail:

   ```
   $ cargo test -p buildl --lib local_ports::
   ```

   Expected: the test module does not build.

   ```text
   error[E0432]: unresolved import `crate::local_ports::LocalPipeline`
   ```

3. Write the bundle, the pipeline over it, and `ports`. Six replacements in `crates/buildl/src/local_ports.rs`.

   The module now holds the bundle too. In `crates/buildl/src/local_ports.rs`, replace

   ```rust
   //! The wiring of a workspace: what its adapters are made from.
   //!
   //! Its own module because the adapters have to meet somewhere: every other module of this crate
   //! serves one port and names no other, so what the adapters of one workspace are made from is
   //! held here, in one place.
   //!
   //! Responsibilities: [`Wiring`], a workspace whose manifest has been read.
   //!
   ```

   with

   ```rust
   //! The bundle of real adapters, and the wiring that makes one instance of each.
   //!
   //! Its own module because the adapters have to meet somewhere: every other module of this crate
   //! serves one port and names no other, so which adapter serves which port, what each is made
   //! from and in which order is decided here, in one place.
   //!
   //! Responsibilities: [`LocalPorts`], the choice of an adapter for every port;
   //! [`LocalPipeline`], the pipeline over that choice; and [`Wiring`], a workspace whose manifest
   //! has been read, which makes the adapters from it.
   //!
   ```

   In `crates/buildl/src/local_ports.rs`, replace

   ```rust
   use std::path::Path;

   use buildl_core::{Diagnostic, EntryName, Error, Isolation, Manifest, PortName, Result, Want};
   use buildl_lua::{DeclarationLimits, LuaSource};

   use crate::manifest::{TomlManifest, WorkspaceRoot};
   use crate::state::{StateLayout, StateLock};
   ```

   with

   ```rust
   use core::marker::PhantomData;
   use std::path::Path;

   use buildl_core::{
       Diagnostic, EntryName, Error, Isolation, Manifest, Pipeline, PortName, PortSet, Ports,
       Reporter, Result, Want,
   };
   use buildl_lua::{DeclarationLimits, LuaSource};

   use crate::cache::LocalCache;
   use crate::cas::LocalCas;
   use crate::clock::SystemClock;
   use crate::digest::FileDigester;
   use crate::dispatch::ThreadPool;
   use crate::exec::Bare;
   use crate::graph::GraphFile;
   use crate::host::LocalHost;
   use crate::log::LocalLog;
   use crate::manifest::{TomlManifest, WorkspaceRoot};
   use crate::search_path::SearchPath;
   use crate::statcache::LocalStatCache;
   use crate::state::{StateLayout, StateLock};
   use crate::tools::PathTools;
   ```

   In `crates/buildl/src/local_ports.rs`, replace

   ```rust
   const MANIFEST_FILE: &str = "buildl.toml";

   ```

   with

   ```rust
   const MANIFEST_FILE: &str = "buildl.toml";

   /// The bundle of real adapters. `R` is the reporter a build is lent.
   ///
   /// A bundle is a choice of types and is never made as a value: it is named as the parameter of
   /// a [`PortSet`] or of a [`Pipeline`].
   #[derive(Debug)]
   pub struct LocalPorts<R>(PhantomData<fn() -> R>);

   impl<R: Reporter> Ports for LocalPorts<R> {
       type Source = LuaSource;
       type Manifest = TomlManifest;
       type Host = LocalHost;
       type Digester = FileDigester;
       type StatCache = LocalStatCache;
       type Tools = PathTools;
       type Cache = LocalCache;
       type Cas = LocalCas;
       type Log = LocalLog;
       type Strategy = Bare;
       type Dispatcher = ThreadPool;
       type Clock = SystemClock;
       type Graph = GraphFile;
       type Reporter = R;
   }

   /// The pipeline over the real adapters.
   pub type LocalPipeline<R> = Pipeline<LocalPorts<R>>;

   ```

   In `crates/buildl/src/local_ports.rs`, replace

   ```rust
       workspace: buildl_core::Workspace,
   }
   ```

   with

   ```rust
       workspace: buildl_core::Workspace,
       search: SearchPath,
   }
   ```

   In `crates/buildl/src/local_ports.rs`, replace

   ```rust
           Ok(Self {
               root,
               manifest,
               workspace,
           })
   ```

   with

   ```rust
           Ok(Self {
               root,
               manifest,
               workspace,
               search: SearchPath::from_process(),
           })
   ```

   In `crates/buildl/src/local_ports.rs`, replace

   ```rust
       }
   }

   #[cfg(test)]
   ```

   with

   ```rust
       }

       /// Every port, for one plan or one build. `lock` proves the state directory is this
       /// process's own.
       ///
       /// The worker count and the timeout of the manifest reach the dispatcher and the strategy
       /// here and nowhere else, and the program lookup and the strategy are handed one search
       /// path, so the file that is run is the file that was fingerprinted.
       ///
       /// # Errors
       ///
       /// Returns whatever an adapter returns when it is made: the host's, the stat cache's and the
       /// action cache's own errors, unchanged.
       pub fn ports<R: Reporter>(
           &self,
           lock: &StateLock,
           reporter: R,
       ) -> Result<PortSet<LocalPorts<R>>> {
           let layout = lock.layout();
           let root = self.root.as_path();
           Ok(PortSet {
               source: self.source(),
               manifest: self.manifest.clone(),
               host: LocalHost::new()?,
               digester: FileDigester::new(root),
               stat_cache: LocalStatCache::open(layout, lock.acquired())?,
               tools: PathTools::new(root, self.search.clone()),
               cache: LocalCache::open(layout)?,
               cas: LocalCas::new(root, layout),
               log: LocalLog::new(layout),
               strategy: Bare::new(
                   root.to_path_buf(),
                   self.workspace.execution.timeout,
                   self.search.clone(),
               ),
               dispatcher: ThreadPool::new(self.workspace.execution.workers),
               clock: SystemClock,
               graph: GraphFile::new(layout),
               reporter,
           })
       }
   }

   #[cfg(test)]
   ```

4. Export the bundle and the pipeline. In `crates/buildl/src/lib.rs`, replace

   ```rust
   pub use local_ports::Wiring;
   ```

   with

   ```rust
   pub use local_ports::{LocalPipeline, LocalPorts, Wiring};
   ```

5. Run the tests and see them pass:

   ```
   $ cargo test -p buildl --lib local_ports::
   ```

   Expected:

   ```text
   test local_ports::tests::a_cache_that_is_not_canonical_stops_the_wiring_with_its_own_error ... ok
   test local_ports::tests::a_pipeline_over_the_ports_plans_a_workspace ... ok
   test local_ports::tests::every_port_is_made_from_the_manifest_the_root_and_the_lock ... ok
   test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 213 filtered out
   ```

6. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo make deny
   ```

   Expected: all three exit `0`, and among the lines printed:

   ```text
   guard-adapter-isolation: OK (23 edges)
   test result: ok. 224 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   advisories ok, bans ok, licenses ok, sources ok
   ```

7. Commit:

   ```
   $ git add crates/buildl/src/local_ports.rs crates/buildl/src/lib.rs
   $ git commit -m "feat(buildl): choose an adapter for every port and make them all"
   ```

### Task 6 — Open a workspace and read a target as a person names it

**Files:**
- Create `crates/buildl/src/workspace.rs`
- Modify `crates/buildl/src/lib.rs`

**Steps:**

1. Declare the module and export its type. In `crates/buildl/src/lib.rs`, replace

   ```rust
   mod local_ports;

   pub use local_ports::{LocalPipeline, LocalPorts, Wiring};
   ```

   with

   ```rust
   mod local_ports;
   mod workspace;

   pub use local_ports::{LocalPipeline, LocalPorts, Wiring};
   pub use workspace::Workspace;
   ```

2. Write the failing tests. Create `crates/buildl/src/workspace.rs` with the test module only, under a one-line module doc that step 4 replaces. This module is one file too, so its tests import through the crate:

   ```rust
   //! The facade over a wired workspace.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::fs;
       use std::path::PathBuf;

       use buildl_core::{Error, Label, NameKind};

       use crate::workspace::Workspace;

       /// A temporary workspace holding each of `files`, and its canonical root.
       fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
           let dir = tempfile::tempdir().unwrap();
           let root = fs::canonicalize(dir.path()).unwrap();
           for (path, text) in files {
               let path = root.join(path);
               fs::create_dir_all(path.parent().unwrap()).unwrap();
               fs::write(path, text).unwrap();
           }
           (dir, root)
       }

       /// A workspace with a `lib/` and a directory no label can name.
       fn directories() -> (tempfile::TempDir, PathBuf) {
           let (dir, root) = workspace(&[("buildl.toml", "")]);
           fs::create_dir(root.join("lib")).unwrap();
           fs::create_dir(root.join("my dir")).unwrap();
           (dir, root)
       }

       fn label(text: &str) -> Label {
           Label::parse(text).unwrap()
       }

       #[test]
       fn the_root_is_canonical_wherever_the_workspace_was_opened_from() {
           let (dir, root) = directories();

           assert_eq!(Workspace::open(dir.path()).unwrap().root(), root);
           assert_eq!(Workspace::open(root.join("lib")).unwrap().root(), root);
       }

       #[test]
       fn a_relative_target_is_read_against_the_directory_the_workspace_was_opened_from() {
           let (_dir, root) = directories();

           let below = Workspace::open(root.join("lib")).unwrap();
           assert_eq!(below.label("words").unwrap(), label("//lib:words"));
           assert_eq!(below.label(":words").unwrap(), label("//lib:words"));

           let at_root = Workspace::open(&root).unwrap();
           assert_eq!(at_root.label("words").unwrap(), label("//:words"));
           assert_eq!(at_root.label(":words").unwrap(), label("//:words"));
       }

       #[test]
       fn a_full_label_is_read_the_same_from_any_directory() {
           let (_dir, root) = directories();

           for start in [root.clone(), root.join("lib"), root.join("my dir")] {
               let workspace = Workspace::open(&start).unwrap();
               assert_eq!(
                   workspace.label("//lib:words").unwrap(),
                   label("//lib:words"),
                   "{}",
                   start.display()
               );
           }
       }

       #[test]
       fn a_start_directory_outside_the_directory_grammar_fails_only_the_relative_forms() {
           let (_dir, root) = directories();
           let workspace = Workspace::open(root.join("my dir")).unwrap();

           for raw in ["words", ":words"] {
               let error = workspace.label(raw).unwrap_err();
               assert!(
                   matches!(
                       &error,
                       Error::InvalidName { kind: NameKind::Directory, value, .. } if value == "my dir"
                   ),
                   "{raw}: {error:?}"
               );
           }
           assert_eq!(
               workspace.label("//lib:words").unwrap(),
               label("//lib:words")
           );
       }

       #[test]
       fn a_target_that_fails_the_label_grammar_is_refused() {
           let (_dir, root) = directories();
           let workspace = Workspace::open(&root).unwrap();

           for raw in ["//lib", "a b"] {
               let error = workspace.label(raw).unwrap_err();
               assert!(
                   matches!(&error, Error::InvalidName { .. }),
                   "{raw}: {error:?}"
               );
           }
       }
   }
   ```

3. Run them and see them fail:

   ```
   $ cargo test -p buildl --lib workspace::
   ```

   Expected: the crate does not build, because the type does not exist yet.

   ```text
   error[E0432]: unresolved import `crate::workspace::Workspace`
   error[E0432]: unresolved import `workspace::Workspace`
   ```

4. Write the facade's first three methods. In `crates/buildl/src/workspace.rs`, replace

   ```rust
   //! The facade over a wired workspace.
   ```

   with

   ```rust
   //! The facade over a wired workspace.
   //!
   //! Its own module because it is what a caller holds: the binary and an embedder alike open a
   //! [`Workspace`] and ask it for what they need, and neither names an adapter.
   //!
   //! Responsibilities: [`Workspace`]: opening one, and reading a target as a person names it.
   //!
   //! Non-responsibilities: making the adapters, which is the wiring's; and showing anything,
   //! which is the caller's.

   use std::path::Path;

   use buildl_core::{Directory, Label, Result};

   use crate::local_ports::Wiring;

   /// A workspace wired with the real adapters.
   ///
   /// Opening one reads its manifest and nothing else.
   #[derive(Debug)]
   pub struct Workspace {
       wiring: Wiring,
   }

   impl Workspace {
       /// The workspace at or above `start`. Reads `buildl.toml`; writes nothing.
       ///
       /// # Errors
       ///
       /// Returns [`Error::Adapter`](buildl_core::Error::Adapter), naming
       /// [`PortName::Manifest`](buildl_core::PortName::Manifest), when no directory at or above
       /// `start` holds a `buildl.toml`, and when the one found cannot be read or is refused.
       pub fn open(start: impl AsRef<Path>) -> Result<Self> {
           Ok(Self {
               wiring: Wiring::read(start.as_ref())?,
           })
       }

       /// The workspace root: canonical and absolute.
       #[must_use]
       pub fn root(&self) -> &Path {
           self.wiring.root().as_path()
       }

       /// A target as a person names it: `//dir:name`, or `:name` / `name` in the directory the
       /// workspace was opened from.
       ///
       /// # Errors
       ///
       /// Returns [`Error::InvalidName`](buildl_core::Error::InvalidName) when `raw` fails the
       /// label grammar, and, for the two relative forms only, when the directory the workspace
       /// was opened from has a path below the root that no label can hold.
       pub fn label(&self, raw: &str) -> Result<Label> {
           if raw.starts_with("//") {
               // A full label names its own directory, so where the workspace was opened from is
               // not read.
               return Label::parse(raw);
           }
           let base = Directory::parse(self.wiring.root().within().to_string_lossy())?;
           Label::resolve(raw, &base)
       }
   }
   ```

5. Run the tests and see them pass:

   ```
   $ cargo test -p buildl --lib workspace::
   ```

   Expected:

   ```text
   test workspace::tests::a_full_label_is_read_the_same_from_any_directory ... ok
   test workspace::tests::a_relative_target_is_read_against_the_directory_the_workspace_was_opened_from ... ok
   test workspace::tests::a_start_directory_outside_the_directory_grammar_fails_only_the_relative_forms ... ok
   test workspace::tests::a_target_that_fails_the_label_grammar_is_refused ... ok
   test workspace::tests::the_root_is_canonical_wherever_the_workspace_was_opened_from ... ok
   test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 224 filtered out
   ```

6. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo make deny
   ```

   Expected: all three exit `0`, and among the lines printed:

   ```text
   guard-adapter-isolation: OK (24 edges)
   test result: ok. 229 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   advisories ok, bans ok, licenses ok, sources ok
   ```

7. Commit:

   ```
   $ git add crates/buildl/src/workspace.rs crates/buildl/src/lib.rs
   $ git commit -m "feat(buildl): open a workspace and read a target as a person names it"
   ```

### Task 7 — Check and resolve build files without the state directory

**Files:**
- Modify `crates/buildl/src/workspace.rs`

**Steps:**

1. Write the failing tests. In `crates/buildl/src/workspace.rs`, replace

   ```rust
       use std::fs;
       use std::path::PathBuf;

       use buildl_core::{Error, Label, NameKind};
   ```

   with

   ```rust
       use std::fs;
       use std::path::{Path, PathBuf};

       use buildl_core::json::canonical;
       use buildl_core::{Error, Label, NameKind};
   ```

   Add before the first test of the `tests` module of `crates/buildl/src/workspace.rs`:

   ```rust
       /// A manifest that allows a build to write its outputs and its state, to read every file,
       /// and to run `cp` and `false`, one action at a time.
       const BARE: &str = "[ceiling]\nproc.run = [\"cp\", \"false\"]\nfs.read = [\".\"]\nfs.write = [\"out\", \".buildl\"]\n\n[execution]\nworkers = 1\nstrategy = \"bare\"\n";

       /// One target, `//:copy`, that copies `a.txt`.
       const COPY: &str = "buildl.target(\"copy\", { inputs = { \"a.txt\" }, run = { \"cp\", \"$in\", \"$out\" } })\n";

       /// The workspace of [`COPY`] under [`BARE`].
       fn copying() -> (tempfile::TempDir, PathBuf) {
           workspace(&[("buildl.toml", BARE), ("build.lua", COPY), ("a.txt", "a\n")])
       }

       /// The names of everything in `dir`, sorted.
       fn names(dir: &Path) -> Vec<String> {
           let mut names: Vec<String> = fs::read_dir(dir)
               .unwrap()
               .map(|entry| entry.unwrap().file_name().into_string().unwrap())
               .collect();
           names.sort();
           names
       }
   ```

   Then add at the end of the `tests` module of `crates/buildl/src/workspace.rs`, before its closing brace:

   ```rust
       #[test]
       fn check_and_graph_read_build_files_and_make_nothing() {
           let (_dir, root) = copying();
           let workspace = Workspace::open(&root).unwrap();

           let declarations = workspace.check().unwrap();
           let graph = workspace.graph().unwrap();

           assert_eq!(declarations.len(), 1);
           assert_eq!(
               canonical::to_string(&graph).unwrap(),
               "{\"aliases\":{},\"settings\":{},\"targets\":{\"//:copy\":{\"deps\":[],\
                \"description\":null,\"env\":[],\"freshness\":\"Cached\",\"inputs\":[\"a.txt\"],\
                \"network\":\"Sealed\",\"outputs\":[\"copy\"],\"provenance\":{\"directory\":\"\",\
                \"file\":\"build.lua\"},\"role\":\"Build\",\"run\":[\"cp\",\"$in\",\"$out\"]}}}"
           );
           assert_eq!(names(&root), ["a.txt", "build.lua", "buildl.toml"]);
       }

       #[test]
       fn check_and_graph_are_stopped_by_neither_a_held_lock_nor_a_cache_that_is_not_canonical() {
           let (_dir, root) = copying();
           let workspace = Workspace::open(&root).unwrap();
           let _held = workspace.wiring.lock().unwrap();
           fs::write(root.join(".buildl/cache.json"), "not a cache").unwrap();

           assert_eq!(workspace.check().unwrap().len(), 1);
           workspace.graph().unwrap();
       }
   ```

2. Run them and see them fail:

   ```
   $ cargo test -p buildl --lib workspace::
   ```

   Expected: the test module does not build.

   ```text
   error[E0599]: no method named `check` found for struct `workspace::Workspace` in the current scope
   error[E0599]: no method named `graph` found for struct `workspace::Workspace` in the current scope
   ```

3. Write `check` and `graph`. Three replacements in `crates/buildl/src/workspace.rs`.

   The module now holds commands. In `crates/buildl/src/workspace.rs`, replace

   ```rust
   //! The facade over a wired workspace.
   //!
   //! Its own module because it is what a caller holds: the binary and an embedder alike open a
   //! [`Workspace`] and ask it for what they need, and neither names an adapter.
   //!
   //! Responsibilities: [`Workspace`]: opening one, and reading a target as a person names it.
   //!
   //! Non-responsibilities: making the adapters, which is the wiring's; and showing anything,
   //! which is the caller's.
   ```

   with

   ```rust
   //! The facade over a wired workspace: one method per command.
   //!
   //! Its own module because it is what a caller holds: the binary and an embedder alike open a
   //! [`Workspace`] and call a command on it, and neither names an adapter. What each command
   //! needs of the workspace is decided here.
   //!
   //! Responsibilities: [`Workspace`]: opening one, reading a target as a person names it, and
   //! the commands.
   //!
   //! Non-responsibilities: the sequence of phases, which is the pipeline's; making the adapters,
   //! which is the wiring's; and showing anything, which is the caller's.
   ```

   In `crates/buildl/src/workspace.rs`, replace

   ```rust
   use buildl_core::{Directory, Label, Result};
   ```

   with

   ```rust
   use buildl_core::{Declaration, Directory, Label, Result, TargetGraph, pipeline};
   ```

   In `crates/buildl/src/workspace.rs`, replace

   ```rust
       }
   }

   #[cfg(test)]
   ```

   with

   ```rust
       }

       /// Evaluates every build file twice and returns the declarations when both runs agree.
       ///
       /// Takes no lock and opens no state file, so it is stopped neither by a running build nor
       /// by a state directory that cannot be used.
       ///
       /// # Errors
       ///
       /// Returns whatever [`pipeline::check`] returns.
       pub fn check(&self) -> Result<Vec<Declaration>> {
           pipeline::check(&self.wiring.source(), self.wiring.entry())
       }

       /// Evaluates every build file once and resolves the declarations into the target graph.
       ///
       /// Takes no lock and opens no state file, as [`Workspace::check`].
       ///
       /// # Errors
       ///
       /// Returns whatever [`pipeline::graph`] returns.
       pub fn graph(&self) -> Result<TargetGraph> {
           pipeline::graph(&self.wiring.source(), self.wiring.entry())
       }
   }

   #[cfg(test)]
   ```

4. Run the tests and see them pass:

   ```
   $ cargo test -p buildl --lib workspace::
   ```

   Expected:

   ```text
   test workspace::tests::check_and_graph_are_stopped_by_neither_a_held_lock_nor_a_cache_that_is_not_canonical ... ok
   test workspace::tests::check_and_graph_read_build_files_and_make_nothing ... ok
   test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 224 filtered out
   ```

5. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo make deny
   ```

   Expected: all three exit `0`, and among the lines printed:

   ```text
   guard-adapter-isolation: OK (24 edges)
   test result: ok. 231 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   advisories ok, bans ok, licenses ok, sources ok
   ```

6. Commit:

   ```
   $ git add crates/buildl/src/workspace.rs
   $ git commit -m "feat(buildl): check and resolve build files without the state directory"
   ```

### Task 8 — Plan a build under the state directory's lock

**Files:**
- Modify `crates/buildl/src/workspace.rs`

**Steps:**

1. Write the failing tests. In `crates/buildl/src/workspace.rs`, replace

   ```rust
       use std::fs;
       use std::path::{Path, PathBuf};

       use buildl_core::json::canonical;
       use buildl_core::{Error, Label, NameKind};
   ```

   with

   ```rust
       use std::collections::BTreeMap;
       use std::fs;
       use std::path::{Path, PathBuf};

       use buildl_core::json::canonical;
       use buildl_core::{Error, Label, NameKind, PortName, Scope, Verdict, Want};
   ```

   Add before the first test of the `tests` module of `crates/buildl/src/workspace.rs`:

   ```rust
       /// The names of everything in the state directory of `root` but the stat cache, sorted.
       /// Whether the stat cache is written depends on how long ago an input was last changed.
       fn kept(root: &Path) -> Vec<String> {
           let mut names = names(&root.join(".buildl"));
           names.retain(|name| name != "statcache.json");
           names
       }
   ```

   Then add at the end of the `tests` module of `crates/buildl/src/workspace.rs`, before its closing brace:

   ```rust
       #[test]
       fn plan_takes_the_lock_and_builds_nothing() {
           let (_dir, root) = copying();
           let workspace = Workspace::open(&root).unwrap();

           let plan = workspace.plan(&Scope::Workspace, &BTreeMap::new()).unwrap();

           assert!(
               matches!(plan.verdicts(), [Verdict::Dirty { .. }]),
               "{:?}",
               plan.verdicts()
           );
           // The lock file and nothing else: no output, no graph, no row.
           assert_eq!(kept(&root), ["lock"]);
           assert!(!root.join("out").exists());
           // The directory was let go when the plan returned.
           workspace.wiring.lock().unwrap();
       }

       #[test]
       fn plan_is_refused_while_another_buildl_holds_the_workspace() {
           let (_dir, root) = copying();
           let workspace = Workspace::open(&root).unwrap();
           let _held = workspace.wiring.lock().unwrap();

           let error = workspace
               .plan(&Scope::Workspace, &BTreeMap::new())
               .unwrap_err();

           assert!(
               matches!(
                   &error,
                   Error::Adapter {
                       port: PortName::ActionCache,
                       ..
                   }
               ),
               "{error:?}"
           );
       }

       #[test]
       fn plan_is_refused_before_the_state_directory_is_made_when_the_ceiling_leaves_it_out() {
           let manifest = BARE.replace("\"out\", \".buildl\"", "\"out\"");
           let (_dir, root) = workspace(&[
               ("buildl.toml", &manifest),
               ("build.lua", COPY),
               ("a.txt", "a\n"),
           ]);
           let workspace = Workspace::open(&root).unwrap();

           let error = workspace
               .plan(&Scope::Workspace, &BTreeMap::new())
               .unwrap_err();

           assert!(
               matches!(&error, Error::Refused { want: Want::Write(directory), site: None, .. } if directory.as_str() == ".buildl"),
               "{error:?}"
           );
           assert!(!root.join(".buildl").exists());
       }

       #[test]
       fn plan_works_under_a_strategy_this_version_cannot_run() {
           let manifest = BARE.replace("strategy = \"bare\"\n", "");
           let (_dir, root) = workspace(&[
               ("buildl.toml", &manifest),
               ("build.lua", COPY),
               ("a.txt", "a\n"),
           ]);
           let workspace = Workspace::open(&root).unwrap();

           let plan = workspace.plan(&Scope::Workspace, &BTreeMap::new()).unwrap();

           assert_eq!(plan.len(), 1);
       }
   ```

2. Run them and see them fail:

   ```
   $ cargo test -p buildl --lib workspace::
   ```

   Expected: the test module does not build.

   ```text
   error[E0599]: no method named `plan` found for struct `workspace::Workspace` in the current scope
   ```

3. Write `plan`. Three replacements in `crates/buildl/src/workspace.rs`.

   In `crates/buildl/src/workspace.rs`, replace

   ```rust
   use std::path::Path;

   use buildl_core::{Declaration, Directory, Label, Result, TargetGraph, pipeline};

   use crate::local_ports::Wiring;
   ```

   with

   ```rust
   use std::collections::BTreeMap;
   use std::path::Path;

   use buildl_core::{
       Declaration, Directory, Label, Pipeline, Plan, Result, Scope, SettingName, SettingValue,
       TargetGraph, pipeline,
   };

   use crate::local_ports::Wiring;
   use crate::report::Silent;
   ```

   In `crates/buildl/src/workspace.rs`, replace

   ```rust
       }
   }

   #[cfg(test)]
   ```

   with

   ```rust
       }

       /// Plans a build of `scope` with `overrides` given to build settings, and runs nothing.
       ///
       /// Holds the state directory while it runs. It writes the lock file, the stat cache when a
       /// digest was added to it, and the action cache when an interrupted run left rows behind;
       /// it writes no output and no graph.
       ///
       /// # Errors
       ///
       /// Returns [`Error::Refused`](buildl_core::Error::Refused) when the state directory is
       /// outside what the workspace allows a build to write into, and
       /// [`Error::Adapter`](buildl_core::Error::Adapter), naming
       /// [`PortName::ActionCache`](buildl_core::PortName::ActionCache), when another process
       /// holds the workspace. Otherwise returns whatever making the adapters returns, then
       /// whatever [`Pipeline::plan`] returns.
       pub fn plan(
           &self,
           scope: &Scope,
           overrides: &BTreeMap<SettingName, SettingValue>,
       ) -> Result<Plan> {
           let lock = self.wiring.lock()?;
           Pipeline::new(self.wiring.ports(&lock, Silent)?).plan(self.wiring.entry(), scope, overrides)
       }
   }

   #[cfg(test)]
   ```

4. Run the tests and see them pass:

   ```
   $ cargo test -p buildl --lib workspace::
   ```

   Expected:

   ```text
   test workspace::tests::plan_is_refused_before_the_state_directory_is_made_when_the_ceiling_leaves_it_out ... ok
   test workspace::tests::plan_is_refused_while_another_buildl_holds_the_workspace ... ok
   test workspace::tests::plan_takes_the_lock_and_builds_nothing ... ok
   test workspace::tests::plan_works_under_a_strategy_this_version_cannot_run ... ok
   test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 224 filtered out
   ```

5. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo make deny
   ```

   Expected: all three exit `0`, and among the lines printed:

   ```text
   guard-adapter-isolation: OK (25 edges)
   test result: ok. 235 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   advisories ok, bans ok, licenses ok, sources ok
   ```

6. Commit:

   ```
   $ git add crates/buildl/src/workspace.rs
   $ git commit -m "feat(buildl): plan a build under the state directory's lock"
   ```

### Task 9 — Build under the lock, with the caller's reporter

**Files:**
- Modify `crates/buildl/src/workspace.rs`

**Steps:**

1. Write the failing tests. In `crates/buildl/src/workspace.rs`, replace

   ```rust
       use std::collections::BTreeMap;
       use std::fs;
       use std::path::{Path, PathBuf};

       use buildl_core::json::canonical;
       use buildl_core::{Error, Label, NameKind, PortName, Scope, Verdict, Want};

       use crate::workspace::Workspace;
   ```

   with

   ```rust
       use std::collections::{BTreeMap, BTreeSet};
       use std::fs;
       use std::path::{Path, PathBuf};

       use buildl_core::json::canonical;
       use buildl_core::{
           Captured, Channel, Error, Label, LiveOutput, NameKind, OnFailure, OutcomeClass,
           OutcomeEvent, PortName, ReadyAction, Reporter, Result, Scope, Summary, Verdict, Want,
       };

       use crate::report::Silent;
       use crate::workspace::Workspace;
   ```

   Add before the first test of the `tests` module of `crates/buildl/src/workspace.rs`:

   ```rust
       /// A whole build of `workspace`, stopping at the first failure, shown to nobody.
       fn build(workspace: &Workspace) -> Result<Vec<(String, OutcomeClass)>> {
           let outcomes = workspace.build(
               &Scope::Workspace,
               &BTreeMap::new(),
               OnFailure::Stop,
               LiveOutput::Off,
               &mut Silent,
           )?;
           Ok(outcomes
               .into_iter()
               .map(|outcome| (outcome.label.to_string(), outcome.class))
               .collect())
       }

       /// Keeps what a build tells its reporter.
       #[derive(Debug, Default)]
       struct Told {
           begun: Vec<usize>,
           started: Vec<String>,
           finished: Vec<String>,
           summaries: Vec<Summary>,
       }

       impl Reporter for Told {
           fn begun(&mut self, targets: usize) {
               self.begun.push(targets);
           }

           fn started(&mut self, action: &ReadyAction) {
               self.started.push(action.label.to_string());
           }

           fn printed(&mut self, _label: &Label, _channel: Channel, _bytes: &[u8]) {}

           fn finished(&mut self, event: &OutcomeEvent, _printed: Captured<'_>) {
               self.finished.push(event.outcome.label.to_string());
           }

           fn summary(&mut self, summary: Summary) {
               self.summaries.push(summary);
           }
       }
   ```

   Then add at the end of the `tests` module of `crates/buildl/src/workspace.rs`, before its closing brace:

   ```rust
       #[test]
       fn a_build_runs_its_actions_and_leaves_the_graph_it_resolved() {
           let (_dir, root) = copying();
           let workspace = Workspace::open(&root).unwrap();

           let outcomes = build(&workspace).unwrap();

           assert_eq!(outcomes, [("//:copy".to_owned(), OutcomeClass::Built)]);
           assert_eq!(fs::read(root.join("out/copy")).unwrap(), b"a\n");
           assert_eq!(
               fs::read(root.join(".buildl/graph.json")).unwrap(),
               canonical::to_vec(&workspace.graph().unwrap()).unwrap()
           );
           // The directory was let go when the build returned.
           workspace.wiring.lock().unwrap();
       }

       #[test]
       fn a_named_target_is_all_a_build_runs_and_the_graph_it_leaves_is_whole() {
           let (_dir, root) = workspace(&[
               ("buildl.toml", BARE),
               (
                   "build.lua",
                   "buildl.target(\"copy\", { inputs = { \"a.txt\" }, run = { \"cp\", \"$in\", \"$out\" } })\n\
                    buildl.target(\"other\", { inputs = { \"a.txt\" }, run = { \"cp\", \"$in\", \"$out\" } })\n",
               ),
               ("a.txt", "a\n"),
           ]);
           let workspace = Workspace::open(&root).unwrap();
           let scope = Scope::Targets(BTreeSet::from([workspace.label("copy").unwrap()]));

           let outcomes = workspace
               .build(
                   &scope,
                   &BTreeMap::new(),
                   OnFailure::Stop,
                   LiveOutput::Off,
                   &mut Silent,
               )
               .unwrap();

           assert_eq!(outcomes.len(), 1);
           assert_eq!(names(&root.join("out")), ["copy"]);
           assert_eq!(
               fs::read(root.join(".buildl/graph.json")).unwrap(),
               canonical::to_vec(&workspace.graph().unwrap()).unwrap()
           );
       }

       #[test]
       fn the_reporter_a_build_is_lent_holds_every_call_once_the_build_returns() {
           let (_dir, root) = copying();
           let workspace = Workspace::open(&root).unwrap();
           let mut told = Told::default();

           for _ in 0..2 {
               workspace
                   .build(
                       &Scope::Workspace,
                       &BTreeMap::new(),
                       OnFailure::Stop,
                       LiveOutput::Off,
                       &mut told,
                   )
                   .unwrap();
           }

           assert_eq!(told.begun, [1, 1]);
           // Started once: the second build found the target cached and ran nothing.
           assert_eq!(told.started, ["//:copy"]);
           assert_eq!(told.finished, ["//:copy", "//:copy"]);
           assert_eq!(
               told.summaries,
               [
                   Summary {
                       failed: 0,
                       skipped: 0,
                       built: 1,
                       cached: 0
                   },
                   Summary {
                       failed: 0,
                       skipped: 0,
                       built: 0,
                       cached: 1
                   }
               ]
           );
       }

       #[test]
       fn build_is_refused_while_another_buildl_holds_the_workspace() {
           let (_dir, root) = copying();
           let workspace = Workspace::open(&root).unwrap();
           let _held = workspace.wiring.lock().unwrap();

           let error = build(&workspace).unwrap_err();

           assert!(
               matches!(
                   &error,
                   Error::Adapter {
                       port: PortName::ActionCache,
                       ..
                   }
               ),
               "{error:?}"
           );
           assert!(!root.join("out").exists());
       }
   ```

2. Run them and see them fail:

   ```
   $ cargo test -p buildl --lib workspace::
   ```

   Expected: the test module does not build.

   ```text
   error[E0599]: no method named `build` found for struct `workspace::Workspace` in the current scope
   ```

3. Write `build`. Three replacements in `crates/buildl/src/workspace.rs`.

   In `crates/buildl/src/workspace.rs`, replace

   ```rust
   use buildl_core::{
       Declaration, Directory, Label, Pipeline, Plan, Result, Scope, SettingName, SettingValue,
       TargetGraph, pipeline,
   };
   ```

   with

   ```rust
   use buildl_core::{
       ActionOutcome, Declaration, Directory, Label, LiveOutput, OnFailure, Pipeline, Plan, Reporter,
       Result, Scope, SettingName, SettingValue, TargetGraph, pipeline,
   };
   ```

   Two commands now take the state directory, so the type says when. In `crates/buildl/src/workspace.rs`, replace

   ```rust
   /// A workspace wired with the real adapters.
   ///
   /// Opening one reads its manifest and nothing else.
   ```

   with

   ```rust
   /// A workspace wired with the real adapters.
   ///
   /// Opening one reads its manifest and nothing else. The state directory is taken by
   /// [`Workspace::plan`] and [`Workspace::build`] only, and only while one of them runs, so a
   /// workspace kept open holds nothing while it is idle.
   ```

   In `crates/buildl/src/workspace.rs`, replace

   ```rust
       }
   }

   #[cfg(test)]
   ```

   with

   ```rust
       }

       /// Builds `scope`, telling `reporter` how it goes.
       ///
       /// Holds the state directory while it runs. The reporter is the caller's: it is lent for
       /// this one build, and holds every call once this returns.
       ///
       /// # Errors
       ///
       /// As [`Workspace::plan`], then whatever [`Pipeline::build`] returns.
       pub fn build<R: Reporter>(
           &self,
           scope: &Scope,
           overrides: &BTreeMap<SettingName, SettingValue>,
           on_failure: OnFailure,
           live: LiveOutput,
           reporter: &mut R,
       ) -> Result<Vec<ActionOutcome>> {
           let lock = self.wiring.lock()?;
           let mut pipeline = Pipeline::new(self.wiring.ports(&lock, reporter)?);
           pipeline.build(self.wiring.entry(), scope, overrides, on_failure, live)
       }
   }

   #[cfg(test)]
   ```

4. Run the tests and see them pass:

   ```
   $ cargo test -p buildl --lib workspace::
   ```

   Expected:

   ```text
   test workspace::tests::a_build_runs_its_actions_and_leaves_the_graph_it_resolved ... ok
   test workspace::tests::a_named_target_is_all_a_build_runs_and_the_graph_it_leaves_is_whole ... ok
   test workspace::tests::build_is_refused_while_another_buildl_holds_the_workspace ... ok
   test workspace::tests::the_reporter_a_build_is_lent_holds_every_call_once_the_build_returns ... ok
   test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 224 filtered out
   ```

5. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo make deny
   ```

   Expected: all three exit `0`, and among the lines printed:

   ```text
   guard-adapter-isolation: OK (25 edges)
   test result: ok. 239 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   advisories ok, bans ok, licenses ok, sources ok
   ```

6. Commit:

   ```
   $ git add crates/buildl/src/workspace.rs
   $ git commit -m "feat(buildl): build under the lock, with the caller's reporter"
   ```

### Task 10 — Fold the rows of every build, whatever it returned

**Files:**
- Modify `crates/buildl/src/workspace.rs`

**Steps:**

1. Write the failing tests. In `crates/buildl/src/workspace.rs`, replace

   ```rust
       use buildl_core::{
           Captured, Channel, Error, Label, LiveOutput, NameKind, OnFailure, OutcomeClass,
           OutcomeEvent, PortName, ReadyAction, Reporter, Result, Scope, Summary, Verdict, Want,
       };

       use crate::report::Silent;
       use crate::workspace::Workspace;
   ```

   with

   ```rust
       use buildl_core::{
           Captured, Channel, Diagnostic, Error, Label, LiveOutput, NameKind, OnFailure, OutcomeClass,
           OutcomeEvent, PortName, ReadyAction, Reporter, Result, Scope, Summary, Verdict, Want,
       };

       use crate::report::Silent;
       use crate::workspace::{Workspace, concluded};
   ```

   Add before the first test of the `tests` module of `crates/buildl/src/workspace.rs`:

   ```rust
       /// An error no adapter makes, told apart by its text.
       fn failure(text: &str) -> Error {
           Error::Adapter {
               port: PortName::ActionCache,
               diagnostic: Diagnostic::new(text),
           }
       }
   ```

   Then add at the end of the `tests` module of `crates/buildl/src/workspace.rs`, before its closing brace:

   ```rust
       #[test]
       fn concluded_returns_the_builds_error_before_the_folds() {
           let both: Result<()> = concluded(Err(failure("the build")), Err(failure("the fold")));
           assert_eq!(both.unwrap_err().to_string(), "action cache: the build");

           let one: Result<()> = concluded(Err(failure("the build")), Ok(()));
           assert_eq!(one.unwrap_err().to_string(), "action cache: the build");
       }

       #[test]
       fn concluded_returns_the_folds_error_for_a_build_that_succeeded() {
           let folded = concluded(Ok(7), Err(failure("the fold")));
           assert_eq!(folded.unwrap_err().to_string(), "action cache: the fold");
       }

       #[test]
       fn concluded_passes_the_value_on_when_both_succeeded() {
           assert_eq!(concluded(Ok(7), Ok(())).unwrap(), 7);
       }

       #[test]
       fn a_build_folds_its_rows_into_the_snapshot_and_removes_the_journal() {
           let (_dir, root) = copying();
           let workspace = Workspace::open(&root).unwrap();

           build(&workspace).unwrap();

           assert_eq!(
               kept(&root),
               ["cache.json", "cas", "graph.json", "lock", "log.jsonl"]
           );
           assert!(
               fs::read_to_string(root.join(".buildl/cache.json"))
                   .unwrap()
                   .starts_with("{\"//:copy\":{")
           );
       }

       #[test]
       fn a_build_finds_the_rows_the_build_before_it_folded() {
           let (_dir, root) = copying();
           let workspace = Workspace::open(&root).unwrap();
           build(&workspace).unwrap();
           let snapshot = fs::read(root.join(".buildl/cache.json")).unwrap();

           let again = build(&workspace).unwrap();

           assert_eq!(again, [("//:copy".to_owned(), OutcomeClass::Cached)]);
           assert_eq!(fs::read(root.join(".buildl/cache.json")).unwrap(), snapshot);
       }

       #[test]
       fn a_build_with_a_failed_action_folds_the_rows_of_what_was_built() {
           let (_dir, root) = workspace(&[
               ("buildl.toml", BARE),
               (
                   "build.lua",
                   "buildl.target(\"a-good\", { inputs = { \"a.txt\" }, run = { \"cp\", \"$in\", \"$out\" } })\n\
                    buildl.target(\"bad\", { run = { \"false\" } })\n",
               ),
               ("a.txt", "a\n"),
           ]);
           let workspace = Workspace::open(&root).unwrap();

           let outcomes = build(&workspace).unwrap();

           assert_eq!(outcomes[0], ("//:a-good".to_owned(), OutcomeClass::Built));
           assert_eq!(outcomes[1].0, "//:bad");
           assert!(
               matches!(outcomes[1].1, OutcomeClass::ActionFailure { .. }),
               "{:?}",
               outcomes[1].1
           );
           let state = root.join(".buildl");
           assert!(!state.join("cache.journal").exists());
           let snapshot = fs::read_to_string(state.join("cache.json")).unwrap();
           assert!(snapshot.starts_with("{\"//:a-good\":{"), "{snapshot}");
           assert!(!snapshot.contains("//:bad"), "{snapshot}");
       }

       #[test]
       fn a_build_that_returns_an_error_folds_the_rows_recorded_before_it() {
           let (_dir, root) = copying();
           // A log that cannot be appended to: the build stops once its first target is recorded.
           fs::create_dir_all(root.join(".buildl/log.jsonl")).unwrap();
           let workspace = Workspace::open(&root).unwrap();

           let error = build(&workspace).unwrap_err();

           assert!(
               matches!(
                   &error,
                   Error::Adapter {
                       port: PortName::EventLog,
                       ..
                   }
               ),
               "{error:?}"
           );
           let state = root.join(".buildl");
           assert!(!state.join("cache.journal").exists());
           assert!(
               fs::read_to_string(state.join("cache.json"))
                   .unwrap()
                   .starts_with("{\"//:copy\":{")
           );
           // The directory was let go although the build failed.
           workspace.wiring.lock().unwrap();
       }
   ```

2. Run them and see them fail:

   ```
   $ cargo test -p buildl --lib workspace::
   ```

   Expected: the test module does not build.

   ```text
   error[E0432]: unresolved import `crate::workspace::concluded`
   ```

3. Fold after the build, and choose between the two results. Three replacements in `crates/buildl/src/workspace.rs`.

   In `crates/buildl/src/workspace.rs`, replace

   ```rust
       /// Holds the state directory while it runs. The reporter is the caller's: it is lent for
       /// this one build, and holds every call once this returns.
       ///
       /// # Errors
       ///
       /// As [`Workspace::plan`], then whatever [`Pipeline::build`] returns.
   ```

   with

   ```rust
       /// Holds the state directory while it runs. Whatever the pipeline returns, the rows the
       /// build recorded are folded into the action cache before the directory is let go. The
       /// reporter is the caller's: it is lent for this one build, and holds every call once this
       /// returns.
       ///
       /// # Errors
       ///
       /// As [`Workspace::plan`], then whatever [`Pipeline::build`] returns. A build that
       /// succeeded and whose rows could not be folded returns the fold's error.
   ```

   In `crates/buildl/src/workspace.rs`, replace

   ```rust
           let mut pipeline = Pipeline::new(self.wiring.ports(&lock, reporter)?);
           pipeline.build(self.wiring.entry(), scope, overrides, on_failure, live)
       }
   ```

   with

   ```rust
           let mut pipeline = Pipeline::new(self.wiring.ports(&lock, reporter)?);
           let built = pipeline.build(self.wiring.entry(), scope, overrides, on_failure, live);
           let folded = pipeline.ports_mut().cache.fold();
           // The adapters are let go before the lock that kept the directory theirs.
           drop(pipeline);
           concluded(built, folded)
       }
   ```

   In `crates/buildl/src/workspace.rs`, replace

   ```rust
   }

   #[cfg(test)]
   ```

   with

   ```rust
   }

   /// What a build returns once its rows were folded: its own error first, then the fold's.
   fn concluded<T>(built: Result<T>, folded: Result<()>) -> Result<T> {
       match (built, folded) {
           (Err(error), _) | (Ok(_), Err(error)) => Err(error),
           (Ok(value), Ok(())) => Ok(value),
       }
   }

   #[cfg(test)]
   ```

4. Run the tests and see them pass:

   ```
   $ cargo test -p buildl --lib workspace::
   ```

   Expected:

   ```text
   test workspace::tests::a_build_finds_the_rows_the_build_before_it_folded ... ok
   test workspace::tests::a_build_folds_its_rows_into_the_snapshot_and_removes_the_journal ... ok
   test workspace::tests::a_build_that_returns_an_error_folds_the_rows_recorded_before_it ... ok
   test workspace::tests::a_build_with_a_failed_action_folds_the_rows_of_what_was_built ... ok
   test workspace::tests::concluded_passes_the_value_on_when_both_succeeded ... ok
   test workspace::tests::concluded_returns_the_builds_error_before_the_folds ... ok
   test workspace::tests::concluded_returns_the_folds_error_for_a_build_that_succeeded ... ok
   test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 224 filtered out
   ```

5. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo make deny
   ```

   Expected: all three exit `0`, and among the lines printed:

   ```text
   guard-adapter-isolation: OK (25 edges)
   test result: ok. 246 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   advisories ok, bans ok, licenses ok, sources ok
   ```

6. Commit:

   ```
   $ git add crates/buildl/src/workspace.rs
   $ git commit -m "feat(buildl): fold the rows of every build, whatever it returned"
   ```

### Task 11 — Refuse to build under a strategy that cannot be run

**Files:**
- Modify `crates/buildl/src/workspace.rs`

**Steps:**

1. Write the failing test. Add at the end of the `tests` module of `crates/buildl/src/workspace.rs`, before its closing brace:

   ```rust
       #[test]
       fn build_refuses_a_strategy_it_cannot_run_before_anything_is_made() {
           let manifest = BARE.replace("strategy = \"bare\"\n", "");
           let (_dir, root) = workspace(&[
               ("buildl.toml", &manifest),
               ("build.lua", COPY),
               ("a.txt", "a\n"),
           ]);
           let workspace = Workspace::open(&root).unwrap();

           let error = build(&workspace).unwrap_err();

           assert!(
               matches!(
                   &error,
                   Error::Adapter {
                       port: PortName::Manifest,
                       ..
                   }
               ),
               "{error:?}"
           );
           assert_eq!(
               error.to_string(),
               "manifest: buildl.toml: [execution] strategy is \"sandbox\" (the default when it is \
                not written), and this version of buildl runs \"bare\" only: write strategy = \
                \"bare\" under [execution]"
           );
           // Refused before the lock was taken: no state directory, no output.
           assert_eq!(names(&root), ["a.txt", "build.lua", "buildl.toml"]);
       }
   ```

2. Run it and see it fail:

   ```
   $ cargo test -p buildl --lib workspace::
   ```

   Expected: the new test fails, because the build ran as plain processes although the manifest asks for the sandbox.

   ```text
   test workspace::tests::build_refuses_a_strategy_it_cannot_run_before_anything_is_made ... FAILED
   called `Result::unwrap_err()` on an `Ok` value: [("//:copy", Built)]
   test result: FAILED. 22 passed; 1 failed; 0 ignored; 0 measured; 224 filtered out
   ```

3. Ask the wiring first. Two replacements in `crates/buildl/src/workspace.rs`.

   In `crates/buildl/src/workspace.rs`, replace

   ```rust
       /// As [`Workspace::plan`], then whatever [`Pipeline::build`] returns. A build that
       /// succeeded and whose rows could not be folded returns the fold's error.
   ```

   with

   ```rust
       /// Returns [`Error::Adapter`](buildl_core::Error::Adapter), naming
       /// [`PortName::Manifest`](buildl_core::PortName::Manifest), when the workspace asks for a
       /// strategy other than `bare`: nothing has been made then, and nothing was run. After that,
       /// as [`Workspace::plan`], then whatever [`Pipeline::build`] returns. A build that
       /// succeeded and whose rows could not be folded returns the fold's error.
   ```

   In `crates/buildl/src/workspace.rs`, replace

   ```rust
           let lock = self.wiring.lock()?;
           let mut pipeline = Pipeline::new(self.wiring.ports(&lock, reporter)?);
   ```

   with

   ```rust
           self.wiring.runs()?;
           let lock = self.wiring.lock()?;
           let mut pipeline = Pipeline::new(self.wiring.ports(&lock, reporter)?);
   ```

4. Run the tests and see them pass:

   ```
   $ cargo test -p buildl --lib workspace::
   ```

   Expected:

   ```text
   test workspace::tests::build_refuses_a_strategy_it_cannot_run_before_anything_is_made ... ok
   test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 224 filtered out
   ```

5. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo make deny
   ```

   Expected: all three exit `0`, and among the lines printed:

   ```text
   guard-adapter-isolation: OK (25 edges)
   test result: ok. 247 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   advisories ok, bans ok, licenses ok, sources ok
   ```

6. Commit:

   ```
   $ git add crates/buildl/src/workspace.rs
   $ git commit -m "feat(buildl): refuse to build under a strategy that cannot be run"
   ```

### Task 12 — Re-export the core and document what a caller uses

**Files:**
- Modify `crates/buildl/src/lib.rs`

**Steps:**

1. Write the failing example. It is the crate's own documentation: a caller that depends on `buildl` alone opens a workspace, checks it and builds it twice. Two replacements in `crates/buildl/src/lib.rs`; the first block holds a fenced example, so it is shown inside four backticks.

   In `crates/buildl/src/lib.rs`, replace

   ```rust
   //! This crate is the composition root: its role is to choose a concrete
   //! implementation for every port the `buildl-core` pipeline needs.
   //!
   //! # Where things live
   ```

   with

   ````rust
   //! This crate is the composition root: its role is to choose a concrete
   //! implementation for every port the `buildl-core` pipeline needs, and to offer the result as
   //! one type, [`Workspace`].
   //!
   //! # Using it
   //!
   //! A caller opens a [`Workspace`] and calls a command on it: [`Workspace::check`],
   //! [`Workspace::graph`], [`Workspace::plan`] or [`Workspace::build`]. It names no adapter. A
   //! build is lent the caller's own [`Reporter`]: [`report::TerminalReporter`] writes what the
   //! `buildl` binary shows, and [`report::Silent`] shows nothing. The values a command takes and
   //! returns are `buildl-core`'s, re-exported here as [`buildl_core`], so a caller depends on this
   //! crate alone.
   //!
   //! ```
   //! use std::collections::BTreeMap;
   //! use std::fs;
   //!
   //! use buildl::Workspace;
   //! use buildl::buildl_core::{LiveOutput, OnFailure, OutcomeClass, Scope};
   //! use buildl::report::Silent;
   //!
   //! let dir = tempfile::tempdir()?;
   //! fs::write(
   //!     dir.path().join("buildl.toml"),
   //!     r#"
   //! [ceiling]
   //! proc.run = ["cp"]
   //! fs.read  = ["."]
   //! fs.write = ["out", ".buildl"]
   //!
   //! [execution]
   //! strategy = "bare"
   //! "#,
   //! )?;
   //! fs::write(
   //!     dir.path().join("build.lua"),
   //!     r#"buildl.target("copy", { inputs = { "a.txt" }, run = { "cp", "$in", "$out" } })"#,
   //! )?;
   //! fs::write(dir.path().join("a.txt"), "a\n")?;
   //!
   //! let workspace = Workspace::open(dir.path())?;
   //! assert_eq!(workspace.check()?.len(), 1);
   //!
   //! let (everything, settings) = (Scope::Workspace, BTreeMap::new());
   //! let built = workspace.build(
   //!     &everything,
   //!     &settings,
   //!     OnFailure::Stop,
   //!     LiveOutput::Off,
   //!     &mut Silent,
   //! )?;
   //! assert_eq!(built[0].class, OutcomeClass::Built);
   //! assert_eq!(fs::read(workspace.root().join("out/copy"))?, b"a\n");
   //!
   //! let again = workspace.build(
   //!     &everything,
   //!     &settings,
   //!     OnFailure::Stop,
   //!     LiveOutput::Off,
   //!     &mut Silent,
   //! )?;
   //! assert_eq!(again[0].class, OutcomeClass::Cached);
   //! # Ok::<(), Box<dyn std::error::Error>>(())
   //! ```
   //!
   //! Actions run as plain processes in the workspace, the strategy a manifest names `bare`. A
   //! build of a workspace that asks for any other strategy is refused, never run under less
   //! isolation than its manifest asked for.
   //!
   //! # Where things live
   ````

   In `crates/buildl/src/lib.rs`, replace

   ```rust
   //! | [`search_path`] | the rule that leads from a program's name to a file |
   //!
   //! # Non-responsibilities
   //!
   //! - Pipeline logic, which lives in `buildl-core`.
   //! - Lua evaluation, which lives in `buildl-lua`.
   //!
   //! This file holds only module declarations, so it carries no logic to unit-test.
   ```

   with

   ```rust
   //! | [`search_path`] | the rule that leads from a program's name to a file |
   //!
   //! The adapters meet in two modules and nowhere else. Neither is public: what they hold is
   //! exported from the crate root.
   //!
   //! | Module | Holds |
   //! |---|---|
   //! | `local_ports` | [`LocalPorts`], the choice of an adapter for every port; [`LocalPipeline`], the pipeline over that choice; and [`Wiring`], which makes the adapters from a workspace's manifest |
   //! | `workspace` | [`Workspace`], the facade: one method per command |
   //!
   //! # Platforms
   //!
   //! The crate builds on Unix targets only, and is tested on Linux and macOS. File modes and the
   //! signal that ended a process are read through `std::os::unix`.
   //!
   //! # Non-responsibilities
   //!
   //! - Pipeline logic, which lives in `buildl-core`.
   //! - Lua evaluation, which lives in `buildl-lua`.
   //! - Parsing a command line and choosing an exit code, which the `buildl-cli` crate does.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to
   //! unit-test.
   ```

2. Run the documentation examples and see the new one fail:

   ```
   $ cargo test -p buildl --doc
   ```

   Expected: the example of the crate's own page does not build, because the core is not reachable through this crate yet.

   ```text
   error[E0432]: unresolved import `buildl::buildl_core`
   test crates/buildl/src/lib.rs - (line 22) ... FAILED
   test result: FAILED. 7 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
   ```

3. Re-export the core under the crate's own name. In `crates/buildl/src/lib.rs`, replace

   ```rust
   mod local_ports;
   mod workspace;

   pub use local_ports::{LocalPipeline, LocalPorts, Wiring};
   ```

   with

   ```rust
   mod local_ports;
   mod workspace;

   pub use buildl_core;
   pub use local_ports::{LocalPipeline, LocalPorts, Wiring};
   ```

4. Run the documentation examples and see them pass:

   ```
   $ cargo test -p buildl --doc
   ```

   Expected:

   ```text
   test crates/buildl/src/lib.rs - (line 22) ... ok
   test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   ```

5. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo make deny
   ```

   Expected: all three exit `0`, and among the lines printed:

   ```text
   guard-adapter-isolation: OK (25 edges)
   test result: ok. 247 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   advisories ok, bans ok, licenses ok, sources ok
   ```

6. Commit:

   ```
   $ git add crates/buildl/src/lib.rs
   $ git commit -m "feat(buildl): re-export buildl-core and document what a caller uses"
   ```

---

## Verification summary (plan-level)

- **The gate.** `cargo fmt --all -- --check`, `cargo make dod` and `cargo make deny` exit `0` after every task. After task 12 `cargo make dod` reports, for `buildl`, `247 passed` unit tests and `8 passed` documentation examples, and prints `guard-adapter-isolation: OK (25 edges)`; the other crates' tests are untouched.
- **The edges.** The 25 are: eight from adapters to the two shared modules, as before this plan (`cache`, `cas`, `exec`, `graph`, `log`, `statcache` to `state`; `exec`, `tools` to `search_path`); fifteen from `local_ports` (the twelve adapters of the bundle, `state`, `search_path`, and `report` for the `Silent` its tests lend); two from `workspace` (`local_ports`, and `report` for `Silent`). No adapter module names `local_ports` or `workspace`.
- **The floor.** `cargo +1.94 check --workspace --all-targets --all-features`, the command of the CI's minimum-version job, ends `Finished` on rustc 1.94.1 with this plan applied.
- **No dependency moved.** `Cargo.toml`, `Cargo.lock`, `crates/buildl/Cargo.toml`, `deny.toml`, `Makefile.toml` and both golden files are untouched. `cargo tree -d` prints `warning: nothing to print.`
- **Every decision of the composition has a test.**

  | Decision | Test |
  |---|---|
  | the manifest is read first, and opening makes nothing | `reading_finds_the_root_above_the_start_and_makes_nothing`, `a_manifest_that_is_refused_fails_the_reading_whatever_else_the_workspace_holds` |
  | build files are evaluated under the manifest's ceilings, out of the output and state directories | `the_source_takes_each_ceiling_the_manifest_sets_and_the_default_for_the_rest` |
  | only `bare` can be run, and the refusal's text | `only_bare_can_be_run` |
  | the state directory is held to the ceiling before the lock makes it | `a_state_directory_outside_the_ceiling_is_refused`, `a_state_directory_inside_an_allowed_directory_is_locked_and_made`, `plan_is_refused_before_the_state_directory_is_made_when_the_ceiling_leaves_it_out` |
  | a busy lock, and a state directory that cannot be made, are the action cache's adapter error | `a_busy_lock_is_the_action_caches_adapter_error`, `a_state_directory_that_cannot_be_made_is_the_action_caches_adapter_error` |
  | every port is made from the manifest, the root and the lock; the worker count and the timeout reach the dispatcher and the strategy; one search path serves two adapters | `every_port_is_made_from_the_manifest_the_root_and_the_lock` |
  | an adapter's own error passes through the wiring unchanged | `a_cache_that_is_not_canonical_stops_the_wiring_with_its_own_error` |
  | the bundle satisfies the pipeline | `a_pipeline_over_the_ports_plans_a_workspace` |
  | a target is read relative to where the workspace was opened | the four `label` tests of `workspace.rs` |
  | `check` and `graph` take no lock and open no state file | `check_and_graph_read_build_files_and_make_nothing`, `check_and_graph_are_stopped_by_neither_a_held_lock_nor_a_cache_that_is_not_canonical` |
  | `plan` and `build` take the lock, for the length of the call | `plan_takes_the_lock_and_builds_nothing`, `plan_is_refused_while_another_buildl_holds_the_workspace`, `build_is_refused_while_another_buildl_holds_the_workspace`, `a_build_runs_its_actions_and_leaves_the_graph_it_resolved` |
  | `plan` works under any strategy; `build` refuses one it cannot run, before anything is made | `plan_works_under_a_strategy_this_version_cannot_run`, `build_refuses_a_strategy_it_cannot_run_before_anything_is_made` |
  | the reporter is the caller's, lent for one build | `the_reporter_a_build_is_lent_holds_every_call_once_the_build_returns` |
  | a build leaves `graph.json`, the whole graph, also when one target was named | `a_build_runs_its_actions_and_leaves_the_graph_it_resolved`, `a_named_target_is_all_a_build_runs_and_the_graph_it_leaves_is_whole` |
  | the journal is folded after every build: one that succeeded, one with a failed action, one that returned an error | `a_build_folds_its_rows_into_the_snapshot_and_removes_the_journal`, `a_build_finds_the_rows_the_build_before_it_folded`, `a_build_with_a_failed_action_folds_the_rows_of_what_was_built`, `a_build_that_returns_an_error_folds_the_rows_recorded_before_it` |
  | the build's error comes before the fold's | the three `concluded` tests |
  | what a caller that depends on `buildl` alone can do | the example of the crate's own page, run by `cargo test -p buildl --doc` |

- **The names the design gives.** `only_bare_can_be_run`, `a_state_directory_outside_the_ceiling_is_refused` and `a_busy_lock_is_the_action_caches_adapter_error` are in `local_ports.rs`; `a_relative_target_is_read_against_the_directory_the_workspace_was_opened_from`, `a_full_label_is_read_the_same_from_any_directory`, `a_start_directory_outside_the_directory_grammar_fails_only_the_relative_forms`, `a_target_that_fails_the_label_grammar_is_refused`, `concluded_returns_the_builds_error_before_the_folds`, `concluded_returns_the_folds_error_for_a_build_that_succeeded` and `concluded_passes_the_value_on_when_both_succeeded` are in `workspace.rs`.
- **Steadiness.** The 34 tests of the two modules were run 30 times over while this plan was written, with no failure. None waits on a clock, and the one file whose presence depends on time, `statcache.json`, is left out of every listing a test compares.
- **A real build, beyond the tests.** While this plan was written, a throwaway program opened two larger workspaces through `Workspace` and ran `check`, `graph`, `plan`, `build`, `build` again and `plan` again: one of six targets (a rule, a setting, an alias, two directory outputs, an executable output that a later target runs) and one of three targets whose outputs hold the same bytes under two modes. Both built every target, the second build of each reported every target cached, and the second plan reported every target clean.
- **Linux.** Nothing here was run on Linux. The tests name no platform, compare no text the system writes, and use only `cp` and `false`; the `ubuntu-latest` leg of `cargo make dod` is their first run there. The two that lean on the filesystem are `a_state_directory_that_cannot_be_made_is_the_action_caches_adapter_error`, where a regular file stands where the state directory belongs, and `a_build_that_returns_an_error_folds_the_rows_recorded_before_it`, where a directory stands where the log belongs.
- **Disjoint files.** The plan writes `crates/buildl/src/local_ports.rs`, `crates/buildl/src/workspace.rs` and `crates/buildl/src/lib.rs`, and no file inside an adapter module.
