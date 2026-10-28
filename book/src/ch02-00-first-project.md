# A First Project

The workspace in the last chapter sorted a list. This chapter builds and tests a real program: a
small Go command and its unit test. It is the Go starter that ships in the repository, in
`examples/go`, so you can copy that folder instead of typing the files out. You need the `buildl`
binary and Go 1.22 or newer.

## The files

The folder holds a Go module (`go.mod`), a program (`main.go`), its test (`main_test.go`), and the
two files `buildl` reads. `main.go` prints one line:

```go
package main

import "fmt"

// Greeting is the line the program prints.
func Greeting() string {
	return "hello from go"
}

func main() {
	fmt.Println(Greeting())
}
```

`buildl.toml` allows exactly what the build needs:

```toml
[workspace]
name   = "go"
entry  = "build.lua"
out    = "out"
config = 1

[ceiling]
proc.run = ["go"]
env.read = ["HOME"]
fs.read  = ["."]
fs.write = ["out", ".buildl"]
```

The `[workspace]` table spells out three settings the last chapter left at their defaults: the
build file to start from, the folder outputs go in, and the version of the config format. The
ceiling is new in one way: `env.read` lets the build see one environment variable, `HOME`. The Go
toolchain keeps its build cache under your home folder, and without `HOME` it stops, because it
cannot find one.

`build.lua` declares two targets:

```lua
local b = buildl

local sources = { "go.mod", b.sources("*.go") }

-- The Go toolchain keeps its build cache under $HOME, so both actions read HOME.
b.target("hello", {
  inputs = sources,
  run = { "go", "build", "-o", "$out", "." },
  env = { "HOME" },
})

b.test("unit", {
  inputs = sources,
  run = { "go", "test", "./..." },
  env = { "HOME" },
})
```

`b.sources("*.go")` lists the files in this folder that match the pattern, sorted by name. It is
the only way a build file can look at the disk, and `buildl` does the looking for it. A list inside
a list is flattened, so `sources` is `go.mod` followed by every `.go` file.

`hello` builds the program into `$out`, which is `out/hello`. `unit` is declared with `b.test`
instead of `b.target`: it makes no file, and what `buildl` keeps for it is whether it passed and
what it printed. Both name `HOME` in `env`. A command sees only the variables its target lists, so
a build cannot read the rest of your environment by accident.

Go is already good at deciding what to recompile. `buildl` does not try to do that job again: it
runs the whole `go build` as one action, and adds what Go lacks, such as a cache shared by every
language in the workspace, and a list of what the build may touch.

## Build and run

```console
$ buildl build
1 built
$ ./out/hello
hello from go
```

With no target named, `build` builds every target except the tests.

## Test

```console
$ buildl test
tests: 1 passed
```

With no target named, `test` runs every test, and builds whatever those tests depend on first.

## Change a file

Make a change to `main.go` that Go will not mind, and ask what a build would do:

```console
$ echo "// an edit" >> main.go
$ buildl plan
run    //:hello: input main.go changed
  run programs:   go
  read variables: HOME
  write into:     .buildl, out
1 to run, 0 conditional, 0 clean
```

`plan` names the file that changed. It lists only `//:hello`, because like `build` it leaves the
tests out when no target is named. The list under it now includes `read variables: HOME`, from the
targets' `env`.

`main.go` is an input of `unit` too, so the next test run runs it again. The run after that has
nothing new to test:

```console
$ buildl test
tests: 1 passed
$ buildl test
tests: 1 passed (1 cached)
```

A passing test whose inputs have not changed is not run again; its result comes from the cache.
When you want it run anyway, `buildl test --rerun` runs it.

## What the test printed

`buildl log` shows what the test printed the last time it actually ran:

```console
$ buildl log //:unit
//:unit: cached at 2026-10-28 13:25:24 UTC
ok  	example.com/hello	(cached)
```

The first line says the latest result came from `buildl`'s cache; the time on your machine will be
your own. The second is `go test`'s own output, from the run before. The `(cached)` on it is Go's
cache, not `buildl`'s: Go keeps test results under `HOME`, and found this one already there. That is
two caches working side by side. `buildl` decides whether `go` runs at all, and when it
does, Go decides how little it has to redo.

## The same project in Rust and Python

The repository has two more starters, `examples/rust` and `examples/python`. Each is the same
project: a program that prints a greeting, and a test, with the same steps in its `README.md`.

| Starter | The program | The test | The ceiling allows |
|---|---|---|---|
| `examples/go` | `go build -o $out .`, giving `out/hello` | `go test ./...` | `go`; `HOME` |
| `examples/rust` | `cargo install`, giving `out/hello/bin/hello` | `cargo test --target-dir tt` | `cargo`; `HOME`, `PATH` |
| `examples/python` | `python3 -m zipapp app -o $out`, giving `out/app.pyz` | `python3 -m unittest discover -s tests` | `python3`; no variables |

The Rust target is worth a closer look, because the obvious command does not work:

```lua
b.target("hello", {
  inputs = sources,
  run = { "cargo", "install", "--path", ".", "--root", "$out", "--target-dir", "tt", "--no-track" },
  env = { "HOME", "PATH" },
})
```

`buildl` looks for a target's output under `out/`, at the path `$out` names. `cargo build
--release` writes its binary under `target/` instead, so the output is never found.
`cargo install --root $out` puts the binary where `buildl` looks, at `out/hello/bin/hello`, which
makes this target's output a folder rather than a single file. `--target-dir tt` keeps cargo's
working files inside the folder the action runs in, and `--no-track` stops cargo writing its
install records into `$out`. Cargo needs `HOME` for its registry and toolchains, and `PATH` to find
the linker.

The next chapter looks at build files properly: labels, dependencies between targets, more than
one folder, and the functions `buildl` gives you to write them with.
