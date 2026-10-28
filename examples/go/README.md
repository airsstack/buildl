# Go starter

A Go program and its test, built and run by buildl.

| Target | What it does |
|---|---|
| `//:hello` | `go build -o $out .`, giving `out/hello` |
| `//:unit` | `go test ./...`, a test target |

Both actions read `HOME`, because the Go toolchain keeps its build cache under it: run without
it, `go` stops because it finds no build cache directory (`GOCACHE is not defined and $HOME is not defined`, as macOS words it). `buildl.toml` lets the
build read `HOME` and run `go`, and nothing else.

## Run it

Needs the `buildl` binary and Go 1.22.

```sh
buildl build             # builds out/hello
./out/hello              # hello from go
buildl test              # runs //:unit
echo "// edit" >> main.go
buildl plan              # run    //:hello: input main.go changed
buildl test              # runs //:unit again: main.go is one of its inputs
buildl test              # tests: 1 passed (1 cached)
buildl log //:unit       # what go test printed when it last ran
```

From `examples/`, `sh check.sh go` runs these steps on a copy and checks each result (`assert.sh`).
