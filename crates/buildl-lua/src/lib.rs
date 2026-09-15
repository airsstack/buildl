//! Lua build-file evaluation for buildl, on the airsl embedded runtime.
//!
//! This crate is the only part of buildl that depends on a Lua runtime, so
//! the effect of an airsl upgrade stays within one crate.
//!
//! # Responsibilities
//!
//! - Evaluating one build file per call inside an airsl sandbox and returning
//!   the declarations it staged.
//! - Installing the `buildl` module table, reachable from Lua both as
//!   `airsstack.buildl` and as the global `buildl`.
//!
//! # Non-responsibilities
//!
//! - Resolving, planning, scheduling, caching, or executing anything a build
//!   file declares.
//!
//! This file holds only module declarations and re-exports, so it carries no
//! logic to unit-test.
