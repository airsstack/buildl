# Tests

A test target is declared with `buildl.test` instead of `buildl.target`. It has inputs and a
command like any target, but no outputs: what it produces is a pass or a fail. The Go project
declares one:

```lua
b.test("unit", {
  inputs = sources,
  run = { "go", "test", "./..." },
  env = { "HOME" },
})
```

`buildl build` leaves test targets out. `buildl test` runs them, and builds whatever they depend on
first, as `build` would. With no targets named it runs every test; name one, and it runs just that
one. Naming a target that is not a test is a usage error.

## A test is cached like anything else

A test target has an action key, worked out exactly as a build target's is. If nothing in the key
changed since the test last passed, the test is not run again:

```console
$ buildl test
tests: 1 passed
$ buildl test
tests: 1 passed (1 cached)
```

The last line always has the same shape: `tests: N passed`, with how many of those came from the
cache in brackets, and the failures after it. The second time, `go test` was never started:
nothing it could see had changed.

When you want the tests run anyway, say so:

```console
$ buildl test --rerun
tests (rerun): 1 passed
```

## A failing test

Break the greeting the test checks:

```console
$ sed -i '' 's/hello from go/hello, tests/' main.go
$ buildl test
--- //:unit
--- FAIL: TestGreeting (0.00s)
    main_test.go:7: Greeting() = "hello, tests"
FAIL
FAIL	example.com/hello	0.532s
FAIL
FAILED //:unit: action failure (exit status 1)
    declared in build.lua
    command: go test ./...
tests: 1 failed
```

`buildl` shows what the failing test printed, names the target, the build file that declared it
and the command, and exits with code 1. A passing test prints nothing unless you ask with
`--verbose`. The time `go test` reports will be different on your machine.

A failure is never cached. Put the line back:

```console
$ sed -i '' 's/hello, tests/hello from go/' main.go
$ buildl test
tests: 1 passed (1 cached)
```

The failed run recorded nothing in the cache. `main.go` is back to the bytes it held when the test
last passed, so the key is that run's key again, and the pass recorded then still stands.

## Other ways to run them

| Flag | What it does |
|---|---|
| `--rerun` | run the selected tests even when nothing they depend on changed |
| `--verbose` | show what every test printed, not only the failing ones |
| `--stop` | start nothing new after the first failure |
| `--stream` | show one running action's output as it is printed |

`buildl log //:unit` shows what the test printed the last time it ran, cached or not.
