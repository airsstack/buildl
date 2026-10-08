//! The ports: the traits through which this crate reaches everything outside itself.
//!
//! Its own directory because the crate's defining property is that it names no I/O API. Every
//! capability this crate needs from the outside world arrives as an implementation of a trait
//! declared here, supplied by a caller, rather than through a direct call to an I/O API.
//!
//! Responsibilities:
//!
//! - [`Clock`] — reading the host wall clock.
//! - [`DeclarationSource`] — evaluating one build file.
//! - [`Ports`] — the bundle naming one implementation of each port.
//!
//! Non-responsibilities: implementations. A type implementing one of these traits does I/O by
//! definition, so it cannot live in this crate.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

pub mod bundle;
pub mod clock;
pub mod declaration_source;

pub use bundle::Ports;
pub use clock::Clock;
pub use declaration_source::DeclarationSource;
