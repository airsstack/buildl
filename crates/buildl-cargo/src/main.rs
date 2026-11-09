//! The `cargo-buildl` binary: every buildl command, and `sync`, run as `cargo buildl <command>`.
//!
//! Cargo runs a `cargo-<name>` program on the `PATH` as `cargo <name>`, handing it the name as
//! its first argument. This binary drops that word and reads what only the process knows. It
//! hands a buildl command to [`buildl::cli::execute`]; the commands, their flags and their exit
//! statuses are those of the `buildl` binary. `sync` is its own command, run by [`sync::sync`].
//! A command line that is wrong ends with 2, with what the parser has to say about it.
//!
//! This file holds `main`, which decides nothing, so it carries no logic to unit-test; the
//! argument rule is tested in `args`, the sync rules in `sync`.

mod args;
mod generate;
mod metadata;
mod section;
mod sync;

use std::env;
use std::io::{self, IsTerminal, Write};
use std::process::ExitCode;

use buildl::cli::{Exit, RemoteSwitches, StdConsole, Terminal};
use clap::Parser;

use args::{CargoCli, CargoCommand};

fn main() -> ExitCode {
    let cli = match CargoCli::try_parse_from(args::arguments(env::args_os())) {
        Ok(cli) => cli,
        Err(error) => error.exit(),
    };
    let command = match cli.command {
        CargoCommand::Buildl(command) => command,
        CargoCommand::Sync(sync) => {
            let exit = match env::current_dir() {
                Ok(start) => sync::sync(
                    &metadata::program(env::var_os("CARGO")),
                    &start,
                    sync.check,
                    &mut io::stderr(),
                ),
                Err(error) => {
                    let _ = writeln!(
                        io::stderr(),
                        "error: cannot read the current directory: {error}"
                    );
                    Exit::Infrastructure
                }
            };
            return ExitCode::from(exit as u8);
        }
    };
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
