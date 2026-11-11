---
status: executing
created: 2026-11-11
depends-on: [01]
built-on: a6e2956d9de2
---

# CI Job Implementation Plan

**Goal:** A CI job builds and tests buildl with the pinned release on Linux and macOS, keeping
`.buildl/` in GitHub's Actions cache.

**Architecture:** One new job in `.github/workflows/ci.yml`, beside the existing ones and on the
workflow's existing triggers; no other job changes. The job installs the version
`.buildl-version` names, checks it, restores `.buildl/`, runs buildl twice, prunes the store with
`buildl gc`, and lets `actions/cache@v5` save on a miss.

**Tech Stack:** GitHub Actions YAML, `actions/checkout@v5`, `actions/cache@v5`, POSIX shell.

**Content authority:** `spec.md` §4 (install and check), §5, §8 (the CI proof), D6, D8, D11.

---

## Files that change

```
.github/workflows/ci.yml   — a new job, `self-build`, after `dod`
```

## Order of work

### Task 1 — Add the self-build job

**Files:**
- Modify `.github/workflows/ci.yml` — a new job `self-build`, named `buildl builds buildl`, with a
  comment above it that says what it proves and that `.buildl-version` is the pin.

**Change:** The job, in this order (spec §5 sketch):

1. `strategy.fail-fast: false`, `matrix.os: [ubuntu-latest, macos-latest]`,
   `runs-on: ${{ matrix.os }}`, `timeout-minutes: 60`.
2. `actions/checkout@v5`.
3. The `dod` job's toolchain step: `rustup toolchain install`, `rustup show`, `cargo --version`.
4. **Install buildl** (step id `pin`): read the pin with `pin="$(cat .buildl-version)"`, run
   `curl -LsSf https://airsstack.github.io/buildl/install.sh | BUILDL_VERSION="$pin"
   BUILDL_CLI_NO_MODIFY_PATH=1 sh`, fail unless `"$HOME/.local/bin/buildl" --version` prints
   exactly `buildl $pin`, then append `$HOME/.local/bin` to `$GITHUB_PATH` and write `version=$pin`
   to `$GITHUB_OUTPUT`.
5. **Toolchain key** (step id `rustc`): write `hash=` and the SHA-256 of `rustc -vV`'s output to
   `$GITHUB_OUTPUT`, the first field of `sha256sum` where it is on `PATH`, else of
   `shasum -a 256` (neither tool's presence on both runner images is recorded, so the step takes
   whichever is there and fails, naming both, when neither is).
6. **`actions/cache@v5`** on `path: .buildl`, with
   `key: buildl-${{ matrix.os }}-${{ steps.pin.outputs.version }}-${{ steps.rustc.outputs.hash }}-${{ hashFiles(...) }}`,
   the `hashFiles` list exactly `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `crates/**`,
   `build.lua`, `buildl.toml`, `buildl.lock`, `.buildlignore`, `.buildl-version`; and
   `restore-keys: buildl-${{ matrix.os }}-${{ steps.pin.outputs.version }}-`. The action's own
   post step saves only on a miss and only when the job passed (spec §5, "Saving").
7. `buildl plugins fetch`.
8. `buildl build`, then `buildl test`; each must exit 0.
9. Second pass: `buildl build`, whose last line must be exactly `4 built, 2 cached`; then
   `buildl test`, whose last line must be exactly `tests: 2 passed (2 cached)`. buildl writes
   these summaries to stderr (`crates/buildl/src/cli/run.rs:427,468,515`, `TerminalReporter` on
   `stderr`), so each step captures both streams (`2>&1`), prints the whole capture, and fails,
   naming the expected line, when the capture's last line differs. The shell runs with
   `pipefail` so a buildl failure is not hidden by a pipe.
10. `buildl gc`.

No `continue-on-error`, no other cache action on this job, no change to any other job.

**Proof:** no local test: the job is proven only by running on GitHub (Task 2). Before the commit,
the file is valid YAML: `ruby -ryaml -e 'YAML.load_file(".github/workflows/ci.yml")'` exits 0.

**Done when:** the YAML check passes, and `cargo make dod` and
`cargo deny check -D unused-wrapper` are green. Commit
`ci(repo): build buildl with the pinned buildl release`.

### Task 2 — Prove the job on GitHub

**Change:** The branch is pushed and its pull request opened (outward-facing: run when the author
says so). The job runs on the pull request.

**Proof:**
- First run, both `ubuntu-latest` and `macos-latest`: the job is green; the cache step reports a
  miss (or a restore-key fallback) and its post step saves a cache.
- A re-run of the same commit (`gh run rerun <run id> --job <job id>`, per OS): the cache step
  reports a hit on the exact key; the first `buildl build` ends `4 built, 2 cached`; the post step
  logs "Cache hit occurred on the primary key" and saves nothing.

**If a run fails (D11):** stop at this task; record the failing step and its log in
`## Deviations`, and take it to the author. No `continue-on-error`, no dropping `ubuntu-latest`,
no change to buildl in this chain.

**Done when:** both checks hold on both operating systems. No commit.

---

## Risks

- Linux is unproven (spec D6): buildl's tests have never run in buildl's sandbox there. The first
  `ubuntu-latest` run is the proof; D11 decides what happens if it fails.
- Runner time, memory and disk for the cold run are not measured (spec §5, "Timeout"); the 60
  minutes and the runner's disk are the limits, and hitting either is handled as D11.
- Task 2 needs the author's word before the push.

## Proof

- Task 1's YAML check and Task 2's runs.
- `cargo make dod` and `cargo deny check -D unused-wrapper` green.

## Review findings

- 🔵 `ci.yml`: the comment said the installer "does not refuse a bad pin"; it refuses a malformed
  or unreleased version, and only an empty pin or `latest` installs the newest. Fixed: the comment
  is reworded; `ruby -ryaml -e 'YAML.load_file(".github/workflows/ci.yml")'` → `yaml ok`; folded
  into `7c95637`.
- ❓ Task 2 (the GitHub run) has not run: it waits for the author's word to push.

## Deviations

- 2026-11-11: the job sets `defaults.run.shell: bash` (GitHub runs it as `bash -e -o pipefail`).
  The second-pass step keeps `set -o pipefail` and captures buildl's exit code
  (`out="$(…)" || rc=$?`), so a failing buildl still prints its output before the step fails.
- 2026-11-11: `cargo make dod` (rc 0, "Build Done in 59.91 seconds") and
  `cargo deny check -D unused-wrapper` (ok) ran once, covering this plan's and plan 03's commits;
  neither changes Rust code. Commit `7c95637`.
- 2026-11-11: Task 2 has not run: it pushes the branch and opens the pull request, which waits for
  the author's word. The plan stays `executing`.
