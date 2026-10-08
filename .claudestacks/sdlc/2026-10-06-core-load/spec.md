---
status: approved
created: 2026-10-06
---

# Spec: Load — from a workspace root to `Vec<Declaration>`

`buildl-core` holds milestone 1's vocabulary but no pipeline phase. This spec adds the first one.
Load starts from a workspace root, asks a `DeclarationSource` port to evaluate one build file at a
time, follows the `subdir` requests each file returns, turns every as-written value into a validated
domain value, and hands back one sorted `Vec<Declaration>`. It also adds the `Ports` bundle, the
`Pipeline<P: Ports>` skeleton with its first command, `check`, and the test fakes every later slice
extends. The design closes the six questions the intent left open (D1–D6), records the decision the
intent already took (`subdir` is explicit only, D7), and settles the queue's size bound, which
declaration's safety on untrusted input raises (D8).

---

## 1. Design premises, and where each one comes from

Every claim here was checked in this task against the artifact named.

|Premise|Evidence|
|---|---|
|Load produces `Vec<Declaration>`; phases communicate through values; the pipeline owns the sequence|`docs/architecture.md:42` (§1.1)|
|The directory queue, `subdir` handling and the sorted merge are Load logic in `buildl-core`; `buildl-lua` evaluates one file at a time|`docs/architecture.md:144` (§3.1), `docs/architecture-building-blocks.md:157-158` (§5.2)|
|`b.target`'s closed table: `rule` or inline `run`, `inputs`, `deps`, `outputs` (default: the target name), `env`, `network = true`, `always = true`|`docs/design.md:149` (§5)|
|The §5 example: `b.rule("cc", { run, desc })`, `b.alias("default", "app")`, `b.test("app_test", { deps, run })`; every declaration is namespaced by, and attributable to, the directory that made it|`docs/design.md:126-141`, `docs/design.md:146` (§5)|
|`b.option(name, { default })` declares a setting referenced as `$opt:name` and overridden with `--set name=value`|`docs/design.md:150` (§5), `docs/design.md:492` (§10.2, `--set test_filter=TestLogin`)|
|Placeholders (`$in`, `$out`, `$deps`) are expanded by the executor at run time|`docs/design.md:147` (§5)|
|The same file name holds the root build file and each directory's build file|`docs/design.md:67-70` (§3 layout: `build.lua` at the root and in `lib/`)|
|`buildl.toml` names the entry file: `entry = "build.lua"`|`docs/design.md:88` (§4)|
|A `pairs` loop must not change the graph|`docs/design.md:584` (§12.1, item 2)|
|`check` evaluates twice and compares the staging lists|`docs/design.md:586` (§12.1), `docs/architecture.md:227` (§5 rule 5)|
|Declaration is meant to be safe on untrusted input|`docs/design.md:56` (§2)|
|`Ports` is a trait of associated types and `Pipeline<P: Ports>` takes it as its one parameter; the signatures are "the shape, not the final API"|`docs/architecture-building-blocks.md:200-207` (§5.3), `docs/architecture-building-blocks.md:136` (§5)|
|Fakes live in `buildl-core` under `#[cfg(test)]`|`docs/architecture-building-blocks.md:305` (§8)|
|The crate's five module rules: no phase names another; `types/` holds no cross-concept decisions; one concept one type; `ports/` gains no production implementation; every phase module is tested against fakes|`crates/buildl-core/src/lib.rs:32-45`|
|I2's module map placed `Pipeline`, `Ports` and `FakePorts` in `pipeline/` and the queue in `load/`|`.claudestacks/sdlc/2026-09-18-core-foundation/spec.md:187-188`|
|A `bool` must not carry a two-state semantic flag|rust-guidelines `strong-types.md:20`|
|One real implementation is used directly, not behind a trait|rust-guidelines `static-dispatch.md:11`|

### 1.1 Facts established by running something

**`pairs` order differs between fresh engines.** Probe: a table of twelve string keys iterated with
`pairs`, run three times in separate processes.

