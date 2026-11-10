---
status: approved
created: 2026-11-10
depends-on: [03]
built-on: 6643cfeb82a2
---

# Walk Filter Implementation Plan

**Goal:** `buildl.sources` never lists, enters or counts a path the workspace's ignore rules report ignored.

**Architecture:** `LuaSource` (`crates/buildl-lua/src/source.rs:75-112`) becomes generic over
`I: IgnoreRules + Clone` and holds the rule set beside its `skipped` list; `Sources`
(`crates/buildl-lua/src/sources.rs:37-60`) holds a clone. In `find`, before walking, the declaring directory
is asked as a `Folder` (unless it is the workspace root, which the port is never asked about, plan
01): if it is ignored, `find` returns nothing, as for a skipped one (`sources.rs:97`). The
`filter_entry` closure (`sources.rs:104`) lets the depth-0 entry (the declaring directory itself)
through unasked, keeps today's skip-list check, then asks the rule set for the entry's
workspace-relative path with `Folder` or `File` from the entry's type; an ignored entry is filtered
out before it is counted, so an ignored folder is never entered. An entry whose relative path is not
UTF-8 is not asked. The predicate returns a `bool`, so a rule-set error is stored in a local slot
the closure writes (the first error only) and the predicate returns `false`; the loop checks the
slot after each item and, once it is filled, returns a `Refusal` whose text is the
`IgnoreFile` message, so it reaches the command line as an `Evaluation` error with that text (spec
§5). The composition root (`crates/buildl/src/local_ports.rs:303`) gives `LuaSource::new` a clone of
the same `BuildlIgnore` plan 03 built for the workspace open. Every other call of `Sources::new` and
`LuaSource::new`, in `buildl-lua`'s unit tests, integration tests and doctest, passes
`NothingIgnored` (plan 01), except the new tests.

**Tech Stack:** Rust 1.94, `buildl-lua` (walkdir, globset; no `ignore` dependency), `buildl`
composition and end-to-end harness.

**Content authority:** `spec.md` §2.2 (listed like any file; out and state stay skipped), §3 (walk
and composition rows), §5 (walk route of `IgnoreFile`), §6 (rows: pattern naming out, self-ignore,
`sources` from an ignored folder, non-UTF-8 walk entry), §8 (`buildl-lua` walk, `buildl` end to end,
`buildl-cli` walk route).

---

## Files that change

```
crates/buildl-lua/src/sources.rs           — modify: Sources holds a rule set; filter_entry asks it; walk tests
crates/buildl-lua/src/source.rs            — modify: LuaSource<I> holds and passes the rule set (:75-151); its doctest (:66) passes NothingIgnored
crates/buildl-lua/src/lib.rs               — modify: re-export of LuaSource keeps its path; docs mention the rule set
crates/buildl-lua/src/engine.rs            — modify: test call of Sources::new (:73)
crates/buildl-lua/src/module.rs            — modify: test calls of Sources::new (:423, :713)
crates/buildl-lua/src/decode_all.rs        — modify: test call of Sources::new (:92)
crates/buildl-lua/src/plugin/environment.rs — modify: test call of Sources::new (:140)
crates/buildl-lua/src/plugin/loader.rs     — modify: test call of Sources::new (:286)
crates/buildl-lua/tests/flows/common.rs    — modify: LuaSource::new call (:41) passes NothingIgnored
crates/buildl-lua/tests/flows/discovery.rs — modify: LuaSource::new call (:40) passes NothingIgnored
crates/buildl-lua/tests/flows/failures.rs  — modify: LuaSource::new call (:108) passes NothingIgnored
crates/buildl-lua/tests/flows/load.rs      — modify: LuaSource::new call (:62) passes NothingIgnored
crates/buildl-lua/tests/flows/plugins.rs   — modify: LuaSource::new calls (:53, :394) pass NothingIgnored
crates/buildl-lua/tests/flows/sandbox.rs   — modify: LuaSource::new call (:52) passes NothingIgnored
crates/buildl-lua/tests/flows/skipped.rs   — modify: LuaSource::new calls (:69-168) pass NothingIgnored
crates/buildl/src/local_ports.rs           — modify: LuaSource::new receives the workspace's BuildlIgnore; type Source = LuaSource<BuildlIgnore>
crates/buildl/tests/e2e/ignore.rs          — create: end-to-end cases of this plan
crates/buildl/tests/e2e/main.rs            — modify: declare the module
crates/buildl-cli/tests/cli/exit_codes.rs  — modify: the walk route of a bad .buildlignore
```

## Order of work

### Task 1 — The walk asks the rule set

**Files:**
- Modify `crates/buildl-lua/src/sources.rs`, `source.rs` (its doctest included), `lib.rs` — the
  generic rule set
