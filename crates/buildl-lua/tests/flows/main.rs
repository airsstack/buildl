//! Real build files evaluated through airsl by `LuaSource`, from the outside.
//!
//! Everything here uses only the public APIs of `buildl_lua` and `buildl_core`, as the composition
//! root will.

#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
)]

mod common;
mod declarations;
mod failures;
mod load;
mod sandbox;
