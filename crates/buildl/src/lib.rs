//! buildl — a sandboxed, deterministic build system whose build files are
//! written in Lua and evaluated on the airsl embedded runtime.
//!
//! Build files *declare* targets; buildl schedules, sandboxes, parallelizes,
//! and caches their execution. The pipeline is
//! Load → Resolve → Plan → Execute → Record, with each phase handing the next
//! a serializable value.
//!
//! This crate is the composition root: its role is to choose a concrete
//! implementation for every port the `buildl-core` pipeline needs.
//!
//! # Responsibilities
//!
//! - The adapters for every port except `DeclarationSource`: `Manifest`,
//!   `Digester`, `StatCache`, `ToolResolver`, `ContentStore`, `ActionCache`,
//!   `EventLog`, `ExecStrategy`, `Dispatcher`, `Approver`, `Clock` and
//!   `Reporter`.
//! - Binding those adapters, together with the Lua adapter from `buildl-lua`
//!   (the `DeclarationSource`), to the `buildl-core` pipeline through
//!   `LocalPorts`, and exposing the result as the `Workspace` facade.
//! - Re-exporting `buildl-core`, so the binary reaches core types through this
//!   crate alone.
//!
//! # Non-responsibilities
//!
//! - Pipeline logic, which lives in `buildl-core`.
//! - Lua evaluation, which lives in `buildl-lua`.
//!
//! The crate does not implement any of this yet: this file declares no
//! modules and re-exports nothing, so there is no logic to unit-test.
