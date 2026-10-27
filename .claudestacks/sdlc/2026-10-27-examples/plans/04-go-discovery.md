---
status: approved
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

**Files:** Create `examples/go-discovery/*`

**Change:**
- First step, before writing the example around it: run `go list -json ./...` as a discovery
  target with `stdout = true` through `buildl` in a scratch copy, and check that the decoded
  values carry `ImportPath` and `Dir` (spec §8 risk 1).
- `buildl.toml`: `proc.run = ["go"]`, `env.read = ["HOME"]`, `fs.read`, `fs.write` as the Go
  starter's.
- `build.lua`: `buildl.discover("pkgs", { run = { "go", "list", "-json", "./..." }, inputs = { "go.mod", buildl.sources("**/*.go") }, env = { "HOME" }, stdout = true })`;
  then, when `buildl.discovered(":pkgs")` is not `nil`, for each value of `buildl.decode_all(…)`
  one `buildl.test` named after the package's path relative to the module, running
  `go test ./<pkg>` with that package's sources and `go.mod` as inputs. Package order follows
  `go list`'s order, which the file lists as written (design §5: a list keeps its order).
- `assert.sh`: after `buildl build`, `buildl graph --json` holds exactly one test target per
  package directory (counted from the directories holding `.go` files); `buildl test` passes each.
  (No `plan` check here: `plan` with no targets leaves the tests out, design.md:307, and the
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
