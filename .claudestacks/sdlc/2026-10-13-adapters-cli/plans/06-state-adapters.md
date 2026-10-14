---
status: done
created: 2026-10-14
depends-on: [04]
---

# State Adapters Implementation Plan

**Goal:** A workspace's state is kept on disk by five adapters of the `buildl` crate, one for each port that stores something.

**Architecture:** Five adapter modules under `crates/buildl/src/`, each one directory with an export-only `mod.rs` and one file per type: `cas/` (`LocalCas: ContentStore`, with `BlobPath` and `Placement`), `cache/` (`LocalCache: ActionCache`, with `Journal` and its `Entry`), `log/` (`LocalLog: EventLog`), `statcache/` (`LocalStatCache: StatCache`), `graph/` (`GraphFile: GraphRecord`). Every one of them names `buildl_core`, `std` and the shared module `crate::state` (the layout of the state directory, the lock, `write_whole`, `clear_path`) and no other module of the crate, so the five never meet outside the composition root. Every JSON byte is written by `buildl_core::json::canonical` and read back by its reader; nothing here names `serde_json`. Every whole-file write goes through `state::write_whole`, a temp file renamed onto its target, so a reader finds the old file or the new one. No constructor writes: `LocalCas::new`, `LocalLog::new` and `GraphFile::new` touch nothing, and `LocalCache::open` and `LocalStatCache::open` only read, apart from the fold of a journal an interrupted run left. The content store keeps a read-only blob per digest under `cas/<2 hex>/<62 hex>` and, for a digest that was placed as a file that may be run, an executable twin `<62 hex>.x`; `put` rewrites whichever of the two differs from the bytes in hand and removes neither, so which files the store holds depends on the graph alone and never on the order workers finished in. A file of the workspace is a hard link to the blob or the twin where a write through the link is seen to be refused, and a copy with the same mode everywhere else. The action cache is a canonical snapshot, `cache.json`, and a journal, `cache.journal`, that takes one line per recorded row; `fold` writes the snapshot once and removes the journal, and `open` replays a journal that was left behind.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, `std` (with `std::os::unix`: the crate builds on Linux and macOS only), `buildl-core`, `serde` (derive, for a journal line and a stat-cache entry), `tempfile` (in tests, for temp directories; production code reaches it only through `state::write_whole`), `cargo-make`, `cargo-deny`. No new dependency: plan `04` adds every one this plan uses.

**Content authority:** spec §5.3 to §5.8 (the five adapters and their errors; §5.1, the layout, and §5.2, `state/`, are used and not written here), §10 (formats, byte for byte), the rows of §11 that name `cas/`, `cache/`, `statcache/`, `log/` and `graph/`, the five lists of §12.2 under those names, §14 (dependencies), §17 (what the ubuntu leg proves).

---

## Context an implementer needs

### What plan `04` leaves, and this plan uses

Plan `04` declares the five modules in `crates/buildl/src/lib.rs` (`pub mod cache; pub mod cas; pub mod graph; pub mod log; pub mod statcache;`) and gives each a `mod.rs` that holds a module doc comment and nothing else. This plan replaces each of those five `mod.rs` files whole, so their text as plan `04` wrote it does not matter. It edits no other file of plan `04`: not `lib.rs`, not `Cargo.toml`, not `state/`.

From `crate::state`, exactly these items, with these signatures:

| Item | Signature | Used for |
|---|---|---|
| `crate::state::StateLayout` | `pub fn new(root: &Path, state: &Directory) -> Self`; `pub fn dir(&self) -> &Path`; `pub fn cache(&self) -> PathBuf` (`cache.json`); `pub fn journal(&self) -> PathBuf` (`cache.journal`); `pub fn stat_cache(&self) -> PathBuf` (`statcache.json`); `pub fn log(&self) -> PathBuf` (`log.jsonl`); `pub fn graph(&self) -> PathBuf` (`graph.json`); `pub fn blobs(&self) -> PathBuf` (`cas`); `pub fn blob_staging(&self) -> PathBuf` (`cas/tmp`) | where every adapter keeps its file |
| `crate::state::StateLock` | `pub fn acquire(layout: StateLayout) -> Result<Self, LockError>` (makes the state directory); `pub const fn layout(&self) -> &StateLayout`; `pub const fn acquired(&self) -> Timestamp` | the stat cache's tests: `acquired()` is when the run began. `LockError` is `Debug` |
| `crate::state::write_whole` | `pub fn write_whole(staging: &Path, target: &Path, bytes: &[u8], access: Access) -> io::Result<()>`: a new file in `staging` named `.tmp-<random>`, written, renamed onto `target`; a missing `staging` and a missing parent of `target` are made; a failure removes the temp file and leaves `target` as it was | every whole-file write |
| `crate::state::Access` | `pub enum Access { Rewritable, ReadOnly, ReadOnlyRunnable }`, `Copy`: the modes 0644, 0444 and 0555, each less the umask | the mode of a written file |
| `crate::state::clear_path` | `pub(crate) fn clear_path(path: &Path) -> io::Result<()>`: removes a file, a link, or a directory with everything below it; nothing there, or a parent that is not a directory, is `Ok` | clearing an output path; removing a damaged blob |

`write_whole` and `Access` are public items of the public module `state`, and `clear_path` is already called by the lock's sweep, so none of the three is dead code before this plan calls it and nothing in `state/` changes when it does.

`crates/buildl/Cargo.toml` already names what this plan's code uses: `buildl-core`, `serde` and `tempfile`, each `{ workspace = true }` under `[dependencies]`. The unit tests call `tempfile::tempdir()` through that same line.

### What `buildl-core` gives, as plans `01` to `03` leave it

| Item | Fact this plan relies on |
|---|---|
| `ContentStore` (`Send + Sync`) | `put(&self, &[u8]) -> Result<Digest>`, `contains(&self, &Digest) -> bool`, `get(&self, &Digest) -> Result<Option<Vec<u8>>>`, `clear(&self, &OutputPath) -> Result<()>`, `place(&self, &Digest, FileMode, &OutputPath) -> Result<()>` |
| `ActionCache` | `row(&self, &Label) -> Option<&CacheRow>`, `record(&mut self, Label, CacheRow) -> Result<()>` |
| `EventLog`, `StatCache`, `GraphRecord` | `append(&mut self, &OutcomeEvent) -> Result<()>`; `entry(&self, &SourcePath) -> Option<StatEntry>`, `record(&mut self, SourcePath, StatEntry)`, `commit(&mut self) -> Result<()>`; `record(&mut self, &TargetGraph) -> Result<()>` |
| `Error::Adapter { port: PortName, diagnostic: Diagnostic }` | rendered `{port}: {diagnostic}`; `PortName::{ContentStore, ActionCache, EventLog, StatCache, GraphRecord}` render `content store`, `action cache`, `event log`, `stat cache`, `graph record` |
| `Error::CorruptState { file: PathBuf, source: Box<Error> }` | rendered `<file> is not a file buildl wrote: <source>`; `source` is `Error::UnreadableJson` or `Error::NonCanonicalJson` |
| `Error` and `PortName` are `#[non_exhaustive]` | a test takes a variant apart with `let Error::Adapter { .. } = error else { unreachable!(..) }` |
| `buildl_core::json::canonical::{to_vec, from_slice}` | `from_slice` accepts exactly the bytes `to_vec` writes, and refuses anything else as `UnreadableJson` or `NonCanonicalJson` |
| `Digest::of(&[u8])`; `Display` is 64 lowercase hexadecimal characters | `Digest::of(b"hello")` is `2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824` |
| `OutputPath` | built only by `OutputPath::of(&Directory, &Label, &OutputName)`: the three joined by `/`, a root directory adding nothing; `as_str()` is the path below the workspace root |
| `FileMode::{Plain, Executable}`; `Timestamp::{from_unix_nanos, as_unix_nanos}`, `Ord`; `StatEntry { stat: FileStat { modified, size }, digest }`, `Copy` | as named |
| `TargetGraph` | has no `Deserialize` and no public constructor: a test gets one from `buildl_core::resolve(Vec<Declaration>)` |

### Gate facts that bite here

- **The import rule.** `cargo make guard-adapter-isolation` (plan `04`) reads test code too. An adapter module may name `crate::state` and nothing else of the crate; a `mod.rs` may not write `super::`; `crate::{` and `super::super` are refused everywhere. So a file names its sibling as `crate::cas::blob::BlobPath`, never through `super::super`, and a test module names its own file's items with `use super::X;`.
- **Dead code between tasks.** A crate-private type with no caller outside its own tests is dead code in the library build, and `-D warnings` fails on it. `BlobPath` (task 1), `Placement` (task 3) and `Journal` (task 5) each arrive one task before what calls them, so the `mod` line that declares each carries `#[cfg_attr(not(test), expect(dead_code, reason = ..))]`, and the task that makes the module live replaces `mod.rs` without it. `expect` and not `allow`: the attribute turns into an error the moment the code becomes live, so it cannot be left behind.
- **`mod.rs` is replaced whole** in every task that touches one, and each code block for it is the complete file.
- **A new file is written in two steps**, tests first: the file with its module docs and its tests, which does not compile; then the implementation, inserted between the docs and the line `#[cfg(test)]` with one blank line on each side. The two blocks joined that way are the whole file.
- **Modes are asked for, and the umask takes its share.** A test never expects `0o444`; it expects `0o444 & allowed(..)`, where `allowed` is the mode of a file the test itself makes asking for `0o777`. No test sets a umask, an environment variable or the current directory.
- **Formats are asserted against literal bytes.** Every expected string of JSON in a test below was printed by a run of that test against this code; none was composed by hand.
- **Clippy, as it was met.** `needless_collect` refuses collecting join handles from an iterator of spawns, though the collect is what makes the threads overlap, so `eight_threads_put_the_same_bytes` pushes handles in a loop. `single_match_else` refuses a `match` on an `Option` whose `None` arm is a block, so `LocalLog` opens its file in a function of its own. `similar_names` refuses `log` beside `long`. `doc_link_code` refuses a link directly followed by code text in a doc comment.
- **Linux and macOS.** Nothing here was run on Linux. The tests that prove the platform facts on the `ubuntu-latest` leg are `a_plain_file_is_a_link_to_the_blob` and `a_write_through_a_placed_file_is_refused` (a hard link shares the inode; a write through a 0444 link is refused) and `the_twin_stays_whatever_order_puts_and_places_come_in` (the store keeps the same files in every order). Three tests return early where the store does not link, which is as root and on a filesystem that keeps no permission bits: the two just named and `one_blob_placed_plain_and_executable_uses_two_inodes`.

### How this plan was proven

All work happens in the worktree, on its branch, never on `main`. Commits follow Conventional Commits, one per task. Every Rust block below is the byte-for-byte content of a prototype that was built in task order, and is already in rustfmt's form. Each quoted test run, the failing one and the passing one, was captured by running that command on the state its step describes. A failing run is quoted by the lines that name the cause, a passing one by its `test` lines in name order and its result line without the time taken; the runner prints `test` lines in whatever order the tests finish. The `filtered out` figure of a result line counts the other tests of the crate: it is what the crate holds when this plan runs directly after plan `04`, and it is larger when plan `05`, `07` or `08` has landed first. The `passed` figure does not change.

The prototype stands on plan `04`'s own prototype: its manifests, `deny.toml`, `Makefile.toml`, golden files, `lib.rs` and `state/` are the ones plan `04` writes. After every task the gate was run there exactly as the task's gate step writes it, with no lint exempted anywhere, and each of the three commands exited `0`; the `guard-adapter-isolation` line quoted in each gate step is the one that run printed. Its edge count is this plan's alone: an adapter plan that has landed first adds its own edges to it. The guard's edges after task 10 are `cache -> state`, `cas -> state`, `graph -> state`, `log -> state` and `statcache -> state`.

## File structure

```
crates/buildl/src/cas/mod.rs                     — [modify] declare blob, local_cas, placement; export LocalCas
crates/buildl/src/cas/blob.rs                    — [create] BlobPath: the two files held for one digest
crates/buildl/src/cas/local_cas.rs               — [create] LocalCas: ContentStore
crates/buildl/src/cas/placement.rs               — [create] Placement, and the probe that chooses it
crates/buildl/src/cache/mod.rs                   — [modify] declare journal, local_cache; export LocalCache
crates/buildl/src/cache/journal.rs               — [create] Journal and Entry
crates/buildl/src/cache/local_cache.rs           — [create] LocalCache: ActionCache, with open and fold
crates/buildl/src/log/mod.rs                     — [modify] declare local_log; export LocalLog
crates/buildl/src/log/local_log.rs               — [create] LocalLog: EventLog
crates/buildl/src/statcache/mod.rs               — [modify] declare local_stat_cache; export LocalStatCache
crates/buildl/src/statcache/local_stat_cache.rs  — [create] LocalStatCache: StatCache
crates/buildl/src/graph/mod.rs                   — [modify] declare graph_file; export GraphFile
crates/buildl/src/graph/graph_file.rs            — [create] GraphFile: GraphRecord
```

Every task ends with the same gate, run from the worktree root: `cargo fmt --all -- --check`, `cargo make dod`, and `cargo make deny`, which is `cargo deny check -D unused-wrapper`, the command CI runs.

### Task 1 — Name the two files the store holds for a digest

**Files:**
- Modify `crates/buildl/src/cas/mod.rs`
- Create `crates/buildl/src/cas/blob.rs`

**Steps:**

1. Replace the whole of `crates/buildl/src/cas/mod.rs` with:

   ```rust
   //! The content store on disk: bytes held by their digest under the state directory.
   //!
   //! Its own module because it is the one place that decides how content is laid out on disk: a
   //! directory named by the first two hexadecimal characters of a digest, and in it a file named
   //! by the other sixty-two.
   //!
   //! Responsibilities: the path of the files held for a digest.
   //!
   //! Non-responsibilities: deciding what is stored, which the build does.
   //!
   //! This file holds only module declarations, so it carries no logic to unit-test.

   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "nothing public reaches this module until a store names its files with it"
       )
   )]
   mod blob;
   ```

   The `cfg_attr` is deliberate: nothing calls `BlobPath` until task 2, which replaces this file without the attribute.

2. Create `crates/buildl/src/cas/blob.rs` holding the module docs and the test only:

   ```rust
   //! The two paths the content store may hold one digest's bytes at.
   //!
   //! Its own file because every method of the store turns a digest into a path, and the rule must
   //! be spelled once: the first two hexadecimal characters of the digest name a directory, the
   //! other sixty-two name the file in it, and the executable twin is that name with `.x` added.
   //! Two characters of directory keep any one directory from holding the whole store.
   //!
   //! Responsibilities: [`BlobPath`].
   //!
   //! Non-responsibilities: reading or writing either file, which the store does.

   #[cfg(test)]
   mod tests {
       use std::path::Path;

       use buildl_core::{Digest, FileMode};

       use super::BlobPath;

       #[test]
       fn a_digest_splits_into_shard_and_rest() {
           let paths = BlobPath::of(Path::new("/ws/.buildl/cas"), &Digest::of(b"hello"));
           assert_eq!(
               paths.for_mode(FileMode::Plain),
               Path::new(
                   "/ws/.buildl/cas/2c/f24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
               )
           );
           assert_eq!(
               paths.for_mode(FileMode::Executable),
               Path::new(
                   "/ws/.buildl/cas/2c/f24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824.x"
               )
           );
       }
   }
   ```

3. Run the test and confirm it fails to compile:

   ```
   $ cargo test -p buildl --lib cas::blob
   ```

   Expected:

   ```text
   error[E0432]: unresolved import `super::BlobPath`
     --> crates/buildl/src/cas/blob.rs:18:9
   error: could not compile `buildl` (lib test) due to 1 previous error
   ```

4. Add the implementation. In `crates/buildl/src/cas/blob.rs`, insert between the module docs and the line `#[cfg(test)]`, with one blank line before it and one after:

   ```rust
   use std::path::{Path, PathBuf};

   use buildl_core::{Digest, FileMode};

   /// The two files the store may hold for one digest.
   #[derive(Debug)]
   pub(crate) struct BlobPath {
       plain: PathBuf,
       runnable: PathBuf,
   }

   impl BlobPath {
       /// The paths for `digest` in the store whose directory is `blobs`.
       pub(crate) fn of(blobs: &Path, digest: &Digest) -> Self {
           let hex = digest.to_string();
           let (shard, rest) = hex.split_at(2);
           let shard = blobs.join(shard);
           Self {
               plain: shard.join(rest),
               runnable: shard.join(format!("{rest}.x")),
           }
       }

       /// The file a placement of that mode is made from: the blob for a file that may not be
       /// run, its executable twin for one that may.
       pub(crate) fn for_mode(&self, mode: FileMode) -> &Path {
           match mode {
               FileMode::Plain => &self.plain,
               FileMode::Executable => &self.runnable,
           }
       }
   }
   ```

5. Run the test and confirm it passes:

   ```
   $ cargo test -p buildl --lib cas::blob
   ```

   Expected:

   ```text
   test cas::blob::tests::a_digest_splits_into_shard_and_rest ... ok
   test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 20 filtered out
   ```

6. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo make deny
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing. `cargo make dod` prints `guard-adapter-isolation: OK (0 edges)` among its first lines, and its last line begins `[cargo-make] INFO - Build Done in`. `cargo make deny` prints `advisories ok, bans ok, licenses ok, sources ok`.

7. Commit:

   ```
   $ git add crates/buildl/src/cas/mod.rs crates/buildl/src/cas/blob.rs
   $ git commit -m "feat(buildl): name the two files the content store holds for a digest"
   ```

### Task 2 — Add the content store, placing files by copy

`LocalCas` arrives whole as a `ContentStore`: `put`, `contains`, `get` and `clear` in their final form, and `place` making every file a copy with the right mode, from the blob or from the executable twin. A store that only copies is a correct store, the one a FAT volume or root gets; task 4 adds the hard link.

**Files:**
- Modify `crates/buildl/src/cas/mod.rs`
- Create `crates/buildl/src/cas/local_cas.rs`

**Steps:**

1. Replace the whole of `crates/buildl/src/cas/mod.rs` with:

   ```rust
   //! The content store on disk: bytes held by their digest, and held bytes made files of the
   //! workspace.
   //!
   //! Its own module because it is the one place that decides how content is laid out under the
   //! state directory and how it reaches the output directory: a read-only blob per digest, and
   //! beside it an executable twin for content that was placed as a file that may be run.
   //!
   //! Responsibilities: [`LocalCas`], the [`ContentStore`](buildl_core::ContentStore) of a local
   //! workspace.
   //!
   //! Non-responsibilities: deciding what is stored, cleared or placed, which the build does; and
   //! where the state directory keeps the store, which
   //! [`StateLayout`](crate::state::StateLayout) says.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

   mod blob;
   mod local_cas;

   pub use local_cas::LocalCas;
   ```

