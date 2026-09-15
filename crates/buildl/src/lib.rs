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
//! - The filesystem, process, thread, and clock adapters for the pipeline's
//!   ports.
//! - Binding those adapters, together with the Lua adapter from `buildl-lua`,
//!   to the `buildl-core` pipeline.
//!
//! # Non-responsibilities
//!
//! - Pipeline logic, which lives in `buildl-core`.
//! - Lua evaluation, which lives in `buildl-lua`.
//!
//! This file holds only module declarations and re-exports, so it carries no
//! logic to unit-test.
