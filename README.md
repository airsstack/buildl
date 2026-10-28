# buildl

**A sandboxed, deterministic build system whose build files are written in Lua and evaluated on
the [airsl](https://github.com/airsstack/airsl) embedded runtime.**

Build files only declare targets: what each reads, what it makes, which command makes it. The
`buildl` program, written in Rust, then works out what is out of date, runs those commands in
parallel, each in a sandbox, and caches what they produce. The rule underneath is airsl's:
**nothing is ambient**. A build cannot run a program, read a file or see an environment variable
unless it was declared and the workspace allowed it. buildl is pre-release.

- **Build files are safe to read:** evaluating them grants them nothing.
- **Rebuilds you can explain:** `buildl plan` tells you what would rebuild, and why.
- **Authority you can see:** what a build may touch is checked against a limit before anything runs.

## A taste

`build.lua`, from [`examples/python`](examples/python):

```lua
local b = buildl

local app = b.sources("app/*.py")

-- zipapp packs the app directory into one runnable archive, out/app.pyz.
b.target("app.pyz", {
  inputs = { app },
  run = { "python3", "-m", "zipapp", "app", "-o", "$out" },
})

b.test("unit", {
  inputs = { app, b.sources("tests/*.py") },
  run = { "python3", "-m", "unittest", "discover", "-s", "tests" },
})
```

Its `buildl.toml` sets the workspace's
[ceiling](https://airsstack.github.io/buildl/ch04-02-the-ceiling.html): here, `python3` is the
one program any build may run.

## Install

buildl runs on Linux and macOS.

```sh
cargo install --git https://github.com/airsstack/buildl buildl-cli --locked   # from GitHub
cargo make install                                                            # from a checkout
```

Then `buildl build`, `buildl test` and `buildl plan` in that example.

## Learn more

**[The buildl Book](https://airsstack.github.io/buildl/)** is the full guide.

| | |
|---|---|
| Start | [Getting started](https://airsstack.github.io/buildl/ch01-00-getting-started.html) · [Build files](https://airsstack.github.io/buildl/ch03-00-build-files.html) |
| Concepts | [Permission](https://airsstack.github.io/buildl/ch04-00-asking-permission.html) · [Caching](https://airsstack.github.io/buildl/ch05-00-caching.html) · [Sandbox](https://airsstack.github.io/buildl/ch06-00-sandbox.html) |
| Features | [Plugins](https://airsstack.github.io/buildl/ch08-00-plugins.html) · [Remote cache and execution](https://airsstack.github.io/buildl/ch09-00-remote.html) · [Watch mode](https://airsstack.github.io/buildl/ch10-00-watch.html) |
| Reference | [Command line](https://airsstack.github.io/buildl/appendix-01-cli.html) · [Exit codes](https://airsstack.github.io/buildl/appendix-04-exit-codes.html) · [Under the hood](https://airsstack.github.io/buildl/ch11-00-under-the-hood.html) |
| Examples | [`examples/`](examples/): working workspaces for Go, Rust and Python |

## Crates

| Crate | Holds |
|---|---|
| `buildl-core` | domain data, ports and the pure logic of every phase: no I/O, no Lua |
| `buildl-lua` | build-file evaluation on airsl |
| `buildl` | the remaining adapters and the composition root: the complete framework |
| `buildl-cli` | the `buildl` binary |

## License

Apache-2.0. buildl is part of the [airsstack](https://github.com/airsstack) ecosystem.
