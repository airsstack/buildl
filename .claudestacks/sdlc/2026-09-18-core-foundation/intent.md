---
status: done
created: 2026-09-18
---

# Intent: The `buildl-core` foundation — names, content identity, errors, determinism

## Problem

`buildl-core` exists but is empty. `crates/buildl-core/src/lib.rs` is a doc-comment stub with no
types, no ports and no logic, and the crate compiles because there is nothing in it to compile.

Every slice after this one needs a vocabulary that does not exist yet:

- **No name type.** A target is written `//dir:name` in a build file and referred to by that name
  in dependency lists, error messages, cache rows and the graph. Nothing in the workspace can
  parse, validate, compare or render one. Until it can, Load has nothing to put in a declaration
  and Resolve has no key to resolve against.
- **No content identity.** Incrementality is built on SHA-256 over file contents, action keys, log
  blobs and outputs. There is no single type carrying a digest, so the first slice that needs one
  would invent its own and the second would invent a different one.
- **No node handle, no provenance, no timestamp.** The graph addresses targets by index; every
  error is supposed to name the build file that declared the target; the event log is supposed to
  carry times. None of the three types exist.
- **No error model.** `architecture.md` §4 settles the architecture — one `thiserror` enum with
  structured fields, never message parsing; anything nameable carries its label and provenance; the
  display impl renders airsl-refusal-shaped messages. None of it is written, so the first fallible
  function in the workspace will decide the error shape by accident.
- **Two determinism rules have no machinery.** `architecture.md` §5 rule 2 requires all JSON to
  leave through one canonical sorted-key serializer, so "the key of X" and "the file of X" can
  never disagree. Rule 4 quarantines the wall clock behind a port whose only real adapter lives in
  `buildl`. Neither the serializer nor the port exists, so both rules are currently aspirations
  rather than code.

This is the blocking slice. `docs/roadmap.md` §3 puts it directly after the completed workspace
chain, and I3 (Load), I4 (Resolve), I5 (Plan) and I6 (Execute + Record) all consume its output.
Nothing else in milestone 1 can start.

Two questions the design pass has to close, both open in the documents today:

- `design.md` §14 leaves label syntax unfinished: relative labels (`:sibling`) and whether `subdir`
  should be implicit via file discovery are both undecided, and both change what a parsed label
  accepts.
- The relationship between the timestamp type and `SOURCE_DATE_EPOCH` (`design.md` reference [3])
  is unstated, as is the canonical serializer's float handling.

## Affected systems

- `crates/buildl-core` — the crate's first real modules, its rustdoc and its `README.md`
- Workspace `Cargo.toml`, only if this slice's dependency needs differ from the catalogued set

## Desired outcome

- `buildl-core` provides the names and identities every later phase shares: a validated label, a
  content digest, a graph node handle, a provenance record and a timestamp. Each is a newtype
  validated at construction, following airsl's `types/` discipline, so an invalid value cannot be
  built.
- The error model of `architecture.md` §4 is real code: one enum, structured fields that are read
  as fields and never parsed from text, label and provenance carried on every error that can name a
  target, and a display impl that states what was wanted, what was found, and where it was
  declared. The enum is `#[non_exhaustive]`, and later slices add the variants they earn without a
  breaking change.
- A single canonical JSON serializer exists and is the only way a value in this workspace becomes
  JSON bytes, satisfying `architecture.md` §5 rule 2.
- A clock port exists as a trait in `buildl-core`, with no implementation in this crate, satisfying
  `architecture.md` §5 rule 4 by construction: the crate cannot name the system clock.
- Every type and the serializer are unit-tested in `buildl-core` with no I/O of any kind.
- `crates/buildl-core`'s rustdoc and `README.md` describe what the crate now holds, closing the
  part of roadmap follow-up 3 that belongs to this crate.
- `cargo make dod` and `cargo deny check` pass.

## Constraints

- `buildl-core` may depend only on `std` — excluding its filesystem, process, thread, environment
  and clock APIs — plus `serde`, `serde_json`, `thiserror` and `sha2`
  (`architecture-building-blocks.md` §1.2). No buildl crate, no airsl.
- Determinism house rules (`architecture.md` §5) bind every line written here: no `HashMap`
  iteration reaches any output, all JSON leaves through the one serializer, and the wall clock is
  reachable only through the port.
- The digest is SHA-256, matching the Remote Execution API's digest model, so the remote-cache
  milestone stays a transport problem rather than a re-keying one.
- Workspace policy is unchanged: `unsafe_code = "forbid"`, `unwrap_used` and `panic` denied,
  pedantic and nursery clippy at warn, every workspace dependency commented with its reason.
- Rust guideline rules apply: `lib.rs` is export-only with its crate-level doc, and rustdoc and
  crate READMEs carry no internal planning references, including `docs/` section citations.
- Section cross-references in `docs/` are load-bearing and survive any edit made here.

## Non-goals

- The phase handoff types — `Declaration`, `BuildFile`, `StagedFile` (I3), `TargetGraph` (I4),
  `Plan` (I5), `ActionOutcome` (I6).
- `OutcomeClass`, `Ceiling`, `WantedSet`, `Grants`, `KeyComponents` and `ActionKey`. Each is settled
  by the slice that first needs it, so its data model is decided under real pressure rather than in
  advance.
- `Pipeline`, the `Ports` bundle and `FakePorts`. `architecture-building-blocks.md` §5.3 and §8
  place them in I3; this chain defines one port trait, not the bundle that carries it.
- Any port adapter, any filesystem or process code, anything in `buildl`, `buildl-lua` or
  `buildl-cli`.
- The workspace gate guards that enforce these constraints mechanically. They are the sibling
  chain, `2026-09-18-workspace-guardrails`.
- Publishing readiness of any kind.
