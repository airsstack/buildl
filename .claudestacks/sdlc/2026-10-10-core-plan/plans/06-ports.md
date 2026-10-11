---
status: approved
created: 2026-10-11
depends-on: [02, 03, 05]
---

# Ports Implementation Plan

**Goal:** A pipeline is built from one set holding every port a plan reads.

**Architecture:** Six new traits under `ports/`, one file each: `Manifest`, `Host`, `Digester`, `StatCache`, `ToolResolver` and `ActionCache`. `Host` is not in the documents' port catalog: the environment and the platform are facts about the machine actions run on, and no existing port reports them. `ActionCache` has `row` alone, because this chain only reads rows. The `Ports` bundle gains one associated type per new port. `PortSet<P>` is the bundle's value-level twin, a struct with one public field per port: `Pipeline::new` takes it in place of the build-file source, because clippy refuses a function of eight parameters and the pipeline will hold more ports yet. `Pipeline` gains `ports` and `ports_mut` so that whoever built the adapters can reach them between two commands. Every fake lives in the flows' shared file and is built from the public API alone; the crate gains no implementation of any port.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, `serde`, `serde_json`, `sha2` and `thiserror` (existing), `cargo-make`. No new dependency.

**Content authority:** spec §2 (D10, D11, D12, D13), §4 (ports), §6 (`Pipeline::new`, `ports`, `ports_mut`), §9 (the fakes).

**Checkpoints:** review once, when all five tasks are done.

---

## Context an implementer needs

What plans `01` to `03` and `05` and the existing crate leave that this plan builds on:

| Item | Where | Used as |
|---|---|---|
| `Workspace`, `Ceiling` | `crates/buildl-core/src/types/` | what `Manifest::workspace` returns |
| `Platform`, `EnvName`, `EnvValue` | `crates/buildl-core/src/types/` | `Host`'s signatures |
| `SourcePath`, `FileStat`, `StatEntry`, `Digest` | `crates/buildl-core/src/types/` | `Digester`'s and `StatCache`'s signatures |
| `Program`, `CacheRow`, `Label` | `crates/buildl-core/src/types/` | `ToolResolver`'s and `ActionCache`'s signatures |
| `Error::Adapter { port, diagnostic }`, `PortName` (plan `05`) | `crates/buildl-core/src/error.rs` | named in the traits' docs, and returned by the fake host |
| `Ports` with one associated type, `Source`; its doc example | `crates/buildl-core/src/ports/bundle.rs` | the trait that gains six associated types |
| `Pipeline { source }`, `Pipeline::new(source)`, `check`, `graph` | `crates/buildl-core/src/pipeline/driver.rs` | the struct rebuilt around a `PortSet` |
| `FakeSource`, `FakePorts`, `dir`, `file`, `workspace` and the staged-item builders | `crates/buildl-core/tests/flows/common.rs` | the file the fakes join |
| Five `Pipeline::new` call sites | `crates/buildl-core/tests/flows/check.rs` (four), `graph.rs` (one) | the callers that switch to a `PortSet` |

Gate facts that bite here:

- The six trait files hold no logic, so tasks 1 to 3 have no failing-test step; the gate's build
  and rustdoc steps check them, and task 4's fakes are what prove each trait can be implemented
  from outside the crate.
- `ports -> error` and `ports -> types` are already in `crates/expected-module-edges.txt`, so
  this plan changes no module edge.
- Extending `Ports` breaks every implementation of it at once: `FakePorts` in the flows, and the
  doc example in `bundle.rs`. Task 4 changes all three in one commit.
- `#[derive(Debug)]` on `PortSet<P>` bounds each field's type, so it needs no hand-written
  `impl`; `Pipeline`'s hand-written `Debug` is bounded on `PortSet<P>: Debug`.

All work happens in the worktree, on its branch, never on `main`. Commits follow Conventional Commits, one per task. Every Rust block below is already in rustfmt's form. Each quoted test run, the failing one and the passing one, was captured by running that command on the state its step describes while this code was prototyped. The gate was run on the tree each plan leaves, and after every task from plan `06` task 4 on; a gate step states what is expected and was not captured separately for each earlier task. The `filtered out` figure in a quoted test result depends on which other plans have landed before this one; the `passed` figure does not.

## File structure

```
crates/buildl-core/src/ports/manifest.rs        — [create] Manifest
crates/buildl-core/src/ports/host.rs            — [create] Host
crates/buildl-core/src/ports/digester.rs        — [create] Digester
crates/buildl-core/src/ports/stat_cache.rs      — [create] StatCache
crates/buildl-core/src/ports/tool_resolver.rs   — [create] ToolResolver
crates/buildl-core/src/ports/action_cache.rs    — [create] ActionCache
crates/buildl-core/src/ports/port_set.rs        — [create] PortSet
crates/buildl-core/src/ports/bundle.rs          — [modify] six associated types; the doc example
crates/buildl-core/src/ports/mod.rs             — [modify] declare and re-export the seven modules
crates/buildl-core/src/lib.rs                   — [modify] re-export the six traits and PortSet
crates/buildl-core/src/pipeline/driver.rs       — [modify] hold a PortSet; new, ports, ports_mut
crates/buildl-core/tests/flows/common.rs        — [modify] six fakes, FakePorts, a PortSet builder
crates/buildl-core/tests/flows/check.rs         — [modify] four Pipeline::new call sites
crates/buildl-core/tests/flows/graph.rs         — [modify] one Pipeline::new call site
```

