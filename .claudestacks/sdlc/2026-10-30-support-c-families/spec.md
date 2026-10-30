---
status: approved
created: 2026-10-30
built-on: 478d0ea47647
---

# Spec: C and C++ support through an official plugin

An official plugin, `c`, kept in this repository under `plugins/c/` and fetched by git tag, lets a
workspace build C and C++ one object per target with every header declared. It finds each source's
headers with one discovery target per source running the compiler's `-MM`, compiles each source in
its own target, and links, archives or tests the result, on Linux and macOS with gcc or clang. It
adds nothing the target kind does not need; helpers that return values make common choices short.
Two stack-agnostic core changes make it possible: an optional `dir` key that fetches a git plugin
from a subfolder of its repository, and `buildl.folder()`, which tells Lua the folder of the build
file being read. Two examples show every function and helper, CI checks them through a real git
fetch, and the book, roadmap and skills stop saying per-file C and C++ is unsupported.

## 1. Premises and their evidence

Every fact the design rests on, proven in this task. buildl probes ran on 2026-10-30 against the
`buildl` binary built from commit `0493a46` (`target/debug/buildl`), on macOS with Apple clang
15.0.0 (`clang-1500.3.9.4`, `arm64-apple-darwin25.3.0`), strategy `sandbox` (the default). Linux
probes ran in a `debian:bookworm-slim` container (aarch64) with gcc 12.2.0, clang 14.0.6 and GNU
binutils 2.40, each command under `env -i`, the empty environment an action gets when it declares
no variable (P34). External documents were downloaded raw on 2026-10-30 and are cited as
`<document>:<line>`.

### 1.1 buildl as it is

