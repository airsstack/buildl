---
status: approved
created: 2026-10-04
depends-on: [01, 02]
---

# Identity Types Implementation Plan

**Goal:** `buildl-core` carries the values that identify content, graph nodes, declaration sites and
instants.

**Architecture:** Four newtypes in `crates/buildl-core/src/types/`, one file each. `Digest` is the
single SHA-256 type for files, outputs, keys and log blobs, and hashing in-memory bytes is pure
computation that belongs here — reading a file to hash it is a port's job. `NodeId`, `Provenance`
and `Timestamp` have infallible constructors because every bit pattern of each is a legal value;
their invariant is total rather than absent.

**Tech Stack:** Rust 2024 edition, rustc 1.94 floor, `sha2` 0.10, `serde` 1.0 with `derive`,
`cargo-make`.

---

## Context an implementer needs

Plans `01` and `02` are `done`:

- `crates/buildl-core/src/error.rs` holds `Error::InvalidName { kind, value, reason }` with
  `NameKind::Digest` among its variants, plus `pub type Result<T> = core::result::Result<T, Error>`.
- `crates/buildl-core/src/types/mod.rs` exists as an export-only index, and
  `crates/buildl-core/src/types/directory.rs` provides `Directory` with a `parse` constructor and an
  `as_str` accessor. Task 3's `Provenance` holds one, which is why this plan depends on `02` and not
  on `01` alone — tasks 1, 2 and 4 need nothing from `02` and may be run before it if someone wants
  to start early, but the plan as a unit does.
- `crates/buildl-core/Cargo.toml` already depends on `serde`, `serde_json`, `sha2` and `thiserror`,
  and `crates/expected-edges.txt` already records those edges. **This plan changes neither file.**
  It is independent of plan `04` in content, but the two are not file-disjoint — each appends a
  line to `crates/buildl-core/src/lib.rs`, and this plan also extends
  `crates/buildl-core/src/types/mod.rs`, which plan `02` creates.

Each task adds its `pub mod` and `pub use` lines to `crates/buildl-core/src/types/mod.rs` in
alphabetical position and extends that file's `Responsibilities` list with a bullet for the type.

Four facts about the gate, each of which turns `cargo make dod` red under `-D warnings`:

- `clippy::unwrap_used` is **denied** (`Cargo.toml:71`), so every test module opens with
  `#![expect(clippy::unwrap_used, reason = "…")]`, and no implementation may call `.unwrap()`.
  `Digest::from_hex` is written without indexing or unwrapping for that reason.
- `clippy::missing_const_for_fn` fires on any method whose body is a field read. `NodeId`'s three
  methods, `Timestamp`'s two accessors and `Digest::as_bytes` are `const fn`.
- `clippy::doc_markdown` fires on a bare crate name in a doc comment. Write `` `serde_json` ``.
- `missing_docs = "warn"` (`Cargo.toml:64`) covers every public item, including each struct field
  that is public — none here is — and every method.

`sha2`'s trait is also called `Digest`, which would collide with this crate's type. Import it as
`use sha2::Digest as _;` so the trait's methods are in scope without its name being bound.

The two `Digest` test vectors below were cross-validated against the system `shasum`, so the test
does not check `sha2` against itself:

```
$ printf '' | shasum -a 256
e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  -
$ printf 'buildl' | shasum -a 256
4363840e4d122eb82ebe6c08be71a793135ecd8af8ad8ff7b984414a49b09064  -
```

Run `cargo fmt` before every `cargo make dod` in this plan. `fmt-check` is the gate's first step
(`Makefile.toml:46-50`), and every Rust block below is given in rustfmt's canonical form — but
retyping or re-wrapping one can drift, and a drifted block turns the gate red before a single test
runs.

All work happens in the worktree, on a branch, never on `main`. Commits follow Conventional
Commits with scope `buildl-core`; one commit per task.

### File map

```
crates/buildl-core/src/types/mod.rs        — [create or modify] export-only index (every task)
crates/buildl-core/src/types/digest.rs     — [create] Digest and its tests (task 1)
crates/buildl-core/src/types/node_id.rs    — [create] NodeId and its tests (task 2)
crates/buildl-core/src/types/provenance.rs — [create] Provenance and its tests (task 3)
crates/buildl-core/src/types/timestamp.rs  — [create] Timestamp and its tests (task 4)
crates/buildl-core/src/lib.rs              — [modify] re-export each type (every task)
```

