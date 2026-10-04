---
status: approved
created: 2026-09-22
---

# Spec: The `buildl-core` foundation — names, content identity, errors, determinism

`buildl-core` compiles today because it is empty: `crates/buildl-core/src/lib.rs` is twenty-one
lines of doc comment and nothing else. This spec fills it with the vocabulary every later slice
consumes — a validated label and its two component names, a content digest, a graph node handle, a
provenance record, a timestamp — plus the three pieces of machinery the documents currently only
assert: one structured error enum (`architecture.md` §4), the single canonical JSON serializer
(§5 rule 2), and the `Clock` port that quarantines the wall clock (§5 rule 4). It also fixes the
crate's module topology, so the five slices that follow have one obvious home each instead of
inventing their own.

Two questions the intent recorded as open are closed here, both with a recorded rationale: label
syntax (`design.md` §14) and the relationship between the timestamp type and `SOURCE_DATE_EPOCH`.
A third — `architecture.md` §5 rule 2's unspecified "fixed float handling" — turned out to be
unimplementable as written and is settled with a compile-time ban.

---

## 1. Design premises, and where each one comes from

Every claim in this section was checked in this task against the artifact named, not carried
forward from the intent.

|Premise|Evidence|
|---|---|
|`buildl-core` may use `std` minus its I/O, process, thread, environment and clock APIs, plus `serde`, `serde_json`, `thiserror`, `sha2` — and nothing else|`architecture-building-blocks.md:27-36` (§1.2), a closed four-row table|
|`Instant`, `SystemTime` and `SystemTimeError` are banned in this crate; `Duration` is not|`crates/buildl-core/clippy.toml:133-135` — those three paths appear under `disallowed-types`; no `std::time::Duration` entry exists anywhere in the file|
|`std::path::PathBuf` is not banned; only `Path`'s filesystem-touching methods are|`crates/buildl-core/clippy.toml:80-88` names `Path::exists`, `try_exists`, `is_file`, `is_dir`, `is_symlink`, `metadata`, `symlink_metadata`, `canonicalize`, `read_dir` under `disallowed-methods`; `PathBuf` is absent from `disallowed-types`|
|`Label` is the universal name `//dir:name`, and a target is a `NodeId` (`u32` arena index) everywhere downstream|`architecture.md:102` (§2)|
|`Provenance` is `{ file: PathBuf, directory: String }`, threaded from `declare` and never reconstructed|`architecture.md:56-59` (the class block), `architecture.md:105` (the rule)|
|`Digest` is SHA-256 for files, outputs, keys and log blobs|`architecture.md:104`, decision record `architecture.md:226`|
|The error model is one `thiserror` enum with structured fields, never message parsing|`architecture.md:194`|
|Errors that can name a target carry `Label + Provenance`|`architecture.md:207`|
|All JSON leaves through one canonical serializer — sorted keys, fixed float handling|`architecture.md:214` (§5 rule 2)|
|`now()` is read in exactly two places, log timestamps and durations, via the `Clock` port|`architecture.md:216` (§5 rule 4)|
|Phases communicate through values; no phase invokes the next, and the pipeline owns the sequence|`architecture.md:42`|
|The crate holds domain data, ports **and** the pure logic of every phase — canonical JSON is one of those logic rows|`architecture-building-blocks.md:229-239` (§5.4), `docs/roadmap.md:47`|
|`abb` §5.2's `LoadError` / `StoreError` / `InfraFailure` are not normative|`architecture-building-blocks.md:124` — "The signatures below are the shape, not the final API"|

### 1.1 Facts established by running something

The documents do not answer these, and the design depends on all six.

**`serde_json`'s derive output is not sorted; a `Value` round-trip is.** A struct declared
`zebra, alpha, middle` serializes in declaration order, and only passing through
`serde_json::to_value` sorts it — recursively:

```
to_string(&struct)        = {"zebra":1,"alpha":2,"middle":3}
to_string(&to_value(&s))  = {"alpha":2,"middle":3,"zebra":1}
nested                    = {"outer_a":9,"outer_z":{"alpha":2,"middle":3,"zebra":1}}
```

So `serde_json::to_string(&T)` is **not** a canonical serializer, and the sorting in this spec's
design is the `Value` round-trip rather than anything hand-written.

**The sorting is available because `preserve_order` is off.** `serde_json`'s own dependency list in
this workspace's lockfile is `itoa, memchr, serde, serde_core, zmij` (`Cargo.lock:623-627`), with no
`indexmap`; `indexmap` enters the graph only through `toml_edit` (`Cargo.lock:749-760`). With that
feature off, `serde_json::Map` is a `BTreeMap`, so key order is byte order of the UTF-8 key:

```
key order = {"":0,"A":0,"_x":0,"a10":0,"a9":0,"b":0,"é":0}
```

Note `a10` sorts before `a9`. That is byte order, not numeric order, and it is stable — which is all
determinism rule 2 requires.

**A post-serialization walk cannot detect a non-finite float.** This is the finding that changes the
float design:

```
to_value(1.5)   = Number(1.5)  is_f64=true
to_value(3.0)   = Number(3.0)  is_f64=true   is_i64=false
to_value(NAN)   = Null         is_f64=false  is_null=true
to_string(&f64::NAN)      = null
to_string(&f64::INFINITY) = null
```

