//! The `cargo-buildl` binary: every buildl command, run as `cargo buildl <command>`.
//!
//! Cargo runs a `cargo-<name>` program on the `PATH` as `cargo <name>`, handing it the name as
//! its first argument. This binary drops that word, reads what only the process knows, and hands
//! the command to [`buildl::cli::execute`]; the commands, their flags and their exit statuses
//! are those of the `buildl` binary. A command line that is wrong ends with 2, with what the
//! parser has to say about it.
//!
//! This file holds `main`, which decides nothing, so it carries no logic to unit-test; the
//! argument rule is tested in `args`.

mod args;
#[cfg_attr(not(test), expect(dead_code, reason = "used by cargo buildl sync"))]
mod metadata;

use std::env;
use std::io::{self, IsTerminal};
use std::process::ExitCode;

use buildl::cli::{RemoteSwitches, StdConsole, Terminal};
use clap::Parser;

use args::{CargoCli, CargoCommand};

fn main() -> ExitCode {
    let cli = match CargoCli::try_parse_from(args::arguments(env::args_os())) {
        Ok(cli) => cli,
        Err(error) => error.exit(),
    };
    let CargoCommand::Buildl(command) = cli.command;
    let terminal = Terminal {
        interactive: io::stderr().is_terminal(),
        term: env::var_os("TERM"),
        no_color: env::var_os("NO_COLOR"),
    };
    let remote = RemoteSwitches {
        upload: env::var_os("BUILDL_REMOTE_UPLOAD"),
        strict: env::var_os("BUILDL_REMOTE_STRICT"),
        execute: env::var_os("BUILDL_REMOTE_EXECUTE"),
    };
    let exit = buildl::cli::execute(
        cli.color,
        command,
        env::current_dir(),
        &terminal,
        &remote,
        &mut io::stdout(),
        &mut io::stderr(),
        &mut StdConsole,
    );
    ExitCode::from(exit as u8)
}