---

## Task 1 — `Digest`, the one content identity

**Files:**
- Create `crates/buildl-core/src/types/digest.rs`
- Create or modify `crates/buildl-core/src/types/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Add `pub mod digest;` and `pub use digest::Digest;` to
   `crates/buildl-core/src/types/mod.rs` in alphabetical position, and add this bullet to that
   file's `Responsibilities` list:

   ```rust
   //! - [`Digest`] — the single SHA-256 identity for files, outputs, keys and log blobs.
   ```

   Then widen the re-export in `crates/buildl-core/src/lib.rs` to include `Digest`, so the line
   plan `02` left as `pub use types::{Directory, Label, TargetName};` becomes:

   ```rust
   pub use types::{Digest, Directory, Label, TargetName};
   ```

2. Write the failing tests. Create `crates/buildl-core/src/types/digest.rs` with the test module
   only:

   ```rust
   //! Placeholder — replaced in step 4.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use super::Digest;

       #[test]
       fn matches_the_known_vectors() {
           assert_eq!(
               Digest::of(b"").to_string(),
               "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
           );
           assert_eq!(
               Digest::of(b"buildl").to_string(),
               "4363840e4d122eb82ebe6c08be71a793135ecd8af8ad8ff7b984414a49b09064"
           );
       }

       #[test]
       fn round_trips_through_hex() {
           let digest = Digest::of(b"buildl");
           assert_eq!(Digest::from_hex(&digest.to_string()).unwrap(), digest);
           assert_eq!(digest.as_bytes().len(), 32);
       }

       #[test]
       fn rejects_every_form_that_is_not_64_lowercase_hex_characters() {
           assert!(Digest::from_hex(&"a".repeat(63)).is_err());
           assert!(Digest::from_hex(&"a".repeat(65)).is_err());
           assert!(Digest::from_hex(&"A".repeat(64)).is_err());
           assert!(Digest::from_hex(&"z".repeat(64)).is_err());
           assert!(Digest::from_hex("").is_err());
       }

       #[test]
       fn debug_shows_the_hex_not_the_byte_array() {
           assert_eq!(
               format!("{:?}", Digest::of(b"")),
               r#"Digest("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")"#
           );
       }

       #[test]
       fn round_trips_through_json_as_a_hex_string() {
           let digest = Digest::of(b"buildl");
           let json = serde_json::to_string(&digest).unwrap();
           assert_eq!(
               json,
               r#""4363840e4d122eb82ebe6c08be71a793135ecd8af8ad8ff7b984414a49b09064""#
           );
           assert_eq!(serde_json::from_str::<Digest>(&json).unwrap(), digest);
       }
   }
   ```

   Uppercase hex is rejected deliberately: `Display` emits lowercase, so accepting uppercase would
   give one digest two spellings and let a cache file name the same content under a different key.

3. Run and confirm failure:

   ```
   $ cargo test -p buildl-core
   error[E0432]: unresolved import `super::Digest`
    --> crates/buildl-core/src/types/digest.rs:10:9
   ```

4. Replace the placeholder doc comment and add the implementation above the test module:

   ```rust
   //! The single SHA-256 content identity used for files, outputs, action keys and log blobs.
   //!
   //! Its own file because one type serves every one of those uses: if each invented its own,
   //! "the key of X" and "the file of X" could disagree. SHA-256 rather than a faster hash for
   //! compatibility with the Remote Execution API's digest model, so a remote cache is later a
   //! transport problem rather than a re-keying.
   //!
   //! Responsibilities: [`Digest`], hashing in-memory bytes, and the hexadecimal form.
   //!
   //! Non-responsibilities: reading anything. Hashing a *file* means touching the filesystem,
   //! which is a port's job; this type hashes a byte slice it is handed.

   use core::fmt;
   use core::str::FromStr;

   use serde::{Deserialize, Serialize};
   use sha2::Digest as _;
   use sha2::Sha256;

   use crate::error::{Error, NameKind, Result};

   /// A SHA-256 content digest.
   ///
   /// Renders as 64 lowercase hexadecimal characters, and [`Digest::from_hex`] accepts only that
   /// form, so every digest has exactly one spelling.
   #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
   #[serde(into = "String", try_from = "String")]
   pub struct Digest([u8; 32]);

   /// The value of one lowercase hexadecimal digit, or `None`.
   const fn hex_value(byte: u8) -> Option<u8> {
       match byte {
           b'0'..=b'9' => Some(byte - b'0'),
           b'a'..=b'f' => Some(byte - b'a' + 10),
           _ => None,
       }
   }

   impl Digest {
       /// Hashes an in-memory byte slice.
       #[must_use]
       pub fn of(bytes: &[u8]) -> Self {
           let out = Sha256::digest(bytes);
           let mut raw = [0_u8; 32];
           raw.copy_from_slice(&out);
           Self(raw)
       }

       /// Parses the 64-character lowercase hexadecimal form.
       ///
       /// # Errors
       ///
       /// Returns [`Error::InvalidName`] when `raw` is not exactly 64 characters, or holds a
       /// character outside `0-9a-f` — uppercase included.
       pub fn from_hex(raw: &str) -> Result<Self> {
           let invalid = |reason: &'static str| Error::InvalidName {
               kind: NameKind::Digest,
               value: raw.to_owned(),
               reason,
           };

           if raw.len() != 64 {
               return Err(invalid("must be exactly 64 hexadecimal characters"));
           }
           let bytes = raw.as_bytes();
           let mut out = [0_u8; 32];
           for (index, byte) in out.iter_mut().enumerate() {
               let high = bytes.get(index * 2).copied().and_then(hex_value);
               let low = bytes.get(index * 2 + 1).copied().and_then(hex_value);
               match (high, low) {
                   (Some(high), Some(low)) => *byte = (high << 4) | low,
                   _ => return Err(invalid("must be lowercase hexadecimal")),
               }
           }
           Ok(Self(out))
       }

       /// The raw 32 bytes.
       #[must_use]
       pub const fn as_bytes(&self) -> &[u8; 32] {
           &self.0
       }
   }

   impl fmt::Display for Digest {
       fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
           for byte in self.0 {
               write!(f, "{byte:02x}")?;
           }
           Ok(())
       }
   }

   impl fmt::Debug for Digest {
       fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
           write!(f, "Digest(\"{self}\")")
       }
   }

   impl FromStr for Digest {
       type Err = Error;
       fn from_str(raw: &str) -> Result<Self> {
           Self::from_hex(raw)
       }
   }

   impl From<Digest> for String {
       fn from(value: Digest) -> Self {
           value.to_string()
       }
   }

   impl TryFrom<String> for Digest {
       type Error = Error;
       fn try_from(raw: String) -> Result<Self> {
           Self::from_hex(&raw)
       }
   }
   ```

   `Debug` is hand-written, which is why `Debug` is absent from the derive list: the derived form
   would print 32 integers. `from_hex` uses `bytes.get(..)` rather than indexing so that no path
   can panic, which matters because `clippy::panic` is denied workspace-wide.

5. Run and confirm green, then run the gate:

   ```
   $ cargo test -p buildl-core
   test result: ok. … passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

   This task adds **5** tests. The absolute total depends on which sibling plans have already
   landed, so `0 failed` is the assertion, not the count — every task in this plan follows that
   convention.