| # | Fact | Evidence |
|---|---|---|
| P1 | Per-file C builds correctly today when a discovery target running `cc -MM` declares each object's headers: only what an edit touches reruns, early cutoff holds, `verify` finds no mismatch | probe workspace: `src/main.c`, `src/util.c`, `include/util.h`; discovery `cc -Iinclude -MM src/main.c src/util.c` (`stdout = true`), one target per `.o` from its output, a link. Edit `util.c` (`a + b` → `a + b + 1`) → discovery `1 built`, then `2 built, 1 cached`; add a comment to `util.h` → `2 built, 1 cached` (both objects; the link cached); add `#include "k.h"` to `main.c` → discovery prints `main.o: src/main.c include/util.h include/k.h`; edit `k.h` (`#define K 10` → `20`) → `2 built, 1 cached`; `buildl verify` → `3 verified, 0 mismatched` |
| P2 | An object target that declares only its `.c` file fails in the sandbox, and can read a system header of the same name instead of the project's | probe: `inputs = { "src/main.c" }`, `-Iinclude` → `src/main.c:3:33: error: call to undeclared function 'add'`; the SDK's `MacOSX.sdk/usr/include/util.h` exists and was found in place of the undeclared `include/util.h` |
| P3 | `[plugins.<name>]` has no subfolder key | probe `dir = "plugins/c"` → ``buildl.toml:15:1: unknown field `dir`, expected one of `path`, `git`, `github`, `rev`, `proc`, `env`, `network` ``, exit 5 |
| P4 | A tag named `plugin-c-v0.1.0` is accepted | probe `rev = "plugin-c-v0.1.0"` → `plugin c is not fetched: buildl.lock does not pin … at plugin-c-v0.1.0` (exit 5, past the name check); control `rev = "-bad"` → `invalid tag name: "-bad" — must not start with '-' or '/'` |
| P5 | A git source is `PluginSource::Git { url, rev }`, built in one place and matched in two | crates/buildl-core/src/types/plugin_source.rs:306-315; built crates/buildl/src/manifest/interpret.rs:197; matched crates/buildl/src/plugins/check.rs:75, crates/buildl/src/plugins/fetch.rs:96 |
| P6 | A fetch clones into a staging folder, removes `.git`, digests the whole staged tree, then renames it to `.buildl/plugins/<plugin>/<commit>`; a commit holding no file is refused with `PluginNotFetched` | crates/buildl/src/plugins/fetch.rs:154-168, :189-201, :220-222; crates/buildl/src/plugins/tree_digest.rs:28-38, :47-59 |
| P7 | A lock row holds `commit`, `git`, `rev`, `sha256` and refuses unknown fields | crates/buildl/src/plugins/lock.rs:24-37 |
| P8 | A lock row matches a source on `git` and `rev` | crates/buildl/src/plugins/fetch.rs:99-101; crates/buildl/src/plugins/check.rs:76-78 |
| P9 | The Lua loader reads `extension.toml` and the entry from the folder it is handed, and `require` is confined to that folder | crates/buildl-lua/src/plugin/loader.rs:209-222; crates/buildl-lua/src/plugin/require.rs:1-6, :72, :121 |
| P10 | `Directory::parse` refuses more than 1024 bytes, a leading or trailing `/`, an empty, `.` or `..` segment, and characters outside ASCII alphanumerics, `.`, `-`, `_`; the empty string is the root, and `is_root` exists | crates/buildl-core/src/types/directory.rs:40-49; crates/buildl/src/local_ports.rs:162 |
| P11 | A `path` plugin refuses `rev` with "a path plugin has no rev" | crates/buildl/src/manifest/interpret.rs:148-155 |
| P12 | Inputs are relative to the build file's folder; command placeholders give paths relative to the workspace root, where commands run; a build file below the root writes its folder into its commands | book/src/appendix-03-lua-api.md:51, :78; examples/polyglot/go/build.lua:3-9 |
| P13 | An input cannot name a file outside its build file's folder | probe in `app/build.lua`, `inputs = { "../lib/include/util.h" }` → `invalid source path: "app/../lib/include/util.h" — must not contain a '.' or '..' segment`, exit 5 |
| P14 | Each build file gets its own plugin instance; nothing a plugin records reaches another build file | probe: a plugin appending to a module-level list, used from `build.lua` and `app/build.lua`, returned `SEEN1` in both |
| P15 | A plugin's `buildl` table shares the calling build file's discovery results, which hold that build file's folder | crates/buildl-lua/src/module.rs:123-128; crates/buildl-lua/src/plugin/environment.rs:60-64; crates/buildl-lua/src/results.rs:39 |
| P16 | Inside a plugin, one discovery target per source reruns only the edited source's discovery | probe plugin over `src/main.c`, `src/a/util.c`, `src/b/util.c`: second build `3 cached`; edit `src/a/util.c` (`a + b` → `a * b`) → `1 built, 2 cached` |
| P17 | A plugin's target that runs a program its manifest does not list is refused, whatever the ceiling allows | probe, manifest `proc.run = ["cc"]`, ceiling and cap allow `clang` → `program clang is outside plugin c@0.1.0 (proc.run allows: cc)`, exit 4 |
| P18 | The sandbox holds a target's inputs and the outputs of its direct dependencies only | probe: test with `deps = { ":app" }`, where `:app` links `:libutil.dylib` → `dyld: Library not loaded: @rpath/libutil.dylib … (no such file)`; with `deps = { ":app", ":libutil.dylib" }` → passed |
| P19 | `$deps` expands in the order the dependencies are written | probe: `deps = { ":libutil.dylib", ":app" }`, `run = { "$deps" }` → `command: out/libutil.dylib out/app` |
| P20 | `sh -c 'exec "$0"' $deps` runs only the first dependency's output | probe: `run = { "sh", "-c", 'exec "$0"', "$deps" }`, `deps = { ":app", ":libutil.dylib" }` → `tests: 1 passed` |
| P21 | A target with `always = true` reads `Dirty`/`Always` in `plan`, and every target depending on it reads `Conditional`, even right after a full build | probe `plan --json` after `build`: `//:app.version Dirty Always`, the objects and the link `Conditional` |
| P22 | A build still serves those dependents from the cache when the always-run output is unchanged | probe on the P1 workspace (one discovery, two objects, one link), `cc --version` as `always` with both objects depending on it: second build → `1 built, 3 cached` |
| P23 | A discovery target or compile that depends on a header-writing target sees it at its workspace path, and `-MM` names it there | probe: `:config.h` writes `out/config.h`; discovery with `deps = { ":config.h" }`, `-Iout` → `obj: src/main.c out/config.h`; compile with the same dep built, program printed `42` |
| P24 | `$ORIGIN` in an argument is passed through unchanged | probe: argument `-Wl,-rpath,$ORIGIN` printed back as `-Wl,-rpath,$ORIGIN`; crates/buildl-core/src/placement/expand.rs:268-282 (a placeholder is exactly `$in`, `$out`, `$deps`, or `$opt:<name>`) |
| P25 | A target cannot be declared twice | probe: two rules naming `util.o` → `//:util.o is declared more than once`, exit 5 |
| P26 | `cc --version` prints the real compiler's version and folder, and `/usr/bin/cc` is one launcher file with 78 links | `cc --version` → `Apple clang version 15.0.0 (clang-1500.3.9.4)` … `InstalledDir: /Library/Developer/CommandLineTools/usr/bin`; `ls -la /usr/bin/cc` → `-rwxr-xr-x 78 root wheel 118880` |
| P27 | `verify` finds no mismatch for a shared library and the binary using it. A static library made by plain `ar rcs` does not hold on macOS (P60) | probe: `2 verified, 0 mismatched` (dylib + binary) |
| P28 | No discovery from discovery: the second pass must declare the same discovery targets, unchanged | book/src/ch07-00-discovery-targets.md:207 |
| P29 | An error in a plugin names the plugin's file and line | book/src/ch08-01-writing-a-plugin.md:143-144 |
| P30 | A program a dependency built is not a want: it needs no `proc.run` entry | book/src/ch08-03-how-buildl-decides.md:31-32 |
| P31 | `examples/check.sh` runs `check`, `build`, `test`, "every target clean", `verify`, then the example's `assert.sh`, on a copy | examples/check.sh:24-31, :68-73 |
| P32 | The examples CI job runs on `ubuntu-latest` and `macos-latest`; `cargo make skills` runs `airsl test` over staged Lua tests | .github/workflows/ci.yml:69-77; Makefile.toml:387-435 |
| P33 | The `buildl` crate builds on Unix only | crates/buildl/src/lib.rs:157-158 |
| P34 | An action's environment holds exactly the declared variables that have a value | book/src/ch11-09-what-it-promises.md:52 |
| P35 | `buildl` lists the functions it holds in the error a misspelt one raises | book/src/appendix-03-lua-api.md:30-33 |
| P36 | `plugins fetch` with no `git` on the search path fails, exit 3 | crates/buildl-cli/tests/cli/exit_codes.rs:1173 |
| P37 | A git commit in a fresh repository is made with an identity passed on the command line in this codebase's own tests | crates/buildl/tests/e2e/fetch.rs:50; crates/buildl/src/plugins/git.rs:238 |
| P59 | A target name holds only ASCII letters, digits, `.`, `-` and `_`, at most 256 bytes, and is neither `.` nor `..`; `+` is refused | crates/buildl-core/src/types/grammar.rs:19-21, :56-70; crates/buildl-core/src/types/target_name.rs:20-22; probe `buildl.target("app.version.c++", …)` → `buildl check`: `invalid target name: "app.version.c++" — may hold only ASCII letters, digits, '.', '-' and '_'`, exit 5; control `app.version.cpp` → `ok: 1 declaration in 1 build file`, exit 0 |

### 1.2 Compilers on macOS, and their documentation

