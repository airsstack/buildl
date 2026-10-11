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
//! - [`Manifest`] — what a workspace's manifest says.
//! - [`Host`] — the platform actions run on, and the values of environment variables.
//! - [`Digester`] — a file's metadata and content digest.
//! - [`StatCache`] — the digests of files already hashed.
//! - [`ToolResolver`] — what identifies the program a command names.
//! - [`ActionCache`] — what the previous build recorded for a target.
//! - [`Ports`] — the bundle naming one implementation of each port, so a pipeline takes one type
//!   parameter instead of one per port.
//! - [`PortSet`] — one instance of each port a bundle names, so a pipeline is built from one
//!   value.
//!
//! Non-responsibilities: production implementations. A real implementation does I/O, so it lives
//! in an adapter crate; the only implementations in this crate are test fakes.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

pub mod action_cache;
pub mod bundle;
pub mod clock;
pub mod declaration_source;
pub mod digester;
pub mod host;
pub mod manifest;
pub mod port_set;
pub mod stat_cache;
pub mod tool_resolver;

pub use action_cache::ActionCache;
pub use bundle::Ports;
pub use clock::Clock;
pub use declaration_source::DeclarationSource;
pub use digester::Digester;
pub use host::Host;
pub use manifest::Manifest;
pub use port_set::PortSet;
pub use stat_cache::StatCache;
pub use tool_resolver::ToolResolver;
