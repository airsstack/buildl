//! Domain data, ports, and pure pipeline logic for the buildl build system.
//!
//! This crate is the center buildl's other crates depend on. It depends on no
//! other buildl crate, on no Lua runtime, and on no filesystem, process,
//! thread, environment, or clock API, so every pipeline flow can be exercised
//! against in-memory implementations of its ports.
//!
//! # Responsibilities
//!
//! - The domain values the pipeline phases hand to one another.
//! - The ports: traits through which the pipeline reaches build-file
//!   evaluation, storage, hashing, and execution.
//! - The pure logic of each phase: Load → Resolve → Plan → Execute → Record.
//!
//! # Non-responsibilities
//!
//! - Implementing any port. Concrete adapters live in the `buildl-lua` and
//!   `buildl` crates.
//!
//! This file holds only module declarations and re-exports, so it carries no
//! logic to unit-test.