| # | Fact | Evidence |
|---|---|---|
| P38 | `-MM` prints one Make rule per input file, leaving out system headers; the rule's target is the input's base name with `.o` unless `-MT` sets it | gcc-preprocessor.html:162-168, :190-194, :246-254; probe: `cc -MM a/util.c b/util.c` → `util.o: a/util.c …` and `util.o: b/util.c` |
| P39 | `-MM` escapes a space as `\ `, breaks a long rule with `\` and a newline, and prints an absolute path for a header found through an absolute `-I` | probe: `util.o: a/util.c my\ dir/sp.h \` / `  /private/…/mm/inc/abs.h` |
| P40 | A header found through an absolute `-isystem` folder is left out of `-MM` output | probe: `cc -isystem <abs>/inc -I"my dir" -MM -MT obj a/util.c` → `obj: a/util.c my\ dir/sp.h` |
| P41 | `-MM` prints a `..` in an include path as written, unresolved | probe: `#include "../inc/abs.h"` in `a/dd.c` → `obj: a/dd.c a/../inc/abs.h` |
| P42 | `-MM` run from the workspace root on root-relative paths prints root-relative paths | probe in `cpp/build.lua`: `util.o: cpp/src/util.c cpp/include/util.h` |
| P43 | `c++ -MM` works on a `.cc` source the same way | probe: `x.o: a/x.cc inc/abs.h` |
| P44 | A data-only `.S` file is preprocessed by `-MM` and assembled and linked by `cc` | probe: `cc -Iinclude -MM -MT obj src/blob.S` → `obj: src/blob.S include/blob.h`; assembled, linked with `main.c`, printed `7` |
| P45 | `-dumpmachine` prints the target triple and nothing else | gcc-developer.html:1441-1443 (`i686-pc-linux-gnu`); probe → `arm64-apple-darwin25.3.0` for `cc` and `c++` |
| P46 | `-shared` makes a shared object; Apple clang accepts it and makes a dylib | gcc-link.html:337-343; probe: `cc -shared -o libu.dylib u.o` → `Mach-O 64-bit dynamically linked shared library arm64` |
| P47 | The linker reads objects and `-l` libraries in command order, so a library goes after the objects that use it | gcc-link.html:188-192 |
| P48 | On macOS a dylib's install name is the `-o` path as written unless `-install_name` sets it | probe: `cc -shared -o tmpo/libu.dylib` → `otool -D` → `tmpo/libu.dylib`; with `-install_name @rpath/libutil.dylib` → `@rpath/libutil.dylib` |
| P49 | `$ORIGIN` in an rpath is the folder holding the program; `-soname` sets a shared object's internal name | ld.so.8.html:153-162; ld-options.html:487 |
| P50 | `@loader_path` rpath finds a dylib next to the binary | probe: binary linked with `-Wl,-rpath,@loader_path` and `libutil.dylib` beside it ran and printed `5` in the sandbox |
| P51 | With `-g` the same source compiled in two folders gives different objects; without `-g`, the same bytes | probe: `cc -g -c -o f.o f.c` in `g1/` and `g2/` → `differ: char 1465`; without `-g` → `same` |

### 1.3 Linux

| # | Fact | Evidence |
|---|---|---|
| P52 | gcc and g++ compile with an empty environment but cannot link without `PATH`; with `PATH` they link; clang and clang++ compile and link with an empty environment | `env -i /usr/bin/gcc -o out/app out/main.o out/util.o` → `collect2: fatal error: cannot find 'ld'`; `env -i PATH=/usr/bin:/bin …` → linked, printed `5`; `cc`, `gcc`, `c++`, `g++` the same; `clang`, `clang++` linked with `env -i` |
| P53 | `-dumpmachine` names Linux | `aarch64-linux-gnu` (gcc), `aarch64-unknown-linux-gnu` (clang) |
| P54 | `ar rcs` gives the same bytes on two runs a second apart | `cmp out/a1.a out/a2.a` → equal |
| P55 | `-shared -Wl,-soname,libutil.so` and `-Wl,-rpath,'$ORIGIN'` make a library and a binary that finds it beside itself from any working folder | `readelf -d` → `Library soname: [libutil.so]`, `Library runpath: [$ORIGIN]`; run from `/` printed `5` |
| P56 | `-lm` before the objects fails to link; after them it links | `cc -lm -o m1 m.o` → failed; `cc -o m2 m.o -lm` → linked |
| P57 | Without `-g`, an object compiled in two folders has the same bytes; the `.S` file of P44 assembles with an empty environment and links; `..` is printed unresolved | `cmp` → `same object bytes`; `assembled with empty env`, printed `7`; `obj: src/dd.c src/../include/blob.h` |
| P58 | `ubuntu-latest` is Ubuntu 24.04 (x64) with GNU C++ 12.4–14.2, Clang 16–18 and Git; `macos-latest` is macOS 26 arm64 with Clang/LLVM 21, the Xcode Command Line Tools and Git | runner-images README.md:25, :32; Ubuntu2404-Readme.md:18, :22, :80; macos-26-arm64-Readme.md:17, :56, :84 |

### 1.4 Found while building the plugin

Probed on 2026-10-30, after the plugin was built, on the same macOS host and Debian image as §1.2
and §1.3, with the `buildl` binary built from commit `3fad016`. The scratch workspace uses the plugin
as a `path` plugin: `c.library`, `c.binary` and `c.test` at the root; `c.platform`,
`c.shared_name`, `c.soname`, `c.rpath`, `c.shared_library` and `c.binary` in `cpp/`.

