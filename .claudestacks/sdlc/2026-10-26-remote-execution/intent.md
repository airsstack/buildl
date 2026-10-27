---
status: done
created: 2026-10-26
milestone: buildl/08
---

# Intent: remote execution via REAPI

## Problem

A repository that has outgrown one machine still builds on one machine.

- **The remote cache only helps with what someone already built.** Since milestone 6 a
  target the local cache cannot serve is looked up on a REAPI server and fetched. A miss —
  a new change, a fresh branch, a clean CI run — still runs every action on the local
  machine, with at most `[execution] workers` at once. For a large repository that is the
  slow path, and it is the common one for CI.
- **The server could run the work, and nothing asks it to.** REAPI servers that execute
  (Buildbarn, Buildfarm, BuildBuddy, NativeLink, design §11.2) run actions on their own
  workers through the Execution service of the Remote Execution API. buildl speaks the
  cache half of that API only; the Execution client bindings are generated and unused.
- **A failed call is final.** The milestone 6 client makes each call once. A server that
  is briefly unavailable fails the lookup or the upload outright, where Bazel, Buck2 and
  Pants each retry a few times with a growing delay.

## Affected systems

- `buildl-core`: the step a worker takes on a remote miss, and the scheduling of how many
  actions run at once.
- `buildl`: the `remote` module (the Execution service beside the cache it already
  speaks, and retries for its calls), the manifest's `[remote]` table, and the in-memory
  test server.
- `buildl-cli`: the switches that turn remote execution on, and `doctor`'s output.
- `docs/design.md` §4, §7, §11.2, §13; `docs/architecture.md` §3.8, §6;
  `docs/architecture-building-blocks.md`; `README.md`; `CLAUDE.md`.

## Desired outcome

- `build` and `test` can send a target the local and remote caches cannot serve to the
  configured server, which runs it; its result is checked and stored exactly as a fetched
  result is (design §11.2), and other machines then fetch it through the milestone 6
  path.
- Many more actions run at once remotely than the local machine has cores.
- Calls to the server are retried on a transient failure before they count as failed.

## Constraints

- **A client only.** buildl builds no server, worker or scheduler, and assumes nothing
  about how a server runs a job beyond what the Remote Execution API specifies.
- **Turned on per run and in the manifest.** A flag and an environment variable turn
  remote execution on for one run, and `buildl.toml` can turn it on for the workspace.
  With neither, actions run locally as today.
- **A failure warns by default.** A server that cannot be reached or a call that fails
  after its retries leaves the action to run locally with a warning; under
  `--remote-strict` (or `BUILDL_REMOTE_STRICT`) it fails the run, as in milestone 6. An
  action that ran on the server and failed is a real failure, not a call failure.
- **Its own limit on actions in flight.** A `[remote]` setting bounds how many actions
  run remotely at once, apart from `[execution] workers`.
- **Refused under `bare`.** Asking for remote execution under `strategy = "bare"` is a
  usage error (exit code 2), as asking for an upload is.
- **Extra platform properties.** The workspace can add platform properties to the
  actions it sends; buildl passes them on as written.
- The milestone 6 rules hold unchanged: the server is trusted for bytes and nothing
  else, every blob is checked, cas first and the cache row after, and failures are cached
  nowhere.
- Settled decisions stand (`architecture.md` §6): plain threads with the tokio runtime
  confined to the remote adapter, reached through `block_on`; the record on network
  fan-out is amended, not reopened as an async rewrite.
- Tests run without a network or a server in `cargo make dod`.

## Non-goals

- Containers of any kind: local `strategy = "container"`, toolchain images, a container
  runtime.
- Tier 3, `os-sandbox`, and `b.fetch` (design §9.6).
- A per-target switch to keep one target off remote execution.
- Running an action locally and remotely at once and taking the first result.
- Anything milestone 6 excluded: the Bazel HTTP cache protocol, compressed blobs, client
  certificates, logins beyond a header token, evicting entries.