6. Commit `feat(buildl-core): identify content by one SHA-256 digest`.

---

## Task 2 — `NodeId`, the graph arena index

**Files:**
- Create `crates/buildl-core/src/types/node_id.rs`
- Modify `crates/buildl-core/src/types/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Write the failing tests. Create `crates/buildl-core/src/types/node_id.rs` with the test module
   only:

   ```rust
   //! Placeholder — replaced in step 3.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use super::NodeId;

       #[test]
       fn carries_the_index_it_was_given() {
           assert_eq!(NodeId::new(0).get(), 0);
           assert_eq!(NodeId::new(7).get(), 7);
           assert_eq!(NodeId::new(u32::MAX).get(), u32::MAX);
       }

       #[test]
       fn widens_for_slice_addressing_without_loss() {
           assert_eq!(NodeId::new(7).index(), 7_usize);
           assert_eq!(NodeId::new(u32::MAX).index(), u32::MAX as usize);
       }

       #[test]
       fn serializes_as_a_bare_number() {
           let json = serde_json::to_string(&NodeId::new(7)).unwrap();
           assert_eq!(json, "7");
           assert_eq!(serde_json::from_str::<NodeId>(&json).unwrap(), NodeId::new(7));
       }

       #[test]
       fn orders_numerically() {
           let mut ids = [NodeId::new(10), NodeId::new(2), NodeId::new(9)];
           ids.sort();
           assert_eq!(ids, [NodeId::new(2), NodeId::new(9), NodeId::new(10)]);
       }
   }
   ```

   Add `pub mod node_id;` and `pub use node_id::NodeId;` to
   `crates/buildl-core/src/types/mod.rs`, extend its `Responsibilities` list, and widen the
   `pub use types::{…};` line in `crates/buildl-core/src/lib.rs` to include `NodeId`.

2. Run and confirm failure:

   ```
   $ cargo test -p buildl-core
   error[E0432]: unresolved import `super::NodeId`
    --> crates/buildl-core/src/types/node_id.rs:10:9
   ```

3. Replace the placeholder doc comment and add the implementation above the test module:

   ```rust
   //! A target's handle inside the graph's arenas.
   //!
   //! Its own file because it is the identity every phase after the graph is built uses in place of
   //! a name: strings appear only at the edges, where something is parsed or reported.
   //!
   //! Responsibilities: [`NodeId`] and the one widening conversion slice addressing needs.
   //!
   //! Non-responsibilities: validity. An index is meaningful only against the arena that issued it,
   //! so bounds are that arena's invariant and construction here cannot fail.

   use serde::{Deserialize, Serialize};

   /// An index into the target graph's arenas.
   ///
   /// Construction is infallible: every `u32` is a legal index, and whether one is *in bounds* is a
   /// property of the arena it indexes, not of this value. The newtype exists so the compiler
   /// refuses to swap an index with a count.
   #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
   #[serde(transparent)]
   pub struct NodeId(u32);

   impl NodeId {
       /// Wraps an arena index.
       #[must_use]
       pub const fn new(index: u32) -> Self {
           Self(index)
       }

       /// The index as declared.
       #[must_use]
       pub const fn get(self) -> u32 {
           self.0
       }

       /// The index widened for slice addressing.
       ///
       /// The one cast site, named rather than spelled `as usize` at each use.
       #[must_use]
       pub const fn index(self) -> usize {
           self.0 as usize
       }
   }
   ```

4. Run and confirm green, then run the gate:

   ```
   $ cargo test -p buildl-core
   test result: ok. … passed; 0 failed; 0 ignored; 0 measured; 0 filtered out   # this task adds 4
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