### Task 1 — Add the manifest and host ports

**Files:**
- Create `crates/buildl-core/src/ports/manifest.rs`
- Create `crates/buildl-core/src/ports/host.rs`
- Modify `crates/buildl-core/src/ports/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Create `crates/buildl-core/src/ports/manifest.rs`:

   ```rust
   //! The port through which Plan learns what a workspace's manifest says.
   //!
   //! Its own file because the manifest is a file with a format, and this crate reads no file and
   //! knows no format: what it needs from one arrives as a value.
   //!
   //! Responsibilities: [`Manifest`].
   //!
   //! Non-responsibilities: implementing it. Finding, reading and parsing a manifest belong to an
   //! adapter crate.
   //!
   //! This file holds only a trait definition, so it carries no logic to unit-test.

   use crate::error::Result;
   use crate::types::Workspace;

   /// Reports what a workspace's manifest says.
   pub trait Manifest {
       /// The workspace's directories and its ceiling.
       ///
       /// # Errors
       ///
       /// Returns [`Error::Adapter`](crate::Error::Adapter), naming
       /// [`PortName::Manifest`](crate::PortName::Manifest), when the manifest cannot be read or is
       /// not valid.
       fn workspace(&self) -> Result<Workspace>;
   }
   ```

2. Create `crates/buildl-core/src/ports/host.rs`:

   ```rust
   //! The port through which Plan learns about the machine actions run on.
   //!
   //! Its own file because the platform and the environment are ambient facts, and this crate
   //! reads neither: an action's key depends on both, so both arrive through one port that names
   //! where they come from.
   //!
   //! Responsibilities: [`Host`].
   //!
   //! Non-responsibilities: implementing it, and deciding which variables may be read. The first
   //! belongs to an adapter crate; the second is decided before this port is asked anything.
   //!
   //! This file holds only a trait definition, so it carries no logic to unit-test.

   use crate::error::Result;
   use crate::types::{EnvName, EnvValue, Platform};

   /// Reports the platform actions run on, and the value of an environment variable.
   pub trait Host {
       /// The operating system and architecture actions run on.
       fn platform(&self) -> Platform;

       /// The value of the variable `name`; `Ok(None)` when it is unset.
       ///
       /// # Errors
       ///
       /// Returns [`Error::Adapter`](crate::Error::Adapter), naming
       /// [`PortName::Host`](crate::PortName::Host), when the variable holds a value an
       /// [`EnvValue`] cannot.
       fn env(&self, name: &EnvName) -> Result<Option<EnvValue>>;
   }
   ```

3. Declare both modules. In `crates/buildl-core/src/ports/mod.rs`, add `pub mod manifest;` and `pub mod host;` to the `pub mod` lines and `pub use manifest::Manifest;` and `pub use host::Host;` to the `pub use` lines, and add two bullets to the module doc, directly above the `Ports` bullet:

   ```rust
   //! - [`Manifest`] — what a workspace's manifest says.
   //! - [`Host`] — the platform actions run on, and the values of environment variables.
   ```

4. Re-export both. In `crates/buildl-core/src/lib.rs`, add `Manifest` and `Host` to the `pub use ports::{…}` list, then run `cargo fmt --all`, which sorts the additions and rewraps the list.

5. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`. That last line was captured on the tree the last plan leaves; no task of this chain changes a manifest.

6. Commit:

   ```
   $ git add crates/buildl-core/src/ports/manifest.rs crates/buildl-core/src/ports/host.rs crates/buildl-core/src/ports/mod.rs crates/buildl-core/src/lib.rs
   $ git commit -m "feat(buildl-core): add the Manifest and Host ports"
   ```

### Task 2 — Add the digester and stat-cache ports