- Modify `crates/buildl-lua/src/engine.rs`, `module.rs`, `decode_all.rs`, `plugin/environment.rs`,
  `plugin/loader.rs` — their test calls pass `NothingIgnored`
- Modify `crates/buildl-lua/tests/flows/common.rs`, `discovery.rs`, `failures.rs`, `load.rs`,
  `plugins.rs`, `sandbox.rs`, `skipped.rs` — their `LuaSource::new` calls pass `NothingIgnored`
- Modify `crates/buildl/src/local_ports.rs` — pass the instance; the `Source` type

**Change:** as the Architecture says. The skip list for out and state stays and is checked first, so
a pattern naming them, or `!out/`, changes nothing. A `.buildlignore` is an ordinary entry. The doc
of `find` says ignored entries are neither visited nor counted.

**Proof:**
- `an_ignored_folder_is_not_entered` — a tree with `notes/` holding 50 files and a rule set ignoring
  the folder `notes`: `find("**/*")` lists none of them, and a walk ceiling smaller than 50 is not
  reached.
- `an_ignored_file_is_not_listed` — a rule set ignoring `a.log` leaves `a.log` out and `b.log` in.
- `a_buildlignore_is_listed_unless_ignored` — `.buildlignore` is listed by `**/*` with a rule set
  that does not ignore it, and is left out by one that does.
- `entries_are_asked_with_their_kind_and_full_path` — from the declaring directory `lib`, the rule
  set records `lib/docs` as `Folder` and `lib/l.rs` as `File`.
- `the_skip_list_wins_over_the_rules` — a rule set that ignores nothing still never lists `out/` or
  `.buildl`; one that "re-includes" them changes nothing.
- `a_walk_from_an_ignored_folder_lists_nothing` — the declaring directory `notes` with `notes`
  ignored returns an empty list.
- `a_rule_error_stops_the_walk_with_its_text` — a rule set failing with an `IgnoreFile` for
  `lib/.buildlignore:1: …` makes `find` return a `Refusal` whose text is that message.
- `the_root_is_never_asked` — a walk declared at the workspace root records no question for the
  empty path.
- `a_non_utf8_entry_is_not_asked` (`#[cfg(target_os = "linux")]`, as the sibling non-UTF-8 test at
  `sources.rs:429-436`, since macOS refuses such names) — a folder with a non-UTF-8 name under the declaring
  directory is not asked; a pattern that matches no file inside it lists nothing from it, as today.

**Done when:** `cargo test -p buildl-lua -p buildl --all-targets --all-features` passes and `cargo make dod`
and `cargo deny check -D unused-wrapper` are green. Commit `feat(buildl-lua): sources leaves out ignored paths`.

### Task 2 — Prove the outcome end to end

**Files:**
- Create `crates/buildl/tests/e2e/ignore.rs`, modify `tests/e2e/main.rs` — the cases
- Modify `crates/buildl-cli/tests/cli/exit_codes.rs` — the walk route

**Change:** none in source.

**Proof:**
- `editing_an_ignored_file_leaves_the_target_clean` — a workspace with `.buildlignore` `notes/`, a
  target whose inputs are `buildl.sources("**/*")`: build, edit `notes/a.md`, `plan` reports the
  target clean; edit `src/a.c`, `plan` reports it to run.
- `a_nested_buildlignore_applies_below_its_folder` — `src/.buildlignore` `gen/`: a file under
  `src/gen/` is not among the target's inputs in `plan --json`, a file under `gen/` at the root is.
- `a_bad_buildlignore_found_by_the_walk_exits_5` (in `exit_codes.rs`) — `lib/.buildlignore` holds
  `[z-a]` and the root build file lists `buildl.sources("**/*")`: `buildl check` exits 5 and stderr
  holds `lib/.buildlignore:1: error parsing glob '[z-a]': invalid range; 'z' > 'a'`.

**Done when:** `cargo test -p buildl --test e2e --all-features ignore` and
`cargo test -p buildl-cli --all-targets --all-features` pass and `cargo make dod` and `cargo deny check -D unused-wrapper` are green.
Commit `test(buildl): ignored paths stay out of the action key`.

---

## Risks

- Making `LuaSource` generic changes its type in `local_ports.rs` and in every test that names it;
  the compiler names each site. The riskiest step is Task 1's `filter_entry`: it must ask with the
  workspace-relative path, not the path relative to the declaring directory, which
  `entries_are_asked_with_their_kind_and_full_path` pins.
- A `Refusal` keeps only text; tests of the walk route assert the text and the exit code, not a
  variant (spec §5).

## Proof

- `cargo make dod` green after each task; `cargo make guard-crate-edges` passes (`buildl-lua` gains
  no dependency); `cargo deny check -D unused-wrapper` passes.