By the time a `Value` exists, `NaN` and `±Infinity` are indistinguishable from a genuine JSON
`null`. A tree walk therefore catches every *finite* float — including whole-valued ones like `3.0`,
which stay `is_f64` — and cannot catch the three values that matter most.

**Clippy can ban a primitive type by path.** Tested with a two-entry `clippy.toml` in a scratch
crate:

```
warning: use of a disallowed type `f64`
 --> src/main.rs:6:13
  |
6 |     let _x: f64 = 1.0;
  |             ^^^
  |
  = note: probe: can clippy ban a primitive by path?
  = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.98.0/index.html#disallowed_types
```

It fired on both a type annotation and an associated-const path (`f64::NAN`). This is what makes
§7.1's compile-time float ban possible.

**The ban does not fire on an un-annotated float literal.** The same `clippy.toml`, run with
`-D clippy::disallowed_types` over a file containing `canonical(&1.5)`, `canonical(&3.0)`,
`canonical(&vec![1.5, 2.5])` and `let d: f64 = 1.5`, produced exactly one error:

```
error: use of a disallowed type `f64`
  --> src/main.rs:16:12
   |
16 |     let d: f64 = 1.5;
   |            ^^^
```

Only the annotated binding. This is what makes §8's float-rejection test writable inside a crate
that bans the type, and §8 records the form it must take.

**A non-UTF-8 `PathBuf` fails loudly, not silently.** `Provenance` can therefore keep the `PathBuf`
that `architecture.md` §2 specifies:

```
to_string(&PathBuf::from("lib/text/build.lua")) = Ok("\"lib/text/build.lua\"")
to_string(&non_utf8_path)                       = Err("path contains invalid UTF-8 characters")
```

**`sha2` 0.10.9 agrees with the system `shasum`.** Cross-validated so §8's test vectors are not
self-referential — the probe's in-process digest of the thirteen bytes `{"a":2,"b":1}` and
`printf '{"a":2,"b":1}' | shasum -a 256` both produce:

```
d3626ac30a87e6f7a6428233b3c68299976865fa5508e4267c5415c76af7a772
```

### 1.2 The external artifact

`SOURCE_DATE_EPOCH` is cited by `design.md` §12.4 and its reference [3]. Fetched from
`https://reproducible-builds.org/specs/source-date-epoch/` (HTTP 200, 12048 bytes) and read as
source, the Specification section says — three non-consecutive sentences, with the intervening MUST
clauses elided:

> A UNIX timestamp, defined as the number of seconds, excluding leap seconds, since 01 Jan 1970
> 00:00:00 UTC.
>
> […]
>
> The value MUST be an ASCII representation of an integer with no fractional component, identical to
> the output format of `date +%s`.
>
> […]
>
> Formatting MUST be deferred until runtime if an end user should observe the value in their own
> locale or timezone.

Two consequences, both used in §5: the mechanism is whole seconds, and the specification itself
places formatting at the edge rather than in the value.

---

## 2. Module topology, and how it grows

The crate's charter has four parts (`abb` §5.1 domain data, §5.2 ports, §5.3 the `Pipeline` and
`Ports` bundle, §5.4 pure logic). The module tree is that charter, with one directory per §5.4 row,
so each later slice adds a directory instead of editing another slice's files.

```text
crates/buildl-core/src/
  lib.rs        export-only; the crate doc carries the map below      [I2]
  error.rs      the one Error enum, grown by variant per slice        [I2]
  types/        §5.1  the vocabulary                                  [I2]
    mod.rs            export-only
    directory.rs      Directory
    target_name.rs    TargetName
    label.rs          Label
    digest.rs         Digest
    node_id.rs        NodeId
    provenance.rs     Provenance
    timestamp.rs      Timestamp
  json/         §5.4 row "canonical JSON"                             [I2]
    mod.rs            export-only
    canonical.rs      to_vec, to_string
  ports/        §5.2  traits only                                     [I2]
    mod.rs            export-only
    clock.rs          Clock
  pipeline/     §5.3  Pipeline<P: Ports>, Ports, FakePorts            [I3]
  load/         §5.4  queue, subdir, sorted merge                     [I3]
  resolve/      §5.4  interning, label resolution, cycles             [I4]
  grants/       §5.4  wanted set ∩ ceiling → grants or refusal        [I4]
  plan/         §5.4  key assembly, dirty reasons                     [I5]
  schedule/     §5.4  the pure state machine                          [I6]
  record/       §5.4  cas → row → log                                 [I6]
```

I2 creates `error.rs`, `types/`, `json/` and `ports/`. Every remaining directory is created by the
one slice that owns it.

Five rules keep the tree from collapsing into a god-crate. They are stated here because I2 is the
slice that establishes them, and restated in the crate-level doc comment so the next implementer
reads them without finding this file.

1. **No phase module names another.** `architecture.md:42` — "no phase invokes the next… The
   pipeline in `buildl-core` owns the sequence." Concretely, `resolve/` must not `use crate::plan::`;
   `pipeline/` is the only module permitted to name more than one phase.
2. **`types/` holds no decisions.** A type's `impl` carries construction, validation, accessors and
   `Display`. A function needing two domain types to decide something is phase logic and belongs in
   a phase module. This is the rule that stops `types/` becoming the accretion point.
3. **One concept, one type.** `types/` is the only place a domain concept is declared, so a later
   slice cannot introduce a second `Label`-alike. (rust-guidelines `modularity`, rule 2.)
