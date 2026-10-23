//! buildl — a sandboxed, deterministic build system whose build files are
//! written in Lua and evaluated on the airsl embedded runtime.
//!
//! Build files *declare* targets; buildl schedules, sandboxes, parallelizes,
//! and caches their execution. The pipeline is
//! Load → Resolve → Plan → Execute → Record, with each phase handing the next
//! a serializable value.
//!
//! This crate is the composition root: its role is to choose a concrete
//! implementation for every port the `buildl-core` pipeline needs, and to offer the result as
//! one type, [`Workspace`].
//!
//! # Using it
//!
//! A caller opens a [`Workspace`] and calls a command on it: [`Workspace::check`],
//! [`Workspace::graph`], [`Workspace::plan`], [`Workspace::build`] or [`Workspace::verify`]. It
//! names no adapter. A build is lent the caller's own [`Reporter`]: [`report::TerminalReporter`] writes what the
//! `buildl` binary shows, and [`report::Silent`] shows nothing. The values a command takes and
//! returns are `buildl-core`'s, re-exported here as [`buildl_core`], so a caller depends on this
//! crate alone.
//!
//! ```
//! use std::collections::BTreeMap;
//! use std::fs;
//!
//! use buildl::{RunOptions, Workspace};
//! use buildl::approver::NoTerminal;
//! use buildl::buildl_core::{LiveOutput, OnFailure, OutcomeClass, Scope};
//! use buildl::report::Silent;
//!
//! let dir = tempfile::tempdir()?;
//! fs::write(
//!     dir.path().join("buildl.toml"),
//!     r#"
//! [ceiling]
//! proc.run = ["cp"]
//! fs.read  = ["."]
//! fs.write = ["out", ".buildl"]
//! "#,
//! )?;
//! fs::write(
//!     dir.path().join("build.lua"),
//!     r#"buildl.target("copy", { inputs = { "a.txt" }, run = { "cp", "$in", "$out" } })"#,
//! )?;
//! fs::write(dir.path().join("a.txt"), "a\n")?;
//!
//! let workspace = Workspace::open(dir.path())?;
//! assert_eq!(workspace.check()?.declarations.len(), 1);
//!
//! let (everything, settings) = (Scope::Workspace, BTreeMap::new());
//! let built = workspace.build(
//!     &everything,
//!     &settings,
//!     RunOptions::new(OnFailure::Stop, LiveOutput::Off),
//!     &mut Silent,
//!     &mut NoTerminal,
//! )?;
//! assert_eq!(built[0].class, OutcomeClass::Built);
//! assert_eq!(fs::read(workspace.root().join("out/copy"))?, b"a\n");
//!
//! let again = workspace.build(
//!     &everything,
//!     &settings,
//!     RunOptions::new(OnFailure::Stop, LiveOutput::Off),
//!     &mut Silent,
//!     &mut NoTerminal,
//! )?;
//! assert_eq!(again[0].class, OutcomeClass::Cached);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! A build and a test run are lent the reporter and an [`approver::Console`] as two `&mut`
//! parameters, and a verify is lent the console alone: the console is where the approver may ask
//! the person about what the run wants. Which approver answers is chosen by `[approver]` in
//! `buildl.toml`: `manifest`, the default, approves whatever the ceiling allows and never asks;
//! `interactive` asks about what is new and remembers a yes outside the workspace, so a caller
//! with nobody to ask passes [`approver::NoTerminal`]; and `deny` refuses a build, a test run and
//! a verify.
//! [`Workspace::with_data_dir`] says where a yes is remembered, in place of the user's data
//! directory.
//!
//! Actions run as plain processes, under the strategy the manifest names: `sandbox`, the default,
//! runs each in a directory of its own holding only the files it declared, and `bare` runs it in
//! the workspace. A build of a workspace that asks for `os-sandbox` or `container` is refused,
//! never run under less isolation than its manifest asked for. A verify re-runs every
//! cached target, or a sample of them when asked, in the input sandbox, and refuses a workspace
//! on `bare`.
//!
//! # Where things live
//!
//! Each port a local build is served through has a module of its own here, named after its
//! concern.
//!
//! | Module | Concern | Port |
//! |---|---|---|
//! | [`manifest`] | `buildl.toml` and the workspace root | [`Manifest`] |
//! | [`host`] | the platform, and the environment values a target reads | [`Host`] |
//! | [`digest`] | hashing the files a build reads | [`Digester`] |
//! | [`tools`] | fingerprinting the programs a command names | [`ToolResolver`] |
//! | [`cas`] | content held by digest, and the files of the output directory | [`ContentStore`] |
//! | [`cache`] | the row a built target leaves behind | [`ActionCache`] |
//! | [`log`] | one line for every action that ended | [`EventLog`] |
//! | [`statcache`] | the digest of a file that has not changed since it was hashed | [`StatCache`] |
//! | [`graph`] | the graph of the last build | [`GraphRecord`] |
//! | [`exec`] | running one action as a process | [`ExecStrategy`] |
//! | [`dispatch`] | the threads actions run on | [`Dispatcher`] |
//! | [`clock`] | the wall clock | [`Clock`] |
//! | [`report`] | what a build shows while it runs, and every rendering of a value as text | [`Reporter`] |
//! | [`approver`] | whether a build may do what it wants | [`Approver`] |
//!
//! [`DeclarationSource`] has no module here: its adapter is the `buildl-lua` crate.
//!
//! No module in the table names another module in the table. What two of them would otherwise
//! each need from the other belongs to neither, so each can be read, tested and replaced alone.
//! It lives in a module that serves no port and names no module of this crate:
//!
//! | Module | Shared by its users |
//! |---|---|
//! | [`state`] | where each file of the state directory is, the lock on it, and writing a file whole |
//! | [`search_path`] | the rule that leads from a program's name to a file |
//!
//! [`plugins`] serves no port and is in neither table: it holds `buildl.lock` and the digest of
//! a plugin's files, and names only [`state`] and `buildl-core`.
//!
//! The adapters meet in two modules and nowhere else. Neither is public: what they hold is
//! exported from the crate root.
//!
//! | Module | Holds |
//! |---|---|
//! | `local_ports` | [`LocalPorts`], the choice of an adapter for every port; [`LocalPipeline`], the pipeline over that choice; and [`Wiring`], which makes the adapters from a workspace's manifest |
//! | `workspace` | [`Workspace`], the facade: one method per command |
//!
//! # Platforms
//!
//! The crate builds on Unix targets only, and is tested on Linux and macOS. File modes and the
//! signal that ended a process are read through `std::os::unix`.
//!
//! # Non-responsibilities
//!
//! - Pipeline logic, which lives in `buildl-core`.
//! - Lua evaluation, which lives in `buildl-lua`.
//! - Parsing a command line and choosing an exit code, which the `buildl-cli` crate does.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to
//! unit-test.
//!
//! [`ActionCache`]: buildl_core::ActionCache
//! [`Approver`]: buildl_core::Approver
//! [`Clock`]: buildl_core::Clock
//! [`ContentStore`]: buildl_core::ContentStore
//! [`DeclarationSource`]: buildl_core::DeclarationSource
//! [`Digester`]: buildl_core::Digester
//! [`Dispatcher`]: buildl_core::Dispatcher
//! [`EventLog`]: buildl_core::EventLog
//! [`ExecStrategy`]: buildl_core::ExecStrategy
//! [`GraphRecord`]: buildl_core::GraphRecord
//! [`Host`]: buildl_core::Host
//! [`Manifest`]: buildl_core::Manifest
//! [`Reporter`]: buildl_core::Reporter
//! [`StatCache`]: buildl_core::StatCache
//! [`ToolResolver`]: buildl_core::ToolResolver

pub mod approver;
pub mod cache;
pub mod cas;
pub mod clock;
pub mod digest;
pub mod dispatch;
pub mod exec;
pub mod graph;
pub mod host;
pub mod log;
pub mod manifest;
pub mod plugins;
pub mod report;
pub mod search_path;
pub mod statcache;
pub mod state;
pub mod tools;

mod local_ports;
mod workspace;

pub use buildl_core;
pub use local_ports::{LocalPipeline, LocalPorts, Wiring};
pub use state::Cleaned;
pub use workspace::{Doctor, RunOptions, Workspace};
