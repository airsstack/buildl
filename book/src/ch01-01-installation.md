# Installation

`buildl` ships as a single prebuilt binary for macOS and Linux. You do not need Rust, Lua or a C
compiler to run it.

## macOS and Linux

```console
$ curl -LsSf https://airsstack.github.io/buildl/install.sh | sh
```

The installer puts `buildl` in `~/.local/bin` and adds that folder to your `PATH`. Open a new
terminal so the change takes effect, then check it worked:

```console
$ buildl --version
buildl 0.1.0
```

`buildl --help` lists every command. If you would rather the installer left your `PATH` alone, set
`BUILDL_CLI_NO_MODIFY_PATH=1` for the `sh` that runs it:

```console
$ curl -LsSf https://airsstack.github.io/buildl/install.sh | BUILDL_CLI_NO_MODIFY_PATH=1 sh
```

That command always installs the newest release. To install a particular one instead, name it in
`BUILDL_VERSION`:

```console
$ curl -LsSf https://airsstack.github.io/buildl/install.sh | BUILDL_VERSION=0.1.0 sh
```

## From crates.io

If you already have a Rust toolchain, you can build `buildl` yourself:

```console
$ cargo install --locked buildl-cli
```

`buildl-cli` is the package that holds the `buildl` binary. This takes several minutes and needs:

- **Rust 1.94 or newer.** That is the oldest compiler `buildl` supports, recorded as `rust-version`
  in its `Cargo.toml`.
- **A C compiler**, such as `cc`, `gcc` or `clang`. `buildl` builds Lua from its C source and links
  it into the binary. That is also why the prebuilt binary needs nothing else installed: Lua is
  already inside it.

`--locked` builds with the exact dependency versions the crate records in its `Cargo.lock`. When it
finishes, Cargo prints where it put the binary, usually `~/.cargo/bin`, which `rustup` has already
added to your `PATH`.

## From GitHub

To build the newest unreleased code, Cargo can install the binary straight from the repository:

```console
$ cargo install --git https://github.com/airsstack/buildl buildl-cli --locked
```

This needs the same Rust and C compiler as above. Cargo prints where it put the binary:

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

A checkout also carries a `rust-toolchain.toml` that asks for the current `stable` release, so
inside a checkout `rustup` uses stable.

## Other ways to install

Each [release](https://github.com/airsstack/buildl/releases) also carries a plain archive for
macOS and for Linux on both x86_64 and aarch64, named `buildl-cli-<target>.tar.xz`. On Linux there
are two builds of each, one for glibc and one for musl, which is fully static and runs on any
distribution, including Alpine. A `.sha256` file sits next to every archive. The installer does not fetch it: it checks the
archive against a checksum built into the installer itself, which is the same value.

Every archive also carries a GitHub artifact attestation, a signed record that ties the file to this
repository and the build that made it. That catches even an archive swapped together with its
checksum file:

```console
$ gh attestation verify buildl-cli-x86_64-unknown-linux-musl.tar.xz --repo airsstack/buildl
```

To have an AI agent write and migrate build files for you, see [With an AI Agent](ch01-04-with-an-ai-agent.md).