| # | Fact | Evidence |
|---|---|---|
| P60 | macOS `ar rcs` writes the time it ran into the archive, so two runs over the same object differ; `ZERO_AR_DATE=1` makes them equal; `SOURCE_DATE_EPOCH` does not. Plain `ar rcs` as the archive step gives a `verify` mismatch | `ar rcs a2.a f.o`, then `ar rcs a3.a f.o` a second later, `f.o` unchanged → `cmp` reports they differ (at a timestamp digit, which varies by run); `ZERO_AR_DATE=1 ar rcs` twice, two seconds apart → equal; `SOURCE_DATE_EPOCH=0 ar rcs` twice → differ; workspace with `run = { "ar", "rcs", "$out", "$deps" }` → `9 verified, 1 mismatched` (`MISMATCH //:util: output util differs: bytes`) |
| P61 | `sh -c 'ZERO_AR_DATE=1 exec ar rcs "$0" "$@"' $out $deps` with `env = { "PATH" }` archives reproducibly on macOS and under GNU `ar`; `ar` run inside `sh` needs no `proc.run` entry of its own | macOS workspace → `10 verified, 0 mismatched, 0 failed, 0 skipped, 0 not cached, 0 not sampled`, then with `proc.run = ["cc", "c++", "sh"]` in the ceiling, the cap and the manifest → `out/util` written, `10 verified, 0 mismatched`; Debian, `GNU ar (GNU Binutils for Debian) 2.40`, the command under `env -i PATH=/usr/bin:/bin` twice, two seconds apart → `equal` |
| P62 | An error the plugin raises at the level of the build file's call is reported with the build file's line; the plugin's frames stay in the traceback; loading fails with exit 5 | build file line 3 `c.binary("app", { srcs = { "src/main.c" }, bogus = true })`, `buildl check` → `build.lua:3: c.binary(app): the option bogus is not known`, traceback `plugins/c/api.lua:24: in upvalue 'relocate'` … `build.lua:3: in main chunk`, `exit 5` |
| P63 | The plugin builds, tests and verifies end to end with the real binary, and a header edit rebuilds only the objects that read it | `build` → `6 built` (discovery), `10 built`; `out/hello` → `hello 42`; `out/cpp/app` → `cpp 42`; `test` → `tests: 1 passed`; a comment appended to `include/u.h` (read by three sources) → discovery `3 built, 3 cached`, build `3 built, 7 cached`; `c.platform`'s discovery printed `arm64-apple-darwin25.3.0` |

## 2. Core: `dir` for git plugins

### 2.1 The key

```toml
[plugins.c]
github = "airsstack/buildl"
rev    = "plugin-c-v0.1.0"
dir    = "plugins/c"
```

`dir` is optional and names a folder of the repository, relative to its root. It is read only with
`git` or `github`; beside `path` it is refused with "a path plugin has no dir", the way `rev` is
(P11). Its value is a `Directory` (P10), so `..`, a leading `/` and empty segments are refused when
`buildl.toml` is read. Absent, it is the root, and fetching behaves as today. `Directory`'s doc
comment, which calls it workspace-relative, is widened to "a relative folder; the holder says what
it is relative to".

| Piece | Change |
|---|---|
| `PluginTable` (crates/buildl/src/manifest/schema.rs:221) | `dir: Option<Spanned<String>>` |
| `source` / `pinned` (crates/buildl/src/manifest/interpret.rs:141-197) | parse `dir` into `Directory`; refuse it beside `path` |
| `PluginSource::Git` (P5) | `dir: Directory` |
| `LockEntry` (P7) | `dir: Directory`, always written, `""` for the root; a lock without it is refused like any other malformed lock |
| lock matching (P8) | `git`, `rev` and `dir` must all equal; a changed `dir` reads as "not pinned", and `buildl plugins fetch` re-fetches |
| `bring_in` (P6) | after `.git` is removed, when `dir` is not the root: walking `dir` one segment at a time from the staging folder, each step must be a folder and not a link, read with `symlink_metadata`, so no link in any parent folder is followed; otherwise `PluginNotFetched` with "`<rev>` fetched as `<commit>`, which has no folder `<dir>`". The digest is taken over `<staging>/<dir>`, which is renamed into the store; the rest of the staging folder is removed, best effort: the plugin is stored by then, and the fetch's prune removes a staging folder left behind (crates/buildl/src/plugins/fetch.rs:207-211, :280-282) |
| `check`, the Lua loader, `require` | unchanged: the stored folder is the plugin (P9) |
| `watch_layout` (crates/buildl/src/local_ports.rs:161) | unchanged: it watches only `path` plugins |

```text
 git fetch rev → checkout → rm .git                       fetch.rs:154-161, as today
        │
   dir is the root? ── yes ──► digest staging/          ──► rename staging → store   (as today)
        │ no
   every segment of dir a real folder, no link? ── no ──► PluginNotFetched "… has no folder <dir>"
        │ yes
   digest staging/<dir> ──► rename staging/<dir> → .buildl/plugins/<name>/<commit> ──► rm staging
   lock row {commit, dir, git, rev, sha256}
```

Two plugins taken from one repository at one commit are two names, so two store folders.

## 3. Core: `buildl.folder()`

`buildl.folder()` returns the folder of the build file being read, relative to the workspace root:
`""` for the root build file, `"cpp"` for `cpp/build.lua`. It takes no arguments.

- **Why.** Commands run from the workspace root and inputs are relative to the build file (P12), so
  a plugin writing a command for a source must know the folder. Learning it from discovery output
  would make each source's discovery preprocess every header (`$in` holds them all), and a discovery
  command may not change between the two passes (P28).
- **In a plugin** it returns the folder of the build file that called `buildl.use`: the plugin's
  table is built over that build file's results (P15).
- **Not ambient.** The folder is already part of every label the build file declares (`//cpp:x`).
- **Where.** One function added to `primitives_table` (crates/buildl-lua/src/module.rs:129),
  returning `results.directory()` as a string; `folder` then appears in the list of functions
  `buildl` holds (P35) and in the API appendix.

## 4. The `c` plugin: layout and manifest

```text
plugins/c/
├── extension.toml   name "c", version "0.1.0", entry "main.lua", api 1
├── main.lua         returns require("api").new(buildl)
├── api.lua          new(buildl): the public table — binary, library, shared_library, test and
│                    the helpers; checks a call's options, then passes them to graph.lua
├── graph.lua        declares the targets of one call through the `buildl` table it is given
├── options.lua      checks and fills a call's options; pure
├── depfile.lua      reads -MM output; pure
├── flags.lua        includes, defines, shared_name, soname, rpath, the platform of a triple; pure
├── README.md        the buildl.toml tables, the API, §5.4–§5.5, the fixed program list
└── tests/           depfile_test.lua, flags_test.lua, options_test.lua, graph_test.lua,
                     api_test.lua, fake_buildl.lua (airsl test)
```

