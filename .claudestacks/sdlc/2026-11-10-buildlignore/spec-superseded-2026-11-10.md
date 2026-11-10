---
status: superseded
created: 2026-11-10
built-on: 00533fb2b252
---

# Spec: .buildlignore, folders and files outside the build

A workspace marks paths as outside the build with `.buildlignore` files, nested per folder and
written in `.gitignore` syntax. One decision, "is this path ignored?", is made in one place and
used by the three parts of buildl that see workspace paths: the `buildl.sources` walk, the load
phase's reading of `inputs` and `subdir`, and `watch`. A workspace with no `.buildlignore` behaves
exactly as today.

## 1. Where things stand (premises)

| Fact | Proof |
|---|---|
| The `sources` walk skips only the directories it is handed; the composition root hands it the output directory and the state directory, nothing else | `crates/buildl/src/local_ports.rs:303-307` (`LuaSource::new(root, limits, [workspace.out, workspace.state], plugins)`); the skip checks are `crates/buildl-lua/src/sources.rs:97` (a declaring directory inside a skipped one yields nothing) and `:104` (`filter_entry` never enters a skipped directory) |
| Every entry the walk visits counts against `[declaration] walk_entries`; a skipped directory is neither visited nor counted | `crates/buildl-lua/src/sources.rs:66-72` (doc of `find`) and `:108-115` (the `visited` counter) |
| The walk refuses a matched file whose name is not UTF-8 | `crates/buildl-lua/src/sources.rs:125-130` (`non-UTF-8 file name`) |
| A walk failure is a `Refusal` carrying an `EvaluationFailure`, which reaches the command line as an `Evaluation` error | `crates/buildl-lua/src/sources.rs:76-79,125-130` (`Refusal::new(EvaluationFailure::…)`); `crates/buildl/src/cli/exit.rs:122` (`Error::Evaluation`) |
| An `inputs` entry is joined with the build file's directory and parsed in the load phase | `crates/buildl-core/src/load/traversal.rs:186-194` (`convert_input`) |
| `subdir` requests are resolved in the load phase | `crates/buildl-core/src/load/traversal.rs:128` (`resolve_subdirs`) |
| `load` takes a `DeclarationSource` and a `Dispatcher`, nothing that reads files; it is called twice by the discovery passes | `crates/buildl-core/src/load/traversal.rs:57-62`; `crates/buildl-core/src/discovery/passes.rs:84,110` |
| `watch` drops a change under the output directory, the state directory or `.git`, decided in core from the last cycle's `WatchLayout` | `crates/buildl-core/src/watch/react.rs:30-41` (`kept`, `.git` hard-coded at `:37`), called at `crates/buildl-core/src/watch/run.rs:50`; `Cycle { layout: Option<WatchLayout>, watched }` at `crates/buildl-core/src/types/cycle.rs:28-33` |
| `watch` opens the workspace anew at the start of every cycle | `crates/buildl/src/workspace/watching.rs:62` (`self.wiring.reopened()` in `Reopening::build`) |
| The `Ports` bundle is one trait with one associated type per port | `crates/buildl-core/src/ports/bundle.rs:277` |
| `InvalidDeclaration` and `Evaluation` errors exit with code 5 (Definition) | `crates/buildl/src/cli/exit.rs:122-124` |
| The rust plugin's inputs are every file below the build file's folder except `target/` and `.git/` | `plugins/rust/README.md:84`; `plugins/rust/graph.lua:44-49` |
| Nothing in the workspace knows the `ignore` crate or an ignore file today | `grep -n -A2 'name = "ignore"' Cargo.lock` → no match; `git grep -n -E "ignore" -- crates \| grep -iE "ignore::\|buildlignore\|ignore file\|ignore_file"` → no match; control: the same `Cargo.lock` grep finds `name = "walkdir"` (`Cargo.lock:1694`) |

## 2. The `.buildlignore` file

### 2.1 Syntax