4. **`ports/` never gains an implementation.** An implementation would need the I/O §1.2 forbids, so
   this is already enforced — it is written down so it is not mistaken for an accident.
5. **Every phase module is exercised against `FakePorts`** once `FakePorts` exists in I3
   (`abb` §8), with tests colocated in the file under test.

Rule 1 is the one that will rot first, and it is the same shape as the crate-edge guard the
`2026-09-18-workspace-guardrails` chain built: a golden file of permitted module edges, diffed by a
`cargo make` task. It is **not** in this chain's scope — I2 ships zero phase modules, so the guard
would assert nothing. §10's roadmap row and §12 record it as a follow-up for I4, the first slice in
which two phase modules coexist and the rule becomes violable.

---

## 3. The name types

Three types, because a label is two grammars joined by a `:` and the two are validated
independently — `Label::resolve(":sibling", base)` checks the name half while reusing the directory
half untouched.

### 3.1 `Directory`

The `//<dir>` half of a label, and the directory component of `Provenance`. A workspace-root-relative
directory path.

|Property|Rule|
|---|---|
|Representation|`Directory(String)`, private field|
|Empty|Permitted — the empty string is the workspace root, rendered `//:name`|
|Separator|Single `/` between segments; no leading `/`, no trailing `/`, no `//` inside|
|Segment|Non-empty; not `.`; not `..`; characters limited to ASCII alphanumeric, `.`, `-`, `_`|
|Length|At most 1024 bytes, so a hostile build file cannot inflate a graph key (airsl's `ModuleName::MAX_LEN` is the precedent for bounding a name at all)|
|Derives|`Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash`|
|Constructor|`Directory::parse(impl Into<String>) -> Result<Self>`|
|Accessor|`as_str(&self) -> &str`|
|Traits|`Display` (the raw path, `""` for root), `AsRef<str>`, `FromStr`|

The character set is deliberately narrow. Excluding whitespace removes a quoting hazard from the
`$in` / `$out` / `$deps` expansion that `design.md` §5 defers to the executor, and widening a
validated set later is a compatible change while narrowing one is not.

### 3.2 `TargetName`

The `:<name>` half.

|Property|Rule|
|---|---|
|Representation|`TargetName(String)`, private field|
|Content|Non-empty; not `.`; not `..`; characters limited to ASCII alphanumeric, `.`, `-`, `_`|
|Length|At most 256 bytes|
|Derives|`Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash`|
|Constructor|`TargetName::parse(impl Into<String>) -> Result<Self>`|
|Accessor|`as_str(&self) -> &str`|
|Traits|`Display`, `AsRef<str>`, `FromStr`|

Every target name appearing in `design.md` §5 is accepted by that rule: `main.o` and `util.o`
(`design.md:136`), `app` (`:135`), `default` (`:140`, via `b.alias`), `app_test` (`:141`, via
`b.test`), and `text` (`:136`, inside `//lib:text`), plus the `.o` names the `:132` loop generates
from `path.stem`. `/` and `:` are excluded, which is what makes a label's single `:` an unambiguous
split point.

### 3.3 `Label`

Absolute by construction. There is no representation of an unresolved reference, so no downstream
site ever has to ask whether the label it holds is relative.

```rust
pub struct Label { directory: Directory, name: TargetName }

impl Label {
    pub fn new(directory: Directory, name: TargetName) -> Self;
    pub fn parse(raw: &str) -> Result<Self>;
    pub fn resolve(raw: &str, base: &Directory) -> Result<Self>;
    pub fn directory(&self) -> &Directory;
    pub fn name(&self) -> &TargetName;
}
```

`parse` accepts exactly one shape: `//`, then a `Directory`, then `:`, then a `TargetName`.

|Input|Result|
|---|---|
|`//lib:text`|`Ok` — directory `lib`, name `text`|
|`//lib/text:core`|`Ok` — nested directory|
|`//:top`|`Ok` — workspace root, empty directory|
|`//lib`|`Err` — a label needs `:` and a target name|
|`//lib:`|`Err` — empty target name|
|`//:`|`Err` — empty target name|
|`//a:b:c`|`Err` — `:` is not a legal character in either half, so exactly one may appear|
|`lib:text`|`Err` — missing the `//` prefix|
|`:sibling`|`Err` from `parse`; `Ok` from `resolve`|

`resolve` is the one place the three reference forms a build file may write are turned into a
`Label`, closing `design.md` §14's relative-label question:

|`raw`|`base`|Result|
|---|---|---|
|`//lib:text`|any|`//lib:text` — delegates to `parse`|
|`:sibling`|`app`|`//app:sibling`|
|`main.o`|`app`|`//app:main.o`|
|`main.o`|`""`|`//:main.o`|

The bare form is not a convenience invention: `design.md` §5's own example writes
`deps = { "main.o", "util.o", "//lib:text" }`, so same-directory bare names are already part of the
documented surface and something has to accept them.

`//lib` is rejected rather than resolved to `//lib:lib` (Bazel's convention). `design.md` documents
no such rule, so adopting it would make the parser the source of a rule the design document does not
contain — and it would give one target two spellings, breaking the `Display` → `parse` round-trip and
admitting two cache keys that mean the same target.

`Display` renders `//{directory}:{name}`, which is `//:{name}` when the directory is root.
`PartialOrd`/`Ord` derive over `(Directory, TargetName)` in that field order — which is what makes
`architecture.md` §2's `by_label: BTreeMap<Label, NodeId>` a deterministic iteration source under
§5 rule 1.

### 3.4 Serialization of the name types

`Label` is a map key in `architecture.md`'s `by_label: BTreeMap<Label, NodeId>` and in that
document's §3.5 action cache, `BTreeMap<Label, CacheRow>`. JSON object keys are strings, and
`serde_json` stringifies a non-string key silently — the probe produced `{"7":"x"}` from a
`BTreeMap<u32, &str>`.

So the rule binds **the four types that can appear as a map key or inside a label** — `Directory`,
`TargetName`, `Label` and `Digest`: each serializes as its `Display` string and deserializes through
its `parse` constructor. A core map may only be keyed by a type whose string form is its canonical
form.

It does not bind the other three, and deliberately so: `NodeId` and `Timestamp` are transparent
scalars (a JSON number each, §4.1 and §5), and `Provenance` is a two-field struct with no string
form and no `parse` (§4.3). None of the three is a map key anywhere in the documents.

---

## 4. Graph and content identity

**On "validated at construction" for the infallible types.** The intent's desired outcome says each
of the five types is "a newtype validated at construction … so an invalid value cannot be built".
Three of them — `NodeId` (§4.1), `Provenance` (§4.3) and `Timestamp` (§5) — have infallible
constructors, which does not weaken that requirement: for each, *every* bit pattern of the
representation is a legal value, so the invariant is total and construction cannot fail. `NodeId` is
an index whose bounds belong to the graph arena that owns it; `Provenance`'s two components are
already validated or are a plain path; every `u64` is a real instant. The four types with a grammar
to get wrong — `Directory`, `TargetName`, `Label`, `Digest` — are the four with fallible
constructors. An invalid value cannot be built in either case, which is the property the intent
asks for.

### 4.1 `NodeId`

```rust
pub struct NodeId(u32);

impl NodeId {
    pub const fn new(index: u32) -> Self;
    pub const fn get(self) -> u32;
    pub const fn index(self) -> usize;
}
```

`u32` per `architecture.md:102`. Infallible construction: a `NodeId` is an index into arenas the
graph owns, so validity is the graph's invariant (I4), not this type's. The newtype exists so the
compiler refuses to swap an index with a count. `Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash`;
serde-transparent, so it is a JSON number.

`index()` is the only cast site (`u32` → `usize`), which is why it is a named method rather than
`as usize` scattered across the graph code.

### 4.2 `Digest`

```rust
pub struct Digest([u8; 32]);

impl Digest {
    pub fn of(bytes: &[u8]) -> Self;
    pub fn from_hex(raw: &str) -> Result<Self>;
    pub const fn as_bytes(&self) -> &[u8; 32];
}
```

|Property|Rule|
|---|---|
|Algorithm|SHA-256 via `sha2`, per `architecture.md:226`'s decision record — REAPI compatibility over blake3's speed|
|`of`|Hashes an in-memory byte slice. Hashing bytes is pure computation and belongs here; *reading a file* to hash it is the `Digester` port's job in `buildl`|
|`from_hex`|Exactly 64 characters, `0-9a-f` only. Uppercase is rejected|
|`Display`|64 lowercase hex characters|
|`Debug`|Hand-written as `Digest("<hex>")`; the derived form would print 32 integers|
|Derives|`Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash`|
|Serialization|The lowercase hex string, not a byte array|

Rejecting uppercase hex is a determinism decision, not fussiness. `Display` emits lowercase, so
accepting uppercase in `from_hex` would let one digest have two spellings — and a cache file holding
the uppercase form would name the same content under a different key. Rejecting it makes that a loud
error at the boundary.

### 4.3 `Provenance`

```rust
pub struct Provenance { file: PathBuf, directory: Directory }

impl Provenance {
    pub fn new(file: PathBuf, directory: Directory) -> Self;
    pub fn file(&self) -> &Path;
    pub fn directory(&self) -> &Directory;
}
```

The shape `architecture.md:56-59` specifies, with the stringly-typed `directory` field replaced by
the §3.1 newtype. Construction is infallible: both components are already validated or are a plain
path. `Display` renders the file path (via `Path::display`, which is not among the banned `Path`
methods).

`PathBuf` is kept rather than replaced with a UTF-8 newtype. Cross-platform byte-identical artifacts
were never promised — `design.md` §12.4 puts an `os/arch` pair in every action key precisely so "a
mac and a Linux machine must never share entries" — and §1.1's probe shows a non-UTF-8 path fails
serialization loudly rather than mangling.

---

## 5. `Timestamp` and the `Clock` port

```rust
pub struct Timestamp(u64);            // nanoseconds since 1970-01-01T00:00:00Z

impl Timestamp {
    pub const UNIX_EPOCH: Self;
    pub const fn from_unix_nanos(nanos: u64) -> Self;
    pub const fn as_unix_nanos(self) -> u64;
    pub fn duration_since(self, earlier: Self) -> Option<Duration>;
}

pub trait Clock {
    fn now(&self) -> Timestamp;
}
```

Nanoseconds, because `architecture.md` §5 rule 4 has the clock feeding *durations* as well as log
timestamps, and `Instant` is banned in this crate (`clippy.toml:133`) — so a duration can only be the
difference of two `Timestamp`s. Second resolution would measure every action shorter than a second
as zero, which is most of a warm build.

`duration_since` returns `Option<Duration>`, `None` when `earlier` is the later of the two. The wall
clock is not monotonic, and `panic` is denied workspace-wide (`Cargo.toml:73`), so a backwards clock
has to be representable rather than fatal. `Duration` is the return type because it is the one
`std::time` item this crate may name.

`Clock` is a bare trait with no implementation in `buildl-core` — the whole content of rule 4. Its
only real adapter is in `buildl` (`abb` §6). The `Ports` bundle that will carry it is I3's
(`abb` §5.3); this chain defines the trait, not the bundle.

**`Timestamp` and `SOURCE_DATE_EPOCH` are unrelated mechanisms**, which the intent flagged as
unstated. They differ in every respect that matters:

|`Timestamp`|`SOURCE_DATE_EPOCH`|
|---|---|
|An observation of the host clock, through the `Clock` port|A value the sandbox *hands to* an action as an environment variable|
|Nanoseconds|Whole seconds — "an ASCII representation of an integer with no fractional component"|
|Appears in `log.jsonl` event timestamps and durations|Appears in an action's environment, alongside `LC_ALL=C` and `TZ=UTC` (`design.md` §12.4)|
|Never an action-key component — it is exactly the value that must not affect a key|An input class *in* the ledger|
|`buildl-core` + the `Clock` adapter|The executor and its isolation tiers, I6|

No date or calendar dependency is added. `chrono` was considered and declined. Its registry metadata,
read from the crates.io sparse index (`curl -sS 'https://index.crates.io/ch/ro/chrono'`, latest
version 0.4.45), gives:

```
default = ['clock', 'std', 'oldtime', 'wasmbind']
clock   = ['winapi', 'iana-time-zone', 'now']
iana-time-zone ^0.1.45   optional=True  kind=normal  target=cfg(unix)
```

So a default-featured `chrono` pulls a platform timezone crate into the one crate built to exclude
ambient authority — in a dependency where `clippy.toml`'s bans cannot reach. (*What*
`iana-time-zone` reads at runtime is **not verified** in this task: the crate is in neither the
local registry source tree nor the download cache, and the argument below does not rest on it.) A
`default-features = false` build would avoid the transitive dependency entirely, but nothing here
needs even that: no document in `docs/` asks for a formatted date
(`grep -rn 'chrono\|jiff\|ISO 8601\|3339\|strftime' docs/*.md` → no match, exit 1; control,
`grep -c 'sorted-key'` → 5 matches across 3 files), `design.md:195` says `log.jsonl` carries
"target, command, hashes, duration, cache hit or miss", and the `SOURCE_DATE_EPOCH` specification
itself puts formatting at the edge. A human-readable rendering of a `Timestamp`, if wanted later,
belongs in the `EventLog` or `Reporter` adapter in `buildl`, where §1.2's dependency table does not
bind.

---

## 6. The error model

One enum, `#[non_exhaustive]`, structured fields. This follows `architecture.md:194` and airsl's own
precedent — one `#[non_exhaustive] pub enum Error` with 29 variants plus a `pub type Result<T>`
alias (`airsl/crates/airsl/src/error.rs:17-22`). `abb` §5.2's `LoadError`, `StoreError` and
`InfraFailure` are the non-normative sketches its own line 124 disclaims; ports read
`Result<StagedFile>` instead.

```rust
#[non_exhaustive]
pub enum Error {
    /// A domain name failed its grammar.
    InvalidName { kind: NameKind, value: String, reason: &'static str },
    /// A value could not be serialized as JSON at all.
    CanonicalJson { source: serde_json::Error },
    /// A floating-point number reached the canonical serializer.
    FloatRejected { path: String },
}

#[non_exhaustive]
pub enum NameKind { Directory, TargetName, Label, Digest }

pub type Result<T> = core::result::Result<T, Error>;
```

`kind` is a type rather than airsl's `&'static str`, which is what satisfies §4's "structured
fields, no message parsing anywhere": a caller branches on `NameKind::Label`, never on the text of a
message. `reason` stays `&'static str` because it is a fixed explanatory phrase chosen at the
failure site, not data a caller interprets.

`#[non_exhaustive]` on both enums is what lets I3 through I6 add the variants they earn without a
breaking change — the property the intent's desired outcome asks for.

**This departs from a guideline rule, on the documents' authority.**
`claudestacks-guideline-rust:rust-guidelines` → `references/strong-types.md` asks for a per-newtype
error type (`InvalidUserId`, `InvalidPort`, each constructor returning its own). One crate-wide enum
is the opposite. `architecture.md:194` mandates the single enum explicitly, and the intent's desired
outcome repeats it ("one enum, structured fields"), so the design follows the project documents and
the guideline yields here. Recorded rather than left for a reviewer to catch as a violation.

**`architecture.md:207`'s `Label + Provenance` rule is not yet met, by construction.** No error this
chain can raise has a `Label` or a `Provenance` available to attach: `InvalidName` is raised by a
constructor, where a name that failed to parse *is* the subject and no label exists yet;
`CanonicalJson` and `FloatRejected` are raised by `json/canonical`, which receives an opaque
`T: Serialize` and knows nothing about targets. The rule binds the variants I3 onward add, where a
failure is about a target that was successfully named and a declaring file is already in hand. This
is recorded here so the gap reads as a boundary rather than an omission.

Display renders one line per variant, in the airsl-refusal shape §4 describes — what was wanted,
what was found. `InvalidName` formats as `invalid <kind>: "<value>" — <reason>`; `NameKind`'s
`Display` gives `directory`, `target name`, `label`, `digest`.

---

## 7. Canonical JSON

One entry point, in `json/canonical.rs`. It is a §5.4 "pure logic" row, not a type.

```rust
pub fn to_vec<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>>;
pub fn to_string<T: Serialize + ?Sized>(value: &T) -> Result<String>;
```

```text
value ──serde_json::to_value──► Value       keys sorted, recursively, because
                                  │          Map is a BTreeMap (§1.1)
                                  │
                                  ├── reject_floats walk ──► Err(FloatRejected { path })
                                  │     Number::is_f64()
                                  ▼
                        serde_json::to_vec ──► bytes
```

The sort is not hand-written: §1.1 shows the `to_value` round-trip already produces byte-order key
sorting at every level, and that two `Map`s built in opposite insertion orders serialize to the same
bytes (`{"a":2,"b":1}` from both). The walk exists only to reject floats, and it names the offending
location — `$`, `$.field`, `$[0]` — so a failure says where rather than only what.

### 7.1 Why the float rule needs a compile-time half

`architecture.md` §5 rule 2 says "fixed float handling" without saying what, and the decision taken
here is that no float reaches JSON at all: the action key is a digest over these bytes, and a value
that serializes differently from what it means makes "the key of X" and "the file of X" disagree —
exactly what rule 2 exists to prevent.

That cannot be delivered by the walk alone. Per §1.1, `to_value(f64::NAN)` is `Value::Null` with
`is_f64() == false` — indistinguishable from a real `null`. The walk catches every finite float,
including `3.0`; it cannot catch the three values (`NaN`, `+∞`, `-∞`) whose silent conversion is the
actual hazard.

So the guarantee is made where the float still exists — at compile time. Two entries join
`crates/buildl-core/clippy.toml`'s `disallowed-types`:

```toml
{ path = "f32", reason = "no float reaches canonical JSON; a non-finite float serializes as `null` and would make an action key disagree with the value it was computed from (architecture.md §5 rule 2)" },
{ path = "f64", reason = "no float reaches canonical JSON; a non-finite float serializes as `null` and would make an action key disagree with the value it was computed from (architecture.md §5 rule 2)" },
```

§1.1 proves clippy fires on primitives named this way, and `cargo make guard-core-purity` — built by
the `2026-09-18-workspace-guardrails` chain — already asserts that these bans resolve, that no
source file suppresses them, and that the config is not switched off. No new tooling is needed.

The result is two lines of defence with different jobs: no type *in this crate* can hold a float
(compile time), and a float arriving through a dependency's `Serialize` implementation is rejected
at the boundary (run time, and such a value is finite or already `null`).

The ban and the boundary test coexist only in one form. Per §1.1, clippy does not fire on an
un-annotated float literal, so `to_vec(&1.5)` is writable inside this crate while `1.5f64`,
`let x: f64 = 1.5`, and a fixture struct with an `f64` field each turn the gate red with no
in-crate remedy — `guard-core-purity` rejects every in-source spelling of an allow attribute, and
`cargo make clippy` covers `--all-targets`, so test code is not exempt. §8 states the form the test
must take.

This edits an artifact the guardrails chain owns, so §10 records the `abb` §1.2 amendment that keeps
that file's stated derivation true.

### 7.2 What is deliberately not guaranteed

Two residues, written down rather than left for a later reader to discover:

- **Non-string map keys stringify silently.** `serde_json` renders a `BTreeMap<u32, _>` as
  `{"7":"x"}`. §3.4's rule — the four types that can appear as a map key or inside a label
  (`Directory`, `TargetName`, `Label`, `Digest`) each serialize as their `Display` string — is what
  keeps core maps honest. It is not a crate-wide property: `NodeId` and `Timestamp` are transparent
  scalars serializing as bare numbers, and `Provenance` is a two-field struct with no string form,
  none of which keys a map. Nothing mechanically prevents a future type from keying a map on a
  number.
- **Float *formatting* is `serde_json`'s, not a specification's.** Finite floats would render
  `1.0`, `1e+300`, `0.30000000000000004`. This is irrelevant while §7.1's ban holds, and is noted
  only so a future amendment knows byte-stability there would be pinned to a dependency version.

---

## 8. Testing

No test in this chain performs I/O of any kind — no filesystem, process, thread or clock. That is
both `abb` §1.2 and the intent's desired outcome, and the purity bans make a violation a gate
failure rather than a review question.

Every file listed in §2 carries a colocated `#[cfg(test)] mod tests`, per rust-guidelines
`unit-test-mandate`. The four export-only files (`lib.rs`, `types/mod.rs`, `json/mod.rs`,
`ports/mod.rs`) ship none under exemption #1, and `ports/clock.rs` under exemption #3 (pure trait
definition); each cites its exemption in its own `//!` block, as that reference requires.

