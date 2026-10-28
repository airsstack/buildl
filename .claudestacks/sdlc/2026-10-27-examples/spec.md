---
status: approved
created: 2026-10-27
built-on: 733a0a0b0f26
---

# Spec: Examples for Go, Rust and Python

This chain gives buildl a top-level `examples/` directory of workspaces in Go, Rust and Python
that a new user can copy and run, and proves each one in CI. Writing them showed two gaps that
stop discovery targets working with real tools in any stack, so the chain also fixes both in
buildl: an opt-in `stdout = true` field that makes an action's standard output its output, and
`buildl.decode_all`, which reads a stream of JSON values.

Evidence in this spec comes from throwaway probes run on 2026-10-27 with the debug `buildl`
binary built from commit `0801141` (Go 1.22.5, cargo 1.99.0, Python 3.12.12, macOS arm64),
deleted afterwards; their output is quoted where it backs a claim.

## 1. Premises proven by probe

|Premise|Probe and result|
|---|---|
|Go builds and tests in the input sandbox once `HOME` is declared|`go build -o $out .` with `env = { "HOME" }`: built and ran (`hello from go`); without it: `GOCACHE is not defined and $HOME is not defined`; `go test ./...`: `tests: 1 passed`; `buildl verify`: `1 verified, 0 mismatched, 0 failed, 0 skipped, 1 not cached, 0 not sampled`|
|Rust builds through `cargo install --root $out`|`cargo install --path . --root $out --target-dir tt --no-track` with `env = { "HOME", "PATH" }`: `out/inst/bin/hello` ran, `verify` reported `1 verified, 0 mismatched, 0 failed, 0 skipped, 0 not cached, 0 not sampled`; without `PATH`: `linker cc not found`; `cargo test --target-dir tt`: passed|
|Rust's other two forms fail|`cargo build --target-dir $out` (a directory output): `output tdir cannot be kept: it is or holds an empty directory` — the settled rule that a directory output holds regular files only (`docs/architecture.md:673`). Design §9.3's own form, `cargo build --release --locked` with `outputs = { "target/release/hello" }` (`docs/design.md:583-594`): `declared output target/release/hello was not produced` — an output is placed under `out/`, while cargo writes under `target/`|
|Python needs nothing beyond the standard library|`python3 -m zipapp app -o $out` built a `.pyz` that ran; `python3 -m unittest discover -s tests` passed, then `tests: 1 passed (1 cached)`; `verify`: `1 verified, 0 mismatched, 0 failed, 0 skipped, 0 not cached, 0 not sampled`|
|A tool that writes its result to stdout produces no output today|`b.discover("pkgs", { run = { "go", "list", "-json", "./..." }, … })` — design §10's own example: `FAILED //:pkgs: action failure (declared output pkgs.json was not produced)`. The message is `crates/buildl/src/report/block.rs:161-163`, from `OutcomeClass::OutputMissing` at `crates/buildl-core/src/execute/perform.rs:337`|
|`sh -c` works around it but hides the program|`{ "sh", "-c", "go list -json ./... > $out" }` built; the wanted set then names `sh`, and the `go` it runs is not a want|
|`go list -json` writes a stream of JSON values, not one|`go list -json=ImportPath fmt strings` printed two objects back to back, no `[ ]`, no commas|
|airsl's JSON decoder reads one value only|`airsstack.json.decode('{"a":1}\n{"a":2}\n')` raised `lua error in <json>: trailing characters at line 2 column 1`. airsl 0.1.4 — the version `Cargo.lock` holds — installs exactly `encode`, `encode_pretty` and `decode` (`airsl/crates/airsl/src/modules/json.rs:8,67,74,82`)|
|`serde_json` 1.0.151 (locked) splits a stream at each value's byte span|`Deserializer::from_str(t).into_iter::<serde::de::IgnoredAny>()` with `byte_offset()`: table in §4; on real `go list -json fmt strings os` output, 3 values, byte-identical to Python's `json.JSONDecoder.raw_decode` spans (`identical: True`)|
|airsl's `decode` reads each split value|each of the 3 values decoded inside `buildl check`; ImportPaths `fmt,strings,os`|
|A state file with a key field this binary does not expect is refused|`cache.json` with an extra `"stdout"` field: `is not a file buildl wrote: … unknown field 'stdout'`; with `network` removed: `… missing field 'network'`; both say to remove `cache.json`, after which every target is built anew|
|`plan --json` reports a clean target as `"verdict": "Clean"`|after a build: `{'//:app.pyz': {'reason': None, 'unfinished': None, 'verdict': 'Clean'}}`; after editing a source, `plan` printed `run    //:app.pyz: input app/greet.py changed`|

