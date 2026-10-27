---
status: approved
created: 2026-10-26
built-on: a1f0493e70c7
---

# Spec: remote execution via REAPI

`build` and `test` gain a third place a target can come from. Today a target is served by the
local cache, fetched from the remote cache (milestone 6), or run on this machine. With remote
execution on, a target neither cache serves is sent to the REAPI server's Execution service, run
there, and its result is checked and placed exactly as a fetched result is. buildl stays a
client only: it speaks what the Remote Execution API v2 specifies and assumes nothing else about
the server. Every call to the server is retried on a transient failure. Every choice below was
made by the author in the design dialogue of 2026-10-26.

Sources cited:

| Short name | File |
|---|---|
| `REAPI` | `crates/buildl/proto/build/bazel/remote/execution/v2/remote_execution.proto` (vendored, remote-apis `c2c07e3`) |
| `ops.proto` | `crates/buildl/proto/google/longrunning/operations.proto` (vendored, googleapis `dff1cd0`) |
| `error_details.proto` | googleapis `dff1cd0`, `google/rpc/error_details.proto`, fetched to the scratchpad `ext/error_details.proto` |
| `nl-README` | NativeLink `main` `README.md`, fetched to the scratchpad `ext/nativelink-README.md` |
| `nl-config` | NativeLink `v1.4.0` `nativelink-config/examples/basic_cas.json5`, fetched to the scratchpad `ext/basic_cas.json5` |
| Bazel, Buck2, Pants | sources fetched to the scratchpad (`RemoteSpawnRunner.java`, `b2src/buck2/…`, `pants/…`) |

## 1. Where a target comes from

```text
 local cache ──hit──► cached                                      (unchanged)
      │miss
 remote cache: GetActionResult ──usable hit──► fetched             (milestone 6, unchanged)
      │miss
      ├─ remote execution off ──► run on this machine ──► built    (unchanged)
      └─ remote execution on  ──► Execute on the server ─┬─► remote   (exit 0, outputs checked)
                                                         ├─► fetched  (the server answered from its cache)
                                                         ├─► failed   (exit ≠ 0, or timed out)
                                                         └─► call failed → run on this machine
                                                             with a warning, or fail the build
                                                             under --remote-strict
```

- **The lookup stays first** (author's choice Q1 a). A miss in milestone 6's `GetActionResult`
  lookup (`crates/buildl-core/src/execute/perform.rs:464` `consult`) is what sends a target to
  `Execute`. All three clients studied do the same: Bazel looks up first and then executes
  (`RemoteSpawnRunner.java:224`, `:316`), Buck2 runs a separate `ActionCacheChecker`
  (`b2src/buck2/app/buck2_execute_impl/src/executors/action_cache.rs`), Pants a separate
  remote-cache runner (`pants/remote_cache.rs`).
- **`skip_cache_lookup` is false.** The server may answer from its own cache: "If false, the
  result may be served from the action cache" (`REAPI:1604`). That catches a result another
  machine stored between the lookup and the `Execute`. Such an answer carries
  `cached_result = true` (`REAPI:1668-1669`) and ends the target `fetched`.
- **Remote execution needs reading.** It happens only on a lookup miss, so a run with the remote
  turned off (`--no-remote`, or turned off by an earlier failed call) executes nothing remotely.
- `verify` never uses the remote. It calls `perform` with no remote
  (`crates/buildl-core/src/verify/assembly.rs:367-372`, `None::<&RemoteRun<'_, NoRemote>>`), and
  that does not change.

## 2. Turning it on

Author's choices: C1 c (per run and in the manifest), Q5 (names, plus a flag that turns off
execution only), C4 a (refused under `bare`).

| Where | Written | Meaning |
|---|---|---|
| `buildl.toml` | `[remote] execute = true` | the workspace's default; `false` when absent |
| environment | `BUILDL_REMOTE_EXECUTE=1` or `=0` | the machine's choice; anything else is a usage error (exit 2) naming the variable, as `BUILDL_REMOTE_UPLOAD` is (design §7) |
| flag | `--remote-execute` | on for this run |
| flag | `--no-remote-execute` | off for this run; the remote cache stays on |
| flag | `--no-remote` (milestone 6) | the whole remote off, execution included |

