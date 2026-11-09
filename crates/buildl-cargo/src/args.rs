//! The command line of `cargo buildl`: the rule for the word cargo inserts, and the parser.

use std::ffi::OsString;

use buildl::cli::{Color, Command};
use clap::{Parser, Subcommand};

/// Returns `args` without its second item when that item is exactly `buildl`.
///
/// Cargo runs `cargo buildl build` as `cargo-buildl buildl build`, so the second item is the
/// subcommand's own name, not an argument. A `buildl` anywhere else is left alone.
pub(crate) fn arguments<I>(args: I) -> Vec<OsString>
where
    I: IntoIterator<Item = OsString>,
{
    args.into_iter()
        .enumerate()
        .filter(|(index, arg)| !(*index == 1 && arg == "buildl"))
        .map(|(_, arg)| arg)
        .collect()
}

/// `cargo buildl`: buildl's commands behind the name cargo gives them.
#[derive(Debug, Parser)]
#[command(
    name = "cargo buildl",
    bin_name = "cargo buildl",
    version,
    propagate_version = true
)]
pub(crate) struct CargoCli {
    /// When to colour what is printed
    #[arg(long, value_enum, value_name = "WHEN", default_value_t = Color::Auto, global = true)]
    pub(crate) color: Color,

    /// The command to run.
    #[command(subcommand)]
    pub(crate) command: CargoCommand,
}

/// One command of `cargo buildl`.
#[derive(Debug, Subcommand)]
pub(crate) enum CargoCommand {
    /// A command of the `buildl` binary.
    #[command(flatten)]
    Buildl(Command),
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        clippy::panic,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use super::*;

    fn os(items: &[&str]) -> Vec<OsString> {
        items.iter().map(OsString::from).collect()
    }

    #[test]
    fn buildl_is_dropped_only_as_the_first_argument() {
        assert_eq!(
            arguments(os(&["cargo-buildl", "buildl", "build"])),
            os(&["cargo-buildl", "build"])
        );
        assert_eq!(
            arguments(os(&["cargo-buildl", "build", "buildl"])),
            os(&["cargo-buildl", "build", "buildl"])
        );
        assert_eq!(
            arguments(os(&["cargo-buildl", "buildlx", "build"])),
            os(&["cargo-buildl", "buildlx", "build"])
        );
        assert_eq!(arguments(os(&["cargo-buildl"])), os(&["cargo-buildl"]));
    }

    #[test]
    fn a_buildl_command_parses() {
        let cli =
            CargoCli::try_parse_from(os(&["cargo-buildl", "build", "-k", "--set", "a=b"])).unwrap();
        let CargoCommand::Buildl(Command::Build(build)) = cli.command else {
            panic!("expected build");
        };
        assert!(build.keep_going);
        assert_eq!(build.selection.overrides.len(), 1);

        let cli = CargoCli::try_parse_from(os(&["cargo-buildl", "test", "--stop"])).unwrap();
        let CargoCommand::Buildl(Command::Test(test)) = cli.command else {
            panic!("expected test");
        };
        assert!(test.stop);
    }

    #[test]
    fn a_usage_error_names_cargo_buildl() {
        let error = CargoCli::try_parse_from(os(&["cargo-buildl", "bogus"])).unwrap_err();
        assert!(error.to_string().contains("cargo buildl"), "{error}");
        assert_eq!(error.exit_code(), 2);
    }
}
