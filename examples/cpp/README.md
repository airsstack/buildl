# C++

A shared C++ library, a program that loads it and a test that loads it, all built with the `c`
plugin. Each source is compiled to its own object, so a change to one header rebuilds only the
objects whose sources read it.

```text
include/greet.h   ◄── lib/greet.cc ──► libgreet.so / libgreet.dylib   (c.shared_library)
                  ◄── src/main.cc  ──► hello                          (c.binary, deps :greet)
                  ◄── test/greet_test.cc ──► //:greet_test            (c.test,   deps :greet)
include/banner.h  ◄── src/main.cc only
```

## What it uses

| Piece | Why |
|---|---|
| `buildl.use("c")` | loads the plugin named in `buildl.toml`, fetched from git with `buildl plugins fetch` |
| `c.shared_library` | links `lib/greet.cc` with `-shared`; `-fPIC` in `cxxflags`, the library's name in `output` |
| `c.platform`, `c.shared_name` | the library is `libgreet.so` on Linux and `libgreet.dylib` on macOS |
| `c.soname` | the library's runtime name: `-Wl,-soname,…` on Linux, `-install_name @rpath/…` on macOS, which needs it |
| `c.rpath` | the binary and the test look for the library in their own folder |
| `c.test` with `deps` | the test binary runs with the library beside it |

## Two passes

The plugin learns which headers a source reads by running the compiler with `-MM`, so a call
declares its targets in two passes:

```text
pass 1   build.lua declares  //:c.platform  and one  .deps  target per source
                   │ buildl runs them: cc -dumpmachine, c++ -MM …
                   ▼
pass 2   build.lua runs again; the objects and the links are declared from what they printed
```

`c.platform()` is `"macos"` or `"linux"`, worked out from the output of `cc -dumpmachine`, which
has not run on the first pass, so it is `nil` there; `c.shared_name`, `c.soname` and `c.rpath` return `nil` or `{}` for it. That is safe
because the library's name and flags reach only the last step of each call, which is declared on the
second pass. The discovery targets must be the same on both passes, so nothing they hold, `cxxflags`
here, may depend on the platform.

`include/banner.h` is read only by `src/main.cc`. A comment added to it runs the discovery targets
again, since each `.deps` target has the headers as inputs (`//:c.platform` does not, and stays
cached), and compiles `src/main.cc` again; the object is the
same bytes, so the link and everything else stay cached.

## Why the test lists the library

A test action runs in the sandbox, which holds the outputs of the targets it names and nothing else.
The test binary needs `libgreet` next to it, so `deps = { ":greet" }` puts the library there, and
`c.rpath` makes the binary look in its own folder. With `deps` the plugin runs the binary through
`sh -c 'exec "$0"'`, which is why `buildl.toml` allows `sh` as well as `cc` and `c++`. Allowing `sh`
lets a target run any program through it, so the ceiling's program list limits less.

## `buildl.toml`

```toml
[ceiling]
proc.run = ["cc", "c++", "sh"]
env.read = ["PATH"]
fs.read  = ["."]
fs.write = ["out", ".buildl"]

[plugins.c]
github   = "airsstack/buildl"
rev      = "plugin-c-v0.1.0"
dir      = "plugins/c"
proc.run = ["cc", "c++", "sh"]
env.read = ["PATH"]
```

`cc` runs `-dumpmachine` for `c.platform`; `c++` compiles and links. `PATH` is read because the
compiler driver finds the linker through it.

## Run it

Needs the `buildl` binary, a C compiler (`cc`) and a C++ compiler (`c++`), both gcc or clang, and `git`.

```sh
buildl plugins fetch     # fetches the plugin at plugin-c-v0.1.0 into buildl.lock and the store
buildl build             # out/hello and out/libgreet.so (out/libgreet.dylib on macOS)
buildl test              # tests: 1 passed
./out/hello
```

The tag `plugin-c-v0.1.0` is for a published plugin. From `examples/`, `sh check.sh cpp` runs the
shared checks on a copy, after `prepare.sh` has made a git repository from this checkout's
`plugins/c`, tagged it and pointed the copy's `buildl.toml` at it; then `assert.sh` checks the
output, the library's file name, that editing `banner.h` builds one target, and the test.