Fetched artifacts (2026-10-27), quoted verbatim:

|Artifact|Lines|
|---|---|
|`https://raw.githubusercontent.com/actions/setup-go/v5/action.yml`|line 7-8: `go-version-file:` / `description: 'Path to the go.mod or go.work file.'`|
|`https://raw.githubusercontent.com/actions/setup-python/v5/action.yml`|line 6-7: `python-version:` / `description: "Version range or exact version of Python or PyPy to use, using SemVer's version range syntax. Reads from .python-version if unset."`|
|REAPI v2 `Command.output_paths`, as vendored (`crates/buildl/proto/build/bazel/remote/execution/v2/remote_execution.proto:845-853`)|`If the resulting path is a file, it will be returned in an [OutputFile]…` / `Other files or directories that may be created during command execution are discarded.`|
|REAPI v2 `ActionResult` stdout, same file `:1389-1400`|`The server SHOULD NOT inline stdout unless requested by the client …` / `Digest stdout_digest = 6;`. buildl requests no inlining and reads only `stdout_digest` (`crates/buildl/src/remote/encode.rs:370`; no `stdout_raw` or `inline_stdout` in `crates/buildl/src/remote/`)|

## 2. The examples

```text
examples/
 ├─ check.sh       the common check (§5), run per example
 ├─ go/            starter: binary + go test
 ├─ rust/          starter: cargo install --root $out → out/<t>/bin/<name>, cargo test
 ├─ python/        starter: zipapp .pyz + unittest, standard library only
 ├─ go-discovery/  go list -json (stdout = true) → buildl.decode_all → one test target per package
 ├─ polyglot/      Go + Rust + Python in one workspace
 │                   schema/ ──python generator──► generated code ──► Go service · Rust CLI · Python tool
 ├─ plugins/       a workspace-local plugin (path = "plugins/…") giving a helper the build file uses
 └─ remote/        the Python starter with a docker compose setup: NativeLink and a worker with python3
```

|Example|Shows|
|---|---|
|`go`, `rust`, `python`|a build and a test target; `plan` naming the changed input after an edit; a second `test` served from cache; `buildl log`|
|`go-discovery`|a discovery target over a tool that writes to stdout, read back on the second pass with `buildl.decode_all`|
|`polyglot`|cross-directory labels across three languages; early cutoff — editing a comment in the schema regenerates identical code, so the targets downstream are served from cache; `query 'rdeps(…)'`|
|`plugins`|`buildl.use`, the plugin's cap in `[plugins.<name>]`, and attribution in `plan` and `doctor`|
|`remote`|`remote` with `--remote-execute`, `fetched` in a fresh copy, then `--remote-upload` of a locally built change and `fetched` for it in another copy|

Every example is a workspace of its own: a `buildl.toml`, its build files, its sources, an
`assert.sh` holding its own checks (§5), and a `README.md` saying what it shows and how to run
it. It needs the `buildl` binary and its stack's toolchain, nothing else; `remote` needs Docker
and a Linux x86_64 machine as well.

|Choice|Reason|
|---|---|
|Rust builds with `cargo install --path . --root $out --no-track`|the only one of the three probed forms that works (§1); the example's `README.md` says why|
|Python tests use `unittest`|pytest would need a venv or `pip` in the sandbox and in CI|
|`polyglot`'s generator is a Python script|no `protoc` to install|
|`plugins` uses a `path` plugin|a `git` plugin needs a hosted repository and `plugins fetch`|
|`examples/remote/` holds its own Dockerfile and NativeLink config|the example must be copyable alone; `ci/` keeps the bare worker buildl's `real_remote` tests use|
|`remote` names Linux x86_64 only|NativeLink's image is x86_64 and its worker matches the client's OSFamily and ISA (`.github/workflows/ci.yml:178-182`); running it elsewhere was not probed, so nothing is claimed for it|

