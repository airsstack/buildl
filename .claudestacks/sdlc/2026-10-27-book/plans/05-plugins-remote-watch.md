---
status: done
created: 2026-10-28
depends-on: [01]
built-on: 136d5b098d7e
---

# Plugins, Remote and Watch Chapters Implementation Plan

**Goal:** Chapters 8–10 of the book teach plugins, the remote cache and remote execution, and
watch mode.

**Architecture:** This plan fills stubs that plan 01 created and touches no other file. It follows
the writing rules every chapter plan shares:
- **Voice.** Plain words, in the voice of airsl's book (`/Users/hiraq/Projects/airsstack/airsl/book/src/ch08-01-writing-an-extension.md` is the model for plugins).
- **Real output only.** Every shell transcript comes from the debug binary built at the branch head (`cargo build -p buildl-cli`), run in a scratch copy of a workspace. Nothing is typed from memory.
- **The binary decides.** Where `docs/` disagrees with the binary, the book says what the binary does.
- **Diagrams.** This plan owns the `docs/` Mermaid diagrams listed under its `## Proof`; each is carried into its chapter as a ` ```mermaid ` block, redrawn where the binary has moved on.
- **A command that disagrees with its `--help`** is raised to the user, not written around.
- **Links.** Links between chapters are relative `.md` links, and no `§` sign appears in the book.

**Tech Stack:** Markdown, mdBook 0.4.52, the `buildl` debug binary, Go 1.22, Python 3, Docker (for `examples/remote`'s cache half).

---

## Files that change

```
book/src/ch08-00-plugins.md               — [modify] chapter opener
book/src/ch08-01-writing-a-plugin.md      — [modify] extension.toml, buildl.use, layers, stack plugins, api versioning
book/src/ch08-02-plugins-from-git.md      — [modify] [plugins.<name>], plugins fetch, buildl.lock
book/src/ch08-03-how-buildl-decides.md    — [modify] manifest, cap, ceiling; plan/doctor/graph naming the plugin
book/src/ch09-00-remote.md                — [modify] [remote], fetched, upload, strict, execute, jobs, retries
book/src/ch10-00-watch.md                 — [modify] buildl watch
```

## Order of work

### Task 1 — Writing a plugin

**Files:** Modify `book/src/ch08-00-plugins.md`, `book/src/ch08-01-writing-a-plugin.md`.

**Change:**
- The opener lists the chapter's sections.
- Writing a Plugin walks through `examples/plugins` (`plugins/gobin/extension.toml`, `main.lua`, `buildl.toml`'s `[plugins.gobin]`, `build.lua`'s `buildl.use`). It covers:
  - the three layers of engineer-owned logic;
  - what a stack plugin encodes;
  - plugin API versioning: within `api = 1` the `buildl` table only gains names, and buildl has no second version number (`docs/design.md:1040-1044`).

Sources: `docs/design.md:663-845` (§10, §10.1, §10.3).

**Proof:**
- Transcripts are captured in a scratch copy of `examples/plugins`: `build`, then both programs run.
- `mdbook build book` exits 0.

**Done when:** both hold. Commit `docs(repo): book chapter 8, writing a plugin`.

### Task 2 — Plugins from git, and how buildl decides

**Files:** Modify `book/src/ch08-02-plugins-from-git.md`, `book/src/ch08-03-how-buildl-decides.md`.

**Change:**
- **Plugins from Git:**
  - a `[plugins.<name>]` entry naming a git source;
  - `buildl plugins fetch` (read its flags from `buildl plugins fetch --help`);
  - what `buildl.lock` records;
  - that the fetched plugin is kept in the store.
- **How buildl Decides:**
  - what a plugin declares is held to its manifest, then to the workspace's cap for it, then to the ceiling;
  - `plan`, `doctor`, the approver and `graph` name the plugin.
  - Include the two narrowing transcripts: `[plugins.gobin] env.read = []` and `proc.run = []` in the manifest, each quoted exactly as printed with its exit code. The message is built at `crates/buildl-core/src/types/plugin.rs:327` (`"{wanted} is outside plugin {} ({list} allows: {})"`), prefixed by the build file and label (`crates/buildl-core/src/error.rs:1748-1759`), so the capture carries an `(… allows: …)` suffix.

Sources: `docs/design.md` §10 (`:663-758`) and `docs/architecture.md:399-413` (§3.7).

**Proof:**
- A fetch from git is captured against a local bare git repository made in the scratch directory, so no network is needed. Its `buildl.lock` is shown.
- Both narrowing transcripts are captured.
- `mdbook build book` exits 0.

**Done when:** all hold. Commit `docs(repo): book chapter 8, plugins from git and how buildl decides`.

### Task 3 — Remote cache and execution

**Files:** Modify `book/src/ch09-00-remote.md`.

**Change:**
- `[remote]` with `url`, and what `build` and `test` do with it: look up, check, store, and report `fetched`.
- `--remote-upload` / `BUILDL_REMOTE_UPLOAD`; under `strategy = "bare"`, upload and execute are refused with exit 2.
- `--remote-strict` / `BUILDL_REMOTE_STRICT`.
- `--no-remote`.
- `execute = true` / `--remote-execute` / `BUILDL_REMOTE_EXECUTE`, reported as `remote`, or as `fetched` when the server answers from its cache.
- `--no-remote-execute`.
- `--remote-jobs` / `[remote] jobs`.
- `[remote] retries` and the retried statuses.
- `examples/remote` and its "Where it runs" table, linked rather than copied.

Sources: `docs/design.md:855-935` (§11.2) and `docs/architecture.md:414-513` (§3.8).

**Proof:**
- Transcripts captured with no server running:
  - the default warning that the lookup failed and the build goes on;
  - `--remote-strict` failing, with the exit code it ends with quoted from the capture and matched to `crates/buildl-cli/src/exit.rs` (`Exit::of_error`);
  - `--no-remote`.
- The cache half captured against `examples/remote`'s `docker compose up`: copy 1 `build --remote-strict --remote-upload`, then copy 2 `build --remote-strict` gives `1 fetched`.
- Execution output is taken from the passing CI job `examples-remote` (run 40633614431, job "Remote example on NativeLink", as `.claudestacks/sdlc/2026-10-27-examples/plans/07-remote.md:141` records it) and cited as such. The CI log never prints buildl's own `1 remote` line: `examples/remote/assert.sh` checks it and prints its own lines, so the chapter quotes those and says what the script checked. On macOS, execution queues (examples/remote/README.md).
- `mdbook build book` exits 0.

**Done when:** all hold. Commit `docs(repo): book chapter 9, remote cache and execution`.

### Task 4 — Watch mode

**Files:** Modify `book/src/ch10-00-watch.md`.

**Change:**
- `buildl watch [target]...` takes `build`'s flags.
- It builds, then builds again when a file the build reads changes or the build files declare a different graph.
- It ignores `out/`, the state directory and `.git`.
- It ends on Ctrl-C, on a usage error (exit 2), or on a file-watcher failure (exit 3).

Sources: `docs/design.md:478-502` (§8.6) and `docs/architecture.md:514-556` (§3.9).

**Proof:**
- A transcript of one watch session, captured by running `buildl watch` in the background in a scratch workspace, editing an input, then stopping it.
- `mdbook build book` exits 0.

**Done when:** both hold. Commit `docs(repo): book chapter 10, watch mode`.

## Risks

- Remote execution cannot be captured on the development Mac (examples/remote/README.md, "Where it runs"). Task 3 cites the CI job's output instead, and says where it came from.
- A plugin fetched from GitHub would depend on the network and on a repository that may not exist. Task 2 uses a local bare repository; a `file://` URL is the same git source path.
- `watch` output is timing-dependent. Capture one cycle and say that timings vary.