**Files:**
- Create `crates/buildl-core/src/ports/digester.rs`
- Create `crates/buildl-core/src/ports/stat_cache.rs`
- Modify `crates/buildl-core/src/ports/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Create `crates/buildl-core/src/ports/digester.rs`:

   ```rust
   //! The port through which Plan reads a file's metadata and content digest.
   //!
   //! Its own file because both mean touching the filesystem, which this crate never does. The two
   //! are separate calls because they cost differently: metadata is cheap and asked of every input,
   //! a digest reads the whole file and is asked only of the inputs whose metadata changed.
   //!
   //! Responsibilities: [`Digester`].
   //!
   //! Non-responsibilities: implementing it, and deciding which files need a digest. The first
   //! belongs to an adapter crate; the second is the caller's.
   //!
   //! This file holds only a trait definition, so it carries no logic to unit-test.

   use crate::error::Result;
   use crate::types::{Digest, FileStat, SourcePath};

   /// Reads the metadata and the content digests of workspace files.
   pub trait Digester {
       /// The metadata of the file at `path`; `Ok(None)` when no file is there.
       ///
       /// # Errors
       ///
       /// Returns [`Error::Adapter`](crate::Error::Adapter), naming
       /// [`PortName::Digester`](crate::PortName::Digester), when the metadata cannot be read.
       fn stat(&self, path: &SourcePath) -> Result<Option<FileStat>>;

       /// The content digest of each file in `paths`, in the order given.
       ///
       /// Takes every path at once so that an implementation may hash them in parallel.
       ///
       /// # Errors
       ///
       /// Returns [`Error::Adapter`](crate::Error::Adapter), naming
       /// [`PortName::Digester`](crate::PortName::Digester), when a file cannot be read.
       fn digest(&self, paths: &[SourcePath]) -> Result<Vec<Digest>>;
   }
   ```

2. Create `crates/buildl-core/src/ports/stat_cache.rs`:

   ```rust
   //! The port through which Plan keeps the digests of files it has already hashed.
   //!
   //! Its own file because the record outlives a run, so it is stored somewhere, and storing is
   //! not this crate's to do. What is recorded, and when a record may be trusted, stay with the
   //! caller.
   //!
   //! Responsibilities: [`StatCache`].
   //!
   //! Non-responsibilities: implementing it, and the rule for reusing an entry. The first belongs
   //! to an adapter crate; the second is the caller's.
   //!
   //! This file holds only a trait definition, so it carries no logic to unit-test.

   use crate::error::Result;
   use crate::types::{SourcePath, StatEntry};

   /// Holds, for each file hashed before, its digest and the metadata it was hashed under.
   pub trait StatCache {
       /// What was recorded for `path`, if anything.
       fn entry(&self, path: &SourcePath) -> Option<StatEntry>;

       /// Replaces the entry for `path`, in memory.
       fn record(&mut self, path: SourcePath, entry: StatEntry);

       /// Persists every entry.
       ///
       /// # Errors
       ///
       /// Returns [`Error::Adapter`](crate::Error::Adapter), naming
       /// [`PortName::StatCache`](crate::PortName::StatCache), when the entries cannot be stored.
       fn commit(&mut self) -> Result<()>;
   }
   ```

3. Declare both modules. In `crates/buildl-core/src/ports/mod.rs`, add `pub mod digester;` and `pub mod stat_cache;` to the `pub mod` lines and `pub use digester::Digester;` and `pub use stat_cache::StatCache;` to the `pub use` lines, and add two bullets to the module doc, directly above the `Ports` bullet:

   ```rust
   //! - [`Digester`] — a file's metadata and content digest.
   //! - [`StatCache`] — the digests of files already hashed.
   ```

4. Re-export both. In `crates/buildl-core/src/lib.rs`, add `Digester` and `StatCache` to the `pub use ports::{…}` list, then run `cargo fmt --all`, which sorts the additions and rewraps the list.

5. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`. That last line was captured on the tree the last plan leaves; no task of this chain changes a manifest.

6. Commit:

   ```
   $ git add crates/buildl-core/src/ports/digester.rs crates/buildl-core/src/ports/stat_cache.rs crates/buildl-core/src/ports/mod.rs crates/buildl-core/src/lib.rs
   $ git commit -m "feat(buildl-core): add the Digester and StatCache ports"
   ```

### Task 3 — Add the tool-resolver and action-cache ports

**Files:**
- Create `crates/buildl-core/src/ports/tool_resolver.rs`
- Create `crates/buildl-core/src/ports/action_cache.rs`
- Modify `crates/buildl-core/src/ports/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Create `crates/buildl-core/src/ports/tool_resolver.rs`:

   ```rust
   //! The port through which Plan identifies the programs commands name.
   //!
   //! Its own file because finding a program means searching the machine, and what identifies one
   //! — its content, or the version it reports — is the finder's to choose. This crate needs only
   //! the answer: a value that changes when the tool does.
   //!
   //! Responsibilities: [`ToolResolver`].
   //!
   //! Non-responsibilities: implementing it, and deciding which programs may run. The first
   //! belongs to an adapter crate; the second is decided before this port is asked anything.
   //!
   //! This file holds only a trait definition, so it carries no logic to unit-test.

   use crate::error::Result;
   use crate::types::{Digest, Program};

   /// Finds programs, and reports what identifies each one.
   pub trait ToolResolver {
       /// The fingerprint of `program`; `Ok(None)` when it cannot be found.
       ///
       /// # Errors
       ///
       /// Returns [`Error::Adapter`](crate::Error::Adapter), naming
       /// [`PortName::ToolResolver`](crate::PortName::ToolResolver), when the search itself fails.
       fn fingerprint(&self, program: &Program) -> Result<Option<Digest>>;
   }
   ```

2. Create `crates/buildl-core/src/ports/action_cache.rs`:

   ```rust
   //! The port through which Plan reads what the previous build recorded.
   //!
   //! Its own file because the rows outlive a run, so they are stored somewhere, and this crate
   //! stores nothing. Reading one is all a plan needs.
   //!
   //! Responsibilities: [`ActionCache`].
   //!
   //! Non-responsibilities: implementing it, and writing rows. The first belongs to an adapter
   //! crate; a row is written only after a target has been built.
   //!
   //! This file holds only a trait definition, so it carries no logic to unit-test.

   use crate::types::{CacheRow, Label};

   /// Holds the row the last successful build of each target recorded.
   pub trait ActionCache {
       /// The row recorded for the target `label` names, if it has one.
       fn row(&self, label: &Label) -> Option<&CacheRow>;
   }
   ```

3. Declare both modules. In `crates/buildl-core/src/ports/mod.rs`, add `pub mod tool_resolver;` and `pub mod action_cache;` to the `pub mod` lines and `pub use tool_resolver::ToolResolver;` and `pub use action_cache::ActionCache;` to the `pub use` lines, and add two bullets to the module doc, directly above the `Ports` bullet:

   ```rust
   //! - [`ToolResolver`] — what identifies the program a command names.
   //! - [`ActionCache`] — what the previous build recorded for a target.
   ```

4. Re-export both. In `crates/buildl-core/src/lib.rs`, add `ToolResolver` and `ActionCache` to the `pub use ports::{…}` list, then run `cargo fmt --all`, which sorts the additions and rewraps the list.

5. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`. That last line was captured on the tree the last plan leaves; no task of this chain changes a manifest.