The `examples/` directory lies outside every crate, so the publish rules hold
(architecture-building-blocks §9): no fifth workspace member, no `[[example]]` in `buildl`, and
no test reads another package's directory.

## 3. `stdout = true`

An opt-in field of `buildl.target` and `buildl.discover`: the action's standard output becomes its
one output file.

```text
build.lua               Load / Resolve             Execute: adapter            Execute: buildl-core decides          Record
─────────               ──────────────             ────────────────            ────────────────────────────          ──────
stdout = true  ──►  Target.output_source  ──►  run, capture stdout   ──►  collect reports Found at the path   ──►  output = cas.put(stdout)
                    = OutputSource::Stdout     (as today)                  Nothing → the stdout bytes are the        the log row's stdout and
                    KeyComponents gains it     collect_in (as today)       output, a plain file                      the output are one blob
                    graph JSON gains it                                    anything else → action failure            (the cas is by content)
```

The adapter does nothing new: it runs and captures as today (`crates/buildl/src/exec/capture.rs:117,164`)
and `collect_in` reports what stands at each declared path (`crates/buildl/src/exec/run.rs:77-89`).
The decision is `buildl-core`'s, after `strategy.run` and `strategy.collect`
(`crates/buildl-core/src/execute/perform.rs:866-888`), as architecture §6 settles it
(`docs/architecture.md:674`: `collect` reports, `buildl-core` decides).

|Rule|Behaviour|
|---|---|
|where|`buildl.target` and `buildl.discover`. `buildl.test` refuses it as an unknown field: a test records its exit status, not an output|
|value|a boolean, read by the existing flag reader (`crates/buildl-lua/src/values.rs:80`); anything else is refused like a non-boolean `network`|
|outputs|exactly one: the target's name by default, `<name>.json` for a discovery target. A `stdout = true` target naming more than one output is a mistake of the build file (exit 5) naming the target|
|the command also writes the output path|`collect` reports something other than `Found::Nothing` there, and the action fails with a new `OutcomeClass::OutputWritten { name }` — the action's fault, exit 1 — rendered `action failure (output {name} is written from stdout, and the command wrote it too)`|
|non-zero exit, timeout|fails as today; nothing is stored|
|`buildl log`, `--stream`|unchanged: stdout is still logged and streamed|
|action key|a component: turning it on or off rebuilds the target, and `plan` names the change as `Difference::Stdout`|
|`bare`, `sandbox`, `verify`|the same rule under each: `bare` collects from the workspace root, `sandbox` and `verify` from the exec dir; `verify` compares the stdout output as any output|
|remote: `Command`|the output path stays in `output_paths` (`crates/buildl/src/remote/encode.rs:126-130`), so a server reports a file the command wrote there (proto `:845-853`)|
|remote: a hit or an execution|a result whose `output_files` holds the path fails as `OutputWritten`, as it would here. Otherwise the output is the blob `stdout_digest` names, downloaded and checked against its digest as a fetched output is. A result with no `stdout_digest` is an unusable result, handled as a failed lookup is (without `--remote-strict` the run goes on locally): buildl cannot tell empty stdout from stdout not sent|
|remote: upload|the `ActionResult` carries the `stdout_digest` and no output file (`encode.rs:265-294`)|
|wanted set, approver, attribution|unchanged: the program is the command's own, so `go list` wants `go`|

**Types.** `OutputSource { Command, Stdout }`, an enum rather than a `bool` (rust guideline
`strong-types.md`), travels the path `network`'s `NetworkAccess` takes: `primitives.rs:72`
(Lua) → `StagedTarget` (`build_file.rs:131`) → `Target` (`declaration.rs:75`) → graph `Node`
(`resolve/assembly.rs:168`, `target_graph.rs:27`, its JSON view `target_graph.rs:172-230`) →
`ReadyAction` (`execute/assembly.rs:297`) → `OwnComponents` (`plan/components.rs:96`,
`action_key.rs:80`) → `Difference` (`plan/difference.rs:67`, `types/plan.rs:30`).