`.gitignore`'s, as git reads it. buildl builds each pattern with the `ignore` crate's
`GitignoreBuilder::add_line` (`ignore-0.4.34/src/gitignore.rs:463-544`), feeding it the lines
itself, so it applies these rules:

| Rule | Proof |
|---|---|
| a line starting with `#` is a comment; a blank line is skipped | `gitignore.rs:470-478` |
| trailing spaces are trimmed unless escaped (`\ `) | `gitignore.rs:473-475` |
| `!` re-includes | `gitignore.rs:491-493` |
| a leading `/` anchors to the file's folder | `gitignore.rs:495-502` |
| a trailing `/` matches folders only | `gitignore.rs:506-514` |
| a pattern with no `/` matches at any depth | `gitignore.rs:519-524` |
| matching is case-sensitive on every platform | `GitignoreBuilder::new` sets `case_insensitive: false` (`gitignore.rs:341`); buildl never turns it on |
| an unclosed `[` is a literal, as the crate allows it | `allow_unclosed_class: true` (`gitignore.rs:342`); probe: a `.rgignore` holding `[bad` loads with no error under `rg --debug` |

Two things buildl does itself, because `add_line` alone differs from git:

- **The BOM.** A UTF-8 byte-order mark at the start of the file is dropped before the first line
  is built. The crate drops it only in `GitignoreBuilder::add` (`gitignore.rs:427-430`), which
  buildl does not call; `add_line` has no BOM handling (`gitignore.rs:463-544`).
- **Braces.** `{` and `}` are escaped (`\{`, `\}`) before a line is built, so they are literal as in
  git. The crate builds a `globset` glob with `backslash_escape(true)` (`gitignore.rs:531-536`), in
  which `{a,b}` is alternation and an unclosed `{` an error. Probes:

  ```text
  git, .gitignore = "{a", file "{a":         $ git check-ignore -v '{a'
                                             .gitignore:1:{a	{a
  ripgrep (same crate), .rgignore = "\{a",   $ rg --files --hidden --no-ignore-vcs --no-ignore-parent --sort path .
  files "{a" and "a,b":                      ./.rgignore
                                             ./a,b
  ```

  Git ignores `{a` as a literal name; the crate with the brace escaped does the same.

### 2.2 Where the files are read from

- A `.buildlignore` may sit in any folder of the workspace. Its patterns apply to that folder and
  everything below it.
- Only files at or below the workspace root are read. A `.buildlignore` above the root never
  applies. buildl reads the files itself rather than through the crate's walker, whose
  `parents(true)` reads ignore files in every ancestor up to `/` (`ignore-0.4.34/src/dir.rs:221-227`).
- `.gitignore`, `.ignore`, git's global file and `info/exclude` are never read.
- A `.buildlignore` that is a symlink is not followed: it is refused as a file that cannot be
  used (§5). This is the walk's rule for links (`sources.rs:101`, `follow_root_links(false)`; walkdir
  follows no link by default, `walkdir-2.5.0/src/lib.rs:292`, `follow_links: false`).
- A `.buildlignore` is an ordinary file for everything else: `sources` lists it unless a pattern
  ignores it, and a target whose inputs include it is dirty when it changes. A pattern that ignores
  the `.buildlignore` itself hides it from `sources` and its rules still apply. Git behaves the
  same:

  ```text
  sub/.gitignore = "x.txt\n.gitignore"; files sub/x.txt sub/y.txt
  $ git status --porcelain --untracked-files=all sub
  ?? sub/y.txt
  ```

- The output directory and the state directory stay skipped as today, whatever the patterns say.

### 2.3 The decision for one path

`ignored(path)` answers git's way, which is the same answer the walk gives by never entering an
ignored folder:

1. Take the folders on the way to the path, from the root down: for `notes/keep/b.md`, `notes`
   then `notes/keep`.
2. For each folder in that order, and then for the path itself, ask the rules: the `.buildlignore`
   files of the root and of every folder above the one being asked, deepest file first; within a
   file the last matching line decides. A folder is asked as a folder, the path as its own kind.