The flags go on `build`, `test` and `watch`, which takes `build`'s flags. Precedence, highest
first:

```text
 --no-remote > --no-remote-execute / --remote-execute > BUILDL_REMOTE_EXECUTE > [remote] execute
```

The variable is not read, and so cannot be an error, when a flag decides, as with the
milestone 6 variables. Usage errors (exit 2), all before the approver is asked:

- `--remote-execute` with `--no-remote` or with `--no-remote-execute`;
- `--remote-execute`, `--no-remote-execute` or `--remote-jobs` in a workspace with no `[remote]`
  table;
- `--remote-jobs` in a run where execution is off;
- execution on, by any of the three sources that turn it on (`[remote] execute`,
  `BUILDL_REMOTE_EXECUTE`, `--remote-execute`), under `strategy = "bare"`. An action at tier 1 can
  read files it never declared, and a worker receives only declared files, so the result could
  differ. This is the same rule and the same place as the upload refusal,
  `ensure_sandboxed_upload` (`crates/buildl-core/src/execute/assembly.rs:52`, error
  `UploadUnsandboxed` at `crates/buildl-core/src/error.rs:414`). A manifest that says
  `execute = true` under `bare` is refused only by `build`, `test` and `watch`, not when it is
  read, so `check`, `plan` and `doctor` still work.

`BUILDL_REMOTE_EXECUTE` in a workspace with no `[remote]` table is not read: a machine-wide
variable does not break a workspace that names no server, and it does not set off the `bare`
refusal there.

This amends design §4's sentence that whether a run uses the remote "is the machine's and the
run's to say, not the manifest's": `execute` is the one switch the manifest may hold. Reading,
uploading and strictness stay as milestone 6 left them.

## 3. New `[remote]` keys

Read as strictly as the existing keys (`crates/buildl/src/manifest/schema.rs:181`,
`deny_unknown_fields`). None is an action-key component.

| Key | Written as | When absent | Author's choice |
|---|---|---|---|
| `execute` | `true` or `false` | `false` | C1 c |
| `jobs` | a whole number from 1 to 1024 | `128` | C3 a, Q6. Pants' default is 128 (`pants/bootstrap_options.py:749`, `process_execution_remote_parallelism=128`), Buck2's 400 (`b2src/buck2/app/buck2_re_configuration/src/lib.rs:432`, `execution_concurrency_limit.unwrap_or(400)`) |
| `queue_timeout` | the grammar of `timeout` (design §4: a whole number followed by `ms`, `s`, `m` or `h`, never zero) | `"1h"` | Q2 b. Pants waits one hour plus the action's timeout (`pants/bootstrap_options.py:772`, `pants/remote.rs:938`) |
| `retries` | a whole number from 0 to 10; `0` turns retrying off | `5` | Q3 b |
| `properties` | a table of names to strings, e.g. `{ pool = "large" }` | none | C5 a, Q7 |

`--remote-jobs N` (N from 1 to 1024) overrides `jobs` for one run.

