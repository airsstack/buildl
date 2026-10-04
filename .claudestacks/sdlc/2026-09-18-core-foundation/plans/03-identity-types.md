---
status: done
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
   would print 32 integers. `from_hex` uses `bytes.get(..)` rather than indexing because an
   out-of-range index would panic at runtime, and `get` makes the bounds check explicit so the
   function returns `Error::InvalidName` instead. No lint enforces this: `clippy::panic` fires only
   on the `panic!` macro, and `clippy::indexing_slicing` is not enabled in the workspace, so the
   gate would accept indexing here. It is a design choice, not a gate requirement.

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

   The backwards-clock test is the load-bearing one: the wall clock is not monotonic, so a reversed
   pair is a legitimate input rather than a fault, and `u64` subtraction on one would underflow —
   panicking in debug, wrapping in release. `Option` makes that case representable. No lint forces
   this: `clippy::panic` fires only on the `panic!` macro and catches neither underflow nor
   indexing. It is a design choice, not a gate requirement.

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

---

## Review findings

- unit-test-mandate (risk) — `impl FromStr for Digest` had no test, while all three sibling newtypes pin theirs with `from_str_routes_through_parse`; the body could have been reverted to anything and no gate step would fail — `crates/buildl-core/src/types/digest.rs:100`. Fixed: `from_str_routes_through_parse` added in the siblings' turbofish form, with both halves that make it discriminate — accept (`"4363…9064".parse::<Digest>().unwrap() == Digest::of(b"buildl")`) and reject (`"A".repeat(64).parse::<Digest>().is_err()`, which catches a `FromStr` that stopped routing through `from_hex`). Verified: `cargo test -p buildl-core` → `test result: ok. 40 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`; `cargo make dod` → `[cargo-make] INFO - Build Done in 4.96 seconds.`
- unit-test-mandate (nit) — the JSON round-trip exercised only the accept path of `TryFrom<String>`, where `directory.rs` also asserts a malformed value fails to deserialize — `crates/buildl-core/src/types/digest.rs:166`. Fixed: `assert!(serde_json::from_str::<Digest>(r#""not-a-digest""#).is_err())` appended into the existing round-trip test, matching how `directory.rs` places its equivalent rather than adding a separate function. Verified by the same run above.
- unit-test-mandate (nit) — `Digest` had no `Ord` test while `node_id.rs` and `timestamp.rs` both pin ordering, and digest order is the sort key any canonical digest listing depends on — `crates/buildl-core/src/types/digest.rs:121`. Fixed: `orders_by_byte_value`, built from `"00".repeat(32)`, `"11".repeat(32)` and `"ff".repeat(32)`, in the shape those two siblings use. Verified by the same run above.
- microsoft-guidelines M-DESIGN-FOR-AI, unit-test-mandate (risk) — no runnable doctest on any non-trivial public item; the workspace `--doc` run reports `0 tests` for `buildl_core`. Accepted as-is, not fixed: the gap is pre-existing across `directory.rs`, `label.rs` and `target_name.rs` as well, so it is a repo-wide convention call rather than a regression this work introduced, and plan `06` owns the documentation pass. Signed-off carry-over, not drift.
- strong-types (risk) — nothing ties a `Provenance`'s `file` to its `directory`: `Provenance::new(PathBuf::from("/etc/passwd"), Directory::parse("lib")?)` is constructible, so an error can name a declaring site that contradicts its own declaring directory — `crates/buildl-core/src/types/provenance.rs:33`. Accepted as-is: this plan's task 3 chooses infallible construction deliberately (plan lines 552-559), documenting the path as a plain value. Recorded as the parse-don't-validate gap it is, worth revisiting when a caller exists that could supply the pairing.
- strict-quality (nit) — `Digest::of` ends with `raw.copy_from_slice(&out)`, a length-checked copy carrying an unreachable panic arm, where `Self(out.into())` on the `GenericArray<u8, U32>` has no panic path at all — `crates/buildl-core/src/types/digest.rs:42`. Accepted as-is: the plan prescribes this line and the shipped implementation is byte-identical to the approved text, SHA-256 output is always 32 bytes so the arm cannot be reached, and deviating from approved code for an unreachable branch buys nothing.
- doc-comment-discipline (nit) — `"The one cast site, named rather than spelled `as usize` at each use"` is a crate-wide claim nothing enforces; `clippy::as_conversions` is not enabled, so a second cast site would appear silently and falsify the sentence — `crates/buildl-core/src/types/node_id.rs:37`. Accepted as-is: the reviewer verified the claim is true today, the only non-test `as` cast under `crates/buildl-core/src` being `node_id.rs:40`.
- doc-comment-discipline (nit) — `Provenance`'s doc quotes serde's own error text `path contains invalid UTF-8 characters`, which no test pins, so a serde bump could change the string with no gate noticing — `crates/buildl-core/src/types/provenance.rs:22`. Accepted as-is: the reviewer resolved it to serde 1.0.229, `src/core/ser/impls.rs`, `impl Serialize for Path`, and it is true on the pinned lockfile.
- mod-rs-export-only / M-DOC-INLINE (nit) — the flattening `pub use` lines carry no `#[doc(inline)]`, so the four new types' docs do not surface on the `types` module page — `crates/buildl-core/src/types/mod.rs:22` and `crates/buildl-core/src/lib.rs:27`. Accepted as-is: pre-existing on all three earlier types and extended to seven here; changing it is a repo-wide documentation convention decision outside this plan's scope.
- precision (nit) — `"widened for slice addressing"` / `"without loss"` holds on every supported host but not on a 16-bit `usize` target — `crates/buildl-core/src/types/node_id.rs:39`. Accepted as-is; no such host is supported.
- derive coverage (nit) — `Provenance`'s `Ord` and `Hash` are derived and unexercised, with field order (`file` before `directory`) as the implicit sort contract — `crates/buildl-core/src/types/provenance.rs:24`. Accepted as-is: no documented ordering contract rests on it yet.
- plan prose (risk, document defect not code defect) — plan lines 312 and 682 each justify a design choice with "`clippy::panic` is denied workspace-wide", which a probe disproved. Not a source defect: all four new files were grepped independently by the reviewer, whose only hits were the three legitimate `#![expect(clippy::unwrap_used, reason = …)]` test attributes. See Probe results; the amendment is the author's call.