## Proof

- `cargo make book-build` exits 0.
- The 7 `docs/` Mermaid diagrams this plan owns each appear as a ` ```mermaid ` block in the chapter named, and each such page renders `<pre class="mermaid">` in `book/book/`; open each page once in a browser (`cargo make book`) and confirm the diagram draws, not an error box: `docs/design.md:685` → `ch08-01`; `docs/design.md:861`, `:899`, `docs/architecture.md:418` and `:481` → `ch09`; `docs/design.md:480` and `docs/architecture.md:518` → `ch10`.
- The spec §3 rows this plan lands are listed below. Mark each one in the execution record.

  | Document | Rows |
  |---|---|
  | design | §8.6 (ch10); §10 and §10.1 (ch08-01, ch08-03); §10.3 (ch08-01); §11.2 (ch09); §14's plugin API rule (ch08-01) |
  | architecture | §3.7 (ch08-03); §3.8 (ch09); §3.9 (ch10) |

## Review findings

- risk — `ch10` "How it ends" lacked the exit 5 a `buildl.toml` that cannot be read before the first
  build gives (reviewer captured `invalid array: expected ']'`, exit 5). Row added. Fixed.
- spec — Task 3's premise that the CI log shows `1 remote` was disproven; the plan's Proof is
  amended (see Deviations). Fixed.
- nit — `ch09:279` "plus 0–50 ms": the jitter is `draw % jitter_millis`, under 50 ms
  (`crates/buildl/src/remote/retry.rs:92`). Now "under 50 ms". Fixed.
- nit — `ch09:84,96` `os error 61` is the macOS number; one clause now says Linux prints 111.
  Fixed.
- nit — `ch10`'s watch-loop diagram drops `[14]` and the `§7` label without a record. Recorded
  under Deviations. Fixed.
- nit — `ch08-02`'s git URL `file:///tmp/buildl-book/p05/git/gobin.git` carries the capture
  folder's name. Declined: it is a neutral `/tmp` path with no user or machine name, and a
  re-capture would change every commit hash on the page for no reader gain.