`graph.lua` and `api.lua` take the `buildl` table as an argument, so their tests hand them a fake
(`fake_buildl.lua`) that records every declaration and serves canned discovery results; the other
modules use no `buildl` at all.

```toml
[capabilities]
proc.run = ["cc", "c++", "gcc", "g++", "clang", "clang++", "sh"]
env.read = ["PATH"]
```

- **Programs.** The manifest lists every program the plugin's targets run, since a program outside
  it is refused even when the ceiling allows it (P17); the workspace's cap and ceiling narrow it. A
  versioned or other compiler (`gcc-13`, `clang-18`) is not on the list; the README says so, and
  that a workspace needing one keeps a copy of the plugin as a `path` plugin with its own manifest.
  `sh` runs the archive step (§5.3) and a test binary that needs shared libraries beside it. `ar`
  runs inside the archive step's `sh`, found through `PATH`, so it needs no entry (P61); another
  archiver means editing `graph.lua` in such a copy.
- **Environment.** Every step that links (`c.binary`, `c.shared_library`, a test's binary) declares
  `env = { "PATH" }`: gcc's linker driver finds `ld` only through `PATH` (P52). The archive step
  declares it too, for `sh` to find `ar` (P61). Compiles and discovery declare nothing (P52).
  `PATH`'s value is then part of each link's and each archive's key.
- **`sh` widens the ceiling.** A workspace allows `sh` only when it has a `c.library` or a `c.test`
  with `deps`; once `sh` is allowed, a target can run any program through `sh -c` (P61 shows `ar`
  running that way with no entry of its own), so the ceiling's program list limits less. The README
  says so.
- **Network.** None.
- **Tags** are `plugin-c-vX.Y.Z` (P4).

A user's `buildl.toml`:

```toml
[ceiling]
proc.run = ["cc"]
env.read = ["PATH"]
fs.read  = ["."]
fs.write = ["out", ".buildl"]

[plugins.c]
github   = "airsstack/buildl"
rev      = "plugin-c-v0.1.0"
dir      = "plugins/c"
proc.run = ["cc"]
env.read = ["PATH"]
```

## 5. The plugin API

### 5.1 Options

Every function takes `(name, options)`. An option it does not know is refused, naming it.

| Option | Type | Meaning | Default |
|---|---|---|---|
| `srcs` | list of strings | sources, relative to the build file; `.c` is C, `.cc` `.cpp` `.cxx` are C++ | required, not empty |
| `headers` | list of strings | the headers a source may include, relative to the build file; every one is an input of every discovery | `{}` |
| `header_deps` | list of labels | targets whose outputs the compiles read, such as a generated `config.h` | `{}` |
| `cflags` | list of strings | arguments of C compiles and their discovery | `{}` |
| `cxxflags` | list of strings | arguments of C++ compiles and their discovery | `{}` |
| `ldflags` | list of strings | link arguments before the objects | `{}` |
| `ldlibs` | list of strings | link arguments after the objects, such as `-lm` (P47, P56) | `{}` |
| `deps` | list of labels | targets linked in, after the objects; for a test, also present when it runs | `{}` |
| `cc` | string | the C compiler | `"cc"` |
| `cxx` | string | the C++ compiler | `"c++"` |
| `linker` | string | the program that links | `cxx` when a source is C++, else `cc` |
| `output` | string | the file the last step writes | `name` (`c.test`: `name .. ".bin"`) |
| `version_probe` | boolean | declare the compiler-version targets of §5.2 | `false` |

`c.library` refuses `ldflags`, `ldlibs`, `linker` and `deps`: an archive is not linked, and a
library another library needs is given to the binary that links them both. Paths in `cflags`,
`cxxflags`, `ldflags` and `ldlibs` are written from the workspace root, as every command's are
(P12); `c.includes` writes include folders that way (§6).

### 5.2 What one call declares

For `c.binary("app", { srcs = { "src/main.c" }, headers = …, cflags = … })` in `cpp/build.lua`
(`buildl.folder()` is `"cpp"`, so the source's root path is `cpp/src/main.c`):

```text
 :app.version.c         only with version_probe = true; one per language the call compiles:
                        :app.version.c runs { cc, "--version" }; :app.version.cpp, for any C++
                        source (.cc, .cpp, .cxx), runs { cxx, "--version" } · stdout · always
 :app.src.main.c.deps   discover · inputs { "src/main.c", headers… } · deps { header_deps… }
                        run { cc, cflags…, "-MM", "-MT", "obj", "cpp/src/main.c" } · stdout
            │ second pass: c.depfile.parse, then §5.4
 :app.src.main.c.o      inputs { "src/main.c", the headers it read }
                        deps { header_deps…, :app.version.c if probed }
                        run { cc, cflags…, "-c", "-o", "$out", "cpp/src/main.c" }
 :app                   deps { the objects, in srcs order, then deps… } · env { "PATH" }
                        run { linker, ldflags…, "-o", "$out", "$deps", ldlibs… }
                        outputs { output }
```

A C++ source uses `cxx` and `cxxflags` in both its discovery and its compile. Object and discovery
names are `<name>.<source path with / as .>`, keeping the extension, so `main.c` and `main.cc` in
one folder do not collide (P38 shows `-MM`'s own names would); a source whose path gives a name
buildl refuses fails with buildl's own error.

The version probe exists because a target's key fingerprints the program file, and `/usr/bin/cc`
is a launcher that stays the same when the compiler behind it changes (P26). Its name carries the
language, `c` or `cpp`, never the program: a target name holds only ASCII letters, digits, `.`, `-`
and `_` (P59), so `c++`, `g++` and `clang++` cannot appear in one. A C++ object depends on
`<name>.version.cpp`, a C object on `<name>.version.c`. It is off by default:
an always-run target leaves every compile depending on it `Conditional` in `plan` (P21), though a
build still takes them from the cache (P22).

### 5.3 The functions

| Function | Last step | Notes |
|---|---|---|
| `c.binary(name, o)` | `linker <ldflags> -o $out $deps <ldlibs>`, `env { "PATH" }` | |
| `c.library(name, o)` | `sh -c 'ZERO_AR_DATE=1 exec ar rcs "$0" "$@"' $out $deps`, `env { "PATH" }` | `ZERO_AR_DATE` keeps macOS `ar` from writing the time into the archive; GNU `ar` gives the same bytes with or without it (P54, P60, P61). §5.1 lists what it refuses |
| `c.shared_library(name, o)` | `linker <ldflags> -shared -o $out $deps <ldlibs>`, `env { "PATH" }` | `-shared` is the one flag the plugin adds (P46); `-fPIC`, the soname and the file name are the engineer's |
| `c.test(name, o)` | a binary `:<name>.bin`, then `buildl.test(name)` | no `deps`: `run { "$deps" }` over `:<name>.bin`; with `deps`: `deps { ":<name>.bin", deps… }` and `run { "sh", "-c", 'exec "$0"', "$deps" }` |

A test lists in `deps` every shared library its binary loads, since the sandbox holds only direct
dependencies' outputs (P18); `$deps` keeps the binary first (P19), and `exec "$0"` runs it alone,
without the libraries as arguments (P20). The binary is a program a dependency built, so it needs
no `proc.run` entry (P30). Each function returns nothing.

### 5.4 Reading discovery output

`c.depfile.parse(text)` returns `{ { target = "obj", prerequisites = { … } }, … }`: it joins a `\`
and the newline after it, splits on whitespace not preceded by `\`, and turns `\ ` back into a
space (P39). The call keeps the one rule whose target is `obj`; its first prerequisite is the
source. Every other prerequisite is first normalised: `.` segments are dropped and each `..` removes
the segment before it (P41, P57); one that climbs above the workspace root is an error naming it.
Then it is placed by these rules, in order:

| Prerequisite | Becomes |
|---|---|
| absolute | an error naming it, saying a header outside the workspace belongs in an `-isystem` folder, whose headers `-MM` leaves out (P40) |
| inside the build file's folder and in `headers` (after taking off `<folder>/`) | an input of the object |
| anything else, when the call has `header_deps` | nothing: it is one of their outputs, and the compile depends on them already (P23) |
| anything else | an error naming it: "not among `headers`; a header outside `<folder>/` cannot be declared (one build file per project)" |

Under `sandbox`, a file the discovery read is one of its inputs or a dependency's output (P18), so
the third row holds only dependency outputs. Under `bare` nothing is confined: a header left out of
`headers` is read from the workspace and, with `header_deps`, dropped silently, as any undeclared
input is at that tier.

### 5.5 The two passes

Before discovery has run, a call declares its discovery targets and, with `version_probe`, its
probes; once every source of the call has a discovery result, it declares the objects and the last
step. The discovery targets must be the same on both passes (P28), so what they hold — `name`,
`srcs`, `headers`, `header_deps`, `cflags`, `cxxflags`, `cc`, `cxx` — must not depend on anything
discovered, such as `c.platform()`. `ldflags`, `ldlibs`, `deps`, `output` and `linker` reach only
the last step and may. A call that breaks this is refused by buildl with exit 5 (P28). §6's
platform helpers return a value that is safe on the first pass.

## 6. Helpers

Each returns a value and declares nothing, except `c.platform`. The engineer puts the value where
they want it.

| Helper | Returns |
|---|---|
| `c.includes(dirs)` | `{ "-I<folder>/<dir>", … }`, each folder relative to the build file; at the root, `-I<dir>` |
| `c.defines(map)` | sorted by key: `-D<key>` for `true`, nothing for `false`, `-D<key>=<value>` for a string or a number |
| `c.platform(options?)` | `"linux"` or `"macos"`, or `nil` before discovery has run. The first call in a build file declares the discovery target `:c.platform`, `run { cc, "-dumpmachine" }` with `cc` from `options.cc` or `"cc"`, `stdout = true`; later calls return the same value without declaring again (P25, P14), and a later call whose `cc` (default `"cc"`) differs from the first call's is an error, so `c.platform()` after `c.platform({ cc = "clang" })` is one. A triple holding `apple-darwin` is `"macos"`, one holding `linux` is `"linux"` (P45, P53), anything else is an error naming it |
| `c.shared_name(name, platform)` | `lib<name>.so` or `lib<name>.dylib`; `nil` when `platform` is `nil` |
| `c.soname(file, platform)` | linux `{ "-Wl,-soname,<file>" }`; macos `{ "-install_name", "@rpath/<file>" }` (P48, P49, P55); `{}` when `platform` is `nil` |
| `c.rpath(platform)` | linux `{ "-Wl,-rpath,$ORIGIN" }` (P24, P49, P55); macos `{ "-Wl,-rpath,@loader_path" }` (P50); `{}` when `platform` is `nil` |
| `c.depfile.parse(text)` | §5.4; for an engineer's own discovery target over a tool that writes Make rules |

`c.rpath` points at the binary's own folder: a shared library and the binaries using it are
declared by one build file, so their outputs share its folder of `out/`. A `platform` that is
neither `nil`, `"linux"` nor `"macos"` is an error. A `nil` result is safe on the first pass because
only the options of §5.5 that reach the last step may take it, and the last step is declared on the
second pass only.

## 7. Errors

The plugin raises a Lua error whose message starts with the function it came from:
`c.<kind>(<name>): …` for `c.binary`, `c.library`, `c.shared_library` and `c.test`, and
`c.includes: …`, `c.defines: …`, `c.platform: …`, `c.shared_name: …`, `c.soname: …` or
`c.rpath: …` for the helpers. A path in a discovery result that climbs above the root is reported
by the call that read it, `c.<kind>(<name>): …`. It raises the error at the level
of the build file's call, so buildl reports it with the build file's line — the line the user has
to fix — and keeps the plugin's frames in the traceback; the build file fails to load, exit 5
(P62). An error raised inside the plugin with no such level would carry the plugin's file and line
(P29); the plugin never tail-calls a function that can raise, since a tail call drops the frame
that level counts on.

| Case | Message holds |
|---|---|
| `srcs` missing or empty; an option of the wrong type; an option not in §5.1 or refused by §5.1; options that are not a table; a kind that is not one of the four | the option, or the kind |
| a source extension other than `.c`, `.cc`, `.cpp`, `.cxx` (case counts: `.C` is refused) | the source |
| an absolute prerequisite, one climbing above the root, one outside `headers` with no `header_deps` (§5.4) | the path |
| a discovery result with no `obj` rule | the source |
| a platform triple that is neither; a `platform` argument that is neither; a later `c.platform` call whose `cc` differs; an unknown `c.platform` option or a `cc` that is not a string | the value |
| `c.includes` given something that is not a list of strings | the value |
| `c.defines` given a key that is not a string, or a value that is not a boolean, string or number | the key |

When several options are wrong at once, the message names one the same way on every run: an
unknown option first (the first by name), then an option `c.library` refuses (in the order
`ldflags`, `ldlibs`, `linker`, `deps`), then an option of the wrong type (the first by name)
(plugins/c/options.lua:98, :102-104, :109-121; probe: `c.library("l", { …, deps = { ":x" },
ldflags = { "-x" } })` → `the option ldflags is refused`; `c.binary("b", { …, cc = 1, zzz = true })`
→ `the option zzz is not known`).

A compiler outside the plugin's manifest, the cap or the ceiling is refused by buildl, exit 4
(P17). A discovery target that differs between the passes is refused by buildl, exit 5 (P28).

## 8. Examples and CI

| Example | Language | Shows |
|---|---|---|
| `examples/c` | C | `c.library` and `c.binary` linking it; `c.test`; a generated `config.h` through `header_deps`; `c.includes`, `c.defines`; a data-only `src/blob.S`, which the plugin does not compile, built by the build file's own discovery target and object target with `c.depfile.parse` and linked through `deps` (P44, P57) |
| `examples/cpp` | C++ | `c.shared_library`, a `c.binary` using it, a `c.test` listing it in `deps`; `c.platform`, `c.shared_name`, `c.soname`, `c.rpath`, `-fPIC` in `cxxflags` |

`examples/cpp/build.lua` in outline, following §5.5:

```lua
local c = buildl.use("c")
local platform = c.platform()                    -- nil on the first pass
local lib = c.shared_name("greet", platform)     -- nil on the first pass

local cxxflags = c.includes({ "include" })
cxxflags[#cxxflags + 1] = "-fPIC"

c.shared_library("greet", {
  srcs = { "lib/greet.cc" }, headers = buildl.sources("include/*.h"), cxxflags = cxxflags,
  ldflags = c.soname(lib, platform), output = lib,
})
c.binary("hello", {
  srcs = { "src/main.cc" }, headers = buildl.sources("include/*.h"), cxxflags = cxxflags,
  ldflags = c.rpath(platform), deps = { ":greet" },
})
c.test("greet_test", {
  srcs = { "test/greet_test.cc" }, headers = buildl.sources("include/*.h"), cxxflags = cxxflags,
  ldflags = c.rpath(platform), deps = { ":greet" },
})
```

Each `buildl.toml` names the plugin as a user would: `github = "airsstack/buildl"`,
`rev = "plugin-c-v0.1.0"`, `dir = "plugins/c"`. Before that tag exists, and to test the plugin as
this checkout holds it, CI fetches it from a local repository:

```text
 examples/check.sh  ── copies the example (P31) ── runs prepare.sh first, when the example has one
   prepare.sh
     1. git init a temporary repository; copy the checkout's plugins/c into it at plugins/c
     2. commit and tag plugin-c-v0.1.0, with -c user.name=… -c user.email=… (P37)
     3. in the copy's buildl.toml: github = "airsstack/buildl" → git = "file://<that repository>"
     4. buildl plugins fetch
   then the shared steps as today: check · build · test · every target clean · verify · assert.sh
```

`check.sh` exports the repository root to `prepare.sh` as `BUILDL_REPO`. "Every target clean" holds
because the version probe is off by default (P21). Both examples join the existing examples job on
`ubuntu-latest` and `macos-latest`, whose images carry gcc, clang and git (P32, P58). Each
`assert.sh` checks the program's output, that editing a header recompiles only the sources
including it, and for `examples/cpp` that the test passes with the shared library loaded.

## 9. Documentation and skills

| File | Change |
|---|---|
| `book/src/ch08-04-the-c-plugin.md` (new), `book/src/SUMMARY.md` | the plugin: `buildl.toml`, the API and helpers, the two passes (§5.5), the one-build-file rule, the version probe and its `plan` cost, the fixed program list, `sh` for `c.library` and what allowing it costs (§4) |
| `book/src/ch07-00-discovery-targets.md`, *The depfile gap* | per-file C and C++ is written with discovery targets; the C plugin does it; no depfile is read by the executor |
| `book/src/ch11-09-what-it-promises.md`, *No per-file C and C++ in this version* | replaced: per-file C and C++ through the plugin, its limits (one build file per project, Linux and macOS, gcc and clang) and the launcher fingerprint |
| `book/src/ch01-04-with-an-ai-agent.md` (lines 161-183) | the migration of a per-file C/C++ build goes to the plugin |
| `book/src/ch11-08-where-buildl-sits.md` (lines 175, 183) | the mindmap's "Fine-grained C — per-file object graphs" leaves *Deferred by decision* for the covered branch, as the C plugin over discovery targets (kept in Mermaid); the sentence "with fine-grained C left out behind them" is rewritten to say discovery targets also carry per-file C and C++ |
| `book/src/ch08-02-plugins-from-git.md`, `book/src/appendix-02-buildl-toml.md` | the `dir` key, and `dir` in the lock row |
| `book/src/appendix-03-lua-api.md` | `folder()` in the function table, its section, and the list in the "buildl holds" error |
| `.claudestacks/sdlc/roadmaps/buildl/roadmap.md` | the non-goal on per-file C/C++ removed; the depfile non-goal kept |
| `CLAUDE.md` | the scope note on per-file C/C++ replaced by the plugin |
| `skills/buildl/SKILL.md` (line 152) | one target per toolchain, except C and C++, which use the `c` plugin |
| `skills/buildl/references/lua-api.md`, `skills/buildl/references/plugins.md`, `skills/buildl/references/buildl-toml.md` | `folder()`; `dir` in the `[plugins.<name>]` key table |
| `skills/buildl-migrate/SKILL.md` (§ B lines 101-105, § E *C and C++ compiled one file at a time*), `skills/buildl-migrate/references/make.md`, `just.md` (153-159), `npm.md` (139-142), `shell.md` (156-159) | a per-file C/C++ build migrates to the `c` plugin, one target per object, no stop rule. `"judge": "tests"` stays for C and C++ outputs whose bytes differ from the old build's, as with `-g`, which records the folder the compile ran in (P51); without `-g` the bytes match (P51, P57) |

## 10. Tests

| Layer | Where | What |
|---|---|---|
| unit | `crates/buildl/src/manifest/toml_manifest.rs` tests | `dir` read with `git` and `github`; refused beside `path`; `..` and a leading `/` refused |
| unit | `crates/buildl/src/plugins/lock.rs` tests | `dir` is written for every row, `""` at the root, and reads back; a row without it is refused; canonical bytes |
| unit | `crates/buildl/src/plugins/check.rs`, `fetch.rs` tests | a row whose `dir` differs does not match, and the message names the folder; `plugin_folder`'s walk: the root, a nested folder, a missing segment, a file where a folder is expected, a link in a parent and at the end, an unreadable parent |
| unit | `crates/buildl-lua/src/module.rs` tests | `folder()` gives `""` at the root (`folder_is_empty_at_the_root`, module.rs:523) |
| flows | `crates/buildl-lua/tests/flows/plugins.rs` | `folder()` names a subdirectory build file's folder, from the build file and from a plugin (`folder_names_a_subdirectory_build_file`, :582; `a_plugin_reads_the_calling_build_file_s_folder`, :592) |
| e2e | `crates/buildl/tests/e2e/fetch.rs` | a subfolder plugin is stored without the rest of the repository and digested alone; a missing folder, a folder that is a link and a folder under a linked parent are refused; a changed `dir` re-fetches and stores the new folder's bytes |
| cli | `crates/buildl-cli/tests/cli/exit_codes.rs` | a plugin fetched from a subfolder loads, `require`s its siblings and builds offline with no `git` on `PATH` (`a_plugin_fetched_from_a_subfolder_builds_offline`, :1290) |
| plugin | `plugins/c/tests/` via `airsl test`, in a new `cargo make plugins` task run by the CI job that has `airsl` | `depfile_test`: escapes, continuations, several rules, normalisation, a climb above the root, CRLF, a rule with no prerequisites; `flags_test`: every helper, `nil` platforms, refused keys and values; `options_test`: every refusal of §7, the sorted naming; `graph_test` with the fake `buildl`: first and second pass declarations, names, `env { "PATH" }` on links and the archive step, the archive command of §5.3, `version_probe = true` adding `:<name>.version.c` to each C compile's dependencies and `:<name>.version.cpp` to each C++ compile's, `header_deps`, the test runner forms; `api_test`: the public table, each function's last step, the helpers, `c.platform`'s one declaration and its `cc` rule, and that an error carries the calling file's line (P62) |
| end to end | `examples/c`, `examples/cpp` through `check.sh` | §8 |

## 11. Risks

| Risk | Handling |
|---|---|
| Every source's discovery reruns when any header in `headers` changes | each is one `-MM` pass of one source; only sources whose header list changed recompile (P1, P16) |
| A compiler update is not seen while the probe is off | the README and ch08-04 say so and name `version_probe = true` (P26) |
| `PATH` in each link's key makes links cache per machine `PATH` | the link is one action per binary or library; objects, the bulk of the work, carry no `PATH` (P52) |
| A test given its shared libraries in the wrong place fails | it fails loudly with the loader's message (P18) |
| Allowing `sh` lets any target run any program through `sh -c` (P61) | `sh` is needed only for `c.library` and a `c.test` with `deps`; the README's setup starts from `proc.run = ["cc"]` and says what allowing `sh` costs (§4) |
| An archive tool other than `ar` that stamps the time is not covered | `c.library` runs `ar` only; another archiver means a `path` copy of the plugin (§4) |
| The Linux probes ran on aarch64; CI runs x64 | the examples job on `ubuntu-latest` runs both examples there (§8) |
| The examples need `git` for `prepare.sh` and the fetch | both runner images carry Git (P58), and a fetch with no `git` fails with exit 3 (P36) |

## 12. Non-goals

- Windows and MSVC: buildl builds on Unix only (P33).
- Cross-compiling.
- Headers shared between build files in different folders: an input cannot leave its folder (P13).
- Wrapping CMake, autotools or meson; a plain target does that.
- A depfile read by the executor, or any other side channel into an action's inputs.
- A core fix for launcher fingerprints across stacks (rustup, pyenv, asdf shims).
- Plugins for other stacks, and plugins bundled into the binary.
- An `env` option on the plugin's functions; links declare `PATH` themselves (§4).
- Versioned or other compiler and archiver names in the manifest (§4).
