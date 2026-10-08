//! The `buildl` binary — the command-line entry point to the `buildl` library.
//!
//! All build logic lives in the library crates; this binary depends on the
//! `buildl` crate alone and reaches core types through its re-exports. Its own
//! job is the argument surface and the mapping of an outcome class to an exit
//! code.
//!
//! The binary does not implement any of this yet: `main` is empty and no
//! command is wired, so there is no logic to unit-test.

const fn main() {}