Tests are table-driven accept/reject sets plus round-trips:

|Unit|Coverage|
|---|---|
|`Directory`|accepts `""`, `lib`, `lib/text`, `a-b_c.d`; rejects `/lib`, `lib/`, `lib//text`, `.`, `..`, `lib/../x`, `lib:x`, `a b`, and 1025 bytes|
|`TargetName`|accepts `app`, `main.o`, `app_test`, `a-b`; rejects `""`, `a/b`, `a:b`, `.`, `..`, ` x`, and 257 bytes|
|`Label::parse`|every row of §3.3's table, accept and reject alike|
|`Label::resolve`|every row of §3.3's second table, including the root-directory base|
|`Label` round-trip|`parse(label.to_string()) == label` for each valid fixture|
|`Label` ordering|a shuffled `Vec` sorts to a written-out expected order, so the `(directory, name)` field order is pinned by a test rather than by field position|
|`Digest`|`of(b"")` is `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`; `of(b"buildl")` is `4363840e4d122eb82ebe6c08be71a793135ecd8af8ad8ff7b984414a49b09064`; hex round-trip; rejects 63 and 65 characters, uppercase, and non-hex|
|`NodeId`|`new`/`get`/`index` agree; serde is a bare number|
|`Provenance`|accessors; serde round-trip|
|`Timestamp`|`duration_since` on ordered, equal and reversed pairs — `Some(d)`, `Some(0)`, `None`|
|`canonical`|byte-exact sorted output; two `Map`s in opposite insertion orders produce identical bytes; nested objects sort at every level; `1.5` and `3.0` are both rejected with the right `path`; `u64::MAX` survives as `18446744073709551615`|

