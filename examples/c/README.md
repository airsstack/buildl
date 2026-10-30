# C

A C program built with the [`c` plugin](../../plugins/c/README.md): a static library, a binary
linking it, a test, a generated header and an assembly source the plugin does not compile.

```text
include/util.h ───────► lib/util.c ───────────► c.library //:util (libutil.a)
                                                      │ deps
include/util.h                                        ▼
include/banner.h ─────► src/main.c ───────────► c.binary //:hello
config.h (from sh)                                    ▲ deps
                                                      │
include/blob.h ───────► src/blob.S ─► discovery //:blob.deps ─► //:blob.o

include/util.h ───────► test/util_test.c ─────► c.test //:util_test   (deps //:util)
```

## What it uses

| Piece | Why |
|---|---|
| `c.library("util", …)` | each source is compiled to its own object, then `ar rcs` makes the archive |
| `c.binary("hello", …)` with `deps = { ":util", ":blob.o" }` | the library and the assembled object are linked in after the binary's own object |
| `c.test("util_test", …)` with `deps = { ":util" }` | a test binary, then a test target that runs it |
| `c.includes`, `c.defines` | `-Iinclude`, and `-DLEFT=2 -DRIGHT=3` in sorted order; `-Iout` is written by hand because it names an output |
| `header_deps = { ":config.h" }` | `config.h` is written by a target of the build file; the compile of `src/main.c` reads it as `out/config.h`, and `header_deps` makes that compile depend on the target and keeps `-MM`'s `out/config.h` from being refused as an unlisted header |
| the build file's own discovery target over `src/blob.S` | `blob.S` is not C, so the plugin leaves it alone; `cc -MM` names the headers it reads, and `c.depfile.parse` reads that output back into the inputs of `//:blob.o` |
| `include/blob.h` | the value, and `SYMBOL(name)`, which adds a leading `_` under `__APPLE__`: Mach-O C symbols carry one, ELF ones do not |

`include/banner.h` is read only by `src/main.c`. Add a comment to it and `buildl build` runs the
discovery targets again, since each lists the headers as inputs, and rebuilds that one object; the
link finds the same bytes and is taken from the cache.

## The `buildl.toml`

```toml
[ceiling]
proc.run = ["cc", "sh"]
env.read = ["PATH"]
fs.read  = ["."]
fs.write = ["out", ".buildl"]

[plugins.c]
github   = "airsstack/buildl"
rev      = "plugin-c-v0.1.0"
dir      = "plugins/c"
proc.run = ["cc", "sh"]
env.read = ["PATH"]
```

`cc` compiles and links. `sh` is there for two reasons: `c.library` runs `ar` through it
(`sh -c 'ZERO_AR_DATE=1 exec ar rcs …'`, so `ar` needs no entry of its own, and the archive is the
same bytes on macOS), and the build file's `config.h` target writes the header with it.
`c.test("util_test", …)` has `deps`, so it runs its binary through `sh` as well. Allowing `sh` lets a target run any program through `sh -c`, so the
ceiling's program list limits less.

## Run it

Needs the `buildl` binary, a C compiler (`cc`) and `git`. With the published tag:

```sh
buildl plugins fetch     # clones the repository at plugin-c-v0.1.0, keeps plugins/c, writes buildl.lock
buildl build             # config.h, the discovery targets, then objects, libutil.a, hello
./out/hello              # [c example] hello from buildl: util_add(2, 3) = 5, blob = 42
buildl test              # tests: 1 passed
```

From `examples/`, `sh check.sh c` runs the shared checks on a copy. Before the tag exists, and to
run the plugin as this checkout holds it, the copy first runs `prepare.sh`: it commits this
checkout's `plugins/c` to a temporary git repository, tags it `plugin-c-v0.1.0`, points the copy's
`buildl.toml` at that repository with `git = "file://…"` instead of `github`, and fetches. Then
`assert.sh` checks the program's output, that a comment added to `include/banner.h` rebuilds only
the object of `src/main.c`, and that the test passes.
