---
status: done
created: 2026-10-27
depends-on: [01, 02, 03]
built-on: 5947af3e315f
---

# Go discovery example — Implementation Plan

**Goal:** an example declares one Go test target per package that `go list -json ./...` reports, through a discovery target with `stdout = true` read back by `buildl.decode_all`.

**Architecture:** a Go module with several packages under `examples/go-discovery/`. Its root
build file declares a discovery target running `go list -json ./...` with `stdout = true`
(plan 01), decodes `buildl.discovered(":pkgs")` with `buildl.decode_all` (plan 02), and declares
one `buildl.test` per package from the result; on the first pass the result is `nil` and only
the discovery target is declared (design §10). Checked by `examples/check.sh` (plan 03).

**Tech Stack:** Go 1.22, the `buildl` binary built with plans 01 and 02.

Authority: `.claudestacks/sdlc/2026-10-27-examples/spec.md` §2, §5, §7. Gates for every task:
`cargo make dod`, `cargo deny check` and `cargo make examples` green; Conventional Commit, one per task.

---

## Files that change

```
examples/go-discovery/{buildl.toml,build.lua,go.mod,assert.sh,README.md} — [create] the workspace
examples/go-discovery/<pkg>/{*.go,*_test.go}                            — [create] at least three packages, each with a passing test
docs/design.md                                                          — [modify] §10 example
```

## Order of work

### Task 1 — The discovery example

**Files:** Create `examples/go-discovery/*`; modify `examples/README.md` (a row for the example)
and `examples/check.sh` (an example with no build target passes the `plan --json` step)

**Change:**
- First step, before writing the example around it: run `go list -json ./...` as a discovery
  target with `stdout = true` through `buildl` in a scratch copy, and check that the decoded
  values carry `ImportPath` and `Dir` (spec §8 risk 1). Probed: they do, and `Dir`, `Root`,
  `Module.Dir` and `Module.GoMod` hold the absolute path of the action's directory, which is the
  same on every run of one checkout but differs between checkouts and machines; the example runs
  `go list -json=ImportPath ./...` instead, which prints only the field it reads.
- `buildl.toml`: `proc.run = ["go"]`, `env.read = ["HOME"]`, `fs.read`, `fs.write` as the Go
  starter's.
- `build.lua`: `buildl.discover("pkgs", { run = { "go", "list", "-json=ImportPath", "./..." }, inputs = { "go.mod", buildl.sources("**/*.go") }, env = { "HOME" }, stdout = true })`;
  then, when `buildl.discovered(":pkgs")` is not `nil`, for each value of `buildl.decode_all(…)`
  one `buildl.test` named after the package's path relative to the module (the module path is
  go.mod's, written in the build file), with `/` written `.` because a target name holds no `/`
  (`grammar::name` in `crates/buildl-core/src/types/grammar.rs`), running
  `go test ./<pkg>` with that package's sources and `go.mod` as inputs. Package order follows
  `go list`'s order, which the file lists as written (design §5: a list keeps its order).
- `examples/check.sh`: `plan --json` for this example reports no target at all (only tests and a
  discovery target are declared), and the `plan --json` step passes when every reported target,
  of none, is `Clean`. The shared `verify` step checks nothing here either (the discovery target
  is outside its default scope, and tests are not verified), so `assert.sh` also runs
  `buildl verify //:pkgs` and expects `1 verified, 0 mismatched`.
- `assert.sh`: after `buildl build`, `buildl graph --json` holds exactly one test target per
  package directory (counted from the directories holding `.go` files); `buildl test` passes each.
  (No `plan` check here: `plan` with no targets leaves the tests out, design §7, and the
  example declares only tests and the discovery target.)
- `README.md`: what a discovery target is, why `stdout = true` (go list prints to stdout), and
  why `decode_all` (go list prints a stream of objects, which `airsstack.json.decode` rejects).

**Proof:** `cargo make examples` passes with `go-discovery` included.

**Done when:** it passes, with `cargo make dod`. Commit `feat(repo): Go discovery example`.

### Task 2 — Make design §10's example true

**Files:** Modify `docs/design.md`

**Change:** §10's discovery example (`:733-741`) gains `stdout = true` and decodes with
`buildl.decode_all`; a sentence says why each is needed, pointing at the example.

**Proof:** `cargo make dod` green.

**Done when:** committed `docs(repo): make the go list example of design §10 true`.

## Risks

- **`go list` output paths.** `Dir` holds the exec dir's absolute path, which differs from the
  workspace's; the build file uses `ImportPath` relative to the module path, never `Dir`.
- **Discovery consistency.** The second pass must declare the same discovery target as the first
  (design §10); the discovery target's own fields never depend on the result.
- **Chose not to:** discover dependencies between packages; cache `go test` across packages.

## Proof

- `cargo make examples` green locally and in the CI `examples` job on both operating systems.

## Review findings

- false claim — the build file, README and plan said the full `go list -json` output differs from run to run; the action's directory is named from the workspace root and the label, so it is stable within one checkout. All three now say the absolute paths (`Dir`, `Root`, `Module.Dir`) differ between checkouts and machines. Fixed at `ec1eab1`.
- placement — the `go-discovery` row in `examples/README.md` sat before `python`, whose "the same" then read wrong; moved after it. Fixed.
- spec gap — the shared `verify` step checks nothing for this example (`0 verified`), so the stdout output was never re-run in CI; `assert.sh` now runs `buildl verify //:pkgs` and expects `1 verified, 0 mismatched`. Fixed; `cargo make examples` passed all four.
- nits — `find ! -path` in place of `-not`; `LC_ALL=C sort` for the expected list; design §10 names `-json=ImportPath`; the plan cites `grammar::name` and design §7 rather than line numbers. Fixed.
- Left as is — `check.sh`'s `plan --json` step no longer fails on an empty plan, which also loosens it for the starters; their own `assert.sh` checks `plan` after an edit. A `.go` file at the module root would give an empty target name; this example has none.

## Probe results

- `go list -json ./...` as a `stdout = true` discovery target in a scratch copy: decoded values carry `ImportPath` and `Dir`; `Dir`, `Module.Dir` and `Module.GoMod` hold the action directory's absolute path — against the plan's command, which now uses `-json=ImportPath` (Task 1 amended).
- A target name holds no `/` (`grammar::name`); `text/words` becomes `//:text.words` — the plan said "named after the package's path"; amended.
- `plan --json` reports `"targets":{}` for this example, and `check.sh`'s step required at least one target; the step now accepts none (Task 1 amended).
- CI run 40630488346: `Examples (ubuntu-latest)` and `Examples (macos-latest)` success with `go-discovery` included.

## Deviations

- 2026-10-28 — `go list -json=ImportPath` in place of `go list -json` (probe above); Task 1 amended.
- 2026-10-28 — test targets named with `/` written `.`; Task 1 amended.
- 2026-10-28 — Task 1 also changed `examples/check.sh` (an empty `plan --json` passes) and `examples/README.md` (a row); added to the plan's file list.
- 2026-10-28 — `assert.sh` adds `buildl verify //:pkgs` (review finding), since the shared `verify` step checks nothing for this example.