The two `Digest` vectors are the ones cross-validated against the system `shasum` in §1.1, so the
test does not check `sha2` against itself.

**The float-rejection test must write its literals un-annotated** — `to_vec(&1.5)`, not `1.5f64`,
not `let x: f64 = 1.5`, and not a fixture struct with a float field. §7.1 bans `f64` in this crate
with no in-crate escape, and §1.1's probe shows clippy fires on the annotated forms only. A plan
author has no way to guess this, so it is a requirement of the task rather than a stylistic note.

---

## 9. Documentation deliverables

`crates/buildl-core/src/lib.rs` and `crates/buildl-core/README.md` are rewritten to describe what
the crate now holds, closing the `buildl-core` share of `docs/roadmap.md` §5 follow-up 3.

Two constraints on that rewrite, both easy to get wrong:

- **The crate's three responsibilities stay three.** `lib.rs:10-13` today lists domain values,
  ports, and the pure logic of every phase. That list is `abb` §5.4 and `roadmap.md:47`, and it is
  still accurate — this chain ships two of those logic rows' worth of algorithm (`Label::resolve`
  and `canonical`). The rewrite updates the status and the non-responsibilities; it must not narrow
  the crate to "domain types".
- **No internal planning vocabulary.** rust-guidelines `doc-comment-discipline` keeps plan
  identifiers, chain paths and `docs/` section citations out of source. The crate doc carries §2's
  topology map and its five rules in their own words, not as a citation to this spec.

