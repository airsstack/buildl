---
status: approved
created: 2026-10-26
depends-on: [09]
built-on: c7841e3774e8
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
other, whose commands use only `sh` and `printf`/`cat`, sets `[remote] url` to
`BUILDL_TEST_REAPI_URL` and `retries = 5`, and runs the binary.

**Proof:** written first (they skip locally without the variable), then run in CI —
- `every_target_runs_on_the_server` — `build --remote-execute`: summary `2 remote`, `out/` holds the expected bytes
- `a_fresh_checkout_fetches_what_the_server_ran` — the same workspace copied to a new directory, `build` with no flags: `2 fetched`, byte-identical outputs
- `a_failing_command_fails_as_an_action_failure` — a third target that exits 1: the build fails with the action-failure exit, `buildl log` shows its stderr
- `tests_skip_without_the_variable` — with `BUILDL_TEST_REAPI_URL` unset every test returns early and passes

**Done when:** `cargo test -p buildl-cli --test real_remote` passes locally (skipping) and
`cargo make dod` is green. Commit `test(buildl-cli): real-server remote execution tests`.

### Task 2 — The CI job

**Files:** `.github/workflows/ci.yml`.

**Change:** a job `remote-execution` on `ubuntu-latest`: check out; install the stable toolchain
and cache as the `dod` job does (`:46-58`); start
`docker run -d -p 50051:50051 -v $PWD/ci/nativelink.json5:/config ghcr.io/tracemachina/nativelink:v1.4.0 config`
(NativeLink's own form, `nativelink-README.md:96-105`); wait until port 50051 accepts a
connection, failing the job after 60 s; run
`BUILDL_TEST_REAPI_URL=grpc://127.0.0.1:50051 cargo test -p buildl-cli --test real_remote`;
on failure print `docker logs` of the container.

**Proof:** the job passes on the branch's pull request; its log shows the three server tests
(`every_target_runs_on_the_server`, `a_fresh_checkout_fetches_what_the_server_ran`,
`a_failing_command_fails_as_an_action_failure`) run against the server, not skipped.

**Done when:** `gh pr checks` reports the `remote-execution` job passed on the pull request.
Commit `ci(repo): run remote execution against NativeLink`.

## Risks

- **Most risky step:** the image's contents. NativeLink's worker runs actions inside its own
  container (`basic_cas.json5:68-78`, `work_directory: /tmp/nativelink/work`), and whether that
  image holds `sh`, `printf` and `cat` was not checked (spec §10). `execute` probes it first with
  `docker run --entrypoint sh ghcr.io/tracemachina/nativelink:v1.4.0 -c 'command -v sh printf cat'`
  on Linux x86_64; if the image lacks them, the test commands change to what it has, or the
  config's worker gains an `entrypoint`/`additional_environment` that provides them.
- Whether `priority` matching admits `OSFamily` `""` was not checked; the config states `linux`
  instead (spec §10).
- The prebuilt image is `x86_64` only (`nativelink-README.md:87`); this job and the probe run on
  `ubuntu-latest`, not on an arm64 Mac.
- How long the job takes is not measured; the image download is its largest unknown.

## Proof

The `remote-execution` CI job green on the pull request, with every real-server test run.
