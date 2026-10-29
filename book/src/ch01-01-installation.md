# Installation

`buildl` is not on crates.io yet, and there are no prebuilt binaries. You build it from source with
Cargo, which takes several minutes.

## What you need

- **Rust 1.94 or newer.** That is the oldest compiler `buildl` supports, recorded as `rust-version`
  in its `Cargo.toml`. A checkout of the repository also carries a `rust-toolchain.toml` that asks
  for the current `stable` release, so inside a checkout `rustup` uses stable.
- **A C compiler**, such as `cc`, `gcc` or `clang`. `buildl` builds Lua from its C source and links
  it into the binary. That is also why the finished binary needs nothing else installed: Lua is
  already inside it.

## From GitHub

Cargo can build and install the binary straight from the repository:

```console
$ cargo install --git https://github.com/airsstack/buildl buildl-cli --locked
```

`buildl-cli` is the package that holds the `buildl` binary. `--locked` builds with the exact
dependency versions the repository records in its `Cargo.lock`. When it finishes, Cargo prints
where it put the binary, usually `~/.cargo/bin`, which `rustup` has already added to your `PATH`:

```console
   Installed package `buildl-cli v0.1.0 (https://github.com/airsstack/buildl#d99e72c9)` (executable `buildl`)
```

The commit after the `#` is whichever one the repository's default branch is at when you install.

## From a checkout

If you have cloned the repository, install from there with
[cargo-make](https://github.com/sagiegurari/cargo-make):

```console
$ cargo make install
```

That runs this, which you can also run yourself if you do not use cargo-make:

```console
$ cargo install --path crates/buildl-cli --locked --force
```

`--force` replaces a `buildl` you installed before, so running it again after a `git pull` gives
you the new version.

## Check that it worked

Ask `buildl` for its version:

```console
$ buildl --version
buildl 0.1.0
```

`buildl --help` lists every command. The rest of this book goes through them one at a time. To have
an AI agent write and migrate build files for you, see [With an AI Agent](ch01-04-with-an-ai-agent.md).