`README.md`'s "**Status:** pre-release; the crate has no public API" line becomes false the moment
task 1 lands and is updated with the rest.

**Every fallible function carries a `# Errors` rustdoc section, and this is a gate requirement, not
a style note.** `Cargo.toml:76` sets `missing_errors_doc = "warn"` and `cargo make clippy` runs
`-- -D warnings`, so an omitted section turns the gate red. Ten public items in this chain return
`Result`: `Directory::parse`, `TargetName::parse`, `Label::parse`, `Label::resolve`,
`Digest::from_hex`, `canonical::to_vec`, `canonical::to_string`, and the three `FromStr` impls.
`missing_docs = "warn"` (`Cargo.toml:64`) additionally covers every public item including each enum
variant of `Error` and `NameKind`.

---

## 10. Amendments this chain owes the design documents

This chain changes types the documents specify and closes a question they record as open. Leaving
those sentences stale is precisely the failure the `2026-09-18-workspace-guardrails` chain existed to
correct, so the edits are in scope here.

|Document|Amendment|
|---|---|
|`architecture.md` §2 class diagram|`Label.directory` becomes `Directory` and `Label.name` becomes `TargetName`; `Provenance.directory` becomes `Directory`|
|`architecture.md` §5 rule 2|"fixed float handling" becomes the rule actually implemented: no float reaches canonical JSON, enforced by the `clippy.toml` ban plus a boundary rejection|
|`architecture.md` §6|Decision records for the four choices settled here: `Label` absolute-only with a separate resolver; `//dir` rejected rather than resolved to `//dir:dir`; floats rejected outright; `Timestamp` in nanoseconds with no date dependency|
|`architecture-building-blocks.md` §1.2|Add a short paragraph beneath the existing table, naming the API families `clippy.toml` actually enumerates — filesystem, environment, process, thread, standard streams, network, platform filesystem extensions, clock — and now floats, with the float entry's reason. `clippy.toml:21` already points here for "derivation and per-family counts" and §1.2 (`abb:27-36`) is a four-row dependency table that has never held either; a banned primitive is not a dependency, so it cannot become a fifth row. This closes a gap the guardrails chain left rather than only adding to it|
|`architecture-building-blocks.md` §5.1|"Names and identity" gains `Directory`, `TargetName` and `Timestamp`|
|`architecture-building-blocks.md` §5.2|The port sketches read `Result<T>` against the one `Error`, with `abb:124`'s disclaimer made concrete|
|`design.md` §14|The label-syntax bullet is partially resolved: relative labels are adopted in both the bare and `:sibling` forms; whether `subdir` should be implicit via file discovery remains open|
|`docs/roadmap.md`|§3's I2 row status; a §4 completed-intent row; §5 follow-up 3 reduced to the crates this chain did not touch; a new §5 row for the intra-crate module-edge guard, targeted at I4|