3. The first folder (or the path) whose answer is "ignored" ends the search: the path is ignored
   by that pattern. A `!` line below it cannot re-include anything under it.
4. If no answer is "ignored", the path is not ignored.

Checked example, against git (`git check-ignore` answers for one path, as the port does):

```text
.gitignore = "notes/\n!notes/keep/"; files notes/a.md, notes/keep/b.md
$ git check-ignore -v notes/keep/b.md
.gitignore:1:notes/	notes/keep/b.md
$ git check-ignore -v --no-index notes/keep
.gitignore:1:notes/	notes/keep
```

Both are ignored by line 1, `notes/`; the `!notes/keep/` line never applies. The crate's
`matched_path_or_any_parents` walks up from the path instead and stops at the first match
(`gitignore.rs:234-256`), which for this example would stop at `notes/keep` on the `!` line and
keep the file. buildl therefore does not call it; it asks the folders root-down as above.

The walk agrees by construction: it asks each folder before entering it and never enters an
ignored one. Probe (ripgrep 15.1.0, same crate, `.rgignore` as the file name):

```text
ws/.rgignore      notes/ · !notes/keep/ · /a.log · .claude/ · docs/
ws/src/.rgignore  gen/* · !gen/keep.rs
ws/lib/.rgignore  only blank lines and a # comment
$ rg --files --hidden --no-ignore-vcs --no-ignore-parent --sort path .
./.hidden.rs  ./.rgignore  ./lib/.rgignore  ./lib/docs.txt  ./lib/l.rs
./src/.rgignore  ./src/gen/keep.rs  ./src/main.rs
```

`notes/keep/b.md` is not listed; the root's `a.log` is gone (anchored); `lib/docs/d.md` is gone,
`lib/docs.txt` kept (`docs/` matches folders at any depth); `src/gen/keep.rs` is listed and
`src/gen/x.rs` is not (nested file, `!`). `lib/.rgignore`, holding only a blank line and a comment,
ignores nothing of `lib/`: `lib/l.rs` and `lib/docs.txt` are listed, and `lib/docs/d.md` is gone
only through the root's `docs/`.

## 3. Components

```text
buildl-core                          buildl                              buildl-lua
───────────                          ──────                              ──────────
port IgnoreRules  ◄── implements ─── ignore_rules adapter module         Sources walk
  ignored(path, kind)                  reads .buildlignore files from      asks the port per entry;
   → Ok(None)                          the root down (§2.3);               an ignored folder is not
   → Ok(Some(Ignoring))                GitignoreBuilder::add_line          entered and not counted
   → Err(Error::IgnoreFile)                  │                                   ▲
                                             │ local_ports builds one instance   │
load(source, rules, …) ◄─ Ports::Ignore ─────┤ per workspace open and hands it ──┘
  convert_input   ─ asks (File)              │ to LuaSource and to the pipeline
  resolve_subdirs ─ asks (Folder)            │
watch: kept ─ asks (Unknown) ◄─ Cycles::rules() of the last opened cycle
```

