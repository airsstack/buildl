//! buildl — a sandboxed, deterministic build system whose build files are
//! written in Lua and evaluated on the airsl embedded runtime.
//!
//! Build files *declare* targets; this library schedules, sandboxes,
//! parallelizes, and caches their execution. The pipeline is
//! Load → Resolve → Plan → Execute → Record, with each phase handing the next
//! a serializable value.
//!
//! This crate is a scaffold: the design is specified in the repository's
//! `docs/design.md` and `docs/architecture.md`, and implementation has not
//! yet begun.