The `design.md` §14 edit narrows an open question rather than deleting it — the file-discovery half
is untouched by this chain.

---

## 11. Definition of Done

```bash
cargo make dod        # fmt, clippy -D warnings, doc -D warnings, tests, doctests,
                      # and guard-core-purity + guard-crate-edges via [tasks.clippy]
cargo make deny       # cargo deny check -D unused-wrapper (Makefile.toml:102-115)
```

`cargo make deny`, not a bare `cargo deny check`: `[tasks.deny]` carries `-D unused-wrapper`, which
promotes cargo-deny's own warning to an error because a wrapper declared for a crate absent from the
graph is a ban list describing a dependency graph that no longer exists. It is deliberately not a
`dod` dependency — the comment at `Makefile.toml:105-109` explains that the Definition of Done is
the five commands the guideline skill owns, and that this check answers to a moving advisory
database rather than to the working tree.

Both green, from the worktree, before each commit. The MSRV job (`cargo +1.94 check --workspace
--all-targets --all-features`) is CI-only by the guardrails chain's design and is not part of the
local gate.

Additionally, and specific to this chain:

- `cargo make guard-core-purity` passes **with** the new `f32`/`f64` entries, which is what proves
  the ban resolves rather than being a dead path.
- No file under `crates/buildl-core/src/` names a filesystem, process, thread, environment or clock
  API — asserted by the guard above, not by review.
