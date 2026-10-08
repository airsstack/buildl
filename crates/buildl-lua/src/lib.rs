//! Lua build-file evaluation for buildl, on the airsl embedded runtime.
//!
//! This crate is the only part of buildl that depends on a Lua runtime, so the effect of an
//! airsl upgrade stays within one crate. It implements `buildl-core`'s [`DeclarationSource`]
//! port as [`LuaSource`].
//!
//! [`DeclarationSource`]: buildl_core::DeclarationSource
//!
//! # Responsibilities
//!
//! - Evaluating one build file per call on a fresh airsl engine, and returning what it staged
//!   exactly as written, or the one failure that stopped it.
//! - Installing the `buildl` module table, reachable from Lua both as `airsstack.buildl` and as
//!   the global `buildl`: `target`, `test`, `rule`, `alias`, `option`, `subdir` and `sources`.
//! - Keeping the declaration phase sandboxed: airsl's minimal language surface, no grant of any
//!   kind, only the `json`, `path`, `regex`, `hash` and `glob` modules, and no `math.random`,
//!   `math.randomseed`, `print` or `airsstack.path.absolute`. `buildl.sources` is the only
//!   filesystem read, and it walks only the declaring directory.
//! - Bounding a build file's memory and instructions: the ceilings of [`DeclarationLimits`], and
//!   a byte budget, equal to the memory ceiling, on what one file may stage. The host-side
//!   `buildl.sources` walk and wall time are outside both ceilings.
//!
//! # Non-responsibilities
//!
//! - Deciding which build files to evaluate, validating names, resolving labels or joining
//!   paths. The caller does all of that with what a file staged.
//! - Resolving, planning, scheduling, caching, or executing anything a build file declares.
//!
//! # Where things live
//!
//! | Module | Holds |
//! |---|---|
//! | `source` | [`LuaSource`]: read, evaluate, report |
//! | `limits` | [`DeclarationLimits`] |
//! | `engine` | the airsl engine configuration |
//! | `curated` | airsl's `path` module without `absolute` |
//! | `module` | the `buildl` host module, which raises refusals |
//! | `primitives` | each primitive's argument shape |
//! | `values` | the Lua value readers every primitive shares |
//! | `refusal` | why a primitive call was refused |
//! | `sources` | the host-side walk behind `buildl.sources` |
//! | `staging` | the staging buffer: order, byte budget, first refusal |
//! | `classify` | airsl failures named as `EvaluationFailure` kinds |
//!
//! This file holds only module declarations and re-exports, so it carries no logic to
//! unit-test.

mod classify;
mod curated;
mod engine;
mod limits;
mod module;
mod primitives;
mod refusal;
mod source;
mod sources;
mod staging;
mod values;

pub use limits::DeclarationLimits;
pub use source::LuaSource;
