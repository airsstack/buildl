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
//! - [`ActionCache`] — what a build recorded for a target, read and written.
//! - [`ContentStore`] — what actions produce, held by content and placed in the workspace.
//! - [`EventLog`] — the record of how each target of a build ended.
//! - [`RunHistory`] — the lines of the build log, read back.
//! - [`Sweep`] — what the content store holds, listed and removed.
//! - [`ExecStrategy`] — running one action in a directory of its own.
//! - [`Dispatcher`] — having several tasks worked on at once.
//! - [`GraphRecord`] — the graph a build resolved, kept where a later reader finds it.
//! - [`Reporter`] — showing a person what a build is doing.
//! - [`Approver`] — saying whether a build may do what it wants.
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
pub mod approver;
pub mod bundle;
pub mod clock;
pub mod content_store;
pub mod declaration_source;
pub mod digester;
pub mod dispatcher;
pub mod event_log;
pub mod exec_strategy;
pub mod graph_record;
pub mod host;
pub mod manifest;
pub mod port_set;
pub mod reporter;
pub mod run_history;
pub mod stat_cache;
pub mod sweep;
pub mod tool_resolver;

pub use action_cache::ActionCache;
pub use approver::Approver;
pub use bundle::Ports;
pub use clock::Clock;
pub use content_store::ContentStore;
pub use declaration_source::DeclarationSource;
pub use digester::Digester;
pub use dispatcher::Dispatcher;
pub use event_log::EventLog;
pub use exec_strategy::ExecStrategy;
pub use graph_record::GraphRecord;
pub use host::Host;
pub use manifest::Manifest;
pub use port_set::PortSet;
pub use reporter::Reporter;
pub use run_history::RunHistory;
pub use stat_cache::StatCache;
pub use sweep::Sweep;
pub use tool_resolver::ToolResolver;