6. Commit:

   ```
   $ git add crates/buildl-core/src/ports/tool_resolver.rs crates/buildl-core/src/ports/action_cache.rs crates/buildl-core/src/ports/mod.rs crates/buildl-core/src/lib.rs
   $ git commit -m "feat(buildl-core): add the ToolResolver and ActionCache ports"
   ```

### Task 4 — Name every port in the bundle

**Files:**
- Modify `crates/buildl-core/tests/flows/common.rs`
- Modify `crates/buildl-core/src/ports/bundle.rs`

**Steps:**

1. Write the failing implementation. In `crates/buildl-core/tests/flows/common.rs`, widen the imports. Replace:

   ```rust
   use std::cell::{Cell, RefCell};
   use std::collections::BTreeMap;

   use buildl_core::{
       BuildFile, DeclarationOrder, DeclarationSource, Diagnostic, Directory, Error, Evaluated,
       EvaluationFailure, Freshness, NetworkAccess, Ports, Result, StagedAlias, StagedDeclaration,
       StagedFile, StagedItem, StagedRule, StagedSetting, StagedSubdir, StagedTarget, TargetRole,
       Written,
   };
   ```

   With:

   ```rust
   use std::cell::{Cell, RefCell};
   use std::collections::{BTreeMap, BTreeSet};

   use buildl_core::{
       ActionCache, BuildFile, CacheRow, DeclarationOrder, DeclarationSource, Diagnostic, Digest,
       Digester, Directory, EnvName, EnvValue, Error, Evaluated, EvaluationFailure, FileStat,
       Freshness, Host, Label, Manifest, NetworkAccess, Platform, PortName, Ports, Program, Result,
       SourcePath, StagedAlias, StagedDeclaration, StagedFile, StagedItem, StagedRule, StagedSetting,
       StagedSubdir, StagedTarget, StatCache, StatEntry, TargetRole, ToolResolver, Workspace, Written,
   };
   ```