```
$ airsl --version
airsl 0.1.2
$ airsl run pairs.lua     # ×3
beta,alpha,mu,kappa,theta,lambda,delta,gamma,iota,eta,epsilon,zeta
zeta,eta,mu,lambda,beta,kappa,delta,theta,alpha,gamma,iota,epsilon
alpha,theta,beta,delta,lambda,epsilon,eta,mu,kappa,zeta,iota,gamma
```

Stock `lua5.4` (5.4.8) gives three different orders as well. Consequence: the order in which a file
issues its declarations is not stable across two evaluations, so nothing in Load's output may depend
on it (§4.4). The probe ran the airsl 0.1.2 CLI; the library series this workspace pins is `0.1`
(`Cargo.toml:24`).

**`Directory::parse` refuses every escape form.** `cargo test -p buildl-core directory` →
`8 passed`, including `rejects_the_documented_forms` (`crates/buildl-core/src/types/directory.rs:144-157`),
which rejects `/lib`, `lib/`, `lib//text`, `.`, `..`, `lib/../x`, `lib:x` and `a b`. Consequence: joining
a base directory and an as-written `subdir` path and passing the result to `Directory::parse` rejects
`../x` and `/x` with no extra check (§4.2).

**`Directory` has no join operation.** `grep -n 'fn join\|fn child' directory.rs` matched nothing; the
same search for `fn parse` matched `directory.rs:46` (control). The join lives on `Written<Directory>`
(§3.1).

---

## 2. Decisions

|#|Question (intent)|Decision|Rationale|
|---|---|---|---|
|D1|Shape of `Declaration`|A header (`provenance`) plus a closed enum `Declared` of four kinds: `Target`, `Rule`, `Alias`, `Setting` (§3.3)|design §5 declares things that are not targets; one struct would carry fields meaningless for most kinds|
|D2|When a dependency string becomes a `Label`|At Load, in `buildl-core`, via `Label::resolve` against the declaring directory|The adapter records, core decides; `Declaration.deps` stays `Vec<Label>` as `architecture.md` §2 draws it; `Label` keeps having no unresolved form (`label.rs:3-5`)|
|D3|Sort key of the merge|(directory, declared name, then the full value)|Declaration order is unstable under `pairs` (§1.1); this key is not|
|D4|Queue rules|A directory already seen is skipped; an escaping `subdir` fails `Directory::parse`; an absent build file is an error carrying the requesting call's provenance|§4.2|
|D5|`check`'s double run|In this slice; compares the two sorted lists by value|§5.2|
|D6|Entry file name|An `EntryName` parameter, used for every directory, until the `Manifest` port supplies it|§1 premises; design §3|
|D7|Implicit `subdir`|Rejected; traversal is explicit only (decided in the intent)|The evaluated set is exactly the root plus the closure of `subdir` requests|
|D8|Queue size|No cap in this slice|§6.2|

---

## 3. Data model

All new types live in `types/` (rule 3: one concept, one type), are validated at construction, and
carry no primitive that has a meaning beyond itself.

### 3.1 `Written<T>` — text as a build file wrote it

```rust
pub struct Written<T> {
    text: String,
    target: PhantomData<fn() -> T>,
}
```

One generic newtype for every as-written value: typed by what it is meant to become, not yet
validated. `PhantomData<fn() -> T>` keeps `Written<T>` `Send + Sync` and covariant whatever `T` is.
Construction is infallible (`Written::new(text)`), and `as_written()` returns the text. Each `T` gets
exactly one conversion, so a `Written<Label>` cannot be converted into a `Directory`:

|Conversion|Behaviour|
|---|---|
|`Written<TargetName>::parse()`|`TargetName::parse(text)`|
|`Written<Label>::resolve(&Directory)`|`Label::resolve(text, base)` — the absolute, `:name` and bare forms|
|`Written<Directory>::under(&Directory)`|refuse empty text (an empty `subdir` at the root would otherwise name the root itself); join `base` and `text` with `/` (just `text` at the root), then `Directory::parse`|
|`Written<OutputName>::parse()`|`OutputName::parse(text)` — relative to the target's output directory, not the base|
|`Written<SourcePath>`, `Written<EnvName>`, `Written<Argument>`, `Written<Description>`, `Written<SettingName>`, `Written<SettingValue>`, `Written<FieldName>` `::parse()`|the target type's `parse`, with no base|