## Probe results

- Plugins (`examples/plugins` copy): `2 built`, `hello from a plugin`, `bye from a plugin`; `plan`
  shows `go: declared by plugin gobin@0.1.0 for //:bye`; `api = 2` → exit 5
  `manifest declares api 2; this runtime supports api 1`.
- From git (local bare repo, tag `v0.1.0`): build before fetch → exit 5 `plugin gobin is not
  fetched`; `fetched gobin c6a408f…`, `buildl.lock` shown; second fetch `unchanged`; an edited
  stored file → exit 6 `was changed after it was fetched`; a fetch restores it.
- Narrowing: `[plugins.gobin] env.read = []` → exit 4 `environment variable HOME is outside plugin
  gobin@0.1.0 (env.read allows: nothing)`; manifest `proc.run = []` → exit 4 `program go is
  outside plugin gobin@0.1.0 (proc.run allows: nothing)`. Reviewer re-ran both: byte-identical.
- Remote, no server: warning `… (after 6 calls); the build goes on without it`, `1 built`;
  `--remote-strict` and `BUILDL_REMOTE_STRICT=1` → exit 3 (`Exit::of_error`, Adapter → 3);
  `--no-remote` → `1 built`; `--remote-upload`/`--remote-execute` under `bare`,
  `--remote-jobs 4` without execution, `BUILDL_REMOTE_UPLOAD=yes` → exit 2.
- Remote cache half (NativeLink in Docker on the Mac): copy 1 `build --remote-strict
  --remote-upload` → `1 built`; copy 2 `build --remote-strict` → `1 fetched`, tests
  `tests: 1 passed (1 fetched)`; both `out/app.pyz` identical. Execution cited from CI run
  40633614431, job "Remote example on NativeLink" (id 122635436196).
- Retries and defaults match `retry.rs:19,22,71-74` and `remote_settings.rs:37-60` (reviewer).
- Watch (`examples/python` copy): an input edit rebuilds; README and `out/` edits print nothing; a
  new `app/extra.py` rebuilds; a broken `build.lua` prints the error and keeps waiting; Ctrl-C →
  130. `watch --remote-jobs 0` and `watch --remote-upload` without `[remote]` → exit 2.
- Diagrams: ch08-01 ×1, ch09 ×4, ch10 ×2 render `<pre class="mermaid">`; opened in Chrome from a
  local build, each is an SVG with no error box.
- Spec §3 rows landed: design §8.6 → ch10; §10, §10.1 → ch08-01, ch08-03; §10.3 → ch08-01; §11.2
  → ch09; §14's plugin API rule → ch08-01; architecture §3.7 → ch08-03 (lock and store in
  ch08-02); §3.8 → ch09; §3.9 → ch10.

## Deviations

- 2026-10-28 — Docker ran on the development Mac, so the cache half was captured here rather than
  by the user.
- 2026-10-28 — the CI log never prints buildl's `1 remote` line; `examples/remote/assert.sh`
  checks it and prints its own lines. `ch09` quotes those lines with a table of what the script
  checks. Plan amended.
- 2026-10-28 — the `deny` approver's refusal does not name the plugin; only the interactive prompt
  does, and it needs a terminal, so it is not captured. `ch08-03` says so and links `ch04-03`.
- 2026-10-28 — on macOS one save sometimes gave watch an extra cycle ending `1 cached`; `ch10`
  shows it and says watch can build once too often. `watch nosuch` is a failed cycle, not an exit.
- 2026-10-28 — the design:480 watch-loop diagram drops `[14]` and "the whole of §7" becomes "a
  whole build", for the book's no-`§` rule.
- 2026-10-28 — `ch09` runs 339 lines (four diagrams) and `ch08-01` 212, past the ~150-line guide.
