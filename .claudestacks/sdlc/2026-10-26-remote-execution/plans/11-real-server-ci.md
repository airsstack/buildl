---
status: executing
disproves: 24837f1a4158
created: 2026-10-26
depends-on: [09]
built-on: 8b1ed74430a0
---

# Real Server CI Implementation Plan

**Goal:** CI builds a workspace on a real NativeLink server with remote execution on every push.

**Architecture:** One integration test file, `crates/buildl-cli/tests/real_remote.rs`, drives the
`buildl` binary against the server named by `BUILDL_TEST_REAPI_URL` and skips every test when it
is unset, so `cargo make dod` and the macOS job never need Docker. A new CI job on
`ubuntu-latest` starts NativeLink `v1.4.0` from its published image with a config kept in the
repository, and runs that file. buildl links nothing of NativeLink (spec §10).

**Tech Stack:** GitHub Actions, Docker, NativeLink `ghcr.io/tracemachina/nativelink:v1.4.0`, Rust integration tests.

**Content authority:** spec §10 (real-server tests, the CI job, the config edit, the risks).

---

## Files that change

```
ci/nativelink.json5                     — [create] NativeLink v1.4.0 basic_cas.json5 with the worker's OSFamily set to "linux"
ci/nativelink.Dockerfile                — [create] the published image's static nativelink binary on debian:bookworm-slim, which has sh and coreutils (spec §10)
crates/buildl-cli/tests/real_remote.rs  — [create] the real-server tests; skipped without BUILDL_TEST_REAPI_URL
.github/workflows/ci.yml                — [modify] a fifth job, remote-execution, after reapi-codegen (:132-171)
```

## Order of work

### Task 1 — The config and the tests

**Files:** `ci/nativelink.json5`, `crates/buildl-cli/tests/real_remote.rs`.

**Change:** copy
`https://raw.githubusercontent.com/TraceMachina/nativelink/v1.4.0/nativelink-config/examples/basic_cas.json5`
(223 lines) to `ci/nativelink.json5`, changing only the worker's `OSFamily` value `""`
(`basic_cas.json5:109-113`) to `"linux"` (spec §10). Its scheduler, local worker and the
`execution` service on port 50051 stay as they are (`:40-66`, `:68-75`, `:136-171`). The test
file writes a small workspace into a temporary directory: two targets, one depending on the
other, whose commands use only `cp` (the Debian base of `ci/nativelink.Dockerfile` has it),
sets `[remote] url` to `BUILDL_TEST_REAPI_URL`, `retries = 5` and `queue_timeout = "2m"` (so a
worker that matches no job fails the test fast), and runs the binary with `--remote-strict`. Each
test writes its own `a.txt`, so its action digests are new to a server that keeps results across
tests.

**Proof:** written first (they skip locally without the variable), then run in CI —
- `every_target_runs_on_the_server` — `build --remote-execute`: summary `2 remote`, `out/` holds the expected bytes
- `a_fresh_checkout_fetches_what_the_server_ran` — the same workspace written to a new directory, `build --remote-strict` with no execute flag: `2 fetched`, byte-identical outputs
- `a_failing_command_fails_as_an_action_failure` — a third target that exits 1: the build fails with the action-failure exit, `buildl log` shows its stderr
- `tests_skip_without_the_variable` — with `BUILDL_TEST_REAPI_URL` unset every test returns early and passes

**Done when:** `cargo test -p buildl-cli --test real_remote` passes locally (skipping) and
`cargo make dod` is green. Commit `test(buildl-cli): real-server remote execution tests`.

### Task 2 — The CI job

**Files:** `.github/workflows/ci.yml`.