Crate rule 2 bounds what lives here. `resolve` and `under` each combine a value with a `Directory` of
the same family: a `Label` contains a `Directory`, and `under` joins two directories. Joining a
`Directory` with a `SourcePath` combines two different concepts, so that join is Load's (§4.3), which
builds the joined text from `as_written()` and passes it to `SourcePath::parse`.

`Written<T>` is not serialized: it never leaves the port boundary.

### 3.2 New value types and their grammars

|Type|Grammar|Reason|
|---|---|---|
|`SourcePath`|`Directory`'s grammar, and non-empty|A workspace-relative file; the shared grammar refuses `..` and absolute paths|
|`OutputName`|`SourcePath`'s grammar|Relative to the target's output directory; defaults to the target name|
|`EnvName`|`[A-Za-z_][A-Za-z0-9_]*`, at most 256 bytes|POSIX-style variable names; checked against the ceiling later|
|`Argument`|any UTF-8 without `NUL`; may be empty|An `argv` element cannot hold `NUL`; an empty argument is legitimate|
|`Command`|`Vec<Argument>`, non-empty|A command with no program is meaningless|
|`Description`|UTF-8, at most 1024 bytes, no control characters|Rendered on one status line|
|`SettingName`|`[A-Za-z0-9_-]`, 1 to 256 bytes|Must end cleanly inside `$opt:name`|
|`SettingValue`|any UTF-8 without `NUL`|Substituted into `argv`|
|`EntryName`|`TargetName`'s grammar|A file name; no `/`|
|`Diagnostic`|any UTF-8|An adapter's rendered message for an evaluation failure; displayed, never parsed (§6.1)|
|`DeclarationOrder`|newtype over `u32`|A file's n-th call; used only to locate an error inside that file|

Grammars shared between types are shared through one private validator per grammar, not by one type
wrapping another. Two-state flags are enums, never `bool`: `TargetRole { Build, Test }`,
`NetworkAccess { Sealed, Declared }`, `Freshness { Cached, Always }`.

### 3.3 `Declaration` — Load's output

```rust
pub struct Declaration { provenance: Provenance, item: Declared }

pub enum Declared {
    Target(Target),   // b.target, and b.test with role Test
    Rule(Rule),       // b.rule
    Alias(Alias),     // b.alias
    Setting(Setting), // b.option
}

pub struct Target {
    label: Label, role: TargetRole, action: Action,
    inputs: Vec<SourcePath>, deps: Vec<Label>, outputs: Vec<OutputName>,
    env: Vec<EnvName>, network: NetworkAccess, freshness: Freshness,
}
pub enum Action { UseRule(Label), Run(Command) }
pub struct Rule    { label: Label, run: Command, description: Option<Description> }
pub struct Alias   { label: Label, target: Label }
pub struct Setting { name: SettingName, default: SettingValue }
```

- **`b.test` is a `Target` with role `Test`**: the same fields, key and caching; a separate kind
  would duplicate `Target`.
- **Rules and aliases share the per-directory label namespace with targets.** One `Label` names one
  thing; `rule = "//tools:cc"` works across directories; a rule named in `deps` becomes a Resolve
  error rather than a collision between namespaces.
- **Settings are workspace-wide**, named by `SettingName`, because `--set name=value` names them
  bare (§1). Two declarations of one setting are a duplicate for Resolve.
- **Inputs are files, targets go through `deps`.** An input is written relative to the declaring
  directory and joined with it.
- **No declaration order.** `Declaration` deliberately carries no `DeclarationOrder`: under `pairs`
  the same declaration gets a different order on each evaluation (§1.1), so an order field would make
  two correct runs compare unequal and fail `check`. Order lives on the staged form only, where it
  locates Load's own errors.
- `Declaration` and every type it holds derive `Serialize`, `Deserialize`, `Clone`, `PartialEq`,
  `Eq`, `PartialOrd`, `Ord`, `Hash` and `Debug`, so the handoff is serializable and totally ordered.