## Probe results

- **Claim: the two SHA-256 test vectors are correct.** The plan cited its own cross-check as the evidence ("cross-validated against system `shasum`"), which is the shape of claim that most needs an independent run: a wrong hex literal produces a failing test whose obvious repair is to paste in whatever `sha2` emitted, silently converting the cross-validation into a tautology. Run directly:
  ```
  $ printf '' | shasum -a 256
  e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  -
  $ printf 'buildl' | shasum -a 256
  4363840e4d122eb82ebe6c08be71a793135ecd8af8ad8ff7b984414a49b09064  -
  ```
  Both match the plan exactly. **Came out for the plan.** Note `printf`, not `echo` — a trailing newline would change both digests.
- **Claim: `clippy::panic` being denied workspace-wide is what forces `Digest::from_hex` to use `bytes.get(..)` rather than indexing, and `Timestamp::duration_since` to return `Option`.** Probed with a throwaway crate holding slice indexing, a `u64` subtraction, and a `panic!` macro as the control, compiled under `-D clippy::panic`. Real output:
  ```
  error: `panic` should not be present in production code
    --> src/lib.rs:14:5
     |
  14 |     panic!("control")
     |     ^^^^^^^^^^^^^^^^^
     = note: requested on the command line with `-D clippy::panic`
  error: could not compile `panicprobe` (lib) due to 1 previous error
  ```
  One error, at the `panic!` macro only. The indexing and the subtraction both passed clean, and the control firing in the same run proves the lint was active. `clippy::indexing_slicing` is absent from the workspace lint table (`Cargo.toml` holds `expect_used` at `:72`, `panic` at `:73`, `todo` at `:74`, `missing_panics_doc` at `:77`, and no indexing lint). **Came out against the plan.** Probe deleted.

  The prescribed code is still the right code — `get` returns the crate's `Error` instead of panicking at runtime, and `Option` is right because the wall clock is not monotonic while `u64` subtraction underflows. Only the stated reason is false. Both occurrences (`:312`, `:682`) sit in plan prose immediately after their code blocks, not inside a doc comment, and all four implementation files were grepped for lint attributions: none was transcribed into source. The defect is confined to this document.