2. In the same file, put the six fakes in front of the bundle and make the bundle choose them. Replace:

   ```rust
   /// The bundle choosing [`FakeSource`].
   #[derive(Debug)]
   pub struct FakePorts;

   impl Ports for FakePorts {
       type Source = FakeSource;
   }
   ```

   With:

   ```rust
   /// A manifest holding one workspace.
   #[derive(Debug, Clone)]
   pub struct FakeManifest {
       /// What the manifest says.
       pub workspace: Workspace,
   }

   impl Manifest for FakeManifest {
       fn workspace(&self) -> Result<Workspace> {
           Ok(self.workspace.clone())
       }
   }

   /// A machine with a fixed platform and a fixed environment.
   #[derive(Debug, Clone)]
   pub struct FakeHost {
       /// The platform it reports.
       pub platform: Platform,
       /// The variables that are set.
       pub variables: BTreeMap<EnvName, EnvValue>,
       /// The variables whose value it fails to report.
       pub unreportable: BTreeSet<EnvName>,
   }

   impl Host for FakeHost {
       fn platform(&self) -> Platform {
           self.platform.clone()
       }

       fn env(&self, name: &EnvName) -> Result<Option<EnvValue>> {
           if self.unreportable.contains(name) {
               return Err(Error::Adapter {
                   port: PortName::Host,
                   diagnostic: Diagnostic::new(format!("{name} does not hold valid text")),
               });
           }
           Ok(self.variables.get(name).cloned())
       }
   }

   /// One file a [`FakeDigester`] holds.
   #[derive(Debug, Clone)]
   pub struct FakeContent {
       /// The file's bytes.
       pub bytes: Vec<u8>,
       /// The metadata reported for it.
       pub stat: FileStat,
   }

   /// Workspace files held in memory, counting every call made to it.
   #[derive(Debug, Default)]
   pub struct FakeDigester {
       /// The files that exist.
       pub files: BTreeMap<SourcePath, FakeContent>,
       stat_calls: Cell<u32>,
       digest_calls: Cell<u32>,
       digested: RefCell<Vec<SourcePath>>,
   }

   impl FakeDigester {
       /// How many times metadata was asked for.
       #[must_use]
       pub const fn stat_calls(&self) -> u32 {
           self.stat_calls.get()
       }

       /// How many times digests were asked for.
       #[must_use]
       pub const fn digest_calls(&self) -> u32 {
           self.digest_calls.get()
       }

       /// Every path a digest was asked for, in the order asked.
       #[must_use]
       pub fn digested(&self) -> Vec<SourcePath> {
           self.digested.borrow().clone()
       }
   }

   impl Digester for FakeDigester {
       fn stat(&self, path: &SourcePath) -> Result<Option<FileStat>> {
           self.stat_calls.set(self.stat_calls.get() + 1);
           Ok(self.files.get(path).map(|file| file.stat))
       }

       fn digest(&self, paths: &[SourcePath]) -> Result<Vec<Digest>> {
           self.digest_calls.set(self.digest_calls.get() + 1);
           self.digested.borrow_mut().extend_from_slice(paths);
           Ok(paths
               .iter()
               .map(|path| {
                   self.files
                       .get(path)
                       .map_or_else(|| Digest::of(b""), |file| Digest::of(&file.bytes))
               })
               .collect())
       }
   }

   /// A stat cache held in memory, counting how often it was committed.
   #[derive(Debug, Default)]
   pub struct FakeStatCache {
       /// What is recorded.
       pub entries: BTreeMap<SourcePath, StatEntry>,
       commits: u32,
   }

   impl FakeStatCache {
       /// How many times the entries were committed.
       #[must_use]
       pub const fn commits(&self) -> u32 {
           self.commits
       }
   }

   impl StatCache for FakeStatCache {
       fn entry(&self, path: &SourcePath) -> Option<StatEntry> {
           self.entries.get(path).copied()
       }

       fn record(&mut self, path: SourcePath, entry: StatEntry) {
           self.entries.insert(path, entry);
       }

       fn commit(&mut self) -> Result<()> {
           self.commits += 1;
           Ok(())
       }
   }

   /// Programs with fixed fingerprints, recording every lookup.
   #[derive(Debug, Default)]
   pub struct FakeTools {
       /// The programs that can be found.
       pub fingerprints: BTreeMap<Program, Digest>,
       lookups: RefCell<Vec<Program>>,
   }

   impl FakeTools {
       /// Every program looked up, in the order asked.
       #[must_use]
       pub fn lookups(&self) -> Vec<Program> {
           self.lookups.borrow().clone()
       }
   }

   impl ToolResolver for FakeTools {
       fn fingerprint(&self, program: &Program) -> Result<Option<Digest>> {
           self.lookups.borrow_mut().push(program.clone());
           Ok(self.fingerprints.get(program).copied())
       }
   }

   /// The rows of a previous build, held in memory.
   #[derive(Debug, Default)]
   pub struct FakeCache {
       /// The rows.
       pub rows: BTreeMap<Label, CacheRow>,
   }

   impl ActionCache for FakeCache {
       fn row(&self, label: &Label) -> Option<&CacheRow> {
           self.rows.get(label)
       }
   }

   /// The bundle choosing the fakes of this file.
   #[derive(Debug)]
   pub struct FakePorts;

   impl Ports for FakePorts {
       type Source = FakeSource;
       type Manifest = FakeManifest;
       type Host = FakeHost;
       type Digester = FakeDigester;
       type StatCache = FakeStatCache;
       type Tools = FakeTools;
       type Cache = FakeCache;
   }
   ```

3. Run the flows and confirm they fail to build, because `Ports` names one port:

   ```
   $ cargo test -p buildl-core --test flows
   error[E0437]: type `Manifest` is not a member of trait `Ports`
   error[E0437]: type `Host` is not a member of trait `Ports`
   error[E0437]: type `Digester` is not a member of trait `Ports`
   error[E0437]: type `StatCache` is not a member of trait `Ports`
   error[E0437]: type `Tools` is not a member of trait `Ports`
   error[E0437]: type `Cache` is not a member of trait `Ports`
   ```