**Change:** a job `remote-execution` on `ubuntu-latest`: check out; install the stable toolchain
and cache as the `dod` job does (`:46-58`); start
`docker build -f ci/nativelink.Dockerfile -t buildl-ci-nativelink ci`, then
`docker run -d -p 50051:50051 -v $PWD/ci/nativelink.json5:/config buildl-ci-nativelink /config`
(NativeLink's run form, `nativelink-README.md:96-105`, on the derived image of spec §10); wait
until `docker logs` shows `Worker registered with scheduler` (a connect to the published port
proves nothing, since Docker's port proxy accepts it as soon as the container starts), failing the
job after 60 s; run
`BUILDL_TEST_REAPI_URL=grpc://127.0.0.1:50051 cargo test -p buildl-cli --test real_remote`;
on failure print `docker logs` of the container.

**Proof:** the job passes on the branch's pull request; its log shows the three server tests
(`every_target_runs_on_the_server`, `a_fresh_checkout_fetches_what_the_server_ran`,
`a_failing_command_fails_as_an_action_failure`) run against the server, not skipped.

**Done when:** `gh pr checks` reports the `remote-execution` job passed on the pull request.
Commit `ci(repo): run remote execution against NativeLink`.

## Risks

- **Most risky step (settled):** the image's contents. NativeLink's worker runs actions inside its
  own container (`basic_cas.json5:68-78`, `work_directory: /tmp/nativelink/work`). The probe ran:
  the published image has no `/bin/sh` and only the `nativelink` binaries, so
  `ci/nativelink.Dockerfile` puts its static binary on `debian:bookworm-slim` (spec §10).
- Whether `priority` matching admits `OSFamily` `""` was not checked; the config states `linux`
  instead (spec §10).
- The prebuilt image is `x86_64` only (`nativelink-README.md:87`); this job and the probe run on
  `ubuntu-latest`, not on an arm64 Mac.
- How long the job takes is not measured; the image download is its largest unknown.

## Proof

The `remote-execution` CI job green on the pull request, with every real-server test run.

## Review findings

One review over plans 11 and 12 (`21816dd..fa82cb2`); `cargo make dod` green. Plan 11's findings, closed in `4a5d18c`:
- ci — the wait step's connect to 127.0.0.1:50051 proved nothing, since Docker's port proxy accepts it as soon as the container starts — fixed: the step waits for NativeLink's `Worker registered with scheduler` log line, seen in a local run right after `Ready, listening on 0.0.0.0:50051`
- ci — `ran=$(grep -c …)` ended the step under `bash -e` before its message when nothing matched — fixed: `|| true`
- ci — both base images were pinned by tag only while the copy names a store hash — fixed: pinned by digest (`nativelink@sha256:56e4a01e…`, `debian:bookworm-slim@sha256:7ff6ba31…`); the pinned image built and passed the server tests locally
- test — with no `queue_timeout`, a worker that matches no job would wait the default hour — fixed: `queue_timeout = "2m"`
- plan — the plan still said `sh`/`printf`/`cat` and described the image probe as pending — fixed in the plan text

## Probe results

- the published image — `docker export` of `ghcr.io/tracemachina/nativelink:v1.4.0` lists 23 entries (the `nativelink`, `cas_speed_check` and `redis_store_tester` binaries) and `docker run --entrypoint /bin/sh` fails "stat /bin/sh: no such file or directory" — disproved the plan's hope that the image holds `sh`; spec §10 amended, `ci/nativelink.Dockerfile` added
- `ci/nativelink.json5` differs from upstream `basic_cas.json5` v1.4.0 (223 lines) in line 111 only: `""` → `"linux"`
- the derived image under `linux/amd64` emulation on this Mac, with a scratch config whose worker offers `macos`/`arm-a64`: `BUILDL_TEST_REAPI_URL=grpc://127.0.0.1:50051 cargo test -p buildl-cli --test real_remote -- --nocapture` printed three `real server:` lines and `4 passed`; by hand, a two-target build printed `2 remote`, a fresh copy `2 fetched` with identical bytes, and a failing `cp` showed GNU `cp`'s message through `buildl log`
- a bare program name — Rust resolves it through the server process's `PATH` after `env_clear` (plan 09's probe), so `cp` needs no absolute path on the worker

## Deviations

- `ci/nativelink.Dockerfile` (not in the plan's file list): the published image's static binary on `debian:bookworm-slim`
- the tests use `cp`, `--remote-strict` and a fresh `a.txt` per test; each prints `real server: <test> against <url>` when it runs, and the CI step requires three such lines
