//! The ports: the traits through which this crate reaches everything outside itself.
//!
//! Its own directory because the crate's defining property is that it names no I/O API. Every
//! capability this crate needs from the outside world arrives as an implementation of a trait
//! declared here, supplied by a caller, rather than through a direct call to an I/O API. Every
//! trait of the crate lives here, the bundle included, so no logic module declares one.
//!
//! Responsibilities:
//!
//! - [`Clock`] — reading the host wall clock.
//! - [`DeclarationSource`] — evaluating one build file.
//! - [`Ports`] — the bundle naming one implementation of each port, so a pipeline takes one type
//!   parameter instead of one per port.
//!
//! Non-responsibilities: production implementations. A real implementation does I/O, so it lives
//! in an adapter crate; the only implementations in this crate are test fakes.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

pub mod bundle;
pub mod clock;
pub mod declaration_source;

pub use bundle::Ports;
pub use clock::Clock;
pub use declaration_source::DeclarationSource;
