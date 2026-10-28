# Getting Started

In this chapter you will install `buildl`, build a workspace with one target in it, and then ask
`buildl` what it did. There are three sections:

- [Installation](ch01-01-installation.md) builds the `buildl` binary and puts it on your `PATH`.
- [Hello, buildl](ch01-02-hello-buildl.md) writes a `buildl.toml` and a `build.lua` from nothing,
  and builds them.
- [What Did It Do?](ch01-03-what-did-it-do.md) asks `buildl` what would rebuild and why, what a
  target printed, and what the workspace allows.

By the end of the chapter you will have seen the whole loop: declare a target, build it, change
one of its files, and watch `buildl` rebuild exactly that.
