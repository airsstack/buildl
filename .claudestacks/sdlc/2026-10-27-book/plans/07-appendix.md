---
status: done
created: 2026-10-28
depends-on: [01]
built-on: 136d5b098d7e
---

# Appendix Implementation Plan

**Goal:** The book's appendix lists every command, `buildl.toml` key, build-file function and
exit code exactly as the code defines them, plus the merged reference list.

**Architecture:** This plan only fills stubs that plan 01 created. Appendices A–D are read from the
code, never from `docs/`:
- **A:** the binary's `--help`.
- **B:** the manifest structures.
- **C:** the `buildl` module's install.
- **D:** the exit enum.

Appendix E merges the four documents' numbered reference lists. The writing rules every chapter
plan shares apply: plain words, relative `.md` links, and no `§` sign in the book.

**Tech Stack:** Markdown, mdBook 0.4.52, the `buildl` debug binary.

---

## Files that change

```
book/src/appendix-00.md              — [modify] what the appendix holds
book/src/appendix-01-cli.md          — [modify] every command and flag
book/src/appendix-02-buildl-toml.md  — [modify] every table and key
book/src/appendix-03-lua-api.md      — [modify] every buildl.* function
book/src/appendix-04-exit-codes.md   — [modify] exit codes 0–7
book/src/appendix-05-references.md   — [modify] merged reference list
```

## Order of work

### Task 1 — Command-line reference

**Files:** Modify `book/src/appendix-00.md`, `book/src/appendix-01-cli.md`.

**Change:**
- The opener lists the five appendices.
- The CLI reference gives one entry for each of the 13 commands that `buildl --help` lists:
  - `check`, `graph`, `query`, `plan`, `build`, `watch`, `test`, `verify`, `doctor`, `log`, `clean`, `gc`, `plugins` (and `plugins fetch`), plus the global `--color`;
  - each with its arguments and every flag, taken from that command's `--help`;
  - the environment variables the binary reads: `NO_COLOR`, `TERM`, `BUILDL_REMOTE_UPLOAD`, `BUILDL_REMOTE_STRICT`, `BUILDL_REMOTE_EXECUTE` (`crates/buildl-cli/src/main.rs:110-119`); `XDG_DATA_HOME` and `HOME`, where `interactive` keeps its approvals (`crates/buildl/src/approver/data_dir.rs:44,47`); `PATH`, searched for programs (`crates/buildl/src/search_path.rs:43`).

**Proof:**
- `buildl <cmd> --help` is run for every command, and `buildl plugins fetch --help` for the subcommand.
- Every flag each prints appears on the page. The task's report shows a script's count of the flags per command, on the page and in `--help`, and the two counts are equal.
- `mdbook build book` exits 0.

**Done when:** all hold. Commit `docs(repo): book appendix A, command-line reference`.

### Task 2 — `buildl.toml` reference

**Files:** Modify `book/src/appendix-02-buildl-toml.md`.

**Change:**
- Every table and key `buildl.toml` accepts, with its type, default and meaning.
- Read from the schema `buildl.toml` is parsed into: `crates/buildl/src/manifest/schema.rs:63` (`Raw`, `#[serde(deny_unknown_fields)]`) and its tables `WorkspaceTable` `:91`, `CeilingTable` `:105`, `ProcTable` `:120`, `EnvTable` `:129`, `FsTable` `:138`, `DeclarationTable` `:150`, `ExecutionTable` `:162`, `ApproverTable` `:174`, `RemoteTable` `:182`, `TokenTable` `:211`, `PluginTable` `:221`. Key names come from the serde field names and renames, not from `TomlManifest` (`toml_manifest.rs:65`), which holds interpreted values. Defaults and meanings may be read from where `toml_manifest.rs` interprets each field.
- Each key is cited to its field.

**Proof:**
- Every field of `Raw` and of each table above is on the page under its key name. The task's report lists the fields and their `file:line`.
- A `buildl.toml` holding every table, written in a scratch workspace, passes `buildl check`. The
  git plugin keys (`git`, `github`, `rev`) are the exception: `check` refuses an unfetched git
  plugin with exit 5 (`PluginNotFetched` → Definition, `crates/buildl-cli/src/exit.rs:141`), and
  fetching needs the network, so they are shown in their own snippet with that refusal.
- `mdbook build book` exits 0.

**Done when:** all hold. Commit `docs(repo): book appendix B, buildl.toml reference`.

### Task 3 — Build file API

**Files:** Modify `book/src/appendix-03-lua-api.md`.

**Change:** One entry per function the `buildl` table installs. Each entry gives the arguments, the options it accepts, what it returns, and a one-line example. The functions:

| Function | Installed at (`crates/buildl-lua/src/module.rs`) |
|---|---|
| `target`, `test`, `discover` | `:139-152` |
| `rule` | `:161` |
| `alias` | `:170` |
| `option` | `:179` |
| `subdir` | `:188` |
| `sources` | `:198` |
| `discovered` | `:211` |
| `decode_all` | `:214` |
| `use` | bound by `plugins.bind_use`, `:102` |

The page also says that `print`, `math.random` and `math.randomseed` are removed (`:107-109`). Options are read from the parsing code each function calls.

**Proof:**
- A scratch build file runs `for k in pairs(buildl) do … end`, and reports the names through a target's command or an error message. The names it reports equal the page's list.
- `mdbook build book` exits 0.