**No backward compatibility.** buildl is unpublished. `output_source` is a required field of
`OwnComponents`, like `network`; every key digest changes once, and a `cache.json` written before
the change is refused by the strict reading `deny_unknown_fields` gives (`action_key.rs:78,101`;
§1), with its own instruction to remove the file. That strictness stays: a state file holding
what this binary does not know is not read in part.

## 4. `buildl.decode_all(text)`

A function of buildl's own `buildl` table. The `airsstack` root table and airsl itself are not
changed.

```text
build file or plugin:  buildl.decode_all(text)
        │   buildl-lua: primitives_table (module.rs:127) — the one table build files get and,
        │               through plugin/environment.rs:21,73, the table each plugin gets
        ▼
split text at each JSON value's byte span ── serde_json stream reader (§1)
        │
        ▼ each value's text
decode with the json table recorded in the registry before any build file ran
        │   CURATED_REGISTRY_KEY (curated.rs:26; Json recorded at engine.rs:29), so a build
        │   file cannot have replaced it
        ▼
{ value1, value2, … }  — the tables, numbers and null airsstack.json.decode gives
```

|Input (probed split)|Result|
|---|---|
|`{"a":1}\n{"a":2}\n`|`{ {a=1}, {a=2} }`|
|`{"a":1}{"a":2}`|two values|
|`[1,2]`|`{ {1,2} }` — always a list of values|
|`1 2 3`; `"x" null true`|three values each|
|`1{"a":2}`|two values, `1` and `{"a":2}`|
|`""`, whitespace only|`{}`|
|text with `é` and `😀`|intact|
|`{"a":1}\n{"a":`|a Lua error, from `EOF while parsing a value at line 2 column 5`|
|`{"a":1} xx {"a":2}`|a Lua error, from `expected value at line 1 column 9`|
|not a string|a Lua error naming the argument|

An error is catchable with `pcall`; uncaught, it fails the build file (exit 5), as a failed
`decode` does. The function is pure: `check`'s double evaluation is unaffected, it is no part of
any action key, and the declaration memory and instruction ceilings still bound it.

`buildl-lua` gains two direct dependencies from the workspace catalog, `serde_json` for the
stream reader and `serde` for `IgnoredAny`, so `crates/expected-edges.txt` gains `serde` and
`serde_json` under `# buildl-lua`.

## 5. Proof in CI

`examples/check.sh <example>`, also run by a `cargo make examples` task:

```text
1 buildl check                       exit 0
2 buildl build && buildl test         exit 0
3 buildl plan --json                  every target "verdict": "Clean"
4 buildl verify                       exit 0, "0 mismatched"
5 the example's assert.sh             its own checks, below; a non-zero exit fails the example
```

|Example|`assert.sh` checks|
|---|---|
|`go`|`out/hello` prints the expected line; after editing `main.go`, `plan` names that input as changed; `build`, then a second `test` reports `cached`; `buildl log` of the test target exits 0 and prints `ok`|
|`rust`|`out/<t>/bin/<name>` prints the expected line; the same edit, `plan`, `cached` and `log` checks (`test result: ok`)|
|`python`|`python3 out/<t>.pyz` prints the expected line; the same edit, `plan`, `cached` and `log` checks (`OK`)|
|`go-discovery`|after `build`, `graph --json` holds one test target per package `go list` found, and `test` passes for each|
|`polyglot`|each of the three programs prints its expected line; after editing a comment in the schema, `build` runs the generator and serves every target downstream from cache, and `plan --json` then reports every target `Clean`; `query 'rdeps(…)'` reaches targets of all three languages|
|`plugins`|`doctor` and `plan` print `declared by plugin <name>@<version> for <label>` (`crates/buildl-core/src/types/attribution.rs:30-35`)|
|`remote`|run by its own job, below|