5. Commit `feat(buildl-core): address a graph target by arena index`.

---

## Task 3 — `Provenance`, the declaring site

**Files:**
- Create `crates/buildl-core/src/types/provenance.rs`
- Modify `crates/buildl-core/src/types/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Write the failing tests. Create `crates/buildl-core/src/types/provenance.rs` with the test
   module only:

   ```rust
   //! Placeholder — replaced in step 3.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use super::Provenance;
       use crate::types::Directory;
       use std::path::{Path, PathBuf};

       fn fixture() -> Provenance {
           Provenance::new(
               PathBuf::from("lib/text/build.lua"),
               Directory::parse("lib/text").unwrap(),
           )
       }

       #[test]
       fn exposes_the_file_and_the_declaring_directory() {
           let provenance = fixture();
           assert_eq!(provenance.file(), Path::new("lib/text/build.lua"));
           assert_eq!(provenance.directory().as_str(), "lib/text");
       }

       #[test]
       fn renders_as_the_declaring_file() {
           assert_eq!(fixture().to_string(), "lib/text/build.lua");
       }

       #[test]
       fn round_trips_through_json() {
           let provenance = fixture();
           let json = serde_json::to_string(&provenance).unwrap();
           assert_eq!(
               json,
               r#"{"file":"lib/text/build.lua","directory":"lib/text"}"#
           );
           assert_eq!(serde_json::from_str::<Provenance>(&json).unwrap(), provenance);
       }
   }
   ```

   Add `pub mod provenance;` and `pub use provenance::Provenance;` to
   `crates/buildl-core/src/types/mod.rs`, extend its `Responsibilities` list, and widen the
   `pub use types::{…};` line in `crates/buildl-core/src/lib.rs` to include `Provenance`.

   This is the task that makes the whole plan depend on `02`: `Provenance` holds a `Directory`, so
   `crates/buildl-core/src/types/directory.rs` must already exist.

2. Run and confirm failure:

   ```
   $ cargo test -p buildl-core
   error[E0432]: unresolved import `super::Provenance`
    --> crates/buildl-core/src/types/provenance.rs:10:9
   ```

3. Replace the placeholder doc comment and add the implementation above the test module:

   ```rust
   //! Where a declaration came from, carried rather than reconstructed.
   //!
   //! Its own file because it is attached once, when a build file is evaluated, and then threaded
   //! through every later value: no phase ever asks "which file declared this?", because it always
   //! already knows. That is what lets an error name the declaring site.
   //!
   //! Responsibilities: [`Provenance`] and its two accessors.
   //!
   //! Non-responsibilities: the filesystem. The path is a value; nothing here reads it or asks
   //! whether it exists.

   use core::fmt;
   use std::path::{Path, PathBuf};

   use serde::{Deserialize, Serialize};

   use crate::types::Directory;

   /// The build file a declaration came from, and the directory it was evaluated in.
   ///
   /// Construction is infallible: the directory is already validated and the path is a plain value.
   /// A path that is not valid UTF-8 fails at serialization with `path contains invalid UTF-8
   /// characters` rather than being silently mangled.
   #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
   pub struct Provenance {
       file: PathBuf,
       directory: Directory,
   }

   impl Provenance {
       /// Records the file and the directory it was evaluated in.
       #[must_use]
       pub const fn new(file: PathBuf, directory: Directory) -> Self {
           Self { file, directory }
       }

       /// The build file that made the declaration.
       #[must_use]
       pub fn file(&self) -> &Path {
           &self.file
       }

       /// The directory the build file was evaluated in.
       #[must_use]
       pub const fn directory(&self) -> &Directory {
           &self.directory
       }
   }

   impl fmt::Display for Provenance {
       fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
           write!(f, "{}", self.file.display())
       }
   }
   ```

   `Path::display` is used rather than a lossy conversion, and it is not among the `Path` methods
   `crates/buildl-core/clippy.toml` bans — those are the ones that touch the filesystem
   (`exists`, `is_file`, `metadata`, `canonicalize`, `read_dir` and four more at `:80-88`).

4. Run and confirm green, then run the gate:

   ```
   $ cargo test -p buildl-core
   test result: ok. … passed; 0 failed; 0 ignored; 0 measured; 0 filtered out   # this task adds 3
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

