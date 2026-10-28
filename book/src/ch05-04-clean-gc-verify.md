# `clean`, `gc` and `verify`

Three commands look after what the cache holds. `clean` throws away the copies in `out/`, `gc`
throws away what no target needs any more, and `verify` checks that what the cache holds is what
the actions really make.

## `buildl clean`

`clean` removes the output directory and nothing else:

```console
$ ls out
hello
$ buildl clean
clean: removed out
$ ls
assert.sh
build.lua
buildl.toml
go.mod
main_test.go
main.go
README.md
$ buildl build
1 cached
$ ls out
hello
```

The cache rows and the store under `.buildl/` are untouched, so the next build runs nothing. It
puts every output back from the store, and says `cached`. `out/` is only ever a copy.

## `buildl gc`

The store keeps every output it was given. Over time it fills with outputs of targets that were
renamed, removed, or rebuilt to something new. `gc` drops the cache rows of targets that are no
longer declared, then removes every stored file that no remaining row and no target's last run
needs.

Rename `hello` to `greeter` in `build.lua`, build, and collect:

```console
$ sed -i '' 's/"hello"/"greeter"/' build.lua
$ buildl build
1 built
$ buildl gc
gc: dropped 1 row, removed 4 blobs (4059395 bytes)
$ buildl gc
gc: dropped 0 rows, removed 0 blobs (0 bytes)
```

`//:hello` is gone from the build files, so its row was dropped and what only it needed was
removed. A *blob* is one stored file. A second `gc` finds nothing left to do. `gc` reads the build
files first, so run it from a checkout whose build files declare everything you want to keep.

## `buildl verify`

The action key promises that the same inputs give the same key, and so a cache hit. It cannot promise that the
command gives the same bytes. A tool that writes the time, a random number or the order a hash
table happened to iterate in will not. If such an output reaches the cache, every later build
trusts it.

`verify` checks. It plans as `build` does, takes the targets that are cached, puts their outputs
back, runs their actions again in [the sandbox](ch06-00-sandbox.md), and compares each new output
with the one the cache holds:

```console
$ buildl verify
verified //:hello
1 verified, 0 mismatched, 0 failed, 0 skipped, 0 not cached, 0 not sampled
```

Go writes the same binary every time, so it passes. To see a failure, here is a small workspace
with two targets: `copy.txt` copies a file, and `stamp.txt` writes the time it ran.

```lua
-- build.lua
local b = buildl

b.target("copy.txt", {
  inputs = { "copy.py", "notes.txt" },
  run = { "python3", "$in", "$out" },
})

b.target("stamp.txt", {
  inputs = { "stamp.py" },
  run = { "python3", "$in", "$out" },
})
```

```python
# stamp.py
import sys, time

with open(sys.argv[1], "w") as f:
    f.write(f"built at {time.time()}\n")
```

```console
$ buildl build
2 built
$ buildl verify
verified //:copy.txt
MISMATCH //:stamp.txt: output stamp.txt differs: bytes
1 verified, 1 mismatched, 0 failed, 0 skipped, 0 not cached, 0 not sampled
```

`verify` names the target, the output and the first thing that differs, and exits with code 1.
It repairs nothing and records nothing: no cache row is written or forgotten, nothing goes in the
log, and the new bytes never reach `out/`. The build after it is served from the cache as before:

```console
$ buildl build
2 cached
```

The summary counts:

| Count | Meaning |
|---|---|
| verified | ran again and gave the same output |
| mismatched | ran again and gave a different output |
| failed | the second run failed |
| skipped | depends on a target that is not cached, so it could not be run |
| not cached | not clean in the plan, or its stored output is gone |
| not sampled | left out by `--sample` |

## Checking a sample

Running every action again costs as much as a full build. `--sample N` runs only `N` of them:

```console
$ buildl verify --sample 1
MISMATCH //:stamp.txt: output stamp.txt differs: bytes
0 verified, 1 mismatched, 0 failed, 0 skipped, 0 not cached, 1 not sampled
```

The sample is not random. It takes the `N` targets whose keys sort lowest, so the same keys pick the
same targets on every run and every machine, and a target moves in or out of the sample only when
its key changes. With no targets named, `verify` takes every target but the tests; name targets to
check those.

`verify` runs actions only in the sandbox. A workspace whose `buildl.toml` says
`strategy = "bare"` is refused, and [the next chapter](ch06-00-sandbox.md) shows why that matters.