- The workspace `Cargo.toml` dependency catalog is unchanged: `serde`, `serde_json`, `thiserror` and
  `sha2` are already catalogued with their reasons (`Cargo.toml:29-37`), and this chain adds no
  dependency to it.

---

## 12. Non-goals, and two departures from the intent

### 12.1 Two places this design exceeds the approved intent

Both need the author's confirmation. Neither is a design change — each is scope the intent does not
currently authorise, recorded here rather than absorbed silently.

**The intent's fifth non-goal is crossed.** It reads: *"The workspace gate guards that enforce these
constraints mechanically. They are the sibling chain, `2026-09-18-workspace-guardrails`."* §7.1 adds
two `disallowed-types` entries to `crates/buildl-core/clippy.toml` — mechanical enforcement of a
constraint, in a file that chain owns.

The reason is §7.1's, and it is not a preference: `architecture.md` §5 rule 2's float requirement is
**not implementable** inside this crate's own code. §1.1 shows `to_value(f64::NAN)` is `Value::Null`
with `is_f64() == false`, so the boundary walk can never see the three values whose silent conversion
is the whole hazard. The only place a non-finite float still exists is at compile time, which puts
the fix in `clippy.toml` or nowhere. The cost is two lines in another chain's file, plus the §10
amendment that keeps that file's stated derivation true; the alternative is shipping rule 2 as a
half-measure with the silent hole intact.

**The intent's "Affected systems" is exceeded by §10.** It lists `crates/buildl-core` and the
workspace `Cargo.toml`. §10 also amends `docs/architecture.md`, `docs/architecture-building-blocks.md`,
`docs/design.md` and `docs/roadmap.md`, and §7.1 touches `crates/buildl-core/clippy.toml`. Those
edits exist because this chain changes types those documents specify and closes a question
`design.md` §14 records as open — leaving them stale is the exact failure the guardrails chain was
run to correct. If the author would rather keep this chain narrow, §10 is the separable part: it can
become a roadmap §5 follow-up instead, at the cost of the documents describing `Label` and
`Provenance` incorrectly until it runs.

Either departure can be made moot by amending the intent through the `intent` skill; this spec does
not amend it.

### 12.2 Carried from the intent, unchanged

- **The workspace gate guards that enforce these constraints mechanically** — the sibling chain
  `2026-09-18-workspace-guardrails`, which is `done`. This chain adds no guard task, no golden file
  and no CI job; §12.1 records its one two-line exception and why.
- The phase handoff types — `Declaration`, `BuildFile`, `StagedFile` (I3), `TargetGraph` (I4),
  `Plan` (I5), `ActionOutcome` (I6).
- `OutcomeClass`, `Ceiling`, `WantedSet`, `Grants`, `KeyComponents` and `ActionKey`. Each is settled
  by the slice that first needs it, under real pressure rather than in advance.
- `Pipeline`, the `Ports` bundle and `FakePorts` — `abb` §5.3 and §8 place them in I3. This chain
  defines one port trait, not the bundle that carries it.
- Any port adapter; any filesystem or process code; anything in `buildl`, `buildl-lua` or
  `buildl-cli`. The `Clock` port's real adapter is `buildl`'s.
- Publishing readiness of any kind.

### 12.3 Added by this design, with the reasoning recorded above

- **A date or calendar dependency** (§5). `Timestamp` is an integer; formatting belongs to an
  adapter.
- **A guard for rule 1 of §2** — no phase module names another. I2 ships no phase module, so the
  guard would assert nothing; §10 assigns it to I4 as a roadmap follow-up.
- **`Duration` as a domain newtype.** `std::time::Duration` is permitted in this crate and is
  sufficient; a newtype over it would carry no invariant.
