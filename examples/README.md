# Examples

Each directory here is a buildl workspace of its own: a `buildl.toml`, its build files, its
sources, a `README.md` saying what it shows, and an `assert.sh` holding its own checks. Copy one
anywhere and run `buildl build` in it; it needs the `buildl` binary and its language's
toolchain, nothing else.

| Example | Shows | Needs |
|---|---|---|
| [`go`](go/) | a Go binary and its test; `plan` naming an edited input; a second `test` served from the cache; `buildl log` | Go 1.22 |
| [`rust`](rust/) | the same for a Rust binary, built with `cargo install --root $out` | a Rust toolchain |
| [`python`](python/) | the same for a Python zipapp tested with `unittest`, standard library only | Python 3 |
| [`go-discovery`](go-discovery/) | a discovery target running `go list` with `stdout = true`, read back with `buildl.decode_all`: one test target per Go package, none written by hand | Go 1.22 |
| [`polyglot`](polyglot/) | a Go service, a Rust CLI and a Python tool built from one generated schema; labels across directories; early cutoff after a comment edit; `buildl query 'rdeps(…)'` | Go 1.22, a Rust toolchain, Python 3 |
| [`plugins`](plugins/) | two Go programs declared through a plugin kept in the workspace, `buildl.use`; its manifest and the workspace's cap; `plan` and `doctor` naming the plugin | Go 1.22 |
| [`remote`](remote/) | the Python starter run on a NativeLink server it starts with `docker compose`: `--remote-execute`, a fresh copy fetching the result, `--remote-upload`; not run by `check.sh` | Linux x86_64, Python 3, Docker |

## Checking an example

```sh
BUILDL=/path/to/buildl sh examples/check.sh examples/go
```

`check.sh` copies the example to a temporary directory and runs, stopping at the first that
fails:

```text
1 buildl check
2 buildl build, then buildl test
3 buildl plan --json         every target's verdict is Clean
4 buildl verify              exits 0 and reports 0 mismatched
5 sh assert.sh               the example's own checks
```

It reads `plan --json` with `python3`, so it needs Python 3 besides the example's own
toolchain. `buildl` is `$BUILDL` when set, else the first `buildl` on `PATH`.

From the repository root, `cargo make examples` builds `buildl` and checks every example.
CI runs it on Linux and macOS.