- The records `Target`, `Rule`, `Alias`, `Setting` (and every staged record in §3.4) have `pub`
  fields: they carry no invariant beyond their fields' own types, and `Target`'s nine fields exceed
  what clippy's `too_many_arguments` allows in a constructor. `Declaration` keeps private fields with
  `new` and accessors.

### 3.4 The port's values

```rust
pub struct BuildFile(Provenance);
pub enum   Evaluated   { Staged(StagedFile), Absent }
pub struct StagedFile  { declarations: Vec<StagedDeclaration>, subdirs: Vec<StagedSubdir> }
pub struct StagedSubdir { path: Written<Directory>, order: DeclarationOrder }
pub struct StagedDeclaration { order: DeclarationOrder, item: StagedItem }

pub enum StagedItem { Target(StagedTarget), Rule(StagedRule), Alias(StagedAlias), Setting(StagedSetting) }

pub struct StagedTarget {
    name: Written<TargetName>, role: TargetRole,
    rule: Option<Written<Label>>, run: Option<Vec<Written<Argument>>>,
    inputs: Vec<Written<SourcePath>>, deps: Vec<Written<Label>>,
    outputs: Option<Vec<Written<OutputName>>>, env: Vec<Written<EnvName>>,
    network: NetworkAccess, freshness: Freshness,
}
pub struct StagedRule    { name: Written<TargetName>, run: Vec<Written<Argument>>, description: Option<Written<Description>> }
pub struct StagedAlias   { name: Written<TargetName>, target: Written<Label> }
pub struct StagedSetting { name: Written<SettingName>, default: Written<SettingValue> }
```