5. Commit `feat(buildl-core): carry the site a declaration came from`.

---

## Task 4 — `Timestamp`, an instant on the host clock

**Files:**
- Create `crates/buildl-core/src/types/timestamp.rs`
- Modify `crates/buildl-core/src/types/mod.rs`
- Modify `crates/buildl-core/src/lib.rs`

**Steps:**

1. Write the failing tests. Create `crates/buildl-core/src/types/timestamp.rs` with the test module
   only:

   ```rust
   //! Placeholder — replaced in step 3.

   #[cfg(test)]
   mod tests {
       #![expect(
           clippy::unwrap_used,
           reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
       )]

       use super::Timestamp;

       #[test]
       fn carries_the_nanosecond_count_it_was_given() {
           assert_eq!(Timestamp::from_unix_nanos(1_758_412_800_123_456_789).as_unix_nanos(),
               1_758_412_800_123_456_789);
           assert_eq!(Timestamp::UNIX_EPOCH.as_unix_nanos(), 0);
       }

       #[test]
       fn measures_the_span_between_two_instants() {
           let early = Timestamp::from_unix_nanos(1_000);
           let late = Timestamp::from_unix_nanos(3_500);
           assert_eq!(late.duration_since(early).unwrap().as_nanos(), 2_500);
           assert_eq!(early.duration_since(early).unwrap().as_nanos(), 0);
       }

       #[test]
       fn reports_no_span_when_the_clock_went_backwards() {
           let early = Timestamp::from_unix_nanos(1_000);
           let late = Timestamp::from_unix_nanos(3_500);
           assert!(early.duration_since(late).is_none());
       }

       #[test]
       fn serializes_as_a_bare_number() {
           let json = serde_json::to_string(&Timestamp::from_unix_nanos(42)).unwrap();
           assert_eq!(json, "42");
           assert_eq!(
               serde_json::from_str::<Timestamp>(&json).unwrap(),
               Timestamp::from_unix_nanos(42)
           );
       }

       #[test]
       fn orders_chronologically() {
           let mut stamps = [
               Timestamp::from_unix_nanos(30),
               Timestamp::UNIX_EPOCH,
               Timestamp::from_unix_nanos(10),
           ];
           stamps.sort();
           assert_eq!(
               stamps,
               [
                   Timestamp::UNIX_EPOCH,
                   Timestamp::from_unix_nanos(10),
                   Timestamp::from_unix_nanos(30)
               ]
           );
       }
   }
   ```

   The backwards-clock test is the load-bearing one: the wall clock is not monotonic and
   `clippy::panic` is denied workspace-wide, so a reversed pair has to be representable rather than
   fatal.

   Add `pub mod timestamp;` and `pub use timestamp::Timestamp;` to
   `crates/buildl-core/src/types/mod.rs`, extend its `Responsibilities` list, and widen the
   `pub use types::{…};` line in `crates/buildl-core/src/lib.rs` to include `Timestamp`.