**Done when:** both hold. Commit `docs(repo): book appendix C, build file API`.

### Task 4 — Exit codes and references

**Files:** Modify `book/src/appendix-04-exit-codes.md`, `book/src/appendix-05-references.md`.

**Change:**
- **Exit codes.** The eight codes of `crates/buildl-cli/src/exit.rs:23-40`:

  | Code | Meaning |
  |---|---|
  | 0 | Success |
  | 1 | ActionFailure |
  | 2 | Usage |
  | 3 | Infrastructure |
  | 4 | Refused |
  | 5 | Definition |
  | 6 | CorruptState |
  | 7 | Internal |

  Each gets its meaning from the doc comments, and the outcome classes that map to it come from `Exit::of_class` and the other three mappings in the same file. Note that codes stay clear of 126, 127 and anything above 128.
- **References.** Merge the four documents' numbered reference lists into one numbered list, with duplicates removed and the entry naming buildl's own `docs/` files left out (the book replaces them):

  | Document | Reference list |
  |---|---|
  | `docs/design.md` | `:1046-1061` |
  | `docs/architecture.md` | `:739-750` |
  | `docs/architecture-building-blocks.md` | `:762-773` |
  | `docs/fundamental-walkthrough.md` | `:290-298` |

**Proof:**
- Each exit code is shown on the page by a captured command that ends with it, wherever one can be produced in a scratch workspace:
  - 0: build;
  - 1: a failing action;
  - 2: a bad flag;
  - 3: `--remote-strict` with no server;
  - 4: an action outside the ceiling;
  - 5: a broken `build.lua`.
- Codes 6 and 7 are described from source.
- Every URL in the four reference lists appears once on the references page.
- `mdbook build book` exits 0.

**Done when:** all hold. Commit `docs(repo): book appendix D and E, exit codes and references`.

## Risks

- How a build file can print the `buildl` table's keys once `print` is removed is left to the coder. Task 3 settles it with a probe: an `error()` carrying the joined names is one way.
- A table may hold a field whose type is defined outside `schema.rs`. Task 2 follows every field's type to its definition.

## Proof

- `cargo make book-build` exits 0.
- Appendices A–D match the code by the counts each task reports.
- These spec §3 rows land here. Mark each in the execution record.

  | Source | Row | Appendix |
  |---|---|---|
  | `docs/design.md` | §4 | B |
  | `docs/design.md` | §5 | C |
  | all four documents | References | E |

## Review findings

- wrong output — `appendix-04:159` showed a `build.lua` missing only `)`, but the captured error
  (`'}' expected (to close '{' at line 2)`) came from a fixture missing `})`. The page now shows
  the captured fixture; re-run: same error, exit 5. Fixed.
- nit — `appendix-04:182` described code 7 as only "no row for"; `exit.rs:146-155` also sends nine
  named errors to Internal. Now "an error that should never happen, or … no row for". Fixed.
- nit — the `schema.rs` line column goes stale on edits. Kept: the plan requires each key cited to
  its field.
- spec risk — the two deviations below were not in the plan. The plan's Task 2 Proof and Task 4
  references line are amended. Fixed.

## Probe results

- Appendix A: `--help` for all 13 commands and `plugins fetch`; flags per command on the page vs
  in `--help` (own + 3 global): check 3/3, graph 5/5, query 4/4, plan 5/5, build 12/12,
  watch 12/12, test 14/14, verify 5/5, doctor 3/3, log 5/5, clean 3/3, gc 3/3, plugins 3/3,
  plugins fetch 3/3. Env vars read at `main.rs:112-118` (plan said 110-119; same block).
- Appendix B: all 45 fields of `Raw` and its 11 tables are on the page under their key names, each
  cited to `schema.rs`; struct lines match the plan. The all-tables `buildl.toml` (minus the git
  keys) passes `buildl check`: `ok: 1 declaration in 1 build file`. Reviewer spot-checked 10
  defaults against `toml_manifest.rs`/`interpret.rs`/`timeout.rs`: all correct.
- Appendix C: `pairs(buildl)` reported through `error()`: `alias decode_all discover discovered
  option rule sources subdir target test use` (11), equal to the page. `print`, `math.random`,
  `math.randomseed` are nil; `airsstack.buildl == buildl`.
- Appendix D: captured 0 (build), 1 (`sh` exits 1), 2 (`--fast`), 3 (`--remote-strict` against
  `grpc://127.0.0.1:1`, after 6 calls), 4 (`python3` outside the ceiling), 5 (syntax error);
  6 and 7 from source. Enum at `exit.rs:23-40` as planned.
- Appendix E: 37 URLs in the four lists, 24 distinct, each once on the page and no other URL.
- Spec §3 rows landed: design §4 → B; design §5 → C; References of all four documents → E.

## Deviations

- 2026-10-28 — the git plugin keys cannot sit in a file that passes `check` (unfetched git plugin
  → exit 5, and fetching needs the network); they are shown in their own snippet with the real
  refusal. Plan amended.
- 2026-10-28 — the references page leaves out the entry naming buildl's own `docs/` files, and its
  intro no longer names them: the book replaces those documents, and plan 08's search forbids
  their names in `book/src`. Plan amended.