|Job in `.github/workflows/ci.yml`|Runs on|Toolchains|Does|
|---|---|---|---|
|`examples`|`ubuntu-latest` and `macos-latest`, the `dod` matrix (`ci.yml:35-36`)|rustup as today; `actions/setup-go@v5` with `go-version-file`; `actions/setup-python@v5` with `python-version` (§1)|builds `buildl` once, then runs `check.sh` for every example but `remote`|
|`examples-remote`|`ubuntu-latest` only (§2)|rustup; `actions/setup-python@v5` with `python-version`, for the copies that build or plan locally; docker|starts `examples/remote`'s server with an empty cache, waits for `Worker registered with scheduler` (as `ci.yml:202-206`), then in order: copy A builds with `--remote-execute` and asserts `remote`; a fresh copy B builds and asserts `fetched` (the server keeps what it ran); copy C edits a source, builds with `--remote-upload` and asserts it ran locally; a fresh copy D with the same edit asserts `fetched`|

## 6. Tests

|Layer (architecture-building-blocks §8)|`stdout = true`|`buildl.decode_all`|
|---|---|---|
|`buildl-core` unit and flows — fake ports, no I/O, processes or threads (`crates/buildl-core/clippy.toml`)|`OutputSource` in the key; `Difference::Stdout`; a run's stdout recorded as the output and as the log's blob; `Found::Nothing` required at the path, anything else `OutputWritten`; a remote hit, an `Executed::Ran` and an `Executed::Cached` result each rebuilt from `stdout_digest` with the fake remote ports; a remote result holding the path fails as `OutputWritten`; one with no `stdout_digest` handled as a failed lookup; more than one output refused; the graph golden updated|—|
|`buildl-lua` — real airsl, fixtures|parsed on `target` and `discover`; refused on `test`; a non-boolean refused|the cases of §4 as expected values; caught by `pcall`; reachable from a plugin; values equal `airsstack.json.decode`'s|
|`buildl` adapters — a temp directory, system tools by argv|the REAPI `Command` keeps the path in `output_paths`; an upload's `ActionResult` carries `stdout_digest` and no output file; a `test-server` round trip of upload and fetch, and of an execution, for a stdout target|—|
|`buildl/tests/e2e`|a fixture whose discovery target prints a JSON stream, built twice, byte-identical, under `bare` and `sandbox`|the same fixture reads it with `decode_all`|
|`buildl-cli` binary|exit 5 for more than one output; exit 1 and the `OutputWritten` message when the command writes the path too; `buildl log` still shows stdout|—|
|examples, in CI (§5)|`go-discovery` end to end on Linux and macOS|the same|

Every change passes `cargo make dod` and `cargo deny check`.

## 7. Documentation in this chain

|Where|Change|
|---|---|
|design §5|the `stdout` field in `b.target`'s closed table; a `buildl.decode_all` bullet|
|design §8.3, §12.4|the new key component|
|design §8.7|the output-capture text and the exit-code table (`docs/design.md:501-546`): the new exit-1 cause (`OutputWritten`) and exit-5 cause (more than one output)|
|design §9.3|the Rust example: the proven `cargo install --root $out` form in place of the failing `outputs = { "target/release/mytool" }`|
|design §10|the `go list` example made true: `stdout = true` and `buildl.decode_all`|
|design §11.2, architecture §3.8|how a stdout output maps to and from REAPI|
|architecture §2 and its `KeyComponents` list (`docs/architecture.md:176`)|`OutputSource` on `Target`, `Node`, `ReadyAction` and `OwnComponents`|

The book chain (`2026-10-27-book`) later carries these into the book.

## 8. Risks for the plan

|Risk|Handling|
|---|---|
|a tool behaving differently in the sandbox than in the probes|each example's first task runs its commands through `buildl` before the example is written around them|
|NativeLink's worker needing more than `python3` to run the zipapp and unittest actions|`examples-remote`'s first task builds the image and runs the actions before the job is written around it|
|NativeLink not keeping what it executed, so copy B misses|the same first task runs copies A and B by hand; if B misses, the plan amends §5's sequence on the evidence|

## 9. Non-goals

- Stacks other than Go, Rust and Python.
- Fine-grained per-file C/C++ compilation (design §13).
- Any change to airsl or to the `airsstack` root table.
- Backward compatibility of state files written before this chain.
- `stdout` on `buildl.test`.
- The `remote` example on anything but Linux x86_64.
- The book's chapters about the examples, the README rewrite and the publish — the chains
  `2026-10-27-book`, `2026-10-27-readmes` and `2026-10-27-first-publish`.