4. Extend the bundle. Replace the whole of `crates/buildl-core/src/ports/bundle.rs` with:

   ````rust
   //! The bundle that names one implementation of every port a pipeline uses.
   //!
   //! Its own file because it is a different kind of trait from the ports themselves: it has no
   //! methods, only associated types, and it exists so that a pipeline takes one type parameter
   //! instead of one per port.
   //!
   //! Responsibilities: [`Ports`].
   //!
   //! Non-responsibilities: choosing the implementations. A composition root does that, by
   //! implementing this trait.
   //!
   //! This file holds only a trait definition, so it carries no logic to unit-test.

   use crate::ports::{
       ActionCache, DeclarationSource, Digester, Host, Manifest, StatCache, ToolResolver,
   };

   /// One implementation of each port, chosen together.
   ///
   /// # Examples
   ///
   /// A bundle for a workspace with no build files, on a machine with nothing in it:
   ///
   /// ```
   /// use std::collections::BTreeSet;
   ///
   /// use buildl_core::{
   ///     ActionCache, BuildFile, CacheRow, Ceiling, DeclarationSource, Digest, Digester, Directory,
   ///     EnvName, EnvValue, Evaluated, FileStat, Host, Label, Manifest, Platform, Ports, Program,
   ///     Result, SourcePath, StatCache, StatEntry, ToolResolver, Workspace,
   /// };
   ///
   /// /// Every port at once, each answering that there is nothing.
   /// struct Nothing;
   ///
   /// impl DeclarationSource for Nothing {
   ///     fn evaluate(&self, _file: &BuildFile) -> Result<Evaluated> {
   ///         Ok(Evaluated::Absent)
   ///     }
   /// }
   ///
   /// impl Manifest for Nothing {
   ///     fn workspace(&self) -> Result<Workspace> {
   ///         Ok(Workspace {
   ///             out: Directory::parse("out")?,
   ///             state: Directory::parse(".buildl")?,
   ///             ceiling: Ceiling {
   ///                 run: BTreeSet::new(),
   ///                 env: BTreeSet::new(),
   ///                 read: BTreeSet::new(),
   ///                 write: BTreeSet::new(),
   ///             },
   ///         })
   ///     }
   /// }
   ///
   /// impl Host for Nothing {
   ///     fn platform(&self) -> Platform {
   ///         Platform::parse("linux/x86_64").unwrap_or_else(|_| unreachable!("a valid platform"))
   ///     }
   ///
   ///     fn env(&self, _name: &EnvName) -> Result<Option<EnvValue>> {
   ///         Ok(None)
   ///     }
   /// }
   ///
   /// impl Digester for Nothing {
   ///     fn stat(&self, _path: &SourcePath) -> Result<Option<FileStat>> {
   ///         Ok(None)
   ///     }
   ///
   ///     fn digest(&self, _paths: &[SourcePath]) -> Result<Vec<Digest>> {
   ///         Ok(Vec::new())
   ///     }
   /// }
   ///
   /// impl StatCache for Nothing {
   ///     fn entry(&self, _path: &SourcePath) -> Option<StatEntry> {
   ///         None
   ///     }
   ///
   ///     fn record(&mut self, _path: SourcePath, _entry: StatEntry) {}
   ///
   ///     fn commit(&mut self) -> Result<()> {
   ///         Ok(())
   ///     }
   /// }
   ///
   /// impl ToolResolver for Nothing {
   ///     fn fingerprint(&self, _program: &Program) -> Result<Option<Digest>> {
   ///         Ok(None)
   ///     }
   /// }
   ///
   /// impl ActionCache for Nothing {
   ///     fn row(&self, _label: &Label) -> Option<&CacheRow> {
   ///         None
   ///     }
   /// }
   ///
   /// /// A bundle choosing it for every port.
   /// struct NothingPorts;
   ///
   /// impl Ports for NothingPorts {
   ///     type Source = Nothing;
   ///     type Manifest = Nothing;
   ///     type Host = Nothing;
   ///     type Digester = Nothing;
   ///     type StatCache = Nothing;
   ///     type Tools = Nothing;
   ///     type Cache = Nothing;
   /// }
   /// ```
   pub trait Ports {
       /// How build files are evaluated.
       type Source: DeclarationSource;
       /// How the workspace's manifest is read.
       type Manifest: Manifest;
       /// How the machine actions run on is asked about itself.
       type Host: Host;
       /// How a file's metadata and content digest are read.
       type Digester: Digester;
       /// Where the digests of files already hashed are kept.
       type StatCache: StatCache;
       /// How the programs commands name are found.
       type Tools: ToolResolver;
       /// Where the previous build's rows are read from.
       type Cache: ActionCache;
   }
   ````

5. Run the flows and confirm they pass:

   ```
   $ cargo test -p buildl-core --test flows
   test result: ok. 21 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   ```

6. Run the bundle's doc example and confirm it passes:

   ```
   $ cargo test -p buildl-core --doc ports::bundle
   test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 5 filtered out
   ```

7. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`. That last line was captured on the tree the last plan leaves; no task of this chain changes a manifest.

8. Commit:

   ```
   $ git add crates/buildl-core/tests/flows/common.rs crates/buildl-core/src/ports/bundle.rs
   $ git commit -m "feat(buildl-core): name every port a plan reads in the Ports bundle"
   ```

### Task 5 — Build a pipeline from one set of ports

**Files:**
- Create `crates/buildl-core/src/ports/port_set.rs`
- Modify `crates/buildl-core/src/ports/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`
- Modify `crates/buildl-core/src/pipeline/driver.rs`
- Modify `crates/buildl-core/tests/flows/common.rs`
- Modify `crates/buildl-core/tests/flows/check.rs`
- Modify `crates/buildl-core/tests/flows/graph.rs`

**Steps:**

1. Write the failing callers. In `crates/buildl-core/tests/flows/common.rs`, widen the `buildl_core` import to add `Ceiling` and `PortSet`. Replace:

   ```rust
   use buildl_core::{
       ActionCache, BuildFile, CacheRow, DeclarationOrder, DeclarationSource, Diagnostic, Digest,
       Digester, Directory, EnvName, EnvValue, Error, Evaluated, EvaluationFailure, FileStat,
       Freshness, Host, Label, Manifest, NetworkAccess, Platform, PortName, Ports, Program, Result,
       SourcePath, StagedAlias, StagedDeclaration, StagedFile, StagedItem, StagedRule, StagedSetting,
       StagedSubdir, StagedTarget, StatCache, StatEntry, TargetRole, ToolResolver, Workspace, Written,
   };
   ```

   With:

   ```rust
   use buildl_core::{
       ActionCache, BuildFile, CacheRow, Ceiling, DeclarationOrder, DeclarationSource, Diagnostic,
       Digest, Digester, Directory, EnvName, EnvValue, Error, Evaluated, EvaluationFailure, FileStat,
       Freshness, Host, Label, Manifest, NetworkAccess, Platform, PortName, PortSet, Ports, Program,
       Result, SourcePath, StagedAlias, StagedDeclaration, StagedFile, StagedItem, StagedRule,
       StagedSetting, StagedSubdir, StagedTarget, StatCache, StatEntry, TargetRole, ToolResolver,
       Workspace, Written,
   };
   ```

2. In the same file, add the builders directly above the doc comment `/// A directory from known-valid text.` Insert:

   ```rust
   /// A workspace that writes to `out` and `.buildl`, under a ceiling allowing `cc` to run, any
   /// workspace file to be read, and those two directories to be written.
   #[must_use]
   pub fn open_workspace() -> Workspace {
       Workspace {
           out: dir("out"),
           state: dir(".buildl"),
           ceiling: Ceiling {
               run: BTreeSet::from([program("cc")]),
               env: BTreeSet::new(),
               read: BTreeSet::from([Directory::root()]),
               write: BTreeSet::from([dir("out"), dir(".buildl")]),
           },
       }
   }

   /// Every port around `source`: an [`open_workspace`], a `linux/x86_64` machine with no variable
   /// set, no workspace file, nothing recorded, no program and no row.
   #[must_use]
   pub fn ports(source: FakeSource) -> PortSet<FakePorts> {
       PortSet {
           source,
           manifest: FakeManifest {
               workspace: open_workspace(),
           },
           host: FakeHost {
               platform: Platform::parse("linux/x86_64")
                   .unwrap_or_else(|error| unreachable!("fixture platform: {error}")),
               variables: BTreeMap::new(),
               unreportable: BTreeSet::new(),
           },
           digester: FakeDigester::default(),
           stat_cache: FakeStatCache::default(),
           tools: FakeTools::default(),
           cache: FakeCache::default(),
       }
   }

   /// A program from known-valid text.
   ///
   /// # Panics
   ///
   /// Panics when `raw` is not a valid program name; a test fixture is wrong then.
   #[must_use]
   pub fn program(raw: &str) -> Program {
       Program::parse(raw).unwrap_or_else(|error| unreachable!("fixture program: {error}"))
   }
   ```

3. Switch the four call sites of `crates/buildl-core/tests/flows/check.rs`. First the import. Replace:

   ```rust
   use crate::common::{FakePorts, FakeSource, file, setting, target, workspace};
   ```

   With:

   ```rust
   use crate::common::{FakePorts, FakeSource, file, ports, setting, target, workspace};
   ```

4. Then the one pipeline built from a plain source. Replace:

   ```rust
       let pipeline = Pipeline::<FakePorts>::new(source);
   ```

   With:

   ```rust
       let pipeline = Pipeline::<FakePorts>::new(ports(source));
   ```

5. Then, three times, the pipeline built from a source with a second run. Replace:

   ```rust
       let pipeline = Pipeline::<FakePorts>::new(FakeSource::with_second_run(first, second));
   ```

   With:

   ```rust
       let pipeline = Pipeline::<FakePorts>::new(ports(FakeSource::with_second_run(first, second)));
   ```

6. Switch the call site of `crates/buildl-core/tests/flows/graph.rs`. First the import. Replace:

   ```rust
   use crate::common::{
       FakeFile, FakePorts, FakeSource, alias, file, rule, setting, target, target_with, workspace,
   };
   ```

   With:

   ```rust
   use crate::common::{
       FakeFile, FakePorts, FakeSource, alias, file, ports, rule, setting, target, target_with,
       workspace,
   };
   ```

7. Then the call. Replace:

   ```rust
       Pipeline::<FakePorts>::new(source).graph(&entry())
   ```

   With:

   ```rust
       Pipeline::<FakePorts>::new(ports(source)).graph(&entry())
   ```

8. Run the flows and confirm they fail to build:

   ```
   $ cargo test -p buildl-core --test flows
   error[E0432]: unresolved import `buildl_core::PortSet`
   ```

9. Add the set. Create `crates/buildl-core/src/ports/port_set.rs`:

   ```rust
   //! One instance of every port, held together.
   //!
   //! Its own file because it is the value beside the [`Ports`] trait: the trait names one
   //! implementation of each port, and this struct holds one instance of each, so a pipeline is
   //! built from one argument and a phase that needs several ports borrows one value.
   //!
   //! Responsibilities: [`PortSet`].
   //!
   //! Non-responsibilities: building the instances. A composition root does that.
   //!
   //! This file holds only a type definition, so it carries no logic to unit-test.

   use crate::ports::Ports;

   /// One instance of each port a bundle names.
   ///
   /// The fields are public: a set has no invariant of its own, and whoever built the adapters may
   /// reach them again between two commands.
   #[derive(Debug)]
   pub struct PortSet<P: Ports> {
       /// Evaluates build files.
       pub source: P::Source,
       /// Reads the workspace's manifest.
       pub manifest: P::Manifest,
       /// Answers for the machine actions run on.
       pub host: P::Host,
       /// Reads a file's metadata and content digest.
       pub digester: P::Digester,
       /// Keeps the digests of files already hashed.
       pub stat_cache: P::StatCache,
       /// Finds the programs commands name.
       pub tools: P::Tools,
       /// Holds the previous build's rows.
       pub cache: P::Cache,
   }
   ```

