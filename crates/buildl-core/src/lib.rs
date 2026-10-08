//! Domain data, ports, and pure pipeline logic for the buildl build system.
//!
//! This crate is the centre buildl's other crates depend on. It depends on no other buildl
//! crate, on no Lua runtime, and on no filesystem, process, thread, network, environment or
//! clock API, so every pipeline flow can be exercised against in-memory implementations of its
//! ports.
//!
//! # Responsibilities
//!
//! - The domain values the pipeline phases hand to one another.
//! - The ports: traits through which the pipeline reaches build-file evaluation, storage,
//!   hashing, and execution.
//! - The pure logic of each phase: Load → Resolve → Plan → Execute → Record.
//!
//! # Non-responsibilities
//!
//! - Implementing any port. Concrete adapters live in the `buildl-lua` and `buildl` crates.
//! - Reading or writing anything. A value arrives from a caller or through a port.
//!
//! # Where things live
//!
//! | Module | Holds |
//! |---|---|
//! | [`error`] | the one error enum and the crate-wide `Result` alias |
//! | [`types`] | the validated domain values: names, identities, instants |
//! | [`json`] | the one canonical serializer every byte of output goes through |
//! | [`ports`] | the traits through which everything outside this crate is reached |
//!
//! A later phase of the pipeline gets its own module beside these, named for the phase. Five
//! rules keep that arrangement navigable as it grows:
//!
//! 1. No phase module names another. Phases communicate through values, and the pipeline owns
//!    the sequence, so a phase that imported the next would be bypassing it.
//! 2. [`types`] holds no decisions between *different* concepts. A type there may decide things
//!    about its own kind — [`Timestamp::duration_since`] decides whether two instants of the
//!    same type went backwards, [`Label::resolve`] decides which of three reference forms it was
//!    handed — but logic needing two different concepts to decide something belongs to the
//!    phase that decides it.
//! 3. One concept, one type. [`types`] is the only place a domain value is declared, so nothing
//!    downstream invents a second spelling of a name that already exists.
//! 4. [`ports`] never gains a production implementation. A real implementation means doing I/O,
//!    which this crate does not do; the only implementations here are test fakes.
//! 5. Every phase module is exercised against in-memory implementations of the ports it uses,
//!    with its tests in the file under test.
//!
//! # Status
//!
//! Pre-release. The vocabulary, the error model, the canonical serializer and the clock port
//! exist; no pipeline phase is implemented yet.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to
//! unit-test.

pub mod error;
pub mod json;
pub mod load;
pub mod ports;
pub mod types;

pub use error::{
    ActionFound, DeclarationField, Error, EvaluationFailure, EvaluationLimit, NameKind, Result,
};
pub use load::load;
pub use ports::{Clock, DeclarationSource, Ports};
pub use types::{
    Action, Alias, Argument, BuildFile, Command, Declaration, DeclarationOrder, Declared,
    Description, Diagnostic, Digest, Directory, EntryName, EnvName, Evaluated, FieldName,
    Freshness, Label, NetworkAccess, NodeId, OutputName, Provenance, Rule, Setting, SettingName,
    SettingValue, SourcePath, StagedAlias, StagedDeclaration, StagedFile, StagedItem, StagedRule,
    StagedSetting, StagedSubdir, StagedTarget, Target, TargetName, TargetRole, Timestamp, Written,
};
