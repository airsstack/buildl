//! The `cargo-buildl` binary: every buildl command, plus `init` and `sync`, run as
//! `cargo buildl <command>`.
//!
//! Cargo runs a `cargo-<name>` program on the `PATH` as `cargo <name>`, handing it the name as
//! its first argument. This binary drops that word and reads what only the process knows. It
//! hands a buildl command to [`buildl::cli::execute`]; the commands, their flags and their exit
//! statuses are those of the `buildl` binary. `init` and `sync` are its own commands, run by
//! [`init::init`] and [`sync::sync`]; `init` ends by running `buildl plugins fetch` through
//! [`buildl::cli::execute`]. A command line that is wrong ends with 2, with what the parser has
//! to say about it.
//!
//! This file holds `main`, which decides nothing, so it carries no logic to unit-test; the
//! argument rule is tested in `args`, the init and sync rules, the unreadable current directory
//! among them, in `init` and `sync`.

mod args;
mod generate;
mod init;
mod metadata;
mod scaffold;
mod section;
mod sync;

use std::env;
use std::io::{self, IsTerminal};
use std::process::ExitCode;

use buildl::cli::{Command, PluginsArgs, PluginsCommand, RemoteSwitches, StdConsole, Terminal};
use clap::Parser;

use args::{CargoCli, CargoCommand};
use sync::Mode;

fn main() -> ExitCode {
    let cli = match CargoCli::try_parse_from(args::arguments(env::args_os())) {
        Ok(cli) => cli,
        Err(error) => error.exit(),
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
    let cargo = metadata::program(env::var_os("CARGO"));
    let exit = match cli.command {
        CargoCommand::Buildl(command) => buildl::cli::execute(
            cli.color,
            command,
            env::current_dir(),
            &terminal,
            &remote,
            &mut io::stdout(),
            &mut io::stderr(),
            &mut StdConsole,
        ),
        CargoCommand::Init => init::init(
            &cargo,
            env::current_dir(),
            &mut io::stderr(),
            |root, stderr| {
                buildl::cli::execute(
                    cli.color,
                    Command::Plugins(PluginsArgs {
                        command: PluginsCommand::Fetch,
                    }),
                    Ok(root.to_path_buf()),
                    &terminal,
                    &remote,
                    &mut io::stdout(),
                    stderr,
                    &mut StdConsole,
                )
            },
        ),
        CargoCommand::Sync(sync) => sync::sync(
            &cargo,
            env::current_dir(),
            Mode::from(&sync),
            &mut io::stderr(),
        ),
    };
    ExitCode::from(exit as u8)
}