2. Create `crates/buildl/src/cas/local_cas.rs` holding the module docs and the tests only:

   ```rust
   //! [`LocalCas`], the content store of a local workspace.
   //!
   //! Its own file because it is where the store's rules meet: what is held is never trusted over
   //! the bytes in hand, nothing a build does removes a held file that is sound, and a file of the
   //! workspace never lets a write reach the store.
   //!
   //! Responsibilities: [`LocalCas`] and its [`ContentStore`] implementation.
   //!
   //! Non-responsibilities: where a digest's files are, which `BlobPath` decides; and deciding
   //! what is stored, cleared or placed, which the build does.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::fs::{self, OpenOptions};
       use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
       use std::path::{Path, PathBuf};
       use std::thread;

       use buildl_core::{
           ContentStore, Digest, Directory, Error, FileMode, Label, OutputName, OutputPath, PortName,
       };

       use super::LocalCas;
       use crate::cas::blob::BlobPath;
       use crate::state::StateLayout;

       /// The blob of the bytes `hello`, below the workspace root.
       const HELLO: &str =
           ".buildl/cas/2c/f24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";

       /// A store in a new, empty workspace, with the workspace's root.
       fn store() -> (tempfile::TempDir, PathBuf, LocalCas) {
           let dir = tempfile::tempdir().unwrap();
           let root = fs::canonicalize(dir.path()).unwrap();
           let layout = StateLayout::new(&root, &Directory::parse(".buildl").unwrap());
           let cas = LocalCas::new(&root, &layout);
           (dir, root, cas)
       }

       /// The place of the output `name` of a target at the workspace root: `out/<name>`.
       fn at(name: &str) -> OutputPath {
           OutputPath::of(
               &Directory::parse("out").unwrap(),
               &Label::parse("//:t").unwrap(),
               &OutputName::parse(name).unwrap(),
           )
       }

       /// The permission bits of the file at `path`.
       fn mode_of(path: &Path) -> u32 {
           fs::metadata(path).unwrap().permissions().mode() & 0o777
       }

       /// The permission bits the umask lets through: those of a file made asking for all of them.
       fn allowed(root: &Path) -> u32 {
           let path = root.join("mask-probe");
           OpenOptions::new()
               .write(true)
               .create_new(true)
               .mode(0o777)
               .open(&path)
               .unwrap();
           let mode = mode_of(&path);
           fs::remove_file(&path).unwrap();
           mode
       }

       /// The inode of the file at `path`.
       fn inode_of(path: &Path) -> u64 {
           fs::metadata(path).unwrap().ino()
       }

       /// Makes the file at `path`, which the store wrote read-only, hold `bytes` instead.
       fn damage(path: &Path, bytes: &[u8]) {
           fs::remove_file(path).unwrap();
           fs::write(path, bytes).unwrap();
       }

       /// Every file the store holds: its path below `root`, its mode and its bytes, in path order.
       /// The staging directory is left out.
       fn held(root: &Path) -> Vec<(String, u32, Vec<u8>)> {
           let mut files = Vec::new();
           let mut pending = vec![root.join(".buildl/cas")];
           while let Some(directory) = pending.pop() {
               let Ok(entries) = fs::read_dir(&directory) else {
                   continue;
               };
               for entry in entries {
                   let path = entry.unwrap().path();
                   if path.is_dir() {
                       if !path.ends_with("cas/tmp") {
                           pending.push(path);
                       }
                   } else {
                       let name = path
                           .strip_prefix(root)
                           .unwrap()
                           .to_str()
                           .unwrap()
                           .to_owned();
                       files.push((name, mode_of(&path), fs::read(&path).unwrap()));
                   }
               }
           }
           files.sort();
           files
       }

       /// How many entries the store's staging directory holds.
       fn staged(root: &Path) -> usize {
           fs::read_dir(root.join(".buildl/cas/tmp")).unwrap().count()
       }

       #[test]
       fn put_holds_the_bytes_under_their_digest_read_only() {
           let (_dir, root, cas) = store();
           assert!(!cas.contains(&Digest::of(b"hello")));

           let digest = cas.put(b"hello").unwrap();

           assert_eq!(digest, Digest::of(b"hello"));
           assert_eq!(
               held(&root),
               [(HELLO.to_owned(), 0o444 & allowed(&root), b"hello".to_vec())]
           );
           assert!(cas.contains(&digest));
           assert!(!cas.contains(&Digest::of(b"other")));
           assert_eq!(cas.get(&digest).unwrap(), Some(b"hello".to_vec()));
           assert_eq!(staged(&root), 0);
       }

       #[test]
       fn put_of_held_bytes_keeps_the_inode_of_the_blob_and_of_its_twin() {
           let (_dir, root, cas) = store();
           let digest = cas.put(b"hello").unwrap();
           cas.place(&digest, FileMode::Executable, &at("tool"))
               .unwrap();
           let blob = root.join(HELLO);
           let twin = root.join(format!("{HELLO}.x"));
           let before = (inode_of(&blob), inode_of(&twin));

           assert_eq!(cas.put(b"hello").unwrap(), digest);

           assert_eq!((inode_of(&blob), inode_of(&twin)), before);
       }

       #[test]
       fn put_replaces_a_blob_whose_bytes_differ() {
           let (_dir, root, cas) = store();
           let digest = cas.put(b"hello").unwrap();
           let blob = root.join(HELLO);
           damage(&blob, b"EVIL!");

           assert_eq!(cas.put(b"hello").unwrap(), digest);

           assert_eq!(
               held(&root),
               [(HELLO.to_owned(), 0o444 & allowed(&root), b"hello".to_vec())]
           );
       }

       #[test]
       fn put_replaces_a_twin_whose_bytes_differ_and_removes_none() {
           let (_dir, root, cas) = store();
           let digest = cas.put(b"hello").unwrap();
           cas.place(&digest, FileMode::Executable, &at("tool"))
               .unwrap();
           let blob = root.join(HELLO);
           let twin = root.join(format!("{HELLO}.x"));
           let sound = [
               (HELLO.to_owned(), 0o444 & allowed(&root), b"hello".to_vec()),
               (
                   format!("{HELLO}.x"),
                   0o555 & allowed(&root),
                   b"hello".to_vec(),
               ),
           ];

           // A damaged twin beside a sound blob: the twin is written again, the blob is left alone.
           damage(&twin, b"EVIL!");
           let untouched = inode_of(&blob);
           cas.put(b"hello").unwrap();
           assert_eq!(held(&root), sound);
           assert_eq!(inode_of(&blob), untouched);

           // A damaged twin beside a damaged blob: both are written again, and both are still there.
           damage(&blob, b"EVIL!");
           damage(&twin, b"EVIL!");
           cas.put(b"hello").unwrap();
           assert_eq!(held(&root), sound);
       }

       #[test]
       fn put_makes_no_twin() {
           let (_dir, root, cas) = store();

           cas.put(b"hello").unwrap();
           cas.put(b"hello").unwrap();

           assert!(!root.join(format!("{HELLO}.x")).exists());
       }

       /// One whole call of the store, for one of two targets whose outputs hold the same bytes:
       /// `X` is placed as a file that may be run, `Y` as one that may not.
       #[derive(Debug, Clone, Copy)]
       enum Call {
           PutX,
           PlaceX,
           PutY,
           PlaceY,
       }

       #[test]
       fn the_twin_stays_whatever_order_puts_and_places_come_in() {
           use Call::{PlaceX, PlaceY, PutX, PutY};

           // Each target's put comes before its own place; these are the six ways two such pairs
           // can interleave.
           let orders = [
               [PutX, PlaceX, PutY, PlaceY],
               [PutX, PutY, PlaceX, PlaceY],
               [PutX, PutY, PlaceY, PlaceX],
               [PutY, PutX, PlaceX, PlaceY],
               [PutY, PutX, PlaceY, PlaceX],
               [PutY, PlaceY, PutX, PlaceX],
           ];
           for order in orders {
               let (_dir, root, cas) = store();
               let digest = Digest::of(b"hello");
               for call in order {
                   match call {
                       PutX | PutY => assert_eq!(cas.put(b"hello").unwrap(), digest),
                       PlaceX => cas.place(&digest, FileMode::Executable, &at("x")).unwrap(),
                       PlaceY => cas.place(&digest, FileMode::Plain, &at("y")).unwrap(),
                   }
               }

               assert_eq!(
                   held(&root),
                   [
                       (HELLO.to_owned(), 0o444 & allowed(&root), b"hello".to_vec()),
                       (
                           format!("{HELLO}.x"),
                           0o555 & allowed(&root),
                           b"hello".to_vec()
                       ),
                   ],
                   "{order:?}"
               );
               let (x, y) = (root.join("out/x"), root.join("out/y"));
               assert_eq!(
                   (mode_of(&x), fs::read(&x).unwrap()),
                   (0o555 & allowed(&root), b"hello".to_vec()),
                   "{order:?}"
               );
               assert_eq!(
                   (mode_of(&y), fs::read(&y).unwrap()),
                   (0o444 & allowed(&root), b"hello".to_vec()),
                   "{order:?}"
               );
           }
       }

       #[test]
       fn eight_threads_put_the_same_bytes() {
           let (_dir, root, cas) = store();
           let bytes = vec![7_u8; 1 << 20];

           // Every thread is started before any is waited for, so the puts overlap.
           let digests: Vec<Digest> = thread::scope(|scope| {
               let mut running = Vec::new();
               for _ in 0..8 {
                   running.push(scope.spawn(|| cas.put(&bytes).unwrap()));
               }
               running
                   .into_iter()
                   .map(|thread| thread.join().unwrap())
                   .collect()
           });

           assert_eq!(digests, [Digest::of(&bytes); 8]);
           let name = BlobPath::of(Path::new(".buildl/cas"), &Digest::of(&bytes));
           let name = name.for_mode(FileMode::Plain).to_str().unwrap().to_owned();
           assert_eq!(held(&root), [(name, 0o444 & allowed(&root), bytes)]);
           assert_eq!(staged(&root), 0);
       }

       #[test]
       fn get_of_nothing_is_none() {
           let (_dir, _root, cas) = store();
           assert_eq!(cas.get(&Digest::of(b"hello")).unwrap(), None);

           // The same once the store's directories exist.
           cas.put(b"other").unwrap();
           assert_eq!(cas.get(&Digest::of(b"hello")).unwrap(), None);
       }

       #[test]
       fn get_drops_a_blob_that_does_not_hash_to_its_name() {
           let (_dir, root, cas) = store();
           let digest = cas.put(b"hello").unwrap();
           cas.place(&digest, FileMode::Executable, &at("tool"))
               .unwrap();
           damage(&root.join(HELLO), b"EVIL!");

           assert_eq!(cas.get(&digest).unwrap(), None);

           assert_eq!(held(&root), []);
           assert!(!cas.contains(&digest));
       }

       #[test]
       fn clear_removes_a_file_or_a_directory_and_nothing_is_no_error() {
           let (_dir, root, cas) = store();
           let digest = cas.put(b"hello").unwrap();

           cas.clear(&at("app")).unwrap();

           cas.place(&digest, FileMode::Plain, &at("app")).unwrap();
           cas.clear(&at("app")).unwrap();
           assert!(!root.join("out/app").exists());

           cas.place(&digest, FileMode::Plain, &at("app/share/text"))
               .unwrap();
           cas.clear(&at("app")).unwrap();
           assert!(!root.join("out/app").exists());
           assert!(root.join("out").is_dir());
           assert_eq!(cas.get(&digest).unwrap(), Some(b"hello".to_vec()));
       }

       #[test]
       fn place_makes_directories_and_replaces_a_file_or_a_directory() {
           let (_dir, root, cas) = store();
           let hello = cas.put(b"hello").unwrap();
           let other = cas.put(b"other").unwrap();
           let target = root.join("out/lib/deep/text");

           // Nothing above the file exists yet.
           cas.place(&hello, FileMode::Plain, &at("lib/deep/text"))
               .unwrap();
           assert_eq!(fs::read(&target).unwrap(), b"hello");

           // A file is there.
           cas.place(&other, FileMode::Executable, &at("lib/deep/text"))
               .unwrap();
           assert_eq!(fs::read(&target).unwrap(), b"other");
           assert_eq!(mode_of(&target), 0o555 & allowed(&root));

           // A directory is there, with a file in it.
           cas.place(&hello, FileMode::Plain, &at("lib/deep")).unwrap();
           assert_eq!(fs::read(root.join("out/lib/deep")).unwrap(), b"hello");
           assert_eq!(mode_of(&root.join("out/lib/deep")), 0o444 & allowed(&root));
       }

       #[test]
       fn place_of_a_digest_not_held_fails() {
           let (_dir, root, cas) = store();
           let digest = Digest::of(b"hello");

           let error = cas.place(&digest, FileMode::Plain, &at("app")).unwrap_err();

           let Error::Adapter { port, diagnostic } = error else {
               unreachable!("an adapter error, got {error:?}")
           };
           assert_eq!(port, PortName::ContentStore);
           assert_eq!(
               diagnostic.as_str(),
               format!(
                   "placing {digest} at {}: the store does not hold it",
                   root.join("out/app").display()
               )
           );
           assert!(!root.join("out").exists());
       }

       #[test]
       fn place_names_a_file_standing_where_a_directory_is_needed() {
           let (_dir, root, cas) = store();
           let digest = cas.put(b"hello").unwrap();
           fs::create_dir_all(root.join("out")).unwrap();
           fs::write(root.join("out/app"), b"an older output").unwrap();

           for name in ["app/bin", "app/share/doc/text"] {
               let error = cas.place(&digest, FileMode::Plain, &at(name)).unwrap_err();

               let Error::Adapter { port, diagnostic } = error else {
                   unreachable!("an adapter error, got {error:?}")
               };
               assert_eq!(port, PortName::ContentStore);
               assert_eq!(
                   diagnostic.as_str(),
                   format!(
                       "placing a file at {}: {} is in the way and is not a directory",
                       root.join("out").join(name).display(),
                       root.join("out/app").display()
                   )
               );
               assert_eq!(fs::read(root.join("out/app")).unwrap(), b"an older output");
           }
       }
   }
   ```

3. Run the tests and confirm they fail to compile:

   ```
   $ cargo test -p buildl --lib cas::local_cas
   ```

   Expected:

   ```text
   error[E0432]: unresolved import `super::LocalCas`
     --> crates/buildl/src/cas/local_cas.rs:28:9
   error[E0432]: unresolved import `local_cas::LocalCas`
     --> crates/buildl/src/cas/mod.rs:20:9
   error: could not compile `buildl` (lib test) due to 2 previous errors
   ```

4. Add the implementation. In `crates/buildl/src/cas/local_cas.rs`, insert between the module docs and the line `#[cfg(test)]`, with one blank line before it and one after:

   ```rust
   use std::fs;
   use std::io::{self, ErrorKind};
   use std::path::{Path, PathBuf};

   use buildl_core::{
       ContentStore, Diagnostic, Digest, Error, FileMode, OutputPath, PortName, Result,
   };

   use crate::cas::blob::BlobPath;
   use crate::state::{Access, StateLayout, clear_path, write_whole};

   /// Holds content under a workspace's state directory, and makes held content a file below the
   /// workspace root.
   ///
   /// Each digest has a blob, a read-only file holding the bytes, and may have an executable twin,
   /// a second read-only file with the same bytes that may be run. A file of the workspace that
   /// may be run is made from the twin, any other from the blob.
   #[derive(Debug)]
   pub struct LocalCas {
       root: PathBuf,
       blobs: PathBuf,
       staging: PathBuf,
   }

   /// The store's failure doing `what` to `path`.
   fn failed(what: &str, path: &Path, error: &io::Error) -> Error {
       refused(format!("{what} {}: {error}", path.display()))
   }

   /// A failure of the store, worded by `message`.
   fn refused(message: String) -> Error {
       Error::Adapter {
           port: PortName::ContentStore,
           diagnostic: Diagnostic::new(message),
       }
   }

   /// Whether a regular file is at `path`. A path that cannot be examined holds none.
   fn is_file(path: &Path) -> bool {
       fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_file())
   }

   /// Whether something other than `bytes` is at `twin`. Nothing being there is not a difference:
   /// only a placement makes a twin.
   fn twin_differs(twin: &Path, bytes: &[u8]) -> bool {
       match fs::read(twin) {
           Ok(held) => held != bytes,
           Err(error) => error.kind() != ErrorKind::NotFound,
       }
   }

   impl LocalCas {
       /// A store for the workspace at `root`, keeping content where `layout` says.
       ///
       /// Touches nothing on disk: the store's directories are made by the first
       /// [`put`](ContentStore::put).
       #[must_use]
       pub fn new(root: &Path, layout: &StateLayout) -> Self {
           Self {
               root: root.to_path_buf(),
               blobs: layout.blobs(),
               staging: layout.blob_staging(),
           }
       }

       /// Makes the executable twin of a blob when it is missing, from the blob's bytes once they
       /// are seen to have the digest.
       fn ensure_twin(&self, paths: &BlobPath, digest: &Digest) -> Result<()> {
           let twin = paths.for_mode(FileMode::Executable);
           if is_file(twin) {
               return Ok(());
           }
           let blob = paths.for_mode(FileMode::Plain);
           let bytes = fs::read(blob).map_err(|error| failed("reading", blob, &error))?;
           if Digest::of(&bytes) != *digest {
               return Err(refused(format!(
                   "{} does not hold the bytes its name says",
                   blob.display()
               )));
           }
           write_whole(&self.staging, twin, &bytes, Access::ReadOnlyRunnable)
               .map_err(|error| failed("writing", twin, &error))
       }

       /// The nearest path above `target`, below the workspace root, that holds something other
       /// than a directory.
       fn in_the_way(&self, target: &Path) -> Option<PathBuf> {
           target
               .ancestors()
               .skip(1)
               .take_while(|above| *above != self.root)
               .find(|above| fs::symlink_metadata(above).is_ok() && !above.is_dir())
               .map(Path::to_path_buf)
       }

       /// Makes every directory above `target` that is missing.
       ///
       /// A file standing where a directory is needed is named and left as it is: the output
       /// directory is the user's choice, and the store removes only at a path it was asked to
       /// clear.
       fn make_parent(&self, target: &Path) -> Result<()> {
           let Some(parent) = target.parent() else {
               return Ok(());
           };
           fs::create_dir_all(parent).map_err(|error| {
               self.in_the_way(target).map_or_else(
                   || failed("making directory", parent, &error),
                   |obstacle| {
                       refused(format!(
                           "placing a file at {}: {} is in the way and is not a directory",
                           target.display(),
                           obstacle.display()
                       ))
                   },
               )
           })
       }

       /// Makes `target` a file of its own holding the bytes of `source`, replacing what is there.
       fn copy(&self, source: &Path, target: &Path, access: Access) -> Result<()> {
           let bytes = fs::read(source).map_err(|error| failed("reading", source, &error))?;
           clear_path(target).map_err(|error| failed("clearing", target, &error))?;
           self.make_parent(target)?;
           let beside = target.parent().unwrap_or(&self.root);
           write_whole(beside, target, &bytes, access)
               .map_err(|error| failed("writing", target, &error))
       }
   }

   impl ContentStore for LocalCas {
       /// Stores `bytes` and returns their digest.
       ///
       /// The blob, and the twin when there is one, are each read and compared with `bytes`, and
       /// written again only when they differ: a name is never trusted over the bytes in hand, so
       /// storing content again heals a damaged file. No twin is made and no file is removed.
       fn put(&self, bytes: &[u8]) -> Result<Digest> {
           let digest = Digest::of(bytes);
           let paths = BlobPath::of(&self.blobs, &digest);
           let blob = paths.for_mode(FileMode::Plain);
           if !fs::read(blob).is_ok_and(|held| held == bytes) {
               write_whole(&self.staging, blob, bytes, Access::ReadOnly)
                   .map_err(|error| failed("writing", blob, &error))?;
           }
           let twin = paths.for_mode(FileMode::Executable);
           if twin_differs(twin, bytes) {
               write_whole(&self.staging, twin, bytes, Access::ReadOnlyRunnable)
                   .map_err(|error| failed("writing", twin, &error))?;
           }
           Ok(digest)
       }

       fn contains(&self, digest: &Digest) -> bool {
           is_file(BlobPath::of(&self.blobs, digest).for_mode(FileMode::Plain))
       }

       /// The content held under that digest.
       ///
       /// A blob whose bytes do not have the digest it is named by is removed with its twin, and
       /// the content is then not held.
       fn get(&self, digest: &Digest) -> Result<Option<Vec<u8>>> {
           let paths = BlobPath::of(&self.blobs, digest);
           let blob = paths.for_mode(FileMode::Plain);
           let bytes = match fs::read(blob) {
               Ok(bytes) => bytes,
               Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
               Err(error) => return Err(failed("reading", blob, &error)),
           };
           if Digest::of(&bytes) == *digest {
               return Ok(Some(bytes));
           }
           for damaged in [blob, paths.for_mode(FileMode::Executable)] {
               clear_path(damaged).map_err(|error| failed("removing", damaged, &error))?;
           }
           Ok(None)
       }

       fn clear(&self, at: &OutputPath) -> Result<()> {
           let target = self.root.join(at.as_str());
           clear_path(&target).map_err(|error| failed("clearing", &target, &error))
       }

       /// Makes the content with that digest the file at `at`.
       ///
       /// A file that may be run is made from the executable twin, which is made first when it is
       /// missing; any other from the blob.
       fn place(&self, digest: &Digest, mode: FileMode, at: &OutputPath) -> Result<()> {
           let paths = BlobPath::of(&self.blobs, digest);
           let target = self.root.join(at.as_str());
           if !is_file(paths.for_mode(FileMode::Plain)) {
               return Err(refused(format!(
                   "placing {digest} at {}: the store does not hold it",
                   target.display()
               )));
           }
           let access = match mode {
               FileMode::Plain => Access::ReadOnly,
               FileMode::Executable => {
                   self.ensure_twin(&paths, digest)?;
                   Access::ReadOnlyRunnable
               }
           };
           self.copy(paths.for_mode(mode), &target, access)
       }
   }
   ```

   Four things in it are easy to get wrong. `put` reads the blob and compares before it skips, and treats the twin the same way, but never makes a twin and never removes a file: only `place` makes one. `get` removes a blob that does not hash to its name together with its twin. `copy` clears the path first, because a rename onto a directory fails. `make_parent` names the file that stands where a directory is needed and leaves it alone.

5. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl --lib cas::local_cas
   ```

   Expected:

   ```text
   test cas::local_cas::tests::clear_removes_a_file_or_a_directory_and_nothing_is_no_error ... ok
   test cas::local_cas::tests::eight_threads_put_the_same_bytes ... ok
   test cas::local_cas::tests::get_drops_a_blob_that_does_not_hash_to_its_name ... ok
   test cas::local_cas::tests::get_of_nothing_is_none ... ok
   test cas::local_cas::tests::place_makes_directories_and_replaces_a_file_or_a_directory ... ok
   test cas::local_cas::tests::place_names_a_file_standing_where_a_directory_is_needed ... ok
   test cas::local_cas::tests::place_of_a_digest_not_held_fails ... ok
   test cas::local_cas::tests::put_holds_the_bytes_under_their_digest_read_only ... ok
   test cas::local_cas::tests::put_makes_no_twin ... ok
   test cas::local_cas::tests::put_of_held_bytes_keeps_the_inode_of_the_blob_and_of_its_twin ... ok
   test cas::local_cas::tests::put_replaces_a_blob_whose_bytes_differ ... ok
   test cas::local_cas::tests::put_replaces_a_twin_whose_bytes_differ_and_removes_none ... ok
   test cas::local_cas::tests::the_twin_stays_whatever_order_puts_and_places_come_in ... ok
   test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 21 filtered out
   ```

6. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo make deny
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing. `cargo make dod` prints `guard-adapter-isolation: OK (1 edges)` among its first lines, and its last line begins `[cargo-make] INFO - Build Done in`. `cargo make deny` prints `advisories ok, bans ok, licenses ok, sources ok`.

7. Commit:

   ```
   $ git add crates/buildl/src/cas/mod.rs crates/buildl/src/cas/local_cas.rs
   $ git commit -m "feat(buildl): add the content store, placing files by copy"
   ```

### Task 3 — Probe whether a store may place by hard link

**Files:**
- Modify `crates/buildl/src/cas/mod.rs`
- Create `crates/buildl/src/cas/placement.rs`

**Steps:**

1. Replace the whole of `crates/buildl/src/cas/mod.rs` with:

   ```rust
   //! The content store on disk: bytes held by their digest, and held bytes made files of the
   //! workspace.
   //!
   //! Its own module because it is the one place that decides how content is laid out under the
   //! state directory and how it reaches the output directory: a read-only blob per digest, and
   //! beside it an executable twin for content that was placed as a file that may be run.
   //!
   //! Responsibilities: [`LocalCas`], the [`ContentStore`](buildl_core::ContentStore) of a local
   //! workspace.
   //!
   //! Non-responsibilities: deciding what is stored, cleared or placed, which the build does; and
   //! where the state directory keeps the store, which
   //! [`StateLayout`](crate::state::StateLayout) says.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

   mod blob;
   mod local_cas;
   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "nothing public reaches this module until the store places a file by a link"
       )
   )]
   mod placement;

   pub use local_cas::LocalCas;
   ```

   The `cfg_attr` is deliberate: nothing calls `Placement` until task 4, which replaces this file without the attribute.

2. Create `crates/buildl/src/cas/placement.rs` holding the module docs and the tests only:

   ```rust
   //! How a held file becomes a file of the workspace: by a hard link, or by a copy.
   //!
   //! Its own file because the choice is made once per store, by trying rather than by asking what
   //! the filesystem or the user is. A link is safe only where a write through it is refused, and
   //! the read-only mode of a held file refuses it only when permission bits are kept and bind the
   //! user: not on FAT, and not for root.
   //!
   //! Responsibilities: [`Placement`] and the probe that chooses it.
   //!
   //! Non-responsibilities: linking or copying, which the store does.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::fs::{self, OpenOptions};

       use super::Placement;
       use crate::state::{Access, write_whole};

       #[test]
       fn the_probe_answers_link_exactly_when_a_write_to_a_read_only_file_is_refused() {
           let dir = tempfile::tempdir().unwrap();
           // What this user and this filesystem do with a read-only file, seen directly.
           let witness = dir.path().join("witness");
           write_whole(dir.path(), &witness, b"", Access::ReadOnly).unwrap();
           let refused = OpenOptions::new().write(true).open(&witness).is_err();

           let placement = Placement::probe(&dir.path().join("cas/tmp")).unwrap();

           let expected = if refused {
               Placement::Link
           } else {
               Placement::Copy
           };
           assert_eq!(placement, expected);
       }

       #[test]
       fn the_probe_makes_its_directory_and_leaves_nothing_in_it() {
           let dir = tempfile::tempdir().unwrap();
           let staging = dir.path().join("cas/tmp");

           Placement::probe(&staging).unwrap();
           Placement::probe(&staging).unwrap();

           assert_eq!(fs::read_dir(&staging).unwrap().count(), 0);
       }

       #[test]
       fn a_probe_that_cannot_write_its_file_is_an_error() {
           let dir = tempfile::tempdir().unwrap();
           let file = dir.path().join("file");
           fs::write(&file, b"").unwrap();

           assert!(Placement::probe(&file.join("tmp")).is_err());
       }
   }
   ```

3. Run the tests and confirm they fail to compile:

   ```
   $ cargo test -p buildl --lib cas::placement
   ```

   Expected:

   ```text
   error[E0432]: unresolved import `super::Placement`
     --> crates/buildl/src/cas/placement.rs:21:9
   error: could not compile `buildl` (lib test) due to 1 previous error
   ```

4. Add the implementation. In `crates/buildl/src/cas/placement.rs`, insert between the module docs and the line `#[cfg(test)]`, with one blank line before it and one after:

   ```rust
   use std::fs::OpenOptions;
   use std::io::{self, ErrorKind};
   use std::path::Path;
   use std::thread;

   use crate::state::{Access, clear_path, write_whole};

   /// How a held file becomes a file of the workspace.
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   pub(crate) enum Placement {
       /// By a hard link: the file of the workspace and the held file are one file.
       Link,
       /// By a copy: the file of the workspace is a file of its own.
       Copy,
   }

   impl Placement {
       /// `Link` when a write to a read-only file in `staging` is refused, `Copy` when it is
       /// allowed.
       ///
       /// A read-only file is written in `staging`, opened for writing, and removed. Where the
       /// write is let through, a link would let a write to a file of the workspace change what the
       /// store holds, so nothing is linked there.
       pub(crate) fn probe(staging: &Path) -> io::Result<Self> {
           // The name is this thread's own, so two threads probing one store at once do not remove
           // each other's file.
           let witness = staging.join(format!("probe-{:?}", thread::current().id()));
           write_whole(staging, &witness, b"", Access::ReadOnly)?;
           let answer = match OpenOptions::new().write(true).open(&witness) {
               Ok(_) => Ok(Self::Copy),
               Err(error) if error.kind() == ErrorKind::PermissionDenied => Ok(Self::Link),
               Err(error) => Err(error),
           };
           clear_path(&witness)?;
           answer
       }
   }
   ```

5. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl --lib cas::placement
   ```

   Expected:

   ```text
   test cas::placement::tests::a_probe_that_cannot_write_its_file_is_an_error ... ok
   test cas::placement::tests::the_probe_answers_link_exactly_when_a_write_to_a_read_only_file_is_refused ... ok
   test cas::placement::tests::the_probe_makes_its_directory_and_leaves_nothing_in_it ... ok
   test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 34 filtered out
   ```

6. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo make deny
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing. `cargo make dod` prints `guard-adapter-isolation: OK (1 edges)` among its first lines, and its last line begins `[cargo-make] INFO - Build Done in`. `cargo make deny` prints `advisories ok, bans ok, licenses ok, sources ok`.

7. Commit:

   ```
   $ git add crates/buildl/src/cas/mod.rs crates/buildl/src/cas/placement.rs
   $ git commit -m "feat(buildl): probe whether a store may place by hard link"
   ```

### Task 4 — Place by hard link where a write through it is refused

**Files:**
- Modify `crates/buildl/src/cas/local_cas.rs`
- Modify `crates/buildl/src/cas/mod.rs`

**Steps:**

1. Write the failing tests. In the `tests` module of `crates/buildl/src/cas/local_cas.rs`, make these replacements: the imports the new tests need, then the tests themselves, added after the last test of the module.

   Replace:

   ```rust
       )]

       use std::fs::{self, OpenOptions};
       use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
       use std::path::{Path, PathBuf};
   ```

   With:

   ```rust
       )]

       use std::cell::Cell;
       use std::fs::{self, OpenOptions};
       use std::io::{self, ErrorKind};
       use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
       use std::path::{Path, PathBuf};
   ```

   Replace:

   ```rust
       use super::LocalCas;
       use crate::cas::blob::BlobPath;
       use crate::state::StateLayout;
   ```

   With:

   ```rust
       use super::LocalCas;
       use crate::cas::blob::BlobPath;
       use crate::cas::placement::Placement;
       use crate::state::StateLayout;
   ```

   Replace:

   ```rust
               assert_eq!(fs::read(root.join("out/app")).unwrap(), b"an older output");
           }
       }
   }
   ```

   With:

   ```rust
               assert_eq!(fs::read(root.join("out/app")).unwrap(), b"an older output");
           }
       }

       /// Whether a store in the workspace at `root` links: not for a user the permission bits do
       /// not bind, and not on a filesystem that does not keep them.
       fn links(root: &Path) -> bool {
           Placement::probe(&root.join(".buildl/cas/tmp")).unwrap() == Placement::Link
       }

       #[test]
       fn a_plain_file_is_a_link_to_the_blob() {
           let (_dir, root, cas) = store();
           if !links(&root) {
               return;
           }
           let digest = cas.put(b"hello").unwrap();

           cas.place(&digest, FileMode::Plain, &at("text")).unwrap();

           let placed = root.join("out/text");
           assert_eq!(inode_of(&placed), inode_of(&root.join(HELLO)));
           assert_eq!(mode_of(&placed), 0o444 & allowed(&root));
       }

       #[test]
       fn one_blob_placed_plain_and_executable_uses_two_inodes() {
           let (_dir, root, cas) = store();
           if !links(&root) {
               return;
           }
           let digest = cas.put(b"hello").unwrap();

           cas.place(&digest, FileMode::Plain, &at("lib/text"))
               .unwrap();
           cas.place(&digest, FileMode::Executable, &at("bin/tool"))
               .unwrap();
           cas.place(&digest, FileMode::Executable, &at("bin/other"))
               .unwrap();

           let blob = inode_of(&root.join(HELLO));
           let twin = inode_of(&root.join(format!("{HELLO}.x")));
           assert_ne!(blob, twin);
           assert_eq!(inode_of(&root.join("out/lib/text")), blob);
           assert_eq!(inode_of(&root.join("out/bin/tool")), twin);
           assert_eq!(inode_of(&root.join("out/bin/other")), twin);
           assert_eq!(mode_of(&root.join("out/lib/text")), 0o444 & allowed(&root));
           assert_eq!(mode_of(&root.join("out/bin/tool")), 0o555 & allowed(&root));
       }

       #[test]
       fn a_write_through_a_placed_file_is_refused() {
           let (_dir, root, cas) = store();
           if !links(&root) {
               return;
           }
           let digest = cas.put(b"hello").unwrap();
           cas.place(&digest, FileMode::Plain, &at("text")).unwrap();
           cas.place(&digest, FileMode::Executable, &at("tool"))
               .unwrap();

           let mut write = OpenOptions::new();
           write.write(true);
           let mut truncate = OpenOptions::new();
           truncate.write(true).truncate(true);
           let mut append = OpenOptions::new();
           append.append(true);
           for placed in ["out/text", "out/tool"] {
               for how in [&write, &truncate, &append] {
                   let error = how.open(root.join(placed)).unwrap_err();
                   assert_eq!(error.kind(), ErrorKind::PermissionDenied, "{placed}");
               }
           }

           assert_eq!(
               held(&root),
               [
                   (HELLO.to_owned(), 0o444 & allowed(&root), b"hello".to_vec()),
                   (
                       format!("{HELLO}.x"),
                       0o555 & allowed(&root),
                       b"hello".to_vec()
                   ),
               ]
           );
       }

       #[test]
       fn a_store_that_copies_shares_no_inode() {
           let (_dir, root, cas) = store();
           cas.placement.set(Placement::Copy).unwrap();
           let digest = cas.put(b"hello").unwrap();

           cas.place(&digest, FileMode::Plain, &at("text")).unwrap();
           cas.place(&digest, FileMode::Executable, &at("tool"))
               .unwrap();

           let held = [
               inode_of(&root.join(HELLO)),
               inode_of(&root.join(format!("{HELLO}.x"))),
           ];
           let (text, tool) = (root.join("out/text"), root.join("out/tool"));
           assert!(!held.contains(&inode_of(&text)));
           assert!(!held.contains(&inode_of(&tool)));
           assert_eq!(
               (mode_of(&text), fs::read(&text).unwrap()),
               (0o444 & allowed(&root), b"hello".to_vec())
           );
           assert_eq!(
               (mode_of(&tool), fs::read(&tool).unwrap()),
               (0o555 & allowed(&root), b"hello".to_vec())
           );

           // A copy replaces a directory standing at the path, as a link does.
           fs::create_dir_all(root.join("out/tree/inner")).unwrap();
           fs::write(root.join("out/tree/inner/old"), b"old").unwrap();
           cas.place(&digest, FileMode::Plain, &at("tree")).unwrap();
           assert_eq!(fs::read(root.join("out/tree")).unwrap(), b"hello");
       }

       #[test]
       fn a_link_that_fails_is_followed_by_a_copy() {
           let (_dir, root, cas) = store();
           cas.placement.set(Placement::Link).unwrap();
           let digest = cas.put(b"hello").unwrap();
           let placed = root.join("out/bin/tool");

           // Whatever the link answers, and it answers differently on each system, the file is made.
           for kind in [
               ErrorKind::Unsupported,
               ErrorKind::CrossesDevices,
               ErrorKind::NotFound,
               ErrorKind::AlreadyExists,
           ] {
               let failing = |_: &Path, _: &Path| Err(io::Error::from(kind));

               cas.place_by(&digest, FileMode::Executable, &at("bin/tool"), failing)
                   .unwrap();

               assert_eq!(
                   (mode_of(&placed), fs::read(&placed).unwrap()),
                   (0o555 & allowed(&root), b"hello".to_vec()),
                   "{kind:?}"
               );
               assert_ne!(
                   inode_of(&placed),
                   inode_of(&root.join(format!("{HELLO}.x"))),
                   "{kind:?}"
               );
           }
       }

       #[test]
       fn a_twin_gone_when_the_link_is_made_is_made_again() {
           let (_dir, root, cas) = store();
           cas.placement.set(Placement::Link).unwrap();
           let digest = cas.put(b"hello").unwrap();
           let attempts = Cell::new(0);
           // The first attempt finds the twin gone, as it would be had something removed it since
           // the store saw it; the second is the real link.
           let link = |source: &Path, target: &Path| {
               attempts.set(attempts.get() + 1);
               if attempts.get() == 1 {
                   fs::remove_file(source)?;
               }
               fs::hard_link(source, target)
           };

           cas.place_by(&digest, FileMode::Executable, &at("bin/tool"), link)
               .unwrap();

           assert_eq!(attempts.get(), 2);
           assert_eq!(
               inode_of(&root.join("out/bin/tool")),
               inode_of(&root.join(format!("{HELLO}.x")))
           );
           assert_eq!(fs::read(root.join("out/bin/tool")).unwrap(), b"hello");
       }
   }
   ```

2. Run the tests and confirm they fail to compile: the store has no `placement` to force and no `place_by` to hand a link to.

   ```
   $ cargo test -p buildl --lib cas::local_cas
   ```

   Expected:

   ```text
   error[E0609]: no field `placement` on type `LocalCas`
      --> crates/buildl/src/cas/local_cas.rs:701:13
   error[E0609]: no field `placement` on type `LocalCas`
      --> crates/buildl/src/cas/local_cas.rs:734:13
   error[E0599]: no method named `place_by` found for struct `LocalCas` in the current scope
      --> crates/buildl/src/cas/local_cas.rs:747:17
   error[E0609]: no field `placement` on type `LocalCas`
      --> crates/buildl/src/cas/local_cas.rs:766:13
   error[E0599]: no method named `place_by` found for struct `LocalCas` in the current scope
      --> crates/buildl/src/cas/local_cas.rs:779:13
   error: could not compile `buildl` (lib test) due to 5 previous errors
   ```

3. Write the implementation. In `crates/buildl/src/cas/local_cas.rs`, above the `tests` module, make these replacements: the two imports, the field with what the type's docs say of it, the field's first value with the method that probes it, `place_by`, and `place` handing it `fs::hard_link`.

   Replace:

   ```rust
   use std::io::{self, ErrorKind};
   use std::path::{Path, PathBuf};

   use buildl_core::{
   ```

   With:

   ```rust
   use std::io::{self, ErrorKind};
   use std::path::{Path, PathBuf};
   use std::sync::OnceLock;

   use buildl_core::{
   ```

   Replace:

   ```rust

   use crate::cas::blob::BlobPath;
   use crate::state::{Access, StateLayout, clear_path, write_whole};
   ```

   With:

   ```rust

   use crate::cas::blob::BlobPath;
   use crate::cas::placement::Placement;
   use crate::state::{Access, StateLayout, clear_path, write_whole};
   ```

   Replace:

   ```rust
   /// a second read-only file with the same bytes that may be run. A file of the workspace that
   /// may be run is made from the twin, any other from the blob.
   #[derive(Debug)]
   pub struct LocalCas {
       root: PathBuf,
       blobs: PathBuf,
       staging: PathBuf,
   }

   /// The store's failure doing `what` to `path`.
   ```

   With:

   ```rust
   /// a second read-only file with the same bytes that may be run. A file of the workspace that
   /// may be run is made from the twin, any other from the blob.
   ///
   /// A file of the workspace is a hard link to the held file where a write through the link is
   /// seen to be refused, and a copy of it everywhere else, so that writing to an output never
   /// changes what the store holds.
   #[derive(Debug)]
   pub struct LocalCas {
       root: PathBuf,
       blobs: PathBuf,
       staging: PathBuf,
       placement: OnceLock<Placement>,
   }

   /// The store's failure doing `what` to `path`.
   ```

   Replace:

   ```rust
               blobs: layout.blobs(),
               staging: layout.blob_staging(),
           }
       }
   ```

   With:

   ```rust
               blobs: layout.blobs(),
               staging: layout.blob_staging(),
               placement: OnceLock::new(),
           }
       }

       /// How this store places a file: probed on the first call, and kept.
       fn placement(&self) -> Result<Placement> {
           if let Some(known) = self.placement.get() {
               return Ok(*known);
           }
           let probed = Placement::probe(&self.staging)
               .map_err(|error| failed("probing", &self.staging, &error))?;
           Ok(*self.placement.get_or_init(|| probed))
       }
   ```

   Replace:

   ```rust
               .map_err(|error| failed("writing", target, &error))
       }
   }
   ```

   With:

   ```rust
               .map_err(|error| failed("writing", target, &error))
       }

       /// [`place`](ContentStore::place), with the step that makes a hard link handed in.
       ///
       /// Where the store links, a link that fails for want of a directory, or because something
       /// is at the path, is tried once more after that is seen to; a link that fails for any
       /// other reason, or a second time, is followed by a copy, whose failure is the one
       /// reported. The link is never made beside the path and renamed onto it: renaming one name
       /// of a file onto another name of the same file does nothing, and leaves both.
       fn place_by(
           &self,
           digest: &Digest,
           mode: FileMode,
           at: &OutputPath,
           link: impl Fn(&Path, &Path) -> io::Result<()>,
       ) -> Result<()> {
           let paths = BlobPath::of(&self.blobs, digest);
           let target = self.root.join(at.as_str());
           if !is_file(paths.for_mode(FileMode::Plain)) {
               return Err(refused(format!(
                   "placing {digest} at {}: the store does not hold it",
                   target.display()
               )));
           }
           let access = match mode {
               FileMode::Plain => Access::ReadOnly,
               FileMode::Executable => {
                   self.ensure_twin(&paths, digest)?;
                   Access::ReadOnlyRunnable
               }
           };
           let source = paths.for_mode(mode);
           if self.placement()? == Placement::Link {
               let again = match link(source, &target) {
                   Ok(()) => return Ok(()),
                   Err(error) if error.kind() == ErrorKind::NotFound => {
                       self.make_parent(&target)?;
                       if mode == FileMode::Executable {
                           self.ensure_twin(&paths, digest)?;
                       }
                       true
                   }
                   Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                       clear_path(&target).map_err(|error| failed("clearing", &target, &error))?;
                       true
                   }
                   Err(_) => false,
               };
               if again && link(source, &target).is_ok() {
                   return Ok(());
               }
           }
           self.copy(source, &target, access)
       }
   }
   ```

   Replace:

   ```rust
       ///
       /// A file that may be run is made from the executable twin, which is made first when it is
       /// missing; any other from the blob.
       fn place(&self, digest: &Digest, mode: FileMode, at: &OutputPath) -> Result<()> {
           let paths = BlobPath::of(&self.blobs, digest);
           let target = self.root.join(at.as_str());
           if !is_file(paths.for_mode(FileMode::Plain)) {
               return Err(refused(format!(
                   "placing {digest} at {}: the store does not hold it",
                   target.display()
               )));
           }
           let access = match mode {
               FileMode::Plain => Access::ReadOnly,
               FileMode::Executable => {
                   self.ensure_twin(&paths, digest)?;
                   Access::ReadOnlyRunnable
               }
           };
           self.copy(paths.for_mode(mode), &target, access)
       }
   }
   ```

   With:

   ```rust
       ///
       /// A file that may be run is made from the executable twin, which is made first when it is
       /// missing; any other from the blob. The file is a hard link where the store links, and a
       /// copy where it does not or where the link cannot be made.
       fn place(&self, digest: &Digest, mode: FileMode, at: &OutputPath) -> Result<()> {
           self.place_by(digest, mode, at, |source, target| {
               fs::hard_link(source, target)
           })
       }
   }
   ```

   The order inside `place_by` is the rule: a link is tried first; `NotFound` means a directory above the file is missing or the twin has gone, so both are seen to and the link is tried once more; `AlreadyExists` means something is at the path, so it is cleared and the link is tried once more; any other failure, and any second failure, is followed by the copy, whose error is the one reported. No table of error kinds is kept, because they differ per system.

4. Replace the whole of `crates/buildl/src/cas/mod.rs` with:

   ```rust
   //! The content store on disk: bytes held by their digest, and held bytes made files of the
   //! workspace.
   //!
   //! Its own module because it is the one place that decides how content is laid out under the
   //! state directory and how it reaches the output directory: a read-only blob per digest, beside
   //! it an executable twin for content that was placed as a file that may be run, and a hard link
   //! from the output directory wherever a write through one is seen to be refused.
   //!
   //! Responsibilities: [`LocalCas`], the [`ContentStore`](buildl_core::ContentStore) of a local
   //! workspace.
   //!
   //! Non-responsibilities: deciding what is stored, cleared or placed, which the build does; and
   //! where the state directory keeps the store, which
   //! [`StateLayout`](crate::state::StateLayout) says.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

   mod blob;
   mod local_cas;
   mod placement;

   pub use local_cas::LocalCas;
   ```

5. Run every test of the module and confirm they pass:

   ```
   $ cargo test -p buildl --lib cas::
   ```

   Expected:

   ```text
   test cas::blob::tests::a_digest_splits_into_shard_and_rest ... ok
   test cas::local_cas::tests::a_link_that_fails_is_followed_by_a_copy ... ok
   test cas::local_cas::tests::a_plain_file_is_a_link_to_the_blob ... ok
   test cas::local_cas::tests::a_store_that_copies_shares_no_inode ... ok
   test cas::local_cas::tests::a_twin_gone_when_the_link_is_made_is_made_again ... ok
   test cas::local_cas::tests::a_write_through_a_placed_file_is_refused ... ok
   test cas::local_cas::tests::clear_removes_a_file_or_a_directory_and_nothing_is_no_error ... ok
   test cas::local_cas::tests::eight_threads_put_the_same_bytes ... ok
   test cas::local_cas::tests::get_drops_a_blob_that_does_not_hash_to_its_name ... ok
   test cas::local_cas::tests::get_of_nothing_is_none ... ok
   test cas::local_cas::tests::one_blob_placed_plain_and_executable_uses_two_inodes ... ok
   test cas::local_cas::tests::place_makes_directories_and_replaces_a_file_or_a_directory ... ok
   test cas::local_cas::tests::place_names_a_file_standing_where_a_directory_is_needed ... ok
   test cas::local_cas::tests::place_of_a_digest_not_held_fails ... ok
   test cas::local_cas::tests::put_holds_the_bytes_under_their_digest_read_only ... ok
   test cas::local_cas::tests::put_makes_no_twin ... ok
   test cas::local_cas::tests::put_of_held_bytes_keeps_the_inode_of_the_blob_and_of_its_twin ... ok
   test cas::local_cas::tests::put_replaces_a_blob_whose_bytes_differ ... ok
   test cas::local_cas::tests::put_replaces_a_twin_whose_bytes_differ_and_removes_none ... ok
   test cas::local_cas::tests::the_twin_stays_whatever_order_puts_and_places_come_in ... ok
   test cas::placement::tests::a_probe_that_cannot_write_its_file_is_an_error ... ok
   test cas::placement::tests::the_probe_answers_link_exactly_when_a_write_to_a_read_only_file_is_refused ... ok
   test cas::placement::tests::the_probe_makes_its_directory_and_leaves_nothing_in_it ... ok
   test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 20 filtered out
   ```

6. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo make deny
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing. `cargo make dod` prints `guard-adapter-isolation: OK (1 edges)` among its first lines, and its last line begins `[cargo-make] INFO - Build Done in`. `cargo make deny` prints `advisories ok, bans ok, licenses ok, sources ok`.

7. Commit:

   ```
   $ git add crates/buildl/src/cas/local_cas.rs crates/buildl/src/cas/mod.rs
   $ git commit -m "feat(buildl): place by hard link where a write through it is refused"
   ```

### Task 5 — Add the action cache's journal

The journal's writing side: `new`, `path`, `append` and `remove`. Reading it back arrives in task 7, with the replay that needs it.

**Files:**
- Modify `crates/buildl/src/cache/mod.rs`
- Create `crates/buildl/src/cache/journal.rs`

**Steps:**

1. Replace the whole of `crates/buildl/src/cache/mod.rs` with:

   ```rust
   //! The action cache on disk: the row each target's last successful build recorded.
   //!
   //! Its own module because it is the one place that decides how rows are kept across runs: a
   //! journal that takes one line for each row as it is recorded.
   //!
   //! Responsibilities: the journal.
   //!
   //! Non-responsibilities: deciding when a row is written, which the build does.
   //!
   //! This file holds only module declarations, so it carries no logic to unit-test.

   #[cfg_attr(
       not(test),
       expect(
           dead_code,
           reason = "nothing public reaches this module until a cache records a row in it"
       )
   )]
   mod journal;
   ```

   The `cfg_attr` is deliberate: nothing calls `Journal` until task 6, which replaces this file without the attribute.

2. Create `crates/buildl/src/cache/journal.rs` holding the module docs and the tests only:

   ```rust
   //! The action cache's journal: one line for each row recorded since the snapshot was written.
   //!
   //! Its own file because a row must be kept from the moment it is recorded, and writing the whole
   //! cache again for each row costs time that grows with the square of the number of rows. One
   //! appended line costs the same however many rows there are.
   //!
   //! Responsibilities: [`Journal`], and [`Entry`], the line it holds.
   //!
   //! Non-responsibilities: the rows in memory and the snapshot, which the cache keeps.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::collections::BTreeMap;
       use std::fs::{self, File};

       use buildl_core::{
           ActionKey, Argument, CacheRow, Command, Digest, Error, KeyComponents, Label, NetworkAccess,
           OutputName, OwnComponents, Platform, PortName,
       };

       use super::{Entry, Journal};

       /// The line the journal holds for the entry of the target `a`.
       const LINE_A: &str = concat!(
           r#"{"label":"//:a","row":{"key":{"components":{"deps":[],"own":{"command":["cc"],"#,
           r#""env":{},"inputs":[],"network":"Sealed","outputs":["a"],"platform":"linux/x86_64","#,
           r#""settings":{},"tool":null}},"#,
           r#""digest":"c39a0161a4194c4ef0cedffc45bb82fdea72693ef4b6cc1f36177bed109be4d7"},"#,
           r#""outputs":["ca978112ca1bbdcafac231b39a23dc4da786eff8147c4e72b9807785afee48bb"],"#,
           r#""stderr":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855","#,
           r#""stdout":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"}}"#,
           "\n"
       );

       /// The entry of a target named `name` at the workspace root, which ran `cc` and produced
       /// one output of that name.
       fn entry(name: &str) -> Entry {
           let key = ActionKey::new(KeyComponents {
               own: OwnComponents {
                   command: Command::new(vec![Argument::parse("cc").unwrap()]).unwrap(),
                   inputs: Vec::new(),
                   outputs: vec![OutputName::parse(name).unwrap()],
                   env: BTreeMap::new(),
                   tool: None,
                   settings: BTreeMap::new(),
                   network: NetworkAccess::Sealed,
                   platform: Platform::parse("linux/x86_64").unwrap(),
               },
               deps: Vec::new(),
           })
           .unwrap();
           let nothing = Digest::of(b"");
           Entry {
               label: Label::parse(&format!("//:{name}")).unwrap(),
               row: CacheRow::new(key, vec![Digest::of(name.as_bytes())], nothing, nothing).unwrap(),
           }
       }

       #[test]
       fn nothing_is_on_disk_before_the_first_entry() {
           let dir = tempfile::tempdir().unwrap();
           let path = dir.path().join("cache.journal");

           let journal = Journal::new(path.clone());

           assert_eq!(journal.path(), path);
           assert!(!path.exists());
       }

       #[test]
       fn an_entry_is_appended_as_one_canonical_line() {
           let dir = tempfile::tempdir().unwrap();
           let path = dir.path().join("cache.journal");
           let mut journal = Journal::new(path.clone());

           journal.append(&entry("a")).unwrap();
           assert_eq!(fs::read_to_string(&path).unwrap(), LINE_A);

           journal.append(&entry("a")).unwrap();
           assert_eq!(fs::read_to_string(&path).unwrap(), LINE_A.repeat(2));
       }

       #[test]
       fn a_write_that_fails_closes_the_journal_for_good() {
           let dir = tempfile::tempdir().unwrap();
           let path = dir.path().join("cache.journal");
           let mut journal = Journal::new(path.clone());
           journal.append(&entry("a")).unwrap();

           // A handle that cannot be written to stands in for a disk that cannot.
           journal.file = Some(File::open(&path).unwrap());
           let error = journal.append(&entry("b")).unwrap_err();
           let Error::Adapter { port, diagnostic } = error else {
               unreachable!("an adapter error, got {error:?}")
           };
           assert_eq!(port, PortName::ActionCache);
           assert!(
               diagnostic
                   .as_str()
                   .starts_with(&format!("appending to {}: ", path.display())),
               "{diagnostic}"
           );

           // The next append would open the file afresh and could write; it is refused all the same.
           journal.file = None;
           let error = journal.append(&entry("c")).unwrap_err();
           assert_eq!(
               error.to_string(),
               format!(
                   "action cache: {} took part of a line, and takes no more",
                   path.display()
               )
           );
           assert_eq!(fs::read_to_string(&path).unwrap(), LINE_A);
       }

       #[test]
       fn remove_deletes_the_file_and_one_not_there_is_no_error() {
           let dir = tempfile::tempdir().unwrap();
           let path = dir.path().join("cache.journal");
           let mut journal = Journal::new(path.clone());

           journal.remove().unwrap();

           journal.append(&entry("b")).unwrap();
           journal.remove().unwrap();
           assert!(!path.exists());

           // An entry appended afterwards starts a new file.
           journal.append(&entry("a")).unwrap();
           assert_eq!(fs::read_to_string(&path).unwrap(), LINE_A);
       }
   }
   ```

3. Run the tests and confirm they fail to compile:

   ```
   $ cargo test -p buildl --lib cache::journal
   ```

   Expected:

   ```text
   error[E0432]: unresolved imports `super::Entry`, `super::Journal`
     --> crates/buildl/src/cache/journal.rs:26:17
   error: could not compile `buildl` (lib test) due to 1 previous error
   ```

4. Add the implementation. In `crates/buildl/src/cache/journal.rs`, insert between the module docs and the line `#[cfg(test)]`, with one blank line before it and one after:

   ```rust
   use std::fs::{self, File, OpenOptions};
   use std::io::{self, ErrorKind, Write};
   use std::os::unix::fs::OpenOptionsExt;
   use std::path::{Path, PathBuf};

   use buildl_core::json::canonical;
   use buildl_core::{CacheRow, Diagnostic, Error, Label, PortName, Result};
   use serde::{Deserialize, Serialize};

   /// One line of the journal: a target, and the row recorded for it.
   #[derive(Debug, Serialize, Deserialize)]
   #[serde(deny_unknown_fields)]
   pub(crate) struct Entry {
       /// The target.
       pub(crate) label: Label,
       /// The row recorded for it.
       pub(crate) row: CacheRow,
   }

   /// The file rows are appended to, one canonical line each.
   #[derive(Debug)]
   pub(crate) struct Journal {
       path: PathBuf,
       file: Option<File>,
       broken: bool,
   }

   /// The journal's failure doing `what` to `path`.
   fn failed(what: &str, path: &Path, error: &io::Error) -> Error {
       refused(format!("{what} {}: {error}", path.display()))
   }

   /// A failure of the journal, worded by `message`.
   fn refused(message: String) -> Error {
       Error::Adapter {
           port: PortName::ActionCache,
           diagnostic: Diagnostic::new(message),
       }
   }

   impl Journal {
       /// A journal at `path`. Touches nothing on disk: the file is made by the first entry.
       pub(crate) const fn new(path: PathBuf) -> Self {
           Self {
               path,
               file: None,
               broken: false,
           }
       }

       /// Where the journal's file is.
       pub(crate) fn path(&self) -> &Path {
           &self.path
       }

       /// The file, opened for appending on first use.
       fn opened(&mut self) -> io::Result<&mut File> {
           let file = match self.file.take() {
               Some(file) => file,
               None => OpenOptions::new()
                   .append(true)
                   .create(true)
                   .mode(0o644)
                   .open(&self.path)?,
           };
           Ok(self.file.insert(file))
       }

       /// Appends `entry` as one line, handed to the system in one write.
       ///
       /// After an append that failed the journal takes no more entries: part of a line may be in
       /// the file, and a line written after it would join it.
       pub(crate) fn append(&mut self, entry: &Entry) -> Result<()> {
           if self.broken {
               return Err(refused(format!(
                   "{} took part of a line, and takes no more",
                   self.path.display()
               )));
           }
           let mut line = canonical::to_vec(entry)?;
           line.push(b'\n');
           let appended = self.opened().and_then(|file| file.write_all(&line));
           appended.map_err(|error| {
               self.broken = true;
               failed("appending to", &self.path, &error)
           })
       }

       /// Removes the file. One that is not there is not an error.
       pub(crate) fn remove(&mut self) -> io::Result<()> {
           self.file = None;
           match fs::remove_file(&self.path) {
               Err(error) if error.kind() != ErrorKind::NotFound => Err(error),
               _ => Ok(()),
           }
       }
   }
   ```

   The file is opened in append mode on first use, asking for mode 0644, and each entry is one `write_all`. `remove` drops the handle before it unlinks, so the next append opens a new file and does not write to the one just removed.

5. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl --lib cache::journal
   ```

   Expected:

   ```text
   test cache::journal::tests::a_write_that_fails_closes_the_journal_for_good ... ok
   test cache::journal::tests::an_entry_is_appended_as_one_canonical_line ... ok
   test cache::journal::tests::nothing_is_on_disk_before_the_first_entry ... ok
   test cache::journal::tests::remove_deletes_the_file_and_one_not_there_is_no_error ... ok
   test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 43 filtered out
   ```

6. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo make deny
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing. `cargo make dod` prints `guard-adapter-isolation: OK (1 edges)` among its first lines, and its last line begins `[cargo-make] INFO - Build Done in`. `cargo make deny` prints `advisories ok, bans ok, licenses ok, sources ok`.

7. Commit:

   ```
   $ git add crates/buildl/src/cache/mod.rs crates/buildl/src/cache/journal.rs
   $ git commit -m "feat(buildl): add the action cache's journal"
   ```

### Task 6 — Add the action cache, folding its journal into cache.json

**Files:**
- Modify `crates/buildl/src/cache/mod.rs`
- Create `crates/buildl/src/cache/local_cache.rs`

**Steps:**

1. Replace the whole of `crates/buildl/src/cache/mod.rs` with:

   ```rust
   //! The action cache on disk: the row each target's last successful build recorded.
   //!
   //! Its own module because it is the one place that decides how rows are kept across runs: a
   //! canonical snapshot of every row, `cache.json`, and beside it a journal, `cache.journal`,
   //! that takes one line for each row as it is recorded. A row is kept from the moment it is
   //! recorded, and the snapshot is written once for a whole build.
   //!
   //! Responsibilities: [`LocalCache`], the [`ActionCache`](buildl_core::ActionCache) of a local
   //! workspace.
   //!
   //! Non-responsibilities: deciding when a row is written, which the build does; and where the
   //! state directory keeps the two files, which [`StateLayout`](crate::state::StateLayout) says.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

   mod journal;
   mod local_cache;

   pub use local_cache::LocalCache;
   ```

2. Create `crates/buildl/src/cache/local_cache.rs` holding the module docs and the tests only:

   ```rust
   //! [`LocalCache`], the action cache of a local workspace.
   //!
   //! Its own file because it holds the two ways rows are kept and the rule that joins them:
   //! `cache.json` is the canonical snapshot of every row, the journal holds what was recorded
   //! since the snapshot was written, and a fold writes the snapshot again and removes the journal.
   //!
   //! Responsibilities: [`LocalCache`], its [`ActionCache`] implementation, and the fold.
   //!
   //! Non-responsibilities: the journal's file, which `Journal` keeps; and deciding when to fold,
   //! which whoever ran the build does.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::collections::BTreeMap;
       use std::fs::{self, File, OpenOptions};
       use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
       use std::path::Path;
       use std::time::{Duration, UNIX_EPOCH};

       use buildl_core::{
           ActionCache, ActionKey, Argument, CacheRow, Command, Digest, Directory, Error,
           KeyComponents, Label, NetworkAccess, OutputName, OwnComponents, Platform, PortName,
       };

       use super::LocalCache;
       use crate::state::StateLayout;

       /// The layout of a new workspace whose state directory exists and is empty.
       fn workspace() -> (tempfile::TempDir, StateLayout) {
           let dir = tempfile::tempdir().unwrap();
           let layout = StateLayout::new(dir.path(), &Directory::parse(".buildl").unwrap());
           fs::create_dir_all(layout.dir()).unwrap();
           (dir, layout)
       }

       /// The label `//:<name>`.
       fn label(name: &str) -> Label {
           Label::parse(&format!("//:{name}")).unwrap()
       }

       /// The row of a target named `name` at the workspace root, which ran `cc` and produced one
       /// output of that name.
       fn row(name: &str) -> CacheRow {
           let key = ActionKey::new(KeyComponents {
               own: OwnComponents {
                   command: Command::new(vec![Argument::parse("cc").unwrap()]).unwrap(),
                   inputs: Vec::new(),
                   outputs: vec![OutputName::parse(name).unwrap()],
                   env: BTreeMap::new(),
                   tool: None,
                   settings: BTreeMap::new(),
                   network: NetworkAccess::Sealed,
                   platform: Platform::parse("linux/x86_64").unwrap(),
               },
               deps: Vec::new(),
           })
           .unwrap();
           let nothing = Digest::of(b"");
           CacheRow::new(key, vec![Digest::of(name.as_bytes())], nothing, nothing).unwrap()
       }

       /// The bytes the row of the target `name` is written as. `key` is the digest of its key and
       /// `output` the digest of its output, both as a run of this crate printed them.
       fn written(name: &str, key: &str, output: &str) -> String {
           [
               r#"{"key":{"components":{"deps":[],"own":{"command":["cc"],"env":{},"inputs":[],"#,
               r#""network":"Sealed","outputs":[""#,
               name,
               r#""],"platform":"linux/x86_64","settings":{},"tool":null}},"digest":""#,
               key,
               r#""},"outputs":[""#,
               output,
               r#""],"stderr":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855","#,
               r#""stdout":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"}"#,
           ]
           .concat()
       }

       /// The row of the target `a`, as it is written.
       fn written_a() -> String {
           written(
               "a",
               "c39a0161a4194c4ef0cedffc45bb82fdea72693ef4b6cc1f36177bed109be4d7",
               "ca978112ca1bbdcafac231b39a23dc4da786eff8147c4e72b9807785afee48bb",
           )
       }

       /// The row of the target `b`, as it is written.
       fn written_b() -> String {
           written(
               "b",
               "74b4485d5a6d40dc17ae98fd2b5dc4979cdf3bbd33288c24dbad18927a330ed8",
               "3e23e8160039594a33894f6564e1b1348bbd7a0088d42c4acb73eeaed59c009d",
           )
       }

       /// The row of the target `c`, as it is written.
       fn written_c() -> String {
           written(
               "c",
               "d110aa5671e75762b1cc66260c40a2309b8c6fb40944b8368a6e5cf7d351e2e9",
               "2e7d2c03a9507ae265ecf5b5356885a53393a2029d241394997265a1a25aefc6",
           )
       }

       /// The permission bits the umask lets through: those of a file made asking for all of them.
       fn allowed(dir: &Path) -> u32 {
           let path = dir.join("mask-probe");
           OpenOptions::new()
               .write(true)
               .create_new(true)
               .mode(0o777)
               .open(&path)
               .unwrap();
           let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
           fs::remove_file(&path).unwrap();
           mode
       }

       /// The names of the entries of the state directory, in order.
       fn entries(layout: &StateLayout) -> Vec<String> {
           let mut names: Vec<String> = fs::read_dir(layout.dir())
               .unwrap()
               .map(|entry| entry.unwrap().file_name().into_string().unwrap())
               .collect();
           names.sort();
           names
       }

       #[test]
       fn no_file_is_no_rows() {
           let (_dir, layout) = workspace();

           let cache = LocalCache::open(&layout).unwrap();

           assert_eq!(cache.row(&label("a")), None);
           assert_eq!(entries(&layout), [] as [&str; 0]);
       }

       #[test]
       fn a_recorded_row_is_returned() {
           let (_dir, layout) = workspace();
           let mut cache = LocalCache::open(&layout).unwrap();

           cache.record(label("a"), row("a")).unwrap();

           assert_eq!(cache.row(&label("a")), Some(&row("a")));
           assert_eq!(cache.row(&label("b")), None);

           // A second row for the same target replaces the first.
           cache.record(label("a"), row("b")).unwrap();
           assert_eq!(cache.row(&label("a")), Some(&row("b")));
       }

       #[test]
       fn fold_writes_canonical_bytes_and_removes_the_journal() {
           let (dir, layout) = workspace();
           let mut cache = LocalCache::open(&layout).unwrap();
           cache.record(label("a"), row("a")).unwrap();
           assert_eq!(entries(&layout), ["cache.journal"]);

           cache.fold().unwrap();

           assert_eq!(entries(&layout), ["cache.json"]);
           assert_eq!(
               fs::read_to_string(layout.cache()).unwrap(),
               format!(r#"{{"//:a":{}}}"#, written_a())
           );
           let mode = fs::metadata(layout.cache()).unwrap().permissions().mode() & 0o777;
           assert_eq!(mode, 0o644 & allowed(dir.path()));

           // The snapshot is what the next run reads.
           let reopened = LocalCache::open(&layout).unwrap();
           assert_eq!(reopened.row(&label("a")), Some(&row("a")));
       }

       #[test]
       fn rows_recorded_in_any_order_fold_to_the_same_bytes() {
           let expected = format!(
               r#"{{"//:a":{},"//:b":{},"//:c":{}}}"#,
               written_a(),
               written_b(),
               written_c()
           );
           for order in [["a", "b", "c"], ["c", "a", "b"], ["b", "c", "a"]] {
               let (_dir, layout) = workspace();
               let mut cache = LocalCache::open(&layout).unwrap();
               for name in order {
                   cache.record(label(name), row(name)).unwrap();
               }

               cache.fold().unwrap();

               assert_eq!(
                   fs::read_to_string(layout.cache()).unwrap(),
                   expected,
                   "{order:?}"
               );
           }
       }

       #[test]
       fn a_fold_with_nothing_recorded_touches_nothing() {
           let (_dir, layout) = workspace();

           // With no snapshot, none is made.
           let mut cache = LocalCache::open(&layout).unwrap();
           cache.fold().unwrap();
           assert_eq!(entries(&layout), [] as [&str; 0]);

           // With a snapshot, it keeps its bytes and its modification time.
           cache.record(label("a"), row("a")).unwrap();
           cache.fold().unwrap();
           let long_ago = UNIX_EPOCH + Duration::from_secs(1_600_000_000);
           File::options()
               .write(true)
               .open(layout.cache())
               .unwrap()
               .set_modified(long_ago)
               .unwrap();
           let before = fs::read(layout.cache()).unwrap();

           cache.fold().unwrap();
           let mut reopened = LocalCache::open(&layout).unwrap();
           reopened.fold().unwrap();

           assert_eq!(fs::read(layout.cache()).unwrap(), before);
           assert_eq!(
               fs::metadata(layout.cache()).unwrap().modified().unwrap(),
               long_ago
           );
           assert_eq!(entries(&layout), ["cache.json"]);
       }

       #[test]
       fn a_cut_or_reordered_cache_json_is_refused_naming_the_file() {
           let whole = format!(r#"{{"//:a":{},"//:b":{}}}"#, written_a(), written_b());
           let cut = &whole[..whole.len() / 2];
           let reordered = format!(r#"{{"//:b":{},"//:a":{}}}"#, written_b(), written_a());
           let with_a_newline = format!("{whole}\n");

           for (damaged, unreadable) in [
               (cut, true),
               (reordered.as_str(), false),
               (with_a_newline.as_str(), false),
           ] {
               let (_dir, layout) = workspace();
               fs::write(layout.cache(), damaged).unwrap();

               let error = LocalCache::open(&layout).unwrap_err();

               assert!(
                   error.to_string().starts_with(&format!(
                       "{} is not a file buildl wrote: ",
                       layout.cache().display()
                   )),
                   "{error}"
               );
               let Error::CorruptState { file, source } = error else {
                   unreachable!("a corrupt state error, got {error:?}")
               };
               assert_eq!(file, layout.cache());
               if unreadable {
                   assert!(
                       matches!(*source, Error::UnreadableJson { .. }),
                       "{source:?}"
                   );
               } else {
                   assert!(
                       matches!(*source, Error::NonCanonicalJson { .. }),
                       "{source:?}"
                   );
               }
               assert_eq!(fs::read_to_string(layout.cache()).unwrap(), damaged);
           }

           // The bytes the damage was done to are themselves accepted.
           let (_dir, layout) = workspace();
           fs::write(layout.cache(), &whole).unwrap();
           let cache = LocalCache::open(&layout).unwrap();
           assert_eq!(cache.row(&label("b")), Some(&row("b")));
       }

       #[test]
       fn a_cache_json_that_cannot_be_read_is_an_adapter_error() {
           let (_dir, layout) = workspace();
           fs::create_dir(layout.cache()).unwrap();

           let error = LocalCache::open(&layout).unwrap_err();

           let Error::Adapter { port, diagnostic } = error else {
               unreachable!("an adapter error, got {error:?}")
           };
           assert_eq!(port, PortName::ActionCache);
           assert!(
               diagnostic
                   .as_str()
                   .starts_with(&format!("reading {}: ", layout.cache().display())),
               "{diagnostic}"
           );
       }
   }
   ```

3. Run the tests and confirm they fail to compile:

   ```
   $ cargo test -p buildl --lib cache::local_cache
   ```

   Expected:

   ```text
   error[E0432]: unresolved import `super::LocalCache`
     --> crates/buildl/src/cache/local_cache.rs:30:9
   error[E0432]: unresolved import `local_cache::LocalCache`
     --> crates/buildl/src/cache/mod.rs:19:9
   error: could not compile `buildl` (lib test) due to 2 previous errors
   ```

4. Add the implementation. In `crates/buildl/src/cache/local_cache.rs`, insert between the module docs and the line `#[cfg(test)]`, with one blank line before it and one after:

   ```rust
   use std::collections::BTreeMap;
   use std::fs;
   use std::io::{self, ErrorKind};
   use std::path::{Path, PathBuf};

   use buildl_core::json::canonical;
   use buildl_core::{ActionCache, CacheRow, Diagnostic, Error, Label, PortName, Result};

   use crate::cache::journal::{Entry, Journal};
   use crate::state::{Access, StateLayout, write_whole};

   /// Holds the row each target's last successful build recorded, across runs.
   ///
   /// The rows are held in memory, read once from the snapshot `cache.json`. A row that is
   /// recorded is appended to the journal before it is held, so it is kept from then on whatever
   /// becomes of the process. [`LocalCache::fold`] writes every row to the snapshot and removes
   /// the journal.
   ///
   /// The snapshot is written from a sorted map by the canonical serializer, so the order rows
   /// were recorded in never reaches it: the same rows always give the same bytes.
   #[derive(Debug)]
   pub struct LocalCache {
       rows: BTreeMap<Label, CacheRow>,
       snapshot: PathBuf,
       staging: PathBuf,
       journal: Journal,
       unfolded: bool,
   }

   /// The cache's failure doing `what` to `path`.
   fn failed(what: &str, path: &Path, error: &io::Error) -> Error {
       Error::Adapter {
           port: PortName::ActionCache,
           diagnostic: Diagnostic::new(format!("{what} {}: {error}", path.display())),
       }
   }

   impl LocalCache {
       /// Reads `cache.json`. A workspace that has none has no rows.
       ///
       /// # Errors
       ///
       /// Returns [`Error::CorruptState`], naming the file, when `cache.json` does not hold what
       /// the canonical serializer writes for a set of rows; the file is left as it is. Returns
       /// [`Error::Adapter`], naming [`PortName::ActionCache`], when the file is there and cannot
       /// be read.
       pub fn open(layout: &StateLayout) -> Result<Self> {
           let snapshot = layout.cache();
           let rows = match fs::read(&snapshot) {
               Ok(bytes) => canonical::from_slice(&bytes).map_err(|source| Error::CorruptState {
                   file: snapshot.clone(),
                   source: Box::new(source),
               })?,
               Err(error) if error.kind() == ErrorKind::NotFound => BTreeMap::new(),
               Err(error) => return Err(failed("reading", &snapshot, &error)),
           };
           Ok(Self {
               rows,
               snapshot,
               staging: layout.dir().to_path_buf(),
               journal: Journal::new(layout.journal()),
               unfolded: false,
           })
       }

       /// Writes every row to `cache.json` and removes the journal. Does nothing when no row was
       /// recorded since the last fold, so a build that records nothing leaves the snapshot as it
       /// was, bytes and modification time.
       ///
       /// The snapshot is written whole before the journal is removed. A run that ends between the
       /// two leaves both, and both hold the same rows.
       ///
       /// # Errors
       ///
       /// Returns [`Error::Adapter`], naming [`PortName::ActionCache`], when the snapshot cannot be
       /// written or the journal cannot be removed. Every recorded row is then still in the
       /// journal.
       pub fn fold(&mut self) -> Result<()> {
           if !self.unfolded {
               return Ok(());
           }
           let bytes = canonical::to_vec(&self.rows)?;
           write_whole(&self.staging, &self.snapshot, &bytes, Access::Rewritable)
               .map_err(|error| failed("writing", &self.snapshot, &error))?;
           self.journal
               .remove()
               .map_err(|error| failed("removing", self.journal.path(), &error))?;
           self.unfolded = false;
           Ok(())
       }
   }

   impl ActionCache for LocalCache {
       fn row(&self, label: &Label) -> Option<&CacheRow> {
           self.rows.get(label)
       }

       /// Appends the row to the journal as one line, then holds it.
       ///
       /// The row is kept once this returns `Ok`: its line has been handed to the system. After an
       /// append that failed, no later row is taken by this cache.
       fn record(&mut self, label: Label, row: CacheRow) -> Result<()> {
           let entry = Entry { label, row };
           self.journal.append(&entry)?;
           self.rows.insert(entry.label, entry.row);
           self.unfolded = true;
           Ok(())
       }
   }
   ```

   `record` appends before it inserts, so after `Err` the rows in memory are what they were. `fold` writes the snapshot before it removes the journal, and does nothing at all when no row was recorded, which is what leaves `cache.json` with its bytes and its modification time after a build of cached targets.

5. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl --lib cache::local_cache
   ```

   Expected:

   ```text
   test cache::local_cache::tests::a_cache_json_that_cannot_be_read_is_an_adapter_error ... ok
   test cache::local_cache::tests::a_cut_or_reordered_cache_json_is_refused_naming_the_file ... ok
   test cache::local_cache::tests::a_fold_with_nothing_recorded_touches_nothing ... ok
   test cache::local_cache::tests::a_recorded_row_is_returned ... ok
   test cache::local_cache::tests::fold_writes_canonical_bytes_and_removes_the_journal ... ok
   test cache::local_cache::tests::no_file_is_no_rows ... ok
   test cache::local_cache::tests::rows_recorded_in_any_order_fold_to_the_same_bytes ... ok
   test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 47 filtered out
   ```

6. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo make deny
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing. `cargo make dod` prints `guard-adapter-isolation: OK (2 edges)` among its first lines, and its last line begins `[cargo-make] INFO - Build Done in`. `cargo make deny` prints `advisories ok, bans ok, licenses ok, sources ok`.

7. Commit:

   ```
   $ git add crates/buildl/src/cache/mod.rs crates/buildl/src/cache/local_cache.rs
   $ git commit -m "feat(buildl): add the action cache, folding its journal into cache.json"
   ```

### Task 7 — Replay a journal an interrupted build left

**Files:**
- Modify `crates/buildl/src/cache/local_cache.rs`
- Modify `crates/buildl/src/cache/journal.rs`

**Steps:**

1. Write the cache's failing tests. In the `tests` module of `crates/buildl/src/cache/local_cache.rs`, make these replacements: one import, then the tests, added after the last test of the module.

   Replace:

   ```rust

       use super::LocalCache;
       use crate::state::StateLayout;
   ```

   With:

   ```rust

       use super::LocalCache;
       use crate::cache::journal::Journal;
       use crate::state::StateLayout;
   ```

   Replace:

   ```rust
                   .starts_with(&format!("reading {}: ", layout.cache().display())),
               "{diagnostic}"
           );
       }
   }
   ```

   With:

   ```rust
                   .starts_with(&format!("reading {}: ", layout.cache().display())),
               "{diagnostic}"
           );
       }

       /// The journal line of the target `name`, whose row is written as `row`, without its
       /// newline.
       fn line(name: &str, row: &str) -> String {
           format!(r#"{{"label":"//:{name}","row":{row}}}"#)
       }

       #[test]
       fn a_row_survives_a_cache_that_was_never_folded() {
           let (_dir, layout) = workspace();
           let mut interrupted = LocalCache::open(&layout).unwrap();
           interrupted.record(label("b"), row("a")).unwrap();
           interrupted.record(label("a"), row("a")).unwrap();
           interrupted.record(label("b"), row("b")).unwrap();
           drop(interrupted);
           assert_eq!(entries(&layout), ["cache.journal"]);

           let cache = LocalCache::open(&layout).unwrap();

           // Each line was applied in order, so the later row of `b` is the one held.
           assert_eq!(cache.row(&label("a")), Some(&row("a")));
           assert_eq!(cache.row(&label("b")), Some(&row("b")));
           assert_eq!(entries(&layout), ["cache.json"]);
           assert_eq!(
               fs::read_to_string(layout.cache()).unwrap(),
               format!(r#"{{"//:a":{},"//:b":{}}}"#, written_a(), written_b())
           );
       }

       #[test]
       fn a_last_line_with_no_newline_is_ignored() {
           let (_dir, layout) = workspace();
           // A whole line, then one that is whole but for the newline that ends it.
           let journal = format!("{}\n{}", line("a", &written_a()), line("b", &written_b()));
           fs::write(layout.journal(), journal).unwrap();

           let cache = LocalCache::open(&layout).unwrap();

           assert_eq!(cache.row(&label("a")), Some(&row("a")));
           assert_eq!(cache.row(&label("b")), None);
           assert_eq!(entries(&layout), ["cache.json"]);
           assert_eq!(
               fs::read_to_string(layout.cache()).unwrap(),
               format!(r#"{{"//:a":{}}}"#, written_a())
           );

           // A journal that holds nothing but a cut line is removed all the same, and the snapshot
           // of no rows is written.
           let (_dir, layout) = workspace();
           fs::write(layout.journal(), r#"{"label":"//:b","row":{"key""#).unwrap();
           let cache = LocalCache::open(&layout).unwrap();
           assert_eq!(cache.row(&label("b")), None);
           assert_eq!(entries(&layout), ["cache.json"]);
           assert_eq!(fs::read_to_string(layout.cache()).unwrap(), "{}");
       }

       #[test]
       fn a_damaged_journal_line_is_refused_naming_the_journal() {
           let (_dir, layout) = workspace();
           // The first line's key no longer has the digest written beside it.
           let damaged = line("a", &written_a()).replacen("c39a0161", "c39a0162", 1);
           let journal = format!("{damaged}\n{}\n", line("b", &written_b()));
           fs::write(layout.journal(), &journal).unwrap();

           let error = LocalCache::open(&layout).unwrap_err();

           assert!(
               error.to_string().starts_with(&format!(
                   "{} is not a file buildl wrote: ",
                   layout.journal().display()
               )),
               "{error}"
           );
           let Error::CorruptState { file, source } = error else {
               unreachable!("a corrupt state error, got {error:?}")
           };
           assert_eq!(file, layout.journal());
           assert!(
               matches!(*source, Error::UnreadableJson { .. }),
               "{source:?}"
           );
           assert_eq!(fs::read_to_string(layout.journal()).unwrap(), journal);
           assert_eq!(entries(&layout), ["cache.journal"]);
       }

       #[test]
       fn after_a_failed_append_no_row_is_taken_and_none_is_lost() {
           let (_dir, layout) = workspace();
           let mut cache = LocalCache::open(&layout).unwrap();
           cache.record(label("a"), row("a")).unwrap();

           // From here on the journal cannot be appended to: its path holds a directory.
           let blocked = layout.dir().join("blocked");
           fs::create_dir(&blocked).unwrap();
           cache.journal = Journal::new(blocked.clone());
           let error = cache.record(label("b"), row("b")).unwrap_err();
           assert!(
               matches!(
                   error,
                   Error::Adapter {
                       port: PortName::ActionCache,
                       ..
                   }
               ),
               "{error:?}"
           );
           assert_eq!(cache.row(&label("b")), None);
           assert_eq!(cache.row(&label("a")), Some(&row("a")));

           // The path is free again, and still no row is taken.
           fs::remove_dir(&blocked).unwrap();
           cache.record(label("c"), row("c")).unwrap_err();
           assert_eq!(cache.row(&label("c")), None);
           assert!(!blocked.exists());
           drop(cache);

           // The row recorded before the failure is the one the next run finds.
           let reopened = LocalCache::open(&layout).unwrap();
           assert_eq!(reopened.row(&label("a")), Some(&row("a")));
           assert_eq!(reopened.row(&label("b")), None);
           assert_eq!(reopened.row(&label("c")), None);
           assert_eq!(
               fs::read_to_string(layout.cache()).unwrap(),
               format!(r#"{{"//:a":{}}}"#, written_a())
           );
       }
   }
   ```

2. Run the tests and confirm the four new ones fail: a cache that is opened does not read the journal.

   ```
   $ cargo test -p buildl --lib cache::local_cache
   ```

   Expected:

   ```text
   test cache::local_cache::tests::a_damaged_journal_line_is_refused_naming_the_journal ... FAILED
   test cache::local_cache::tests::a_last_line_with_no_newline_is_ignored ... FAILED
   test cache::local_cache::tests::a_row_survives_a_cache_that_was_never_folded ... FAILED
   test cache::local_cache::tests::after_a_failed_append_no_row_is_taken_and_none_is_lost ... FAILED
   test result: FAILED. 7 passed; 4 failed; 0 ignored; 0 measured; 47 filtered out
   ```

3. Write the journal's failing tests. In the `tests` module of `crates/buildl/src/cache/journal.rs`, make this replacement, which adds the tests after the last test of the module:

   Replace:

   ```rust
           assert_eq!(fs::read_to_string(&path).unwrap(), LINE_A);
       }
   }
   ```

   With:

   ```rust
           assert_eq!(fs::read_to_string(&path).unwrap(), LINE_A);
       }

       /// The label of each entry, in order.
       fn labels(entries: &[Entry]) -> Vec<String> {
           entries
               .iter()
               .map(|entry| entry.label.to_string())
               .collect()
       }

       #[test]
       fn a_file_that_is_not_there_holds_no_line() {
           let dir = tempfile::tempdir().unwrap();

           let entries = Journal::read(&dir.path().join("cache.journal")).unwrap();

           assert_eq!(labels(&entries), [] as [&str; 0]);
       }

       #[test]
       fn every_whole_line_is_read_in_order_and_a_cut_last_one_is_left_out() {
           let dir = tempfile::tempdir().unwrap();
           let path = dir.path().join("cache.journal");
           let mut journal = Journal::new(path.clone());
           for name in ["b", "a", "b"] {
               journal.append(&entry(name)).unwrap();
           }

           let entries = Journal::read(&path).unwrap();
           assert_eq!(labels(&entries), ["//:b", "//:a", "//:b"]);
           assert_eq!(entries[1].row, entry("a").row);

           // A line with no newline was never acknowledged: whether it is cut short or whole.
           let whole = fs::read_to_string(&path).unwrap();
           for cut in [&LINE_A[..40], LINE_A.trim_end()] {
               fs::write(&path, format!("{whole}{cut}")).unwrap();
               assert_eq!(
                   labels(&Journal::read(&path).unwrap()),
                   ["//:b", "//:a", "//:b"]
               );
           }
           fs::write(&path, LINE_A.trim_end()).unwrap();
           assert_eq!(labels(&Journal::read(&path).unwrap()), [] as [&str; 0]);
       }

       #[test]
       fn a_whole_line_that_is_not_canonical_is_refused_naming_the_file() {
           let dir = tempfile::tempdir().unwrap();
           let path = dir.path().join("cache.journal");
           let line = LINE_A.trim_end();

           // A space before the newline, a line that is empty, and a key nothing declares.
           for damaged in [
               format!("{LINE_A}{line} \n"),
               format!("{LINE_A}\n"),
               format!(
                   "{}\n",
                   line.replacen(r#"{"label""#, r#"{"extra":1,"label""#, 1)
               ),
           ] {
               fs::write(&path, &damaged).unwrap();

               let error = Journal::read(&path).unwrap_err();

               let Error::CorruptState { file, source } = error else {
                   unreachable!("a corrupt state error, got {error:?}")
               };
               assert_eq!(file, path);
               assert!(
                   matches!(
                       *source,
                       Error::UnreadableJson { .. } | Error::NonCanonicalJson { .. }
                   ),
                   "{source:?}"
               );
               assert_eq!(fs::read_to_string(&path).unwrap(), damaged);
           }
       }

       #[test]
       fn a_journal_that_cannot_be_read_is_an_adapter_error() {
           let dir = tempfile::tempdir().unwrap();

           let error = Journal::read(dir.path()).unwrap_err();

           let Error::Adapter { port, diagnostic } = error else {
               unreachable!("an adapter error, got {error:?}")
           };
           assert_eq!(port, PortName::ActionCache);
           assert!(
               diagnostic
                   .as_str()
                   .starts_with(&format!("reading {}: ", dir.path().display())),
               "{diagnostic}"
           );
       }
   }
   ```

4. Run the journal's tests and confirm they fail to compile: there is no `Journal::read`.

   ```
   $ cargo test -p buildl --lib cache::journal
   ```

   Expected, the first of six errors that differ only in the line they name:

   ```text
   error[E0599]: no associated function or constant named `read` found for struct `Journal` in the current scope
      --> crates/buildl/src/cache/journal.rs:249:32
   error: could not compile `buildl` (lib test) due to 6 previous errors
   ```

5. Read the journal back. In `crates/buildl/src/cache/journal.rs`, above the `tests` module, make this replacement, which adds `read` after `path`:

   Replace:

   ```rust
       pub(crate) fn path(&self) -> &Path {
           &self.path
       }
   ```

   With:

   ```rust
       pub(crate) fn path(&self) -> &Path {
           &self.path
       }

       /// Every whole line of the file at `path`, in order. A file that is not there holds none.
       ///
       /// A last line with no newline is left out: the append that wrote it did not finish, so
       /// the row was never acknowledged.
       ///
       /// # Errors
       ///
       /// Returns [`Error::CorruptState`], naming the file, when a whole line is not what the
       /// canonical serializer writes for an entry, and [`Error::Adapter`], naming
       /// [`PortName::ActionCache`], when the file is there and cannot be read.
       pub(crate) fn read(path: &Path) -> Result<Vec<Entry>> {
           let bytes = match fs::read(path) {
               Ok(bytes) => bytes,
               Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
               Err(error) => return Err(failed("reading", path, &error)),
           };
           bytes
               .split_inclusive(|byte| *byte == b'\n')
               .filter_map(|line| line.strip_suffix(b"\n"))
               .map(|line| {
                   canonical::from_slice(line).map_err(|source| Error::CorruptState {
                       file: path.to_path_buf(),
                       source: Box::new(source),
                   })
               })
               .collect()
       }
   ```

6. Run the journal's tests and confirm they pass:

   ```
   $ cargo test -p buildl --lib cache::journal
   ```

   Expected:

   ```text
   test cache::journal::tests::a_file_that_is_not_there_holds_no_line ... ok
   test cache::journal::tests::a_journal_that_cannot_be_read_is_an_adapter_error ... ok
   test cache::journal::tests::a_whole_line_that_is_not_canonical_is_refused_naming_the_file ... ok
   test cache::journal::tests::a_write_that_fails_closes_the_journal_for_good ... ok
   test cache::journal::tests::an_entry_is_appended_as_one_canonical_line ... ok
   test cache::journal::tests::every_whole_line_is_read_in_order_and_a_cut_last_one_is_left_out ... ok
   test cache::journal::tests::nothing_is_on_disk_before_the_first_entry ... ok
   test cache::journal::tests::remove_deletes_the_file_and_one_not_there_is_no_error ... ok
   test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 54 filtered out
   ```

7. Replay the journal when the cache is opened. In `crates/buildl/src/cache/local_cache.rs`, above the `tests` module, make these replacements: what `open` says of itself with the binding of the rows it reads, and the end of `open`.

   Replace:

   ```rust

   impl LocalCache {
       /// Reads `cache.json`. A workspace that has none has no rows.
       ///
       /// # Errors
       ///
       /// Returns [`Error::CorruptState`], naming the file, when `cache.json` does not hold what
       /// the canonical serializer writes for a set of rows; the file is left as it is. Returns
       /// [`Error::Adapter`], naming [`PortName::ActionCache`], when the file is there and cannot
       /// be read.
       pub fn open(layout: &StateLayout) -> Result<Self> {
           let snapshot = layout.cache();
           let rows = match fs::read(&snapshot) {
               Ok(bytes) => canonical::from_slice(&bytes).map_err(|source| Error::CorruptState {
                   file: snapshot.clone(),
   ```

   With:

   ```rust

   impl LocalCache {
       /// Reads `cache.json`, then folds in a journal an earlier run left. A workspace that has
       /// neither has no rows.
       ///
       /// A journal is there when the run that wrote it ended before its fold, or its fold failed.
       /// Each whole line is applied in order, as it was recorded, and the cache is folded at
       /// once, so the snapshot is whole and the journal gone before anything is planned. A last
       /// line with no newline is left out.
       ///
       /// # Errors
       ///
       /// Returns [`Error::CorruptState`], naming the file, when `cache.json` or a whole line of
       /// the journal does not hold what the canonical serializer writes there; both files are
       /// left as they are. Returns [`Error::Adapter`], naming [`PortName::ActionCache`], when a
       /// file that is there cannot be read, or the fold fails.
       pub fn open(layout: &StateLayout) -> Result<Self> {
           let snapshot = layout.cache();
           let mut rows: BTreeMap<Label, CacheRow> = match fs::read(&snapshot) {
               Ok(bytes) => canonical::from_slice(&bytes).map_err(|source| Error::CorruptState {
                   file: snapshot.clone(),
   ```

   Replace:

   ```rust
               Err(error) => return Err(failed("reading", &snapshot, &error)),
           };
           Ok(Self {
               rows,
               snapshot,
               staging: layout.dir().to_path_buf(),
               journal: Journal::new(layout.journal()),
               unfolded: false,
           })
       }
   ```

   With:

   ```rust
               Err(error) => return Err(failed("reading", &snapshot, &error)),
           };
           let journal = layout.journal();
           for entry in Journal::read(&journal)? {
               rows.insert(entry.label, entry.row);
           }
           // A journal that holds no whole line is folded too: a cut line left in it would join
           // the next line appended.
           let unfolded = journal
               .try_exists()
               .map_err(|error| failed("examining", &journal, &error))?;
           let mut cache = Self {
               rows,
               snapshot,
               staging: layout.dir().to_path_buf(),
               journal: Journal::new(journal),
               unfolded,
           };
           cache.fold()?;
           Ok(cache)
       }
   ```

   A journal is folded whenever its file is there, even when it holds no whole line: a cut line left in it would join the next line appended.

8. Run every test of the module and confirm they pass:

   ```
   $ cargo test -p buildl --lib cache::
   ```

   Expected:

   ```text
   test cache::journal::tests::a_file_that_is_not_there_holds_no_line ... ok
   test cache::journal::tests::a_journal_that_cannot_be_read_is_an_adapter_error ... ok
   test cache::journal::tests::a_whole_line_that_is_not_canonical_is_refused_naming_the_file ... ok
   test cache::journal::tests::a_write_that_fails_closes_the_journal_for_good ... ok
   test cache::journal::tests::an_entry_is_appended_as_one_canonical_line ... ok
   test cache::journal::tests::every_whole_line_is_read_in_order_and_a_cut_last_one_is_left_out ... ok
   test cache::journal::tests::nothing_is_on_disk_before_the_first_entry ... ok
   test cache::journal::tests::remove_deletes_the_file_and_one_not_there_is_no_error ... ok
   test cache::local_cache::tests::a_cache_json_that_cannot_be_read_is_an_adapter_error ... ok
   test cache::local_cache::tests::a_cut_or_reordered_cache_json_is_refused_naming_the_file ... ok
   test cache::local_cache::tests::a_damaged_journal_line_is_refused_naming_the_journal ... ok
   test cache::local_cache::tests::a_fold_with_nothing_recorded_touches_nothing ... ok
   test cache::local_cache::tests::a_last_line_with_no_newline_is_ignored ... ok
   test cache::local_cache::tests::a_recorded_row_is_returned ... ok
   test cache::local_cache::tests::a_row_survives_a_cache_that_was_never_folded ... ok
   test cache::local_cache::tests::after_a_failed_append_no_row_is_taken_and_none_is_lost ... ok
   test cache::local_cache::tests::fold_writes_canonical_bytes_and_removes_the_journal ... ok
   test cache::local_cache::tests::no_file_is_no_rows ... ok
   test cache::local_cache::tests::rows_recorded_in_any_order_fold_to_the_same_bytes ... ok
   test result: ok. 19 passed; 0 failed; 0 ignored; 0 measured; 43 filtered out
   ```

9. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo make deny
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing. `cargo make dod` prints `guard-adapter-isolation: OK (2 edges)` among its first lines, and its last line begins `[cargo-make] INFO - Build Done in`. `cargo make deny` prints `advisories ok, bans ok, licenses ok, sources ok`.

10. Commit:

    ```
    $ git add crates/buildl/src/cache/local_cache.rs crates/buildl/src/cache/journal.rs
    $ git commit -m "feat(buildl): replay a journal an interrupted build left"
    ```

### Task 8 — Add the append-only build log

**Files:**
- Modify `crates/buildl/src/log/mod.rs`
- Create `crates/buildl/src/log/local_log.rs`

**Steps:**

1. Replace the whole of `crates/buildl/src/log/mod.rs` with:

   ```rust
   //! The build log on disk: how each target of each build ended, one line per target.
   //!
   //! Its own module because the log is kept apart from the rest of a workspace's state: it is an
   //! artifact a person reads, it grows across builds, and nothing a build decides is read from it.
   //!
   //! Responsibilities: [`LocalLog`], the [`EventLog`](buildl_core::EventLog) of a local workspace.
   //!
   //! Non-responsibilities: building the events, which the build does; and where the state
   //! directory keeps the log, which [`StateLayout`](crate::state::StateLayout) says.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

   mod local_log;

   pub use local_log::LocalLog;
   ```

2. Create `crates/buildl/src/log/local_log.rs` holding the module docs and the tests only:

   ```rust
   //! [`LocalLog`], the build log of a local workspace.
   //!
   //! Its own file because the log is the one state file that is only ever added to: one line for
   //! each target of each build, kept across builds, so the newest line for a target says how its
   //! latest build ended. A line cut short by a run that died is removed before the next is added,
   //! which keeps every line of the file one whole event.
   //!
   //! Responsibilities: [`LocalLog`] and its [`EventLog`] implementation.
   //!
   //! Non-responsibilities: building the events, which the build does; and reading the log back
   //! or bounding its size, which nothing here does.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::fs;
       use std::path::PathBuf;

       use buildl_core::{
           ActionOutcome, Argument, Command, Digest, Directory, Error, EventLog, Label, OutcomeClass,
           OutcomeEvent, PortName, Provenance, Timestamp,
       };

       use super::LocalLog;
       use crate::state::StateLayout;

       /// The line the log holds for the built target `//:a`.
       const BUILT_A: &str = concat!(
           r#"{"at":30,"class":"Built","command":["cc"],"diagnostic":null,"file":"build.lua","#,
           r#""finished":20,"#,
           r#""key":"2c70e12b7a0646f92279f427c7b38e7334d8e5389cff167a1dc30e73f826b683","#,
           r#""label":"//:a","outputs":[],"started":10,"#,
           r#""stderr":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855","#,
           r#""stdout":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"}"#,
           "\n"
       );

       /// The line the log holds for the target `//:b`, whose action a signal ended.
       const KILLED_B: &str = concat!(
           r#"{"at":30,"class":{"Killed":{"signal":11}},"command":["cc"],"diagnostic":null,"#,
           r#""file":"build.lua","finished":20,"#,
           r#""key":"2c70e12b7a0646f92279f427c7b38e7334d8e5389cff167a1dc30e73f826b683","#,
           r#""label":"//:b","outputs":[],"started":10,"#,
           r#""stderr":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855","#,
           r#""stdout":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"}"#,
           "\n"
       );

       /// The layout of a new workspace whose state directory exists and is empty.
       fn workspace() -> (tempfile::TempDir, StateLayout) {
           let dir = tempfile::tempdir().unwrap();
           let layout = StateLayout::new(dir.path(), &Directory::parse(".buildl").unwrap());
           fs::create_dir_all(layout.dir()).unwrap();
           (dir, layout)
       }

       /// How the build of the target `//:<name>` ended, for a target that ran `cc` and printed
       /// nothing.
       fn event(name: &str, class: OutcomeClass) -> OutcomeEvent {
           let nothing = Digest::of(b"");
           OutcomeEvent {
               at: Timestamp::from_unix_nanos(30),
               provenance: Provenance::new(PathBuf::from("build.lua"), Directory::root()),
               command: Command::new(vec![Argument::parse("cc").unwrap()]).unwrap(),
               key: Some(Digest::of(b"key")),
               started: Some(Timestamp::from_unix_nanos(10)),
               finished: Some(Timestamp::from_unix_nanos(20)),
               outcome: ActionOutcome {
                   label: Label::parse(&format!("//:{name}")).unwrap(),
                   class,
                   diagnostic: None,
                   outputs: Vec::new(),
                   stdout: Some(nothing),
                   stderr: Some(nothing),
               },
           }
       }

       /// The event [`BUILT_A`] is the line of.
       fn built_a() -> OutcomeEvent {
           event("a", OutcomeClass::Built)
       }

       /// The event [`KILLED_B`] is the line of.
       fn killed_b() -> OutcomeEvent {
           event("b", OutcomeClass::Killed { signal: 11 })
       }

       #[test]
       fn nothing_is_written_before_the_first_event() {
           let (_dir, layout) = workspace();

           let _log = LocalLog::new(&layout);

           assert_eq!(fs::read_dir(layout.dir()).unwrap().count(), 0);
       }

       #[test]
       fn each_event_is_one_canonical_line() {
           let (_dir, layout) = workspace();
           let mut log = LocalLog::new(&layout);

           log.append(&built_a()).unwrap();
           assert_eq!(fs::read_to_string(layout.log()).unwrap(), BUILT_A);

           log.append(&killed_b()).unwrap();
           assert_eq!(
               fs::read_to_string(layout.log()).unwrap(),
               [BUILT_A, KILLED_B].concat()
           );
       }

       #[test]
       fn a_second_run_appends() {
           let (_dir, layout) = workspace();
           let mut first = LocalLog::new(&layout);
           first.append(&built_a()).unwrap();
           drop(first);

           let mut second = LocalLog::new(&layout);
           second.append(&killed_b()).unwrap();
           second.append(&built_a()).unwrap();

           assert_eq!(
               fs::read_to_string(layout.log()).unwrap(),
               [BUILT_A, KILLED_B, BUILT_A].concat()
           );
       }

       #[test]
       fn a_cut_last_line_is_removed_before_the_first_append() {
           // A line cut short after whole ones, a cut line longer than one read of the scan, and a
           // file that holds nothing but a cut line.
           let filler = "x".repeat(10_000);
           for (left, kept) in [
               (format!("{BUILT_A}{}", &KILLED_B[..40]), BUILT_A),
               (format!("{BUILT_A}{filler}"), BUILT_A),
               (KILLED_B[..40].to_owned(), ""),
               (filler, ""),
           ] {
               let (_dir, layout) = workspace();
               fs::write(layout.log(), &left).unwrap();
               let mut log = LocalLog::new(&layout);

               log.append(&killed_b()).unwrap();
               log.append(&built_a()).unwrap();

               assert_eq!(
                   fs::read_to_string(layout.log()).unwrap(),
                   [kept, KILLED_B, BUILT_A].concat()
               );
           }
       }

       #[test]
       fn a_log_that_cannot_be_opened_is_an_adapter_error() {
           let (_dir, layout) = workspace();
           fs::create_dir(layout.log()).unwrap();
           let mut log = LocalLog::new(&layout);

           let error = log.append(&built_a()).unwrap_err();

           let Error::Adapter { port, diagnostic } = error else {
               unreachable!("an adapter error, got {error:?}")
           };
           assert_eq!(port, PortName::EventLog);
           assert!(
               diagnostic
                   .as_str()
                   .starts_with(&format!("appending to {}: ", layout.log().display())),
               "{diagnostic}"
           );
       }
   }
   ```

3. Run the tests and confirm they fail to compile:

   ```
   $ cargo test -p buildl --lib log::
   ```

   Expected:

   ```text
   error[E0432]: unresolved import `super::LocalLog`
     --> crates/buildl/src/log/local_log.rs:28:9
   error[E0432]: unresolved import `local_log::LocalLog`
     --> crates/buildl/src/log/mod.rs:15:9
   error: could not compile `buildl` (lib test) due to 2 previous errors
   ```

4. Add the implementation. In `crates/buildl/src/log/local_log.rs`, insert between the module docs and the line `#[cfg(test)]`, with one blank line before it and one after:

   ```rust
   use std::fs::{File, OpenOptions};
   use std::io::{self, Write};
   use std::os::unix::fs::{FileExt, OpenOptionsExt};
   use std::path::{Path, PathBuf};

   use buildl_core::json::canonical;
   use buildl_core::{Diagnostic, Error, EventLog, OutcomeEvent, PortName, Result};

   use crate::state::StateLayout;

   /// How many bytes one read of the scan for the last newline takes.
   const SCAN: usize = 4096;

   /// Appends one line per event to `log.jsonl`, across builds.
   ///
   /// Each event is written as its canonical JSON and a newline, in one write, with nothing held
   /// back in memory: an event is in the file once [`append`](EventLog::append) has returned.
   #[derive(Debug)]
   pub struct LocalLog {
       path: PathBuf,
       file: Option<File>,
   }

   /// The log's failure doing `what` to `path`.
   fn failed(what: &str, path: &Path, error: &io::Error) -> Error {
       Error::Adapter {
           port: PortName::EventLog,
           diagnostic: Diagnostic::new(format!("{what} {}: {error}", path.display())),
       }
   }

   /// Where the whole lines of `file` end: just after its last newline, or at its start when it
   /// holds none. The file is read from its end, a piece at a time.
   fn end_of_whole_lines(file: &File) -> io::Result<u64> {
       let mut piece = [0_u8; SCAN];
       let mut end = file.metadata()?.len();
       while end > 0 {
           let length = usize::try_from(end).map_or(SCAN, |left| left.min(SCAN));
           let start = end - length as u64;
           let read = &mut piece[..length];
           file.read_exact_at(read, start)?;
           if let Some(newline) = read.iter().rposition(|byte| *byte == b'\n') {
               return Ok(start + newline as u64 + 1);
           }
           end = start;
       }
       Ok(0)
   }

   /// Opens the log at `path` for appending, making it when it is missing.
   ///
   /// A last line with no newline is removed: the run that was writing it died, so the event was
   /// never acknowledged, and a line added after it would join it.
   fn open(path: &Path) -> io::Result<File> {
       let file = OpenOptions::new()
           .read(true)
           .append(true)
           .create(true)
           .mode(0o644)
           .open(path)?;
       let whole = end_of_whole_lines(&file)?;
       if whole != file.metadata()?.len() {
           file.set_len(whole)?;
       }
       Ok(file)
   }

   impl LocalLog {
       /// A log where `layout` keeps it. Touches nothing on disk until the first event.
       #[must_use]
       pub fn new(layout: &StateLayout) -> Self {
           Self {
               path: layout.log(),
               file: None,
           }
       }

       /// The file, opened on first use.
       fn opened(&mut self) -> io::Result<&mut File> {
           let file = match self.file.take() {
               Some(file) => file,
               None => open(&self.path)?,
           };
           Ok(self.file.insert(file))
       }
   }

   impl EventLog for LocalLog {
       fn append(&mut self, event: &OutcomeEvent) -> Result<()> {
           let mut line = canonical::to_vec(event)?;
           line.push(b'\n');
           self.opened()
               .and_then(|file| file.write_all(&line))
               .map_err(|error| failed("appending to", &self.path, &error))
       }
   }
   ```

   The file is opened for reading and appending. A write to a file opened for appending always lands at its end, whatever was read before, so the scan for the last newline and the cut with `set_len` need no seek.

5. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl --lib log::
   ```

   Expected:

   ```text
   test log::local_log::tests::a_cut_last_line_is_removed_before_the_first_append ... ok
   test log::local_log::tests::a_log_that_cannot_be_opened_is_an_adapter_error ... ok
   test log::local_log::tests::a_second_run_appends ... ok
   test log::local_log::tests::each_event_is_one_canonical_line ... ok
   test log::local_log::tests::nothing_is_written_before_the_first_event ... ok
   test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 62 filtered out
   ```

6. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo make deny
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing. `cargo make dod` prints `guard-adapter-isolation: OK (3 edges)` among its first lines, and its last line begins `[cargo-make] INFO - Build Done in`. `cargo make deny` prints `advisories ok, bans ok, licenses ok, sources ok`.

7. Commit:

   ```
   $ git add crates/buildl/src/log/mod.rs crates/buildl/src/log/local_log.rs
   $ git commit -m "feat(buildl): add the append-only build log"
   ```

### Task 9 — Add the stat cache with its two-second rule

**Files:**
- Modify `crates/buildl/src/statcache/mod.rs`
- Create `crates/buildl/src/statcache/local_stat_cache.rs`

**Steps:**

1. Replace the whole of `crates/buildl/src/statcache/mod.rs` with:

   ```rust
   //! The stat cache on disk: the digest of each file already hashed, beside the metadata it was
   //! hashed under.
   //!
   //! Its own module because the record is an optimisation and nothing more: it holds nothing a
   //! hash cannot give again, no artifact depends on it, and a file that cannot be trusted is
   //! thrown away instead of stopping a run.
   //!
   //! Responsibilities: [`LocalStatCache`], the [`StatCache`](buildl_core::StatCache) of a local
   //! workspace.
   //!
   //! Non-responsibilities: deciding when an entry may be reused, which Plan does; and where
   //! the state directory keeps the record, which [`StateLayout`](crate::state::StateLayout) says.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

   mod local_stat_cache;

   pub use local_stat_cache::LocalStatCache;
   ```

2. Create `crates/buildl/src/statcache/local_stat_cache.rs` holding the module docs and the tests only:

   ```rust
   //! [`LocalStatCache`], the stat cache of a local workspace.
   //!
   //! Its own file because it holds the rule that makes a recorded digest safe to reuse. A file
   //! changed twice inside one tick of the filesystem's clock keeps its modification time, and its
   //! whole metadata when its length is the same too, so an entry taken from a file modified that
   //! recently could vouch for bytes that are gone. An entry is therefore kept only for a file last
   //! modified well before the run began.
   //!
   //! Responsibilities: [`LocalStatCache`] and its [`StatCache`] implementation.
   //!
   //! Non-responsibilities: deciding whether a recorded digest may be reused, which Plan does
   //! by comparing metadata; and reading a file's metadata or hashing it, which the digester does.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::fs::{self, File};
       use std::path::Path;
       use std::time::{Duration, SystemTime, UNIX_EPOCH};

       use buildl_core::{
           Digest, Directory, Error, FileStat, PortName, SourcePath, StatCache, StatEntry, Timestamp,
       };

       use super::LocalStatCache;
       use crate::state::{StateLayout, StateLock};

       /// A new workspace whose state directory this test holds: its root, and the lock, whose
       /// stamp is when the run began.
       fn workspace() -> (tempfile::TempDir, StateLock) {
           let dir = tempfile::tempdir().unwrap();
           let layout = StateLayout::new(dir.path(), &Directory::parse(".buildl").unwrap());
           let lock = StateLock::acquire(layout).unwrap();
           (dir, lock)
       }

       /// The workspace path `raw`.
       fn source(raw: &str) -> SourcePath {
           SourcePath::parse(raw).unwrap()
       }

       /// The instant `nanos` after the epoch, as the filesystem's clock counts.
       fn instant(nanos: u64) -> SystemTime {
           UNIX_EPOCH + Duration::from_nanos(nanos)
       }

       /// Writes `bytes` to the file `name` under `root`, sets its modification time to
       /// `modified`, and returns the entry Plan records for it: the metadata the file then
       /// has, and the digest of its bytes.
       fn hashed(root: &Path, name: &str, bytes: &[u8], modified: SystemTime) -> StatEntry {
           let path = root.join(name);
           fs::write(&path, bytes).unwrap();
           File::options()
               .write(true)
               .open(&path)
               .unwrap()
               .set_modified(modified)
               .unwrap();
           let metadata = fs::metadata(&path).unwrap();
           let since = metadata.modified().unwrap().duration_since(UNIX_EPOCH);
           StatEntry {
               stat: FileStat {
                   modified: Timestamp::from_unix_nanos(
                       u64::try_from(since.unwrap().as_nanos()).unwrap(),
                   ),
                   size: metadata.len(),
               },
               digest: Digest::of(bytes),
           }
       }

       /// An entry for a file of five bytes last modified at `nanos` after the epoch.
       fn entry_at(nanos: u64) -> StatEntry {
           StatEntry {
               stat: FileStat {
                   modified: Timestamp::from_unix_nanos(nanos),
                   size: 5,
               },
               digest: Digest::of(b"hello"),
           }
       }

       #[test]
       fn the_entry_of_an_old_file_comes_back_after_commit_and_open() {
           let (dir, lock) = workspace();
           let began = lock.acquired();
           let mut cache = LocalStatCache::open(lock.layout(), began).unwrap();
           assert_eq!(cache.entry(&source("a.c")), None);

           // A file whose time is fixed, so the bytes written for it are too.
           let fixed = hashed(
               dir.path(),
               "a.c",
               b"hello",
               instant(1_600_000_000_000_000_000),
           );
           cache.record(source("a.c"), fixed);
           assert_eq!(cache.entry(&source("a.c")), Some(fixed));
           cache.commit().unwrap();
           assert_eq!(
               fs::read_to_string(lock.layout().stat_cache()).unwrap(),
               concat!(
                   r#"{"a.c":{"digest":"#,
                   r#""2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824","#,
                   r#""modified":1600000000000000000,"size":5}}"#
               )
           );

           // A file last modified ten seconds before the run began.
           let ten_seconds_old = instant(began.as_unix_nanos()) - Duration::from_secs(10);
           let old = hashed(dir.path(), "b.c", b"other", ten_seconds_old);
           cache.record(source("b.c"), old);
           cache.commit().unwrap();
           drop(cache);

           let reopened = LocalStatCache::open(lock.layout(), began).unwrap();
           assert_eq!(reopened.entry(&source("a.c")), Some(fixed));
           assert_eq!(reopened.entry(&source("b.c")), Some(old));
           assert_eq!(reopened.entry(&source("c.c")), None);
       }

       #[test]
       fn an_entry_inside_the_window_is_not_kept_and_removes_the_older_one() {
           let (dir, lock) = workspace();
           let began = lock.acquired();
           let mut cache = LocalStatCache::open(lock.layout(), began).unwrap();
           let ten_seconds_old = instant(began.as_unix_nanos()) - Duration::from_secs(10);
           let one_second_old = instant(began.as_unix_nanos()) - Duration::from_secs(1);
           let old = hashed(dir.path(), "a.c", b"hello", ten_seconds_old);
           cache.record(source("a.c"), old);
           cache.commit().unwrap();

           // The file is edited a second before the run and hashed again.
           let recent = hashed(dir.path(), "a.c", b"HELLO", one_second_old);
           cache.record(source("a.c"), recent);

           assert_eq!(cache.entry(&source("a.c")), None);
           cache.commit().unwrap();
           let reopened = LocalStatCache::open(lock.layout(), began).unwrap();
           assert_eq!(reopened.entry(&source("a.c")), None);

           // The window is the two seconds before the run began, its first instant included; an
           // entry from after the run began is not kept either.
           let edge = began.as_unix_nanos() - 2_000_000_000;
           for (nanos, kept) in [
               (edge - 1, true),
               (edge, false),
               (began.as_unix_nanos(), false),
               (began.as_unix_nanos() + 5_000_000_000, false),
           ] {
               cache.record(source("edge.c"), entry_at(nanos));
               assert_eq!(cache.entry(&source("edge.c")).is_some(), kept, "{nanos}");
           }
       }

       #[test]
       fn a_commit_with_nothing_changed_writes_nothing() {
           let (dir, lock) = workspace();
           let began = lock.acquired();
           let file = lock.layout().stat_cache();

           // Nothing recorded, and an entry that is not kept for a path that had none.
           let mut cache = LocalStatCache::open(lock.layout(), began).unwrap();
           cache.commit().unwrap();
           cache.record(source("new.c"), entry_at(began.as_unix_nanos()));
           cache.commit().unwrap();
           assert!(!file.exists());

           // The entry already held, recorded again.
           let ten_seconds_old = instant(began.as_unix_nanos()) - Duration::from_secs(10);
           let old = hashed(dir.path(), "a.c", b"hello", ten_seconds_old);
           cache.record(source("a.c"), old);
           cache.commit().unwrap();
           let long_ago = instant(1_600_000_000_000_000_000);
           File::options()
               .write(true)
               .open(&file)
               .unwrap()
               .set_modified(long_ago)
               .unwrap();
           let before = fs::read(&file).unwrap();

           cache.commit().unwrap();
           let mut reopened = LocalStatCache::open(lock.layout(), began).unwrap();
           reopened.record(source("a.c"), old);
           reopened.commit().unwrap();

           assert_eq!(fs::read(&file).unwrap(), before);
           assert_eq!(fs::metadata(&file).unwrap().modified().unwrap(), long_ago);
       }

       #[test]
       fn a_file_that_is_not_canonical_is_discarded_and_replaced() {
           // Bytes that are not JSON, JSON written another way than the canonical one, and an
           // entry with a key nothing declares.
           for damaged in [
               "not json",
               r#"{ "a.c":{"digest":"2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824","modified":7,"size":5}}"#,
               r#"{"a.c":{"digest":"2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824","inode":1,"modified":7,"size":5}}"#,
           ] {
               let (_dir, lock) = workspace();
               let file = lock.layout().stat_cache();
               fs::write(&file, damaged).unwrap();

               let mut cache = LocalStatCache::open(lock.layout(), lock.acquired()).unwrap();

               assert_eq!(cache.entry(&source("a.c")), None, "{damaged}");
               cache.commit().unwrap();
               assert_eq!(fs::read_to_string(&file).unwrap(), "{}", "{damaged}");
           }
       }

       #[test]
       fn a_stat_cache_that_cannot_be_read_is_an_adapter_error() {
           let (_dir, lock) = workspace();
           let file = lock.layout().stat_cache();
           fs::create_dir(&file).unwrap();

           let error = LocalStatCache::open(lock.layout(), lock.acquired()).unwrap_err();

           let Error::Adapter { port, diagnostic } = error else {
               unreachable!("an adapter error, got {error:?}")
           };
           assert_eq!(port, PortName::StatCache);
           assert!(
               diagnostic
                   .as_str()
                   .starts_with(&format!("reading {}: ", file.display())),
               "{diagnostic}"
           );
       }
   }
   ```

   The tests take the state directory's lock and use `StateLock::acquired()` as the instant the run began, as the composition root does: it is the lock file's own modification time, read from the filesystem that stamps the inputs. A file's time is set back with `File::set_modified`, and the entry recorded for it holds the time the file then reports.

3. Run the tests and confirm they fail to compile:

   ```
   $ cargo test -p buildl --lib statcache::
   ```

   Expected:

   ```text
   error[E0432]: unresolved import `super::LocalStatCache`
     --> crates/buildl/src/statcache/local_stat_cache.rs:29:9
   error[E0432]: unresolved import `local_stat_cache::LocalStatCache`
     --> crates/buildl/src/statcache/mod.rs:18:9
   error: could not compile `buildl` (lib test) due to 2 previous errors
   ```

4. Add the implementation. In `crates/buildl/src/statcache/local_stat_cache.rs`, insert between the module docs and the line `#[cfg(test)]`, with one blank line before it and one after:

   ```rust
   use std::collections::BTreeMap;
   use std::fs;
   use std::io::{self, ErrorKind};
   use std::path::{Path, PathBuf};

   use buildl_core::json::canonical;
   use buildl_core::{
       Diagnostic, Digest, Error, FileStat, PortName, Result, SourcePath, StatCache, StatEntry,
       Timestamp,
   };
   use serde::{Deserialize, Serialize};

   use crate::state::{Access, StateLayout, write_whole};

   /// How long before the run began a file must have been last modified for its entry to be
   /// kept, in nanoseconds: two seconds, the coarsest tick a filesystem's clock is known to have.
   const WINDOW_NANOS: u64 = 2_000_000_000;

   /// Holds the digest of each file already hashed, with the metadata it was hashed under, across
   /// runs, in `statcache.json`.
   ///
   /// An entry is kept only when the file was last modified more than two seconds before the run
   /// began. An edit the metadata cannot show leaves modification time and length as they were, so
   /// it falls in the same tick of the filesystem's clock as the recorded time; no file modified
   /// that long ago can still be inside its tick.
   #[derive(Debug)]
   pub struct LocalStatCache {
       path: PathBuf,
       staging: PathBuf,
       entries: BTreeMap<SourcePath, StatEntry>,
       trusted_before: Timestamp,
       changed: bool,
   }

   /// An entry as it is written.
   #[derive(Serialize, Deserialize)]
   #[serde(deny_unknown_fields)]
   struct StoredEntry {
       digest: Digest,
       modified: Timestamp,
       size: u64,
   }

   impl From<StatEntry> for StoredEntry {
       fn from(entry: StatEntry) -> Self {
           Self {
               digest: entry.digest,
               modified: entry.stat.modified,
               size: entry.stat.size,
           }
       }
   }

   impl From<StoredEntry> for StatEntry {
       fn from(stored: StoredEntry) -> Self {
           Self {
               stat: FileStat {
                   modified: stored.modified,
                   size: stored.size,
               },
               digest: stored.digest,
           }
       }
   }

   /// The cache's failure doing `what` to `path`.
   fn failed(what: &str, path: &Path, error: &io::Error) -> Error {
       Error::Adapter {
           port: PortName::StatCache,
           diagnostic: Diagnostic::new(format!("{what} {}: {error}", path.display())),
       }
   }

   impl LocalStatCache {
       /// Reads `statcache.json`. `began` is when the run began, by the clock of the filesystem
       /// that stamps the files: the time the lock on the state directory was taken.
       ///
       /// A workspace that has no such file has no entries. A file that does not hold what the
       /// canonical serializer writes for a set of entries is discarded: the cache starts empty,
       /// and the next [`commit`](StatCache::commit) replaces the file.
       ///
       /// # Errors
       ///
       /// Returns [`Error::Adapter`], naming [`PortName::StatCache`], when the file is there and
       /// cannot be read.
       pub fn open(layout: &StateLayout, began: Timestamp) -> Result<Self> {
           let path = layout.stat_cache();
           let (entries, changed) = match fs::read(&path) {
               Ok(bytes) => canonical::from_slice::<BTreeMap<SourcePath, StoredEntry>>(&bytes)
                   .map_or_else(
                       |_| (BTreeMap::new(), true),
                       |stored| {
                           let entries = stored
                               .into_iter()
                               .map(|(path, entry)| (path, StatEntry::from(entry)))
                               .collect();
                           (entries, false)
                       },
                   ),
               Err(error) if error.kind() == ErrorKind::NotFound => (BTreeMap::new(), false),
               Err(error) => return Err(failed("reading", &path, &error)),
           };
           Ok(Self {
               path,
               staging: layout.dir().to_path_buf(),
               entries,
               trusted_before: Timestamp::from_unix_nanos(
                   began.as_unix_nanos().saturating_sub(WINDOW_NANOS),
               ),
               changed,
           })
       }
   }

   impl StatCache for LocalStatCache {
       fn entry(&self, path: &SourcePath) -> Option<StatEntry> {
           self.entries.get(path).copied()
       }

       /// Replaces the entry for `path` when the file was last modified more than two seconds
       /// before the run began. Any other entry is not kept, and the one held for `path` is
       /// removed: the file has changed since that one was taken.
       fn record(&mut self, path: SourcePath, entry: StatEntry) {
           if entry.stat.modified < self.trusted_before {
               self.changed |= self.entries.insert(path, entry) != Some(entry);
           } else {
               self.changed |= self.entries.remove(&path).is_some();
           }
       }

       /// Writes every entry to `statcache.json`, whole. Does nothing when no entry changed since
       /// the file was read or last written.
       fn commit(&mut self) -> Result<()> {
           if !self.changed {
               return Ok(());
           }
           let stored: BTreeMap<&SourcePath, StoredEntry> = self
               .entries
               .iter()
               .map(|(path, entry)| (path, StoredEntry::from(*entry)))
               .collect();
           let bytes = canonical::to_vec(&stored)?;
           write_whole(&self.staging, &self.path, &bytes, Access::Rewritable)
               .map_err(|error| failed("writing", &self.path, &error))?;
           self.changed = false;
           Ok(())
       }
   }
   ```

5. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl --lib statcache::
   ```

   Expected:

   ```text
   test statcache::local_stat_cache::tests::a_commit_with_nothing_changed_writes_nothing ... ok
   test statcache::local_stat_cache::tests::a_file_that_is_not_canonical_is_discarded_and_replaced ... ok
   test statcache::local_stat_cache::tests::a_stat_cache_that_cannot_be_read_is_an_adapter_error ... ok
   test statcache::local_stat_cache::tests::an_entry_inside_the_window_is_not_kept_and_removes_the_older_one ... ok
   test statcache::local_stat_cache::tests::the_entry_of_an_old_file_comes_back_after_commit_and_open ... ok
   test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 67 filtered out
   ```

6. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo make deny
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing. `cargo make dod` prints `guard-adapter-isolation: OK (4 edges)` among its first lines, and its last line begins `[cargo-make] INFO - Build Done in`. `cargo make deny` prints `advisories ok, bans ok, licenses ok, sources ok`.

7. Commit:

   ```
   $ git add crates/buildl/src/statcache/mod.rs crates/buildl/src/statcache/local_stat_cache.rs
   $ git commit -m "feat(buildl): add the stat cache with its two-second rule"
   ```

### Task 10 — Record the resolved graph as graph.json

**Files:**
- Modify `crates/buildl/src/graph/mod.rs`
- Create `crates/buildl/src/graph/graph_file.rs`

**Steps:**

1. Replace the whole of `crates/buildl/src/graph/mod.rs` with:

   ```rust
   //! The graph record on disk: the graph the last build resolved, as `graph.json`.
   //!
   //! Its own module because the graph on disk is what a later reader holds beside the rows and
   //! the content a build left: it is written by every build, before anything is planned, so the
   //! three always describe the same build files.
   //!
   //! Responsibilities: [`GraphFile`], the [`GraphRecord`](buildl_core::GraphRecord) of a local
   //! workspace.
   //!
   //! Non-responsibilities: resolving the graph, which the pipeline does; and where the state
   //! directory keeps the record, which [`StateLayout`](crate::state::StateLayout) says.
   //!
   //! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

   mod graph_file;

   pub use graph_file::GraphFile;
   ```

2. Create `crates/buildl/src/graph/graph_file.rs` holding the module docs and the tests only:

   ```rust
   //! [`GraphFile`], the record of the graph a local workspace's last build resolved.
   //!
   //! Its own file because the record has one rule and it is about bytes: `graph.json` holds what
   //! the canonical serializer writes for the whole graph and nothing else, written whole. The
   //! same build files therefore leave the same file, whichever targets a build was asked for, and
   //! a reader finds the old record or the new one, never part of either.
   //!
   //! Responsibilities: [`GraphFile`] and its [`GraphRecord`] implementation.
   //!
   //! Non-responsibilities: resolving the graph, and deciding when it is recorded; both are the
   //! pipeline's.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use std::fs;
       use std::path::PathBuf;

       use buildl_core::{
           Action, Alias, Argument, Command, Declaration, Declared, Directory, Error, Freshness,
           GraphRecord, Label, NetworkAccess, OutputName, PortName, Provenance, Target, TargetGraph,
           TargetRole, resolve,
       };

       use super::GraphFile;
       use crate::state::StateLayout;

       /// The layout of a new workspace whose state directory exists and is empty.
       fn workspace() -> (tempfile::TempDir, StateLayout) {
           let dir = tempfile::tempdir().unwrap();
           let layout = StateLayout::new(dir.path(), &Directory::parse(".buildl").unwrap());
           fs::create_dir_all(layout.dir()).unwrap();
           (dir, layout)
       }

       /// The label `//:<name>`.
       fn label(name: &str) -> Label {
           Label::parse(&format!("//:{name}")).unwrap()
       }

       /// A graph of two targets declared at the workspace root, `app` depending on `lib`, each
       /// running `cc` and producing one output of its own name, with the alias `default` for
       /// `app`.
       fn graph() -> TargetGraph {
           let file = Provenance::new(PathBuf::from("build.lua"), Directory::root());
           let target = |name: &str, deps: &[&str]| {
               Declaration::new(
                   file.clone(),
                   Declared::Target(Target {
                       label: label(name),
                       role: TargetRole::Build,
                       action: Action::Run(
                           Command::new(vec![Argument::parse("cc").unwrap()]).unwrap(),
                       ),
                       inputs: Vec::new(),
                       deps: deps.iter().map(|dep| label(dep)).collect(),
                       outputs: vec![OutputName::parse(name).unwrap()],
                       env: Vec::new(),
                       network: NetworkAccess::Sealed,
                       freshness: Freshness::Cached,
                   }),
               )
           };
           let alias = Declaration::new(
               file.clone(),
               Declared::Alias(Alias {
                   label: label("default"),
                   target: label("app"),
               }),
           );
           resolve(vec![target("app", &["lib"]), target("lib", &[]), alias]).unwrap()
       }

       /// The names of the entries of the state directory, in order.
       fn entries(layout: &StateLayout) -> Vec<String> {
           let mut names: Vec<String> = fs::read_dir(layout.dir())
               .unwrap()
               .map(|entry| entry.unwrap().file_name().into_string().unwrap())
               .collect();
           names.sort();
           names
       }

       #[test]
       fn the_record_is_the_canonical_bytes_and_nothing_else() {
           let (_dir, layout) = workspace();
           let mut record = GraphFile::new(&layout);
           assert_eq!(entries(&layout), [] as [&str; 0]);

           record.record(&graph()).unwrap();

           assert_eq!(
               fs::read_to_string(layout.graph()).unwrap(),
               concat!(
                   r#"{"aliases":{"//:default":"//:app"},"settings":{},"targets":{"#,
                   r#""//:app":{"deps":["//:lib"],"description":null,"env":[],"#,
                   r#""freshness":"Cached","inputs":[],"network":"Sealed","outputs":["app"],"#,
                   r#""provenance":{"directory":"","file":"build.lua"},"role":"Build","#,
                   r#""run":["cc"]},"#,
                   r#""//:lib":{"deps":[],"description":null,"env":[],"#,
                   r#""freshness":"Cached","inputs":[],"network":"Sealed","outputs":["lib"],"#,
                   r#""provenance":{"directory":"","file":"build.lua"},"role":"Build","#,
                   r#""run":["cc"]}}}"#
               )
           );
           assert_eq!(entries(&layout), ["graph.json"]);
       }

       #[test]
       fn a_second_record_replaces_the_first_and_leaves_no_temp_file() {
           let (_dir, layout) = workspace();
           let mut record = GraphFile::new(&layout);
           record.record(&graph()).unwrap();

           record.record(&resolve(Vec::new()).unwrap()).unwrap();

           assert_eq!(
               fs::read_to_string(layout.graph()).unwrap(),
               r#"{"aliases":{},"settings":{},"targets":{}}"#
           );
           assert_eq!(entries(&layout), ["graph.json"]);
       }

       #[test]
       fn a_graph_that_cannot_be_written_is_an_adapter_error() {
           let (_dir, layout) = workspace();
           fs::create_dir(layout.graph()).unwrap();
           let mut record = GraphFile::new(&layout);

           let error = record.record(&graph()).unwrap_err();

           let Error::Adapter { port, diagnostic } = error else {
               unreachable!("an adapter error, got {error:?}")
           };
           assert_eq!(port, PortName::GraphRecord);
           assert!(
               diagnostic
                   .as_str()
                   .starts_with(&format!("writing {}: ", layout.graph().display())),
               "{diagnostic}"
           );
           assert_eq!(entries(&layout), ["graph.json"]);
       }
   }
   ```

3. Run the tests and confirm they fail to compile:

   ```
   $ cargo test -p buildl --lib graph::
   ```

   Expected:

   ```text
   error[E0432]: unresolved import `super::GraphFile`
     --> crates/buildl/src/graph/graph_file.rs:29:9
   error[E0432]: unresolved import `graph_file::GraphFile`
     --> crates/buildl/src/graph/mod.rs:17:9
   error: could not compile `buildl` (lib test) due to 2 previous errors
   ```

4. Add the implementation. In `crates/buildl/src/graph/graph_file.rs`, insert between the module docs and the line `#[cfg(test)]`, with one blank line before it and one after:

   ```rust
   use std::path::PathBuf;

   use buildl_core::json::canonical;
   use buildl_core::{Diagnostic, Error, GraphRecord, PortName, Result, TargetGraph};

   use crate::state::{Access, StateLayout, write_whole};

   /// Keeps the graph the last build resolved, in `graph.json`.
   #[derive(Debug)]
   pub struct GraphFile {
       path: PathBuf,
       staging: PathBuf,
   }

   impl GraphFile {
       /// A record where `layout` keeps it. Touches nothing on disk.
       #[must_use]
       pub fn new(layout: &StateLayout) -> Self {
           Self {
               path: layout.graph(),
               staging: layout.dir().to_path_buf(),
           }
       }
   }

   impl GraphRecord for GraphFile {
       fn record(&mut self, graph: &TargetGraph) -> Result<()> {
           let bytes = canonical::to_vec(graph)?;
           write_whole(&self.staging, &self.path, &bytes, Access::Rewritable).map_err(|error| {
               Error::Adapter {
                   port: PortName::GraphRecord,
                   diagnostic: Diagnostic::new(format!("writing {}: {error}", self.path.display())),
               }
           })
       }
   }
   ```

5. Run the tests and confirm they pass:

   ```
   $ cargo test -p buildl --lib graph::
   ```

   Expected:

   ```text
   test graph::graph_file::tests::a_graph_that_cannot_be_written_is_an_adapter_error ... ok
   test graph::graph_file::tests::a_second_record_replaces_the_first_and_leaves_no_temp_file ... ok
   test graph::graph_file::tests::the_record_is_the_canonical_bytes_and_nothing_else ... ok
   test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 72 filtered out
   ```

6. Run the gate:

   ```
   $ cargo fmt --all -- --check
   $ cargo make dod
   $ cargo make deny
   ```

   Expected: all three exit `0`. `cargo fmt --all -- --check` prints nothing. `cargo make dod` prints `guard-adapter-isolation: OK (5 edges)` among its first lines, and its last line begins `[cargo-make] INFO - Build Done in`. `cargo make deny` prints `advisories ok, bans ok, licenses ok, sources ok`.

7. Commit:

   ```
   $ git add crates/buildl/src/graph/mod.rs crates/buildl/src/graph/graph_file.rs
   $ git commit -m "feat(buildl): record the resolved graph as graph.json"
   ```

---

## Verification summary (plan-level)

- `cargo make dod` and `cargo make deny` exit `0`. `crates/buildl/src/lib.rs`, `crates/buildl/Cargo.toml`, `Makefile.toml`, `deny.toml`, the golden files and everything under `crates/buildl/src/state/` are as plan `04` left them: `git diff --stat` over this plan's ten commits names only the thirteen files of the file structure.
- `cargo test -p buildl --lib -- cas:: cache:: log:: statcache:: graph::` runs 55 tests, and all pass: 23 under `cas::`, 19 under `cache::`, 5 under `log::`, 5 under `statcache::`, 3 under `graph::`.
- `cargo make guard-adapter-isolation` passes, and the five modules name `crate::state` and no other module of the crate.
- Every test spec §12.2 names for the five modules exists under that name:

  | Module | Tests |
  |---|---|
  | `cas/blob.rs` | `a_digest_splits_into_shard_and_rest` |
  | `cas/local_cas.rs` | `put_holds_the_bytes_under_their_digest_read_only`, `put_of_held_bytes_keeps_the_inode_of_the_blob_and_of_its_twin`, `put_replaces_a_blob_whose_bytes_differ`, `put_replaces_a_twin_whose_bytes_differ_and_removes_none`, `put_makes_no_twin`, `the_twin_stays_whatever_order_puts_and_places_come_in`, `eight_threads_put_the_same_bytes`, `get_of_nothing_is_none`, `get_drops_a_blob_that_does_not_hash_to_its_name`, `a_plain_file_is_a_link_to_the_blob`, `one_blob_placed_plain_and_executable_uses_two_inodes`, `place_makes_directories_and_replaces_a_file_or_a_directory`, `place_of_a_digest_not_held_fails`, `place_names_a_file_standing_where_a_directory_is_needed`, `a_write_through_a_placed_file_is_refused`, `a_store_that_copies_shares_no_inode`, `a_link_that_fails_is_followed_by_a_copy` |
  | `cache/local_cache.rs` | `no_file_is_no_rows`, `a_recorded_row_is_returned`, `a_row_survives_a_cache_that_was_never_folded`, `fold_writes_canonical_bytes_and_removes_the_journal`, `rows_recorded_in_any_order_fold_to_the_same_bytes`, `a_fold_with_nothing_recorded_touches_nothing`, `a_last_line_with_no_newline_is_ignored`, `a_cut_or_reordered_cache_json_is_refused_naming_the_file`, `a_damaged_journal_line_is_refused_naming_the_journal`, `after_a_failed_append_no_row_is_taken_and_none_is_lost` |
  | `log/local_log.rs` | `nothing_is_written_before_the_first_event`, `each_event_is_one_canonical_line`, `a_second_run_appends`, `a_cut_last_line_is_removed_before_the_first_append` |
  | `statcache/local_stat_cache.rs` | `the_entry_of_an_old_file_comes_back_after_commit_and_open`, `an_entry_inside_the_window_is_not_kept_and_removes_the_older_one`, `a_commit_with_nothing_changed_writes_nothing`, `a_file_that_is_not_canonical_is_discarded_and_replaced` |
  | `graph/graph_file.rs` | `the_record_is_the_canonical_bytes_and_nothing_else`, `a_second_record_replaces_the_first_and_leaves_no_temp_file` |

- The store's rules hold, each by a test: a blob is held under `cas/<2 hex>/<62 hex>` with mode 0444 less the umask; `put` rewrites a blob or a twin whose bytes differ, makes no twin and removes no file; two outputs of equal bytes, one placed as a file that may be run and one not, leave the blob and the twin in each of the six orders their calls can come in; `get` drops a blob that does not hash to its name; a placed file shares the inode of the blob or of the twin and refuses a write, or shares no inode where the store copies; a link that fails is followed by a copy; a file standing where a directory is needed is named and kept.
- The formats are the bytes of spec §10, asserted as literals: a `cache.json` of one, two and three rows, with no trailing newline, and `{}` for no rows; a journal line `{"label":..,"row":..}` and its newline; a log line of the twelve keys, with the class `{"Killed":{"signal":11}}`; a `statcache.json` entry `{"digest":..,"modified":<nanoseconds>,"size":..}`; a `graph.json` of two targets and an alias.
- The three damages to the action cache's files are refused as `Error::CorruptState` naming the file, with the file left as it was: a cut `cache.json`, a reordered one (and one with a newline added), and a journal line that is not canonical. A file that cannot be read at all is `Error::Adapter` naming the adapter's own port, for each of the five adapters.
- What plan `09` builds on:

  | Adapter | Made by | Beyond its port |
  |---|---|---|
  | `buildl::cas::LocalCas` | `LocalCas::new(root: &Path, layout: &StateLayout) -> Self` | nothing |
  | `buildl::cache::LocalCache` | `LocalCache::open(layout: &StateLayout) -> buildl_core::Result<Self>` | `fold(&mut self) -> buildl_core::Result<()>` |
  | `buildl::log::LocalLog` | `LocalLog::new(layout: &StateLayout) -> Self` | nothing |
  | `buildl::statcache::LocalStatCache` | `LocalStatCache::open(layout: &StateLayout, began: Timestamp) -> buildl_core::Result<Self>` | nothing |
  | `buildl::graph::GraphFile` | `GraphFile::new(layout: &StateLayout) -> Self` | nothing |

  Each of the five is `Debug`, and `LocalCas` is `Send + Sync`.