| Component | Crate | Responsibility |
|---|---|---|
| `IgnoreRules` port | `buildl-core` (`ports/`) | `fn ignored(&self, path: &str, kind: EntryKind) -> Result<Option<Ignoring>>`. `path` is workspace-relative, `/`-separated text, the same form as a watch path and as a joined input. `Sync`, because load evaluates build files in parallel through the `Dispatcher`. |
| `EntryKind`, `Ignoring` | `buildl-core` (`types/`) | `EntryKind` is `File`, `Folder` or `Unknown` (a path the asker cannot classify, §6). `Ignoring` holds the workspace-relative path of the `.buildlignore` whose pattern decided and the pattern as written. |
| `Error::Ignored`, `Error::IgnoreFile` | `buildl-core` (`error.rs`) | §5 |
| `Ports::Ignore` | `buildl-core` (`ports/bundle.rs`) | a new associated type, `IgnoreRules` |
| load checks | `buildl-core` (`load/`) | `load` gains a `rules: &I` parameter, `I: IgnoreRules`, passed by both discovery passes from `Ports::Ignore`. After `convert_input` parses an input it asks `(input, File)`; after `resolve_subdirs` resolves a subdir it asks `(subdir, Folder)`. `Some` is `Error::Ignored`. |
| watch filter | `buildl-core` (`watch/`) | the `Cycles` trait gains a method that returns the rule set of the last cycle that opened the workspace. `kept` also drops a path that set reports ignored when asked with `Unknown`. Before the first successful open, no path is dropped by rules. A port error keeps the path, so the next cycle's load reports it. |
| `ignore_rules` adapter | `buildl` (new adapter module) | implements the port with §2.3's algorithm: reads a folder's `.buildlignore` once per instance (BOM dropped, braces escaped, each line through `add_line`); answers `Unknown` by looking at the path: an existing folder is asked as `Folder`, an existing file and a missing path as `File`. The folders on the way to the path are asked as folders whether or not they still exist (§2.3), so a missing file under an ignored folder is ignored. Imports no other adapter module (`cargo make guard-adapter-isolation`). |
| walk | `buildl-lua` (`sources.rs`) | `filter_entry` asks the port for each entry before visiting it, with `File` or `Folder` from the entry's type; an ignored folder is not entered and an ignored file is not listed; neither counts against `walk_entries`. An entry whose path is not UTF-8 is not asked; the walk treats it as today (a matched non-UTF-8 file is refused, `sources.rs:125-130`). The out and state skip list stays. |
| composition | `buildl` (`local_ports.rs`, `workspace/watching.rs`) | one `ignore_rules` instance per workspace open, given to `LuaSource` and as `Ports::Ignore`; `Reopening` keeps the instance of the cycle it last opened and returns it through the `Cycles` method. A changed `.buildlignore` takes effect from the next cycle. |

`buildl-core` stays free of I/O and of the `ignore` crate: it only asks the port. `buildl-lua` does
not depend on `ignore`; it reaches the rules through the core port, which it already depends on
(`deny.toml:86`: `buildl-core` wrappers include `buildl-lua`).

## 4. Data flow

```text
buildl check / plan / build / test / watch
  └─ open workspace ─► ignore_rules (fresh)
       ├─ Load: LuaSource evaluates build.lua
       │    └─ buildl.sources("**/*") ─► walk ─► port per entry ─► ignored entries never listed
       ├─ Load: each `inputs` entry ─► port (File)   ─► Some ⇒ Error::Ignored (exit 5)
       ├─ Load: each `subdir`       ─► port (Folder) ─► Some ⇒ Error::Ignored (exit 5)
       └─ watch: change batch ─► kept() ─► out / state / .git / port(Unknown) ignored ⇒ dropped
```

The check runs on every `inputs` entry, those a plugin writes included, after it is joined with the
build file's folder. A dependency's outputs are not workspace files and are not checked.

## 5. Errors

All are definition errors, exit code 5, raised while loading, so `buildl check` reports them.

| Error | When | Message |
|---|---|---|
| `Error::Ignored { field: Input, … }` | an `inputs` entry is ignored | `<build file>: inputs: notes/a.md is ignored by .buildlignore (notes/)` |
| `Error::Ignored { field: Subdir, … }` | a `subdir` names an ignored folder | `<build file>: subdir: examples is ignored by .buildlignore (examples/)` |
| `Error::IgnoreFile`, a bad line | a pattern does not parse | `lib/.buildlignore:1: error parsing glob '[z-a]': invalid range; 'z' > 'a'` |
| `Error::IgnoreFile`, not UTF-8 | the file's bytes are not UTF-8 | `lib/.buildlignore:<line>: not UTF-8`, `<line>` the first line holding invalid bytes |
| `Error::IgnoreFile`, unusable file | the file cannot be read, or is a symlink | `lib/.buildlignore: <the I/O error's text>` or `lib/.buildlignore: a symlink, not a file` (no line) |

