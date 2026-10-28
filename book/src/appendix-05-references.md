# E. References

The works the design and architecture of `buildl` draw on, each listed once.

## The Lua runtime

1. airsstack. _airsl — Embeddable Lua runtime with a host standard library_ (source, its
   architecture, sandbox and extension documents, and its workspace lint and dependency policy).
   GitHub, 2026. <https://github.com/airsstack/airsl>
2. airsstack. _airsl crate_. crates.io, 2026. <https://crates.io/crates/airsl>
3. khvzak et al. _mlua — high-level Lua bindings for Rust_. crates.io.
   <https://crates.io/crates/mlua>

## Build systems

4. Andrey Mokhov, Neil Mitchell, Simon Peyton Jones. _Build Systems à la Carte_. Microsoft
   Research / ICFP, 2018.
   <https://www.microsoft.com/en-us/research/uploads/prod/2018/03/build-systems.pdf>
5. _Build System Fundamentals_. A study document, not published, drawing on Mokhov, Mitchell,
   Peyton Jones et al.
6. _Task Runner Fundamentals_. A study document, not published.
7. Evan Martin et al. _The Ninja build system — manual_ (depfiles, restat). ninja-build.org.
   <https://ninja-build.org/manual.html>
8. xmake-io. _Xmake — a cross-platform build utility based on Lua_. xmake.io.
   <https://xmake.io/>
9. Google / Bazel authors. _Hermeticity_. bazel.build. <https://bazel.build/basics/hermeticity>
10. Google / Bazel authors. _Sandboxing_. bazel.build. <https://bazel.build/docs/sandboxing>
11. Reproducible Builds project. _SOURCE_DATE_EPOCH specification_. reproducible-builds.org.
    <https://reproducible-builds.org/specs/source-date-epoch/>

## Remote cache and execution

12. Bazel authors. _remote-apis — An API for caching and execution of actions on a remote system_
    (the Digest and ActionCache model). GitHub. <https://github.com/bazelbuild/remote-apis>
13. Bazel authors. _remote-apis — `build/bazel/remote/execution/v2/remote_execution.proto` and
    `platform.md`, at commit `c2c07e3d718d3d490a4958344d9c3e47de0bfaf3`_. GitHub.
    <https://github.com/bazelbuild/remote-apis/tree/c2c07e3d718d3d490a4958344d9c3e47de0bfaf3>
14. Google. _googleapis — `google/bytestream/bytestream.proto` and the other protos the REAPI
    imports, at commit `dff1cd0c7ff8c26ff4c9f7a87cbe175fb5124d4e`_. GitHub.
    <https://github.com/googleapis/googleapis/tree/dff1cd0c7ff8c26ff4c9f7a87cbe175fb5124d4e>
15. BuildBuddy. _Bazel's Remote Caching and Remote Execution Explained_. buildbuddy.io.
    <https://www.buildbuddy.io/blog/bazels-remote-caching-and-remote-execution-explained/>
16. Buchgr et al. _bazel-remote — a remote cache for REAPI clients_. GitHub.
    <https://github.com/buchgr/bazel-remote>

## Rust libraries

17. petgraph developers. _petgraph — graph data structures and algorithms (toposort)_. docs.rs.
    <https://docs.rs/petgraph/>
18. rayon developers. _rayon — data parallelism with work stealing_. crates.io.
    <https://crates.io/crates/rayon>
19. notify-rs authors. _notify — cross-platform filesystem notification library for Rust_ (8.2.1:
    FSEvents on macOS, inotify on Linux). crates.io, 2026. <https://crates.io/crates/notify>

## Approvals

20. direnv authors. _direnv — `internal/cmd/rc.go`, the allow list under
    `$XDG_DATA_HOME/direnv/allow`_. GitHub.
    <https://github.com/direnv/direnv/blob/master/internal/cmd/rc.go>

## Software architecture

21. Simon Brown. _The C4 model for visualising software architecture_. c4model.com.
    <https://c4model.com/>
22. arc42. _Building block view_. <https://docs.arc42.org/section-5/>
23. Robert C. Martin. _The Dependency Inversion Principle_. C++ Report, 1996.
24. Alistair Cockburn. _Hexagonal architecture (ports and adapters)_.
    <https://alistair.cockburn.us/hexagonal-architecture/>
25. Rust library team. _Rust API Guidelines — Future proofing (C-SEALED)_.
    <https://rust-lang.github.io/api-guidelines/future-proofing.html>
26. Microsoft. _Pragmatic Rust Guidelines_. <https://microsoft.github.io/rust-guidelines/>
27. The Cargo team. _The Cargo Book — Publishing on crates.io_.
    <https://doc.rust-lang.org/cargo/reference/publishing.html>