10. Declare it. In `crates/buildl-core/src/ports/mod.rs`, add `pub mod port_set;` to the `pub mod` lines and `pub use port_set::PortSet;` to the `pub use` lines, and add one bullet to the module doc, directly below the two-line `Ports` bullet:

   ```rust
   //! - [`PortSet`] — one instance of each port a bundle names, so a pipeline is built from one
   //!   value.
   ```

11. Re-export it. In `crates/buildl-core/src/lib.rs`, add `PortSet` to the `pub use ports::{…}` list, then run `cargo fmt --all`. The list reads:

   ```rust
   pub use ports::{
       ActionCache, Clock, DeclarationSource, Digester, Host, Manifest, PortSet, Ports, StatCache,
       ToolResolver,
   };
   ```

12. Rebuild the pipeline around the set. In `crates/buildl-core/src/pipeline/driver.rs`, widen the import. Replace:

   ```rust
   use crate::ports::Ports;
   ```

   With:

   ```rust
   use crate::ports::{PortSet, Ports};
   ```

13. Replace the struct, its `Debug` and its constructor. Replace:

   ```rust
   /// The pipeline over one chosen implementation of each port.
   pub struct Pipeline<P: Ports> {
       source: P::Source,
   }

   impl<P: Ports> fmt::Debug for Pipeline<P>
   where
       P::Source: fmt::Debug,
   {
       fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
           f.debug_struct("Pipeline")
               .field("source", &self.source)
               .finish()
       }
   }

   impl<P: Ports> Pipeline<P> {
       /// Assembles a pipeline from its ports.
       #[must_use]
       pub const fn new(source: P::Source) -> Self {
           Self { source }
       }
   ```

   With:

   ```rust
   /// The pipeline over one chosen implementation of each port.
   pub struct Pipeline<P: Ports> {
       ports: PortSet<P>,
   }

   impl<P: Ports> fmt::Debug for Pipeline<P>
   where
       PortSet<P>: fmt::Debug,
   {
       fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
           f.debug_struct("Pipeline")
               .field("ports", &self.ports)
               .finish()
       }
   }

   impl<P: Ports> Pipeline<P> {
       /// Assembles a pipeline from its ports.
       #[must_use]
       pub const fn new(ports: PortSet<P>) -> Self {
           Self { ports }
       }

       /// The ports this pipeline runs over.
       #[must_use]
       pub const fn ports(&self) -> &PortSet<P> {
           &self.ports
       }

       /// The ports this pipeline runs over, for a caller that changes one between two commands.
       #[must_use]
       pub const fn ports_mut(&mut self) -> &mut PortSet<P> {
           &mut self.ports
       }
   ```

14. Read the source through the set, in `check`. Replace:

   ```rust
           let first = load(&self.source, entry)?;
           let second = load(&self.source, entry)?;
   ```

   With:

   ```rust
           let first = load(&self.ports.source, entry)?;
           let second = load(&self.ports.source, entry)?;
   ```

15. And in `graph`. Replace:

   ```rust
           resolve(load(&self.source, entry)?)
   ```

   With:

   ```rust
           resolve(load(&self.ports.source, entry)?)
   ```

16. Update the module doc of the same file. Replace:

   ```rust
   //! Responsibilities: [`Pipeline`], [`Pipeline::check`] and [`Pipeline::graph`].
   ```

   With:

   ```rust
   //! Responsibilities: [`Pipeline`], its ports, [`Pipeline::check`] and [`Pipeline::graph`].
   ```

17. Run the flows and confirm they pass:

   ```
   $ cargo test -p buildl-core --test flows
   test result: ok. 21 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   ```

18. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo deny check
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing, `cargo make dod` ends with `[cargo-make] INFO - Build Done in … seconds.`, and `cargo deny check` ends with `advisories ok, bans ok, licenses ok, sources ok`. That last line was captured on the tree the last plan leaves; no task of this chain changes a manifest.

19. Commit:

   ```
   $ git add crates/buildl-core/src/ports/port_set.rs crates/buildl-core/src/ports/mod.rs crates/buildl-core/src/lib.rs crates/buildl-core/src/pipeline/driver.rs crates/buildl-core/tests/flows/common.rs crates/buildl-core/tests/flows/check.rs crates/buildl-core/tests/flows/graph.rs
   $ git commit -m "feat(buildl-core): build a pipeline from one PortSet"
   ```

---

## Verification summary (plan-level)

- `cargo make dod` and `cargo deny check` exit `0`.
- The six traits have the signatures of spec §4, and every fallible method returns the crate's
  `Result`.
- `Ports` names seven ports; `FakePorts` and the doc example in `bundle.rs` implement all seven
  from the public API alone.
- `Pipeline::new` takes a `PortSet`; `check` and `graph` keep their signatures, and the 21
  existing flows pass unchanged but for their five constructor calls.
- `crates/buildl-core/src` holds no `impl` of any port outside a doc example and `ports/clock.rs`'s
  test module.
- `crates/expected-module-edges.txt` is unchanged.
