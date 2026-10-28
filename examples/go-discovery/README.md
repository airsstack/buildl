# Go discovery

A Go module with several packages. The build file does not list them: it asks `go list`, and
declares one test target for each package it reports.

```text
pass 1   build.lua declares //:pkgs only ─► buildl runs go list -json=ImportPath ./...
                                                   │ stdout = true
                                                   ▼
                                              out/pkgs.json
pass 2   build.lua reads it back ─► buildl.decode_all ─► one buildl.test per package
                                                         //:greet  //:mathx  //:text.words
```

## What it uses

| Piece | Why |
|---|---|
| a discovery target, `buildl.discover("pkgs", …)` | a cached action whose output a second declaration pass reads; on the first pass `buildl.discovered(":pkgs")` is `nil`, and the build file declares only the discovery target |
| `stdout = true` | `go list` prints what it found to stdout; with it, what the command prints becomes the target's output |
| `buildl.decode_all` | `go list -json` prints one JSON object per package, back to back, which is not one JSON value: `airsstack.json.decode` rejects it, `decode_all` returns the list of values |
| `-json=ImportPath` | the full objects also hold absolute paths (`Dir`, `Root`, `Module.Dir`) naming the action's directory, so the output would differ from one checkout or machine to another |

A target name holds no `/`, so the package `text/words` gets the test target `//:text.words`.
Adding a package directory adds a test target on the next build: the discovery target's inputs
are every `.go` file, so `go list` runs again.

## Run it

Needs the `buildl` binary and Go 1.22.

```sh
buildl build             # runs go list; the build file then declares the tests
buildl graph             # //:pkgs and one test target per package
buildl test              # tests: 3 passed
```

From `examples/`, `sh check.sh go-discovery` runs the shared checks on a copy, then
`assert.sh`: the graph holds one test target per directory holding Go files, and each passes.