2. Run and confirm failure:

   ```
   $ cargo test -p buildl-core
   error[E0432]: unresolved import `super::Timestamp`
    --> crates/buildl-core/src/types/timestamp.rs:10:9
   ```

3. Replace the placeholder doc comment and add the implementation above the test module:

   ```rust
   //! An instant on the host wall clock, as a nanosecond count since the Unix epoch.
   //!
   //! Its own file because it is the only value in this crate that comes from outside the build
   //! graph, and the only one a port must hand in: `std::time::Instant` and `SystemTime` are banned
   //! here, so a duration can only be the difference of two of these.
   //!
   //! Responsibilities: [`Timestamp`], its epoch constant, and the span between two instants.
   //!
   //! Non-responsibilities: reading the clock, and formatting. Reading is the clock port's; a
   //! human-readable rendering belongs to whichever adapter shows it, where a date library may be
   //! taken.

   use core::time::Duration;

   use serde::{Deserialize, Serialize};

   /// An instant on the host wall clock, in nanoseconds since 1970-01-01T00:00:00Z.
   ///
   /// Nanoseconds rather than seconds because the same clock feeds action durations, and second
   /// resolution would measure every action shorter than a second as zero.
   ///
   /// Unrelated to `SOURCE_DATE_EPOCH`, which is a whole-second value the sandbox hands to an
   /// action so *its* output is reproducible. A `Timestamp` is never an action-key component — it
   /// is precisely the value that must not affect a key.
   #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
   #[serde(transparent)]
   pub struct Timestamp(u64);

   impl Timestamp {
       /// 1970-01-01T00:00:00Z.
       pub const UNIX_EPOCH: Self = Self(0);

       /// Wraps a nanosecond count since the Unix epoch.
       #[must_use]
       pub const fn from_unix_nanos(nanos: u64) -> Self {
           Self(nanos)
       }

       /// The nanosecond count since the Unix epoch.
       #[must_use]
       pub const fn as_unix_nanos(self) -> u64 {
           self.0
       }

       /// The span from `earlier` to this instant, or `None` when `earlier` is the later of the two.
       ///
       /// `None` rather than a panic or a zero: the wall clock can go backwards between two reads,
       /// and a caller that cares needs to be able to tell that apart from no elapsed time.
       #[must_use]
       pub fn duration_since(self, earlier: Self) -> Option<Duration> {
           self.0.checked_sub(earlier.0).map(Duration::from_nanos)
       }
   }
   ```

4. Run and confirm green, then run the gate:

   ```
   $ cargo test -p buildl-core
   test result: ok. … passed; 0 failed; 0 ignored; 0 measured; 0 filtered out   # this task adds 5
   $ cargo make dod
   [cargo-make] INFO - Build Done in …
   ```

5. Commit `feat(buildl-core): record an instant on the host wall clock`.

---

## Verification summary (plan-level)

```
$ cargo fmt
$ cargo make dod
$ cargo make deny
$ cargo +1.94 check --workspace --all-targets --all-features
```

All three exit 0. `cargo make guard-core-purity` runs inside `cargo make dod` and is the proof that
nothing here reached for `SystemTime`, `Instant`, the filesystem or the environment —
`Timestamp` takes its nanoseconds from a caller, and the port that supplies them arrives in
plan `05`.

At the end of this plan `buildl-core` exports `Digest`, `NodeId`, `Provenance` and `Timestamp`, and
`Timestamp` is ready for `Clock` to return.