- The bad-line text is the crate's own: `Error::Glob` displays as `error parsing glob '<glob>':
  <err>` (`ignore-0.4.34/src/lib.rs:351-352`); buildl prefixes `<file>:<line>: `. Probe of a
  failing pattern, through ripgrep's use of the same crate:

  ```text
  .rgignore = "[z-a]"
  rg: ./.rgignore: line 1: error parsing glob '[z-a]': invalid range; 'z' > 'a'
  ```

  Git, given the same line, does not complain; buildl refuses it, by the intent's rule that a
  `.buildlignore` that cannot be used is refused, never skipped.
- The `Ignored` message names the file and the pattern, not a line: the crate's `Glob` keeps the
  source file and the original text but no line number (`gitignore.rs:32-45`).
- `IgnoreFile` is raised by whichever asker first reads the file. From a load check it is the
  `Error::IgnoreFile` above. From the walk it travels as the walk's other failures do, a `Refusal`
  that reaches the command line as an `Evaluation` error whose text is the `IgnoreFile` message;
  both exit 5 (`exit.rs:122-124`). Tests assert the exit code and the message text, not the
  variant, for the walk route.
- The crate's own `GitignoreBuilder::add` keeps going after a bad line and returns a partial error
  (`gitignore.rs:401-437`). buildl does not call it: it reads the file, feeds the lines to
  `add_line`, and stops at the first error.

## 6. Edge cases

| Case | Behaviour |
|---|---|
| no `.buildlignore` anywhere | nothing ignored; every existing test and example passes unchanged |
| an empty file, or only blank lines and comments | ignores nothing (§2.3 probe, `lib/`) |
| a `!` pattern for a path under an ignored folder (`notes/` then `!notes/keep/`) | the path stays ignored (§2.3 checked example) |
| a pattern naming the output or state directory, or `!out/` | no effect: those stay skipped by the existing list |
| a pattern ignoring a `.buildlignore` itself | that file is not listed by `sources`; its rules still apply (§2.2) |
| a pattern ignoring `build.lua` or `buildl.toml` at the root | no effect on loading: the root build file and the manifest are always read; only `inputs`, `subdir`, `sources` and `watch` consult the rules |
| `sources` called from a build file whose own folder is ignored | returns nothing, as for a skipped folder today (`sources.rs:97`) |
| a `.buildlignore` above the workspace root | never read (§2.2) |
| a `.buildlignore` that is a symlink | refused, `IgnoreFile` (§5) |
| `watch`: an ignored folder `notes` (pattern `notes/`) is created, or a file below it changes | `notes` exists and is asked as a folder; a path below it meets `notes` on the way down. Ignored; no cycle starts. |
| `watch`: an event for files below a removed `notes` | each meets `notes` on the way down, asked as a folder whether or not it still exists (§2.3). Ignored; no cycle starts. |
| `watch`: an event for the removed folder `notes` itself | the path is missing, so it is asked as a file, and `notes/` matches folders only: kept. One extra cycle runs and finds nothing to build. Asking a missing path as a folder instead would drop the removal of a file named `docs` under a `docs/` pattern, a file `sources` does list, and leave the build stale, so the extra cycle is the safe side. |
| `watch`: a removed file `a.log` with `/a.log` | asked as a file; ignored. No cycle starts. |
| `watch`: a `.buildlignore` changes | a change like any other unless a pattern ignores that file; the next cycle opens a fresh rule set |
| a path the source-path grammar refuses | refused by the grammar first, as today (`traversal.rs:193`); the port is never asked |
| a walk entry whose path is not UTF-8 | not asked (§3 walk row) |

## 7. Dependency

- `ignore = "0.4"` in `[workspace.dependencies]`, with its reason comment, used by `buildl` only.
  0.4.34, licence `Unlicense OR MIT`, `rust-version` 1.88 (crates.io API, fetched 2026-11-10; the
  same values in the extracted `ignore-0.4.34/Cargo.toml`).
- Probe, in a scratch copy of the repository: `cargo add ignore@0.4.34 -p buildl` adds exactly one
  package to `Cargo.lock` (`> name = "ignore"` is the only new `name` line); `cargo deny check -D
  unused-wrapper` then reports `advisories ok, bans FAILED, licenses ok, sources ok`, the bans
  failing only on `walkdir` and `globset`, whose `wrappers` lists do not include `ignore`
  (`deny.toml:102,109`).
- `deny.toml` therefore adds `ignore` to the `wrappers` of `walkdir` and `globset`, with the reason
  extended, and adds a ban entry `{ crate = "ignore", wrappers = ["buildl"] }` whose reason confines
  it to the `ignore_rules` adapter.

## 8. Testing

| Layer | Tests |
|---|---|
| `buildl-core` flows, fake `IgnoreRules` | an ignored input refuses with `Error::Ignored{Input}`; an ignored subdir with `Error::Ignored{Subdir}`; a plugin-written input is checked like a hand-written one; a port error from a load check surfaces as `Error::IgnoreFile`; `kept` drops a path the fake ignores and keeps one it errors on; before any cycle opened, `kept` drops nothing by rules; with a fake that ignores nothing, every existing flow is unchanged |
| `buildl` `ignore_rules` unit tests | root file; nested file overriding the root; `!` re-include; the §2.3 example (`notes/keep/b.md` ignored by `notes/`); leading `/`; trailing `/` against a file and a folder; slash-less pattern at depth; case-sensitivity; a BOM file whose first pattern matches; `{a` matched literally; empty and comment-only file; a `.buildlignore` above the root not read; `Unknown` for an existing folder (asked as a folder), an existing file and a missing path (asked as a file), and a missing file under an ignored folder (ignored); `[z-a]` gives `IgnoreFile` with file and line 1 and the §5 text; non-UTF-8 text gives `IgnoreFile` with file and line; an unreadable file and a symlinked file give `IgnoreFile` with file and no line |
| `buildl-lua` walk | an ignored folder is not entered and costs no `walk_entries`; an ignored file is not listed; a `.buildlignore` is listed unless ignored |
| `buildl` end to end | a workspace whose `.buildlignore` ignores `notes/`: build, edit `notes/a.md`, `plan` shows the target clean; an `inputs` entry under `notes/` exits 5 with the §5 message; `watch` starts no cycle for a change under `notes/` |
| `buildl-cli` | the exit code and message text of each §5 row, the walk route included |

## 9. Documentation

- `book/src/ch03-02-inputs-outputs-deps.md`: a section on leaving paths out with `.buildlignore`.
- `book/src/ch10-00-watch.md`: ignored paths start no cycle.
- `book/src/appendix-03-lua-api.md`: `sources`, `inputs` and `subdir` against ignored paths.
- `book/src/appendix-02-buildl-toml.md:67`: `walk_entries` does not count ignored folders.
- `book/src/appendix-04-exit-codes.md`: the §5 messages under code 5.
- `book/src/ch11-03-four-crates.md`: the new port in the ports list and the `ignore_rules` adapter.
- `book/src/ch11-04-errors.md`: the two new `Error` variants.
- `plugins/rust/README.md:84` and `:145`: the inputs row and the workspace note mention
  `.buildlignore`.
- `skills/buildl/`: where it describes `sources`.

## 10. Non-goals

- buildl building its own repository (root `buildl.toml`, `build.lua`, the pin): a sibling chain
  after a release that has `.buildlignore`.
- Reading `.gitignore` or any ignore file other than `.buildlignore`.
- Changing the `rust` or `c` plugin's code: they gain the filter through `sources`.
- Applying the rules to the sandbox copy, the store, `plugins fetch`, `clean` or `gc`.
- Moving `watch`'s hard-coded `.git` into a `.buildlignore`.
- A command-line flag or `buildl.toml` key to turn the files off.
