//! Pipeline flows through the build-file evaluation port, driven by fakes built from the public
//! API alone.
//!
//! One test binary, so the fakes are shared by every flow without being compiled once per file.
//! That the fakes compile at all is itself a check: an adapter outside this crate can build staged
//! targets, settings and subdirs and implement `DeclarationSource` the same way.

pub mod common;

mod load;