- **A declared name is a `Written<TargetName>`, never a `Written<Label>`.** Load builds the label
  as `Label::new(declaring directory, name)`, so a build file cannot declare into another directory
  (`docs/design.md:146`). References to other targets (`rule`, `deps`, an alias's `target`) are
  `Written<Label>` and resolve against the declaring directory.
- `rule` and `run` are both optional; Load enforces exactly one (§4.3). `outputs` is optional; Load
  supplies the default.
- Two-state flags arrive as their enums: they carry no text to validate.

`BuildFile` is a `Provenance`: the file's workspace-relative path and the directory it is evaluated
in are exactly the two fields `Provenance` already holds (`provenance.rs:25-28`), so one concept keeps
one type. Load builds it as `Provenance::new(<dir>/<entry>, dir)`. `Provenance` is not in
`StagedFile`: core built the `BuildFile`, so it already knows which file the result belongs to.

---

## 4. Load

### 4.1 Signature and placement

```rust
// load/traversal.rs
pub fn load<S: DeclarationSource>(source: &S, entry: &EntryName) -> Result<Vec<Declaration>>;
```

`load/` depends on `ports::DeclarationSource` and `types/` only. It takes the one port it needs, not
the `Ports` bundle, so it is testable against a single fake.

### 4.2 The traversal

```text
 queue = [(root, requested_by: None)]   seen = {root}
 while let Some((dir, requested_by)) = queue.pop_front():
     file = BuildFile(Provenance::new(dir/entry, dir))
     match source.evaluate(&file)?:
         Absent         → Err(MissingBuildFile { directory: dir, requested_by })
         Staged(staged) →
             resolve every subdir: subdir.path.under(&dir)           ─ Err → InvalidDeclaration { field: Subdir }
             sort the resolved directories
             for each: if seen.insert(next): queue.push_back((next, Some(file's provenance)))
             for each declaration: convert (§4.3)                    ─ Err → InvalidDeclaration / ActionConflict
 sort (§4.4) and return
```

- **Explicit only.** The evaluated set is exactly the root plus the closure of `subdir` requests.
  Nothing asks the filesystem what exists.
- **Repeats are skipped**, not errors. A cycle cannot be written at all: `under` only joins downward and `..` is refused, so a file can name only directories below its own. The seen set catches repeats, such as the root requesting `a/x` while `a` also requests `x`.
- **Escapes fail at construction**: `under` builds `lib/../x`, which `Directory::parse` rejects
  (§1.1).
- **Absence is core's decision.** The port reports `Absent`; core raises `MissingBuildFile`, with
  `requested_by: None` for the root and `Some(provenance)` of the requesting `subdir` call otherwise.
  The provenance of a `subdir` request is that of the file that made it.
- **Traversal order is deterministic.** Each file's resolved `subdir` directories are sorted before
  they are queued, so the breadth-first order — and therefore which requester a `MissingBuildFile`
  names — does not depend on a file's `subdir` call order.
- **Serial.** `buildl-core` cannot name a thread. On the success path, the sort (§4.4) makes later
  parallel evaluation unobservable.
- **The first error stops Load.** On the error path, when one file holds several invalid declarations
  issued from a `pairs` loop, which one is reported first may vary between runs; the `Ok` output does
  not (§4.4).
- **Adapter errors pass through unchanged** (`?`). The variant they use is defined here
  (`Evaluation`, §6.1), so the adapter needs no change to `buildl-core`.

### 4.3 Staged to typed

Every conversion error is wrapped with where it happened:
`InvalidDeclaration { provenance, order, field, source }`. Beyond the per-field conversions of §3.1,
Load applies three rules:

|Rule|Outcome|
|---|---|
|A declared name becomes a `Label`|`Label::new(declaring directory, name.parse()?)`|
|An input becomes a `SourcePath`|the declaring directory and `as_written()` joined with `/`, then `SourcePath::parse`|
|A target names exactly one of `rule` and `run`|both → `ActionConflict { found: Both }`; neither → `ActionConflict { found: Neither }`|
|A `run` list is empty|`InvalidDeclaration { field: Run, source: InvalidName { kind: Command, .. } }`|
|`outputs` absent|`[OutputName]` equal to the target's name|
|Placeholders in `Argument`s|kept as text: whether `$opt:name` names a declared setting needs every file, so it is Resolve's; expansion is the executor's|

### 4.4 The merge

Load's output is sorted by **(declaring directory, declared name, then the full `Declaration` value)**,
where the declared name is the label's `TargetName` or the setting's `SettingName`, compared as
UTF-8 bytes of its text whichever type it is. The final
component makes the order total without declaration order: two declarations equal in every field are
interchangeable, and any other two are ordered by their content. The result is independent of
evaluation order, of a file's `pairs` order, and of future parallelism.

---

## 5. Ports and the pipeline

### 5.1 Every trait lives in `ports/`

The dependency-inversion rule this slice applies: **every capability that touches the outside world
is a trait in `ports/`; phase logic depends only on those traits and on `types/`; concrete adapters
are named only in a composition root** (`LocalPorts` in `buildl`, `FakePorts` in tests).

```rust
// ports/declaration_source.rs
pub trait DeclarationSource {
    fn evaluate(&self, file: &BuildFile) -> Result<Evaluated>;
}

// ports/bundle.rs
pub trait Ports {
    type Source: DeclarationSource;
}
```

- `Ports` moves from `pipeline/`, where I2's map placed it, to `ports/bundle.rs`, so that every trait
  of the crate is in one module and no logic module declares one. The trait keeps its documented name.
  `ports/` still gains no production implementation: `LocalPorts` lives in `buildl`.
- `Ports` holds only `Source`. `Clock` and the other ports join in the slice that first uses them.
- No `Send + Sync` bound on `DeclarationSource` yet: Load is serial. The slice that parallelises Load
  adds it.
- `Pipeline` calls `load::load` directly. `load` is pure logic with one implementation whose I/O is
  already behind `DeclarationSource`; a trait in front of it would invert nothing (static-dispatch
  rule 1), and `architecture.md` §1.1 gives the pipeline the sequence.

### 5.2 `Pipeline` and `check`

```rust
// pipeline/driver.rs
pub struct Pipeline<P: Ports> { source: P::Source }

impl<P: Ports> Pipeline<P> {
    pub const fn new(source: P::Source) -> Self;
    pub fn check(&self, entry: &EntryName) -> Result<Vec<Declaration>>;
}
```

```text
 check(entry)
   ├─ run 1: load(&source, entry) → A   (sorted)
   ├─ run 2: load(&source, entry) → B   (sorted)
   └─ A == B ? Ok(A) : Err(Nondeterministic { provenance, first_run, second_run })
```

The two lists are compared by value, not by digest: both are in one process. At the first index `i`
where they differ, the error carries `A[i]` and `B[i]` (`None` past the end of a list) — the diff
design §12.1 promises — and `provenance` is that of the lesser of the two. Both lists are sorted and
agree before `i`, so the lesser element is the one the other run lacks, and its file is the one to
fix. Example: `A = [y1]`, `B = [x1, y1]` with `x1 < y1` names `x1`'s file. Because the comparison
happens after the sort and `Declaration` carries no order, a `pairs` loop passes `check`; only real
nondeterminism fails it.

### 5.3 Fakes

The fakes live outside `src/`, in the integration-test helper `crates/buildl-core/tests/flows/common.rs`,
and implement the public traits through the public API only. That placement is deliberate: if the
fakes can build every `StagedFile` and implement `DeclarationSource` from outside the crate, so can
`buildl-lua`, which the compiler then checks on every build. It also keeps `load/` from naming
`pipeline/`, even in tests.

`FakeSource` holds a `BTreeMap<Directory, FakeFile>`, where `FakeFile` is either a `StagedFile` to
return or an `EvaluationFailure` to raise as `Error::Evaluation`, and returns `Absent` for a directory
it does not hold; `FakePorts` implements `Ports` with `Source = FakeSource`. A fake that returns a
different `StagedFile` on its second call (interior mutability) exercises `Nondeterministic`.

---

## 6. Errors and limits

### 6.1 New variants

All are additions to the `#[non_exhaustive]` `Error`; none reshapes an existing one.

|Variant|Fields|Raised when|
|---|---|---|
|`Evaluation`|`provenance`, `failure: EvaluationFailure`, `diagnostic: Diagnostic`|the adapter could not evaluate a build file|
|`MissingBuildFile`|`directory: Directory`, `requested_by: Option<Provenance>`|the port returns `Absent`|
|`InvalidDeclaration`|`provenance`, `order: DeclarationOrder`, `field: DeclarationField`, `source: Box<Self>` (`use_self` rejects `Box<Error>` inside the enum)|a `Written<T>` fails its conversion, including an escaping `subdir`|
|`ActionConflict`|`provenance`, `order`, `found: ActionFound`|a target names both or neither of `rule` and `run`|
|`Nondeterministic`|`provenance`, `first_run: Option<Box<Declaration>>`, `second_run: Option<Box<Declaration>>`|`check`'s two runs differ|

`EvaluationFailure` is the closed set of ways an adapter can fail one file, fixed here so the adapter
never has to amend `buildl-core`:

|Variant|Meaning|
|---|---|
|`Syntax`|the build file does not parse|
|`Runtime`|evaluation raised an error|
|`Refused`|the runtime refused an operation the declaration policy does not grant|
|`LimitReached { limit: EvaluationLimit }`|`Instructions`, `Memory` or `Staging` (the per-file staging cap) was hit|
|`UnknownField { field: Written<FieldName> }`|an option table holds a field the primitive does not accept|
|`WrongFieldType { field: Written<FieldName> }`|a field holds a value of the wrong Lua type|

`FieldName` uses `SettingName`'s grammar. `Diagnostic` carries the adapter's rendered message for
display; nothing branches on it. `DeclarationField` (`Name`, `Dep`, `Input`, `Output`, `Env`, `Run`,
`Rule`, `Description`, `Target`, `SettingName`, `SettingValue`, `Subdir`) and `ActionFound` (`Both`,
`Neither`) are enums, so a caller reads a field and never parses a message. Each `Display` names what
was wanted, what was found and where it was declared. `NameKind` gains one variant per new validated
type, `Command` included.

### 6.2 Queue size, and the risk left to I-lua

The queue is not capped. Its size is bounded by construction: a directory is queued at most once, and
every evaluated directory must hold a real build file, or Load stops. Since a `subdir` can only name a descendant, the
traversal is a tree walk and cannot loop.

One resource risk is real and is not the queue's: the adapter stages declarations in a Rust-side
buffer (`architecture.md:148`), which the Lua memory ceiling (`[declaration] memory`,
`docs/design.md:103`) does not count. The instruction ceiling bounds a file's calls, but how many
declarations it admits depends on airsl's per-call cost, which is not verified. Since declaration is
meant to be safe on untrusted input (`docs/design.md:56`), I-lua adds a per-file staging cap, refused
in airsl's refusal style. It is recorded as a roadmap follow-up (§9).

---

## 7. Module layout

```text
crates/buildl-core/src/
  lib.rs          + pub mod load; pub mod pipeline; re-exports; the module map in the crate doc
  error.rs        + Evaluation, MissingBuildFile, InvalidDeclaration, ActionConflict, Nondeterministic,
                    EvaluationFailure, EvaluationLimit, DeclarationField, ActionFound, new NameKind variants
  types/          + written.rs  source_path.rs  output_name.rs  env_name.rs  argument.rs
                    description.rs  setting.rs  entry_name.rs  field_name.rs  diagnostic.rs
                    declaration.rs  build_file.rs
  ports/          + declaration_source.rs  bundle.rs
  load/           NEW  mod.rs (export-only)  traversal.rs
  pipeline/       NEW  mod.rs (export-only)  driver.rs

crates/buildl-core/tests/flows/      one integration-test binary
  main.rs         crate doc; `pub mod common; mod load; mod check;`
  common.rs       FakeSource, FakeFile, FakePorts — public-API fakes, `pub` and documented
  load.rs         traversal flows through the port
  check.rs        Pipeline::check flows

The logic files are `traversal.rs` and `driver.rs`, not `load/load.rs` and `pipeline/pipeline.rs`:
clippy's `module_inception` fails the gate on a module named after its parent (probe: `error: module
has the same name as its containing module` under `-D warnings`).

One binary rather than one per file, because the gate's lints make a shared helper module
unworkable across several (probe: `tests/common/mod.rs` shared by two test files failed
`cargo clippy --all-targets -- -D warnings` with `missing documentation for the crate`,
`unreachable pub item` and `function only_a is never used`; `pub(crate)` instead fails
`redundant_pub_crate`). A `pub mod common` with documented `pub` items in a single
`tests/flows/main.rs` binary passed the same command.
```

`declaration.rs` holds `Declaration`, `Declared`, `Target`, `Action`, `Rule`, `Alias`, `Setting` and
the three flag enums; `build_file.rs` holds `BuildFile`, `Evaluated`, `StagedFile`,
`StagedDeclaration`, `StagedItem` and its variants, `StagedSubdir` and `DeclarationOrder`;
`argument.rs` holds `Argument` and `Command`; `setting.rs` holds `SettingName` and `SettingValue`. The
crate-doc rules still hold: `pipeline/` is the only module that names `load/`, and `load/` names no
other phase.

---

## 8. Testing

Two layers, and neither replaces the other:

- **Colocated unit tests.** Every logic file carries `#[cfg(test)]` tests; `mod.rs` and `lib.rs` are
  export-only, and the two trait files are pure trait definitions. In `load/traversal.rs` they cover the
  pure helpers that need no port: resolving a `subdir`, converting a staged declaration, the sort key.
- **Integration tests** in `crates/buildl-core/tests/`, against the public-API fakes of §5.3: every
  flow that goes through `DeclarationSource`, both for `load` and for `Pipeline::check`.

No test uses Lua, the filesystem, a process or a thread.

|Scenario|Assertion|
|---|---|
|`subdir` requests enqueued out of order (`architecture-building-blocks.md` §8)|output sorted by directory, then declared name|
|the same file's declarations shuffled, as `pairs` would|canonical JSON of the output is byte-identical|
|a `subdir` repeated in one file, and the root requesting `a/x` while `a` requests `x`|each directory evaluated exactly once|
|`subdir("../x")`, `subdir("/x")`|`InvalidDeclaration { field: Subdir }` with the requester's provenance|
|root absent; a requested directory absent|`MissingBuildFile` with `requested_by` `None`; `Some(provenance)`|
|a target with both, and with neither, of `rule` and `run`|`ActionConflict { found: Both }`; `{ found: Neither }`|
|no `outputs`|`[target name]`|
|`deps` mixing `main.o`, `:util.o`, `//lib:text` in `//app`|`//app:main.o`, `//app:util.o`, `//lib:text`|
|an invalid name in each field|`InvalidDeclaration` with that `DeclarationField`|
|two files requesting the same absent directory, their `subdir` calls shuffled|`MissingBuildFile.requested_by` is the same on every run|
|a fake holding an `EvaluationFailure`|`load` returns `Error::Evaluation` unchanged|
|a target named `//other:x`|`InvalidDeclaration { field: Name }`: a declaration cannot leave its directory|
|`run = {}`|`InvalidDeclaration { field: Run }`|
|a fake whose second evaluation differs|`check` → `Nondeterministic` carrying both differing declarations and the lacking file's provenance|
|a `pairs`-shuffled fake across the two runs|`check` succeeds|
|every new type|accept and reject sides of its grammar; a serde round-trip for every serialized type (`Written<T>` and `Diagnostic` never leave the port boundary or an error, so they have none); `Display` where it is hand-written|
|every new `Error` variant|its `Display` line|

---

## 9. Documentation deliverables and amendments

|Document|Change|
|---|---|
|`crates/buildl-core/src/lib.rs` crate doc and `crates/buildl-core/README.md`|the module map gains `load` and `pipeline`; `ports` is described as holding every trait, the bundle included; the status paragraph names Load as implemented|
|`docs/design.md` §14|implicit `subdir`: resolved — explicit only|
|`docs/design.md` §12.1|"compare staging-list hashes" becomes a comparison of the two sorted staging lists; "with the diff" stays, now true (§5.2)|
|`docs/architecture.md` §2|the Mermaid `Declaration` class redrawn to §3.3; the `Label` bullet says labels are resolved at Load|
|`docs/architecture.md` §3.1|the merge key becomes (directory, declared name, value); the staging buffer holds staged declarations; `b.subdir` requests return inside `StagedFile` instead of pushing onto the queue|
|`docs/architecture.md` §7|the `declare` row asserts exact `StagedFile`s for `buildl-lua` fixtures, not `Vec<Declaration>`|
|`docs/architecture.md` §5 rule 5|"diffs staging hashes" becomes "compares the two staging lists"|
|`crates/buildl-core/src/lib.rs` crate rule 5|a phase's pure helpers are tested in the file under test; its flows through a port are tested in `tests/` against public-API fakes|
|`docs/architecture-building-blocks.md` §8|the fakes live in `buildl-core`'s integration tests (`tests/flows/common.rs`), not under `#[cfg(test)]`|
|`docs/architecture-building-blocks.md` §5.2, §5.3|`evaluate` returns `Result<Evaluated>`; `Ports` is declared in `ports/`|
|`docs/architecture-building-blocks.md` §8|the out-of-order scenario's assertion becomes "sorted by directory, then declared name"|
|`docs/roadmap.md`|the I3 row and §4 on completion; a new §5 follow-up: per-file staging cap, target I-lua|
|`README.md` (repository)|the status line no longer says no pipeline phase is implemented|

Section numbers and Mermaid blocks in `docs/` are preserved.

---

## 10. Definition of Done

- `cargo make dod` and `cargo deny check` pass, including `guard-crate-edges` and `guard-core-purity`.
- `buildl-core`'s dependency set is unchanged.
- A `FakeSource` holding fixed build files yields one sorted `Vec<Declaration>` through `load`, and
  through `Pipeline::check`.
- Every scenario of §8 has a test.

---

## 11. Non-goals

- The airsl engine, the declaration policy, the `buildl` module table and its `buildl` global binding,
  and the per-file staging cap — I-lua.
- `b.sources()`, `b.fetch`, `b.use`.
- Duplicate detection, unknown references, `$opt:name` validation, rule expansion, cycles,
  `TargetGraph` — Resolve.
- The `Manifest` port, the ceiling, the wanted set, grants, a workspace-wide load cap.
- Parallel load and the `Send + Sync` bound it would need.
- Any adapter in `buildl`, anything in `buildl-cli`, a published fake-ports kit.
- Roadmap follow-ups 3, 4 and 11.