- **Claim: the nine `Path` methods `crates/buildl-core/clippy.toml` bans are the filesystem-touching ones, and `Path::display` is not among them.** Read directly at `clippy.toml:80-88`: `exists`, `try_exists`, `is_file`, `is_dir`, `is_symlink`, `metadata`, `symlink_metadata`, `canonicalize`, `read_dir` — exactly nine, exactly those lines, each with the reason "this Path method hits the filesystem; the Storage port answers it". `display` absent. Confirmed, and the plan's own `:80-88` citation is precise.
- **Claim: `clippy::unwrap_used` is denied at `Cargo.toml:71` and `missing_docs = "warn"` at `:64`.** Both read directly and exact. The plan's companion claims about `clippy::missing_const_for_fn` and `clippy::doc_markdown` are sound but indirect: neither has its own entry, arriving instead through `nursery` (`:70`) and `pedantic` (`:69`) at warn, which the gate's `-D warnings` promotes to failures. Same net effect the plan assumes, reached by a different route.
- **Claim: `fmt-check` is the gate's first step.** Read at `Makefile.toml:44` — `dependencies = ["fmt-check", "clippy", "doc", "test", "test-doc"]`, with `[tasks.fmt-check]` immediately following. Confirmed.
- **Each task's red step and test delta.** Every one matched the plan's prediction exactly: task 1 `error[E0432]: unresolved import `super::Digest`` at `digest.rs:10:9` then 26 tests; task 2 the same error for `super::NodeId` at `node_id.rs:10:9` then 30; task 3 for `super::Provenance` at `provenance.rs:10:9` then 33; task 4 for `super::Timestamp` at `timestamp.rs:10:9` then 38. Deltas +5, +4, +3, +5 as written.

## Deviations

- **2026-10-04 — no commits.** Each task's final step names a commit; none was run. The commit gate belongs to the author, and no agent in this flow runs a commit. Messages held for the author: `feat(buildl-core): identify content by one SHA-256 digest` (task 1), `feat(buildl-core): address a graph target by arena index` (task 2), `feat(buildl-core): carry the site a declaration came from` (task 3), `feat(buildl-core): record an instant on the host wall clock` (task 4).
- **2026-10-04 — the four tasks ran strictly sequentially, one batch each, despite being symbol-independent.** `Digest`, `NodeId`, `Provenance` and `Timestamp` reference none of each other — `Provenance` holds a `Directory` and a `PathBuf`, nothing from a sibling task — so the usual symbol test permits a single batch. They were still serialized, because every task verifies with `cargo test -p buildl-core` over one crate: concurrent coders in a shared compilation unit see each other's half-written files through their own test runs, and a red step specified as one `error[E0432]` at a named line is unrecognisable beside three unrelated broken files. Symbol-independence does not buy parallelism when the verification is a whole-crate build. Parallelism was taken on the read-only side instead — all four task briefs were extracted concurrently, as were plan `04`'s.
- **2026-10-04 — each task's red step produced two `error[E0432]`s, not the one the plan quotes.** The plan names the test module's `use super::<Type>` failure; the `pub use <module>::<Type>` line added to `types/mod.rs` in the same step fails from the same cause, in the same compile. Same root cause and same error code, two sites instead of one. Matches in kind, not in count.
- **2026-10-04 — three tests beyond the plan's count, from the review fix round.** The plan ends at 38 tests; the tree holds 40. Two new functions were added — `from_str_routes_through_parse` and `orders_by_byte_value` on `Digest` — plus one reject assertion appended into the existing `round_trips_through_json_as_a_hex_string`. They add no behaviour; they pin invariants the plan left unguarded, bringing `Digest` to the coverage its three siblings already had.
- **2026-10-04 — the orchestrator's fix-round brief misstated the expected test total** as 41 where 40 is correct. The brief counted the malformed-JSON reject assertion as a fourth standalone test, but the finding it came from models that assertion on `directory.rs`, which appends it inside the existing round-trip test rather than adding a new `#[test]` function. The coder followed the sibling shape as instructed, reported 40 explicitly, and named the discrepancy rather than inventing a test to reach the stated number. Recorded because the brief, not the plan, was the wrong half — the same error the orchestrator made on plan `02` task 1.
- **2026-10-04 — the review's applied findings were fixed through one fresh coder spawn, and the reviewer was not re-spawned over the result.** Per the one-fix-round budget: only the verification the fix touches was re-run. The reviewer's `blocking:` line already read `none` before the fix round, so the three applied findings were improvements over work that was already shippable, not repairs to a blocked state.
- **2026-10-04 — the new `FromStr` guard was not mutation-tested.** The identical guard shape on `Directory` was mutation-tested during plan `02` and shown to fail when the `FromStr` body stopped routing through the validating constructor; this is the same accept/reject mechanism on a different type, so re-proving it would re-verify an established result rather than test something new.
- **2026-10-04 — this plan's prose was amended after execution, on the author's authorization.** The two passages that justified a design choice with "`clippy::panic` is denied workspace-wide" (then at lines 312 and 682) now state the real reasons — an out-of-range index panics at runtime, and `u64` subtraction on a reversed clock pair underflows — and say explicitly that no lint enforces either, since `clippy::panic` fires only on the `panic!` macro and `clippy::indexing_slicing` is not enabled. The prescribed code is unchanged and the shipped source never carried the false attribution; only this document did. Amended here rather than left for plan `05`, which consumes `Timestamp`, to read.