`properties`:
- Each entry becomes one `Platform.Property` of every `Action` buildl describes: the lookup, the
  upload and the execution all use one `Action`, so a result is found under the digest it was
  stored under. Properties are sorted by name, as the API requires ("the properties MUST be
  lexicographically sorted by name, and then by value", `REAPI:965-969`).
- The names buildl sends itself, `OSFamily` and `ISA` (`crates/buildl/src/remote/encode.rs:142-143`,
  values from `RemotePlatform::of`, `crates/buildl-core/src/types/remote_action.rs:44`), are refused as manifest errors (exit 5) naming the key, so a property
  cannot replace them silently. So is an empty name. Names are compared as written: the API
  says names are "typically case-sensitive" (`REAPI:953`).
- buildl passes them on as written and does not check them against the server's
  `supported_node_properties` (`REAPI:2584`). Buck2 doesn't either: its client reads only the
  compressors and the batch limit, under `// TODO use more of the capabilities of the remote
  build executor` (`b2src/buck2/remote_execution/oss/re_grpc/src/client.rs:372`).
- Adding or changing a property changes every `Action` digest, so results stored before the
  change are not found after it. That is the API's rule ("the platform is implicitly part of the
  action digest", `REAPI:953-956`), not buildl's.

## 4. One remote execution, call by call

```text
 worker thread (blocking)                     remote adapter (tokio, via block_on)
 ────────────────────────                     ─────────────────────────────────────
 consult: lookup miss
 RemoteCache::execute(action) ─────────────►  1. capabilities (once per build)
                                              2. FindMissingBlobs(inputs, Directory
                                                 messages, Command, Action) → upload missing
                                              3. Execute{action_digest, skip_cache_lookup=false}
                                                 read the Operation stream until done
                                                 stream ends early → WaitExecution(name)
                                                 NOT_FOUND → Execute again
                                                 FAILED_PRECONDITION/MISSING → re-upload, again
                                              4. Operation.response → ExecuteResponse
 ◄──────────────────────────── Executed::{Ran, Cached, TimedOut, InputChanged} or Err
 core: check the ActionResult as a fetch is checked, download, store
```

**Step 1, capabilities.** Milestone 6 already calls `GetCapabilities` once per build
(`crates/buildl/src/remote/capabilities.rs:66`) and reads SHA-256, action-cache updates and the
batch limit (`capabilities.rs:34-55`). With execution on, it also requires
`execution_capabilities.exec_enabled` (`REAPI:2577-2578`, "Whether remote execution is enabled
for the particular server/instance") and SHA-256 among `execution_capabilities.digest_functions`,
or in the legacy `digest_function` when that list is empty (`REAPI:2571-2575`, `:2586-2587`).
A server without them is a failed call to `Execute` (§6).

**Step 2, upload.** "The client must first upload all of the inputs, the Command to run, and the
Action into the ContentAddressableStorage. It then calls Execute with an action_digest referring
to them" (`REAPI:47-52`). The milestone 6 upload already does most of this: it hashes each input
as it reads it and sends what `FindMissingBlobs` reports missing (`crates/buildl/src/remote/reapi.rs:99-177`,
`Parcel`, `Inputs`). Execution reuses it, without the output and log blobs and without
`UpdateActionResult`. An input whose bytes no longer match its digest gives
`Executed::InputChanged(path)`: the target runs locally with milestone 6's notice for a changed
input, and the remote stays on.

**Step 3, `Execute` and the stream.** `Execute` "returns a stream of google.longrunning.Operation
messages describing the resulting execution, with eventual response ExecuteResponse"
(`REAPI:64-67`). `Operation` holds `name`, `metadata`, `done` and a result that is `error` or
`response` (`ops.proto:121-156`). The adapter reads messages until `done`.
- A stream that ends, or breaks, before `done` is resumed with `WaitExecution(name)`
  (`REAPI:122-136`). `NOT_FOUND` from `WaitExecution` means "the operation no longer exists …
  The client should call Execute to retry" (`REAPI:132-135`), and it does.
- `FAILED_PRECONDITION` with a `google.rpc.PreconditionFailure` detail whose violations are all
  of type `MISSING` (`REAPI:103-112`) re-uploads the named blobs and calls `Execute` again.
  Any other detail is a failed call. This holds whether the code comes back as the call's
  error or in `ExecuteResponse.status` (step 4). tonic gives the bytes of the encoded
  `google.rpc.Status` as `Status::details()` (`tonic-0.14.6/src/status.rs:524`); the
  `PreconditionFailure` is one of that status's `details` `Any` entries, picked by its
  `type_url`, as Pants does (`pants/remote.rs:660-668`). `PreconditionFailure` is in `google/rpc/error_details.proto`
  (`error_details.proto:210-221`, fields `type` and `subject`), which is not vendored today:
  `find crates/buildl/proto -name "*.proto"` lists `google/rpc/status.proto` and no
  `error_details.proto`. It is vendored from the same googleapis commit, `dff1cd0`, and the code is
  regenerated with `cargo make reapi-codegen`.
- The operation metadata (`ExecuteOperationMetadata.stage`, `REAPI:1733-1735`) is not shown in
  this slice.

**Step 4, the answer.** `ExecuteResponse` (`REAPI:1664-1695`):

| `ExecuteResponse` | Ends as |
|---|---|
| `status` OK, `cached_result` true, exit code 0 | `Executed::Cached(hit)`: checked as a fetch; the target ends `fetched` |
| `status` OK, `cached_result` false, exit code 0, every output as declared | `Executed::Ran(hit)`: checked as a fetch, outputs placed; the target ends `remote` |
| `status` OK, exit code 0, an output missing, a symbolic link, or a directory where a discovery target needs a file | the action ran and failed: the target ends `OutputMissing` or `OutputRefused`, the classes a local run gets for the same outputs (`crates/buildl-core/src/execute/perform.rs:589-598`, `collected`). Not a failed call, so the remote stays on |
| `status` OK, exit code not 0, `cached_result` either way | `Executed::Ran(hit)`: stdout and stderr downloaded, checked and stored; the target ends `ActionFailure { status }` as a local run with that exit would. Not cached (`REAPI:1677`) |
| `status` `DEADLINE_EXCEEDED` | `Executed::TimedOut { stdout, stderr }`: "if the operation times out during execution, the status will have a DEADLINE_EXCEEDED code" (`REAPI:1672-1673`). The target fails with the class a local timeout gets, and is not run locally: Bazel likewise falls back "`!RemoteRetrierUtils.causedByExecTimeout(cause)`" only (`RemoteSpawnRunner.java:594`) |
| `status` `UNAVAILABLE`, `RESOURCE_EXHAUSTED`, `ABORTED` or `CANCELLED` | retried as a call error with that code is (§5). "Errors that occurred while running the action will be reported in the status field of the ExecuteResponse" (`REAPI:81-84`), and of `UNAVAILABLE` "The client should retry" (`REAPI:93-95`). Pants reads this field with the rules it applies to call errors (`pants/remote.rs:647-687`) |
| `status` `FAILED_PRECONDITION` with `MISSING` violations only | re-upload and `Execute` again, as in step 3 |
| any other non-OK `status`, or a retried one after its last try | a failed call (§6) |

A blob of the answer that fails its length or SHA-256 check is a failed call, as in milestone 6:
the server, not the action, gave bad bytes. So is an `ActionResult` that names a path the
`Command` did not list in `output_paths`: the action could not have written it there, so the
server answered for something else.

`Action.timeout` already carries `[execution] timeout` (`crates/buildl-core/src/execute/remote_action.rs:87`),
so the server enforces the action's own limit.

**The client's wait limit** (Q2 b). The whole of step 3, from `Execute` to the last message,
is bounded by `queue_timeout + [execution] timeout`. Exceeding it is a failed call to `Execute`.
Milestone 6's `[remote] timeout` still bounds the connection, every unary call and every
ByteStream message (design §11.2). It does not bound the silence between `Execute` stream
messages, since a queued job may say nothing for longer.

**No upload afterwards.** A target the server ran is not uploaded with `UpdateActionResult`,
even under `--remote-upload`. A server that completes an action "MAY choose to cache the result
in the ActionCache unless `do_not_cache` is `true`. Clients SHOULD expect the server to do so"
(`REAPI:663-667`), and buildl never sets `do_not_cache`. A server that does not cache it leaves
other machines a miss: they execute the target again, and nothing breaks. A target that falls
back to a local run is uploaded as milestone 6 uploads any local run.

**A server may run an action more than once** ("The server MAY execute the action multiple
times, potentially in parallel", `REAPI:114-117`). Nothing in buildl depends on running exactly
once, since the result is checked by content.

**Stopping.** When a build stops on a failure, a target not yet sent is not sent. A job already
sent is waited for, so its result is still recorded. buildl does not call `CancelOperation`,
which servers "NEED NOT implement" (`REAPI:78-79`).

## 5. Retries

Author's choices: Q3 b, Q4 a (every call; the four codes the two checked clients share).

```text
 retries = 5 (default): the first call, then up to 5 retries
 call ─✗─► 100 ms ─► retry 1 ─✗─► 200 ms ─► retry 2 ─✗─► 400 ms ─► retry 3 ─✗─► 800 ms
      ─► retry 4 ─✗─► 1.6 s ─► retry 5 ─✗─► failed call
            (each delay doubles, at most 5 s, plus 0–50 ms at random)
 retries = 0: the first call only
```

- **Calls:** every call to the server: `GetCapabilities`, `GetActionResult`, `FindMissingBlobs`,
  `BatchReadBlobs`, `BatchUpdateBlobs`, `UpdateActionResult`, each ByteStream read or write as a
  whole, `Execute` and `WaitExecution`. No retry code exists today: a search for `retry` and
  `backoff` in hand-written source finds nothing, while the same search finds
  `find_missing_blobs` at `crates/buildl/src/remote/client.rs:270` (control). One wrapper in the
  adapter's client does it, so every call gets the same rule.
- **Codes retried:** `UNAVAILABLE`, `RESOURCE_EXHAUSTED`, `ABORTED`, `CANCELLED`. Buck2 retries
  exactly these (`b2src/buck2/remote_execution/oss/re_grpc/src/client.rs:608-611`), and Pants these
  plus `INTERNAL` and `UNKNOWN` (`pants/retry.rs`). A connection that cannot be made or a call that
  passes `[remote] timeout` counts as `UNAVAILABLE`. The same codes in `ExecuteResponse.status`
  are retried the same way (§4 step 4). No other code is retried.
- **Count:** `retries` is the number of retries after the first call, so `5` means at most six
  calls and `0` means one. A missing-blob re-upload (§4 step 3) and a `WaitExecution` resume each
  use one retry. A stream that delivers messages and then ends or breaks before `done` resets the
  count, so a long job on a server that closes its streams from time to time is not given up while
  it makes progress; a stream that reaches `done` with a status that is retried does not, since
  the server sends stage messages before every answer and a server that answers each try that way
  would otherwise be tried forever; Pants counts "the number of retries allowed since the last successful
  connection" the same way (`pants/remote.rs:802-804`).
- **Delay:** 100 ms doubled after each retry, at most 5 s, plus 0 to 50 ms at random: Buck2's
  delays (`re_grpc/src/client.rs:671-679`: `INITIAL_DELAY` 100 ms, `MAX_DELAY` 5 s,
  `random_range(0..50)` ms). Buck2's count differs: `MAX_ATTEMPTS: u32 = 5` in
  `for attempt in 1..=MAX_ATTEMPTS` (`:670-675`) is five calls in all, where buildl's default is
  six.
- **`RetryInfo` is not read.** The API says clients "SHOULD respect" a `RetryInfo` detail
  (`REAPI:40-43`); buildl uses its own delays (§12). The random part comes from the standard library's
  `RandomState`, not a new crate: `grep '^name = "rand"' Cargo.lock` finds nothing today. The wait
  is a `tokio::time` sleep inside the adapter's runtime, so no wall clock reaches `buildl-core`
  (architecture §5).
- After the last try the call has failed, and milestone 6's handling applies (§6).

## 6. Failure handling

Author's choice: C2 c, the milestone 6 behaviour.

| What happens | Lenient (default) | `--remote-strict` / `BUILDL_REMOTE_STRICT=1` |
|---|---|---|
| a call fails after its retries, the server cannot execute (§4 step 1), or the wait limit passes | one warning naming the call; the remote is turned off for the rest of the command; this target and every later one run locally | the build fails, exit 3 |
| the action ran on the server and exited non-zero, or timed out | the target fails, as a local run would | same |
| a result's blob fails its length or SHA-256 check | a failed call, as in milestone 6 | the build fails, exit 3 |
| an input changed after its digest was taken | that target runs locally with a notice; the remote stays on | same |

The milestone 6 mechanism is reused: `failed_call` / `failed_with` and the build's
`RemoteSwitch` (`crates/buildl-core/src/execute/perform.rs:498-526`). A new `RemoteCall::Execute`
names the call.

## 7. How many run at once

Author's choice: C3 a, Q6.

```text
 dispatcher: up to max(workers, jobs) tasks with a worker thread at once
 ├─ remote jobs:  at most `jobs` at once           (a gate in buildl-core around `execute`)
 └─ local runs:   at most `workers` at once        (a gate in buildl-core around `run`)
```

The `ThreadPool` hands a task to a thread only when one is free, up to its limit
(`crates/buildl/src/dispatch/thread_pool.rs:63-70`, `serve`). Today that limit is `[execution] workers`
(`crates/buildl/src/local_ports.rs:417`, `ThreadPool::new(self.workspace.execution.workers)`). With
execution on, the limit becomes the larger of `workers` and `jobs`, so up to `jobs` threads each
block on one remote job through `block_on`. Several threads blocking on one runtime and channel
is the milestone 6 arrangement, measured then with eight threads (architecture §3.8).

`buildl-core` gains a counting gate (a `Mutex` and `Condvar`, no I/O), used twice. One admits
`workers` local runs at once and is passed before `run` (`perform.rs:563`), so a build that falls
back to local runs never runs more local actions together than it does today. The other admits
`jobs` remote jobs at once and is passed before `execute`, so `jobs` bounds the server's share
even when it is below `workers`. The lookup and the fetch of milestone 6 pass neither gate, as
today. The number passed to the core is `Workers` resolved for
the machine, as `ThreadPool::new` resolves it today (`thread_pool.rs:32-35`).

The `architecture.md` §6 record on the async runtime, which says the network fan-out of remote
execution would reopen the question, is amended: the fan-out is served by more threads blocking
on the one runtime, and nothing else becomes async.

## 8. The core and the port

```text
 buildl-core                                       buildl: remote/
 ┌───────────────────────────────────┐             ┌──────────────────────────────────┐
 │ perform: consult miss             │  execute    │ Reapi::execute                   │
 │   → execute (when on) or run      │ ──────────► │   capabilities, upload, Execute, │
 │ executed: check like fetch,       │ ◄────────── │   stream, WaitExecution, retries │
 │   or a failure, or a fallback     │  Executed   │ test_server: Execution service   │
 │ gate around local run             │             └──────────────────────────────────┘
 └───────────────────────────────────┘
```

- **Port.** `RemoteCache` (`crates/buildl-core/src/ports/remote_cache.rs:23`) gains one method,
  `execute(&self, action: &RemoteAction) -> Result<Executed>`, with
  `Executed::{Ran(RemoteHit), Cached(RemoteHit), TimedOut { stdout, stderr }, InputChanged(RootPath)}`.
  It stays one port because it is one server, one channel and one capabilities answer. `NoRemote`
  (`crates/buildl-core/src/ports/no_remote.rs`) and `Remote::Off` are never asked to execute,
  since the policy is off under them, and answer with an adapter error if they are.
- **Description.** `RemoteAction` (`crates/buildl-core/src/types/remote_action.rs:71`) gains the
  workspace's extra properties beside `platform`. The lookup, the upload and the execution share
  it (§3).
- **Policy.** `RemotePolicy` gains whether to execute and the number of jobs. `ensure_sandboxed_upload`
  becomes the check for both upload and execution under `bare`, with an error that names which.
- **Outcome.** `Ended` (`perform.rs:164`) gains `Executed { outputs, printed }`, which the calling
  thread places and records exactly as `Fetched` (`perform.rs:183`). `OutcomeClass`
  (`crates/buildl-core/src/types/outcome.rs:66`, `#[non_exhaustive]`) gains `Remote`, which counts
  as success wherever `Built` and `Fetched` do (`outcome.rs:112`). A remote job is reported as
  started when it is sent, and what it printed is replayed when it ends, as for a fetched target.
- **Checks.** `Executed::Ran` and `Executed::Cached` go through milestone 6's `usable` and `fetch`
  (`crates/buildl-core/src/execute/fetch.rs:177`). For `Ran`, a non-zero exit is not unusable: the
  core downloads and checks stdout and stderr and ends the task as `exited` (`perform.rs:582`) ends
  a local run with that status.
- **Settings.** `RemoteSettings` (`crates/buildl-core/src/types/remote_settings.rs:411`) gains
  `execute`, `jobs`, `queue_timeout`, `retries` and `properties`, each checked once where it is
  read (`crates/buildl/src/manifest/interpret.rs:334`).

## 9. What a user sees

- A target the server ran is shown with its own word, `remote`, beside `built` and `fetched`.
  Its block is marked `(remote)`, and the summary counts it after `fetched`
  (`1 built, 3 cached, 2 fetched, 4 remote`). A test run counts a remote test as passed and shows
  its share (`tests: 6 passed (1 cached, 2 fetched, 3 remote)`).
- The log event's class is `remote`, with the key and the span from the `Execute` to the stored
  result. `buildl log` shows what it printed, as for any run.
- `--stream` shows nothing live for a remote job; what it printed is shown when it ends.
- `doctor` prints, without connecting, `execute` as `buildl.toml` holds it and as this machine's
  `BUILDL_REMOTE_EXECUTE` sets it, with a note that the flags change it for one run, as it prints
  `upload` and `strict` "as this machine's variables set them" (design §7). It also prints
  `jobs`, `queue_timeout`, `retries` and each property, beside milestone 6's remote lines
  (`crates/buildl-cli/src/doctor_text.rs:226`, `remote_lines`).

## 10. Testing

| Layer | What it proves |
|---|---|
| `buildl-core` unit tests over a fake `RemoteCache` | every row of §4 step 4 and §6; that a gate counts its places and frees one when its holder leaves, on one thread (`buildl-core` starts no thread, not even in a test: `crates/buildl-core/clippy.toml` bans `std::thread`, and `cargo make guard-core-purity` refuses any exception); that a remote result is placed and recorded as `remote`; that `bare` refuses execution from each of the three sources that turn it on |
| `buildl` tests running the core over the `ThreadPool` dispatcher and a fake `RemoteCache` that holds each `execute` until released | that the two gates admit `workers` local runs and `jobs` remote jobs at once, with `jobs` above and below `workers` (as `no_more_tasks_run_at_once_than_there_are_workers` proves the pool's limit, `crates/buildl/src/dispatch/thread_pool.rs`) |
| `buildl` adapter tests over the in-memory test server | upload then `Execute`; stream resumed with `WaitExecution`, and the count reset by a message; `NOT_FOUND` then `Execute` again; `FAILED_PRECONDITION`/`MISSING` then re-upload, as a call error and in `ExecuteResponse.status`; each retried code retried `retries` times, as a call error and in `ExecuteResponse.status`, and no other code retried; `retries = 0`; the delay schedule; `exec_enabled` false; `DEADLINE_EXCEEDED`; the wait limit |
| `buildl-cli` binary tests over the test server | the switches, their precedence and every usage error of §2; the output of §9; `doctor` |
| real-server tests, CI only | a workspace built with `--remote-execute` against NativeLink, every target `remote`; a second build in a fresh checkout, every target `fetched`, outputs byte-identical |

**The in-memory test server** (`crates/buildl/src/remote/test_server.rs`, behind the `test-server`
feature) serves Capabilities, ActionCache, ContentAddressableStorage and ByteStream today
(`test_server.rs:203-206`). It gains the Execution service: it writes the input root from its CAS
into a temporary directory, runs `Command.arguments` there with `Command.environment_variables`
and nothing else, reads `Command.output_paths`, stores what it finds and answers with an
`ActionResult`. Switches on the server make it break a stream after its first message, forget an
operation, report a blob missing once, refuse a call with a chosen code a chosen number of times,
answer `DEADLINE_EXCEEDED`, hold a job past a deadline, and answer `exec_enabled = false`. It needs
no network beyond loopback, so `cargo make dod` stays Docker-free.

**Real-server tests** (author's CI choice b). They read the server's address from
`BUILDL_TEST_REAPI_URL` and are skipped when it is unset, so `cargo make dod` and the macOS CI job
are unchanged. A new CI job on `ubuntu-latest` starts NativeLink and runs them:

```yaml
# .github/workflows/ci.yml, new job (sketch)
remote-execution:
  runs-on: ubuntu-latest
  steps:
    - docker run -d -p 50051:50051 -v $PWD/ci/nativelink.json5:/config
        ghcr.io/tracemachina/nativelink:v1.4.0 config
    - BUILDL_TEST_REAPI_URL=grpc://127.0.0.1:50051
        cargo test -p buildl-cli --test real_remote
```

The real-server tests live in one integration test file, `crates/buildl-cli/tests/real_remote.rs`,
which drives the `buildl` binary.

- The image name, tag and run form are NativeLink's own (`nl-README:96-105`). Its example config
  has a scheduler, a local worker and the Execution service on port 50051
  (`nl-config:40-66` schedulers, `:68-75` the local worker, `:136-171` the listener and the
  `execution` service).
- `ci/nativelink.json5` is that example with one edit: the worker's `OSFamily` value `""` becomes
  `"linux"`, the value buildl on `ubuntu-latest` sends (`remote_action.rs:44-55`). The example
  matches `OSFamily` by `priority` (`nl-config:59`); whether `priority` admits a worker that offers
  `""` was not checked, so the config states `linux` rather than rely on it. `ISA` needs no edit:
  the worker offers `x86-64`, matched `exact` (`nl-config:62`, `:124-128`), which is what buildl
  sends on that runner.
- NativeLink is licensed under the Functional Source License 1.1 with an Apache-2.0 future
  licence (`nl-README:171`). It is run by the CI job and is not shipped with or linked into buildl.

**Not verified, carried as risks for the plan:**
- The prebuilt image is "currently limited to `x86_64` systems" (`nl-README:87`). This machine is
  `arm64` (`uname -m`), with Docker Engine 20.10.24 `arm64` (`docker version`). Whether the image
  runs here under emulation was not tried. Even if it runs, its worker advertises `ISA` `x86-64` and
  a buildl on this machine sends `arm-a64`, so no job would match. The real-server tests are
  therefore expected to run in CI, not on this machine.
- NativeLink's local worker runs each action inside the NativeLink container, in
  `/tmp/nativelink/work` (`nl-config:68-78`). Which programs that image holds (a shell, `cp`) was
  not checked, so the real-server test workspace's commands may need to change to what the image
  has.
- How long the new CI job takes was not measured.
- That NativeLink `v1.4.0` accepts every call of §4 as described: only the CI run proves it.

## 11. Documents

| Document | Change |
|---|---|
| `docs/design.md` §4 | the five keys of §3; the amended sentence on the manifest's say (§2) |
| `docs/design.md` §7 | the switches of §2, their usage errors, the output of §9, `doctor` |
| `docs/design.md` §8.7 | the exit-code table's usage errors |
| `docs/design.md` §11.2 | remote execution built, as a client; the toolchain-container paragraph dropped |
| `docs/design.md` §13 item 8 | "Remote execution", without toolchain containers. Built |
| `docs/architecture.md` §3.4, §3.8, §6 | the gate, `execute`, retries, the amended async record |
| `docs/architecture-building-blocks.md` | the `RemoteCache` row of the port table |
| `crates/buildl/proto/README.md` | `error_details.proto` in the googleapis row |
| `README.md`, `CLAUDE.md` | milestone 8 built |

## 12. Non-goals

- Containers of any kind: local `strategy = "container"`, toolchain images, a container runtime
  (milestone 8 non-goal, amended 2026-10-26).
- Tier 3, `os-sandbox`, and `b.fetch` (design §9.6).
- A per-target switch to keep one target off remote execution.
- Running a target locally and remotely at once and taking the first result (Buck2's hybrid
  mode).
- Showing a remote job's stage (queued, executing) or streaming what it prints while it runs.
- `CancelOperation`, execution priorities, `ResultsCachePolicy`, `ExecutionPolicy`, inlined
  outputs.
- Reading a `RetryInfo` detail for the delay before a retry (`REAPI:40-43`).
- Checking `properties` against `supported_node_properties`.
- Anything milestone 6 excluded: the Bazel HTTP cache protocol, compressed blobs, client
  certificates, logins beyond a header token, evicting entries.
