//! The `buildl` command line: the arguments, one run of a command over values, and the status
//! it ends with.
//!
//! The `buildl` binary and any other front end that offers buildl's commands share it. A
//! caller reads what only the process knows — whether standard error is a terminal, `TERM`,
//! `NO_COLOR` and the remote-cache variables, into [`Terminal`] and [`RemoteSwitches`] — and
//! hands it on with the streams and a [`StdConsole`]. [`run()`] parses a command line with
//! [`Cli`] and runs it; [`execute()`] runs a [`Command`] that is already parsed, so a parser of
//! another shape can flatten [`Command`] into its own and call it.
//!
//! | Module | Holds |
//! |---|---|
//! | `args` | the parser types: [`Cli`], [`Command`], [`Color`] and the arguments of each command |
//! | `run` | [`run()`], [`execute()`] and one function per command |
//! | `exit` | [`Exit`], the status each ending gets |
//! | `terminal` | [`Terminal`], whether a status line is kept and what is coloured |
//! | `remote_switches` | [`RemoteSwitches`], the variables that steer the remote cache |
//! | `console` | [`StdConsole`], where the interactive approver asks |
//! | `doctor_text` | the text `buildl doctor` prints |
//!
//! This file holds only module declarations and re-exports, so it carries no logic to
//! unit-test.

mod args;
mod console;
mod doctor_text;
mod exit;
mod remote_switches;
mod run;
mod terminal;

pub use args::{
    BuildArgs, Cli, Color, Command, GraphArgs, GraphFormat, LogArgs, Override, PlanArgs,
    PluginsArgs, PluginsCommand, QueryArgs, RemoteArgs, Selection, Streams, TestArgs, VerifyArgs,
};
pub use console::StdConsole;
pub use exit::Exit;
pub use remote_switches::RemoteSwitches;
pub use run::{execute, run};
pub use terminal::Terminal;
