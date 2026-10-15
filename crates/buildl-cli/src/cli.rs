//! The argument surface: the commands, their flags, and how each becomes a value a build takes.
//!
//! Its own file because the command line is a contract of its own: the names here are what a
//! person types and what a script depends on, and nothing else in the binary reads an argument
//! as text.
//!
//! Responsibilities:
//!
//! - [`Cli`] and [`Command`] — the commands and the flags of each.
//! - [`Color`] — when to colour what is printed, as asked for.
//! - [`GraphArgs`], [`QueryArgs`], [`PlanArgs`], [`BuildArgs`], [`LogArgs`] and [`Selection`] —
//!   the arguments of a command.
//! - [`Streams`] — which streams of a run `log` prints.
//! - [`Override`] and [`overrides`] — `--set NAME=VALUE`, and the settings of one run.
//!
//! Non-responsibilities: running a command, which the `run` module does; reading a target
//! argument as a label, which needs the workspace; and the texts of `--help` and of a usage
//! error, which are the argument parser's own.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use buildl::buildl_core::{LiveOutput, OnFailure, SettingName, SettingValue};
use clap::error::ErrorKind;
use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};

/// A sandboxed, deterministic build system
#[derive(Debug, Parser)]
#[command(name = "buildl", version, propagate_version = true)]
pub(crate) struct Cli {
    /// When to colour what is printed
    #[arg(long, value_enum, value_name = "WHEN", default_value_t = Color::Auto, global = true)]
    pub color: Color,

    /// The command to run.
    #[command(subcommand)]
    pub command: Command,
}

/// When to colour what is printed, as the command line asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum Color {
    /// Colour when standard error is a terminal and `NO_COLOR` is unset
    Auto,
    /// Always colour
    Always,
    /// Never colour
    Never,
}

/// One command of the binary.
#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Evaluate every build file twice and run nothing
    Check,
    /// Print the target graph
    Graph(GraphArgs),
    /// Ask what a target needs, or what needs it
    Query(QueryArgs),
    /// Say what a build would run, and why
    Plan(PlanArgs),
    /// Build targets and what they need
    Build(BuildArgs),
    /// Show what a target printed when it last ran
    Log(LogArgs),
}

/// The arguments of `graph`.
#[derive(Debug, Args)]
pub(crate) struct GraphArgs {
    /// The form the graph is printed in.
    #[command(flatten)]
    pub format: GraphFormat,
}

/// The form a graph is printed in: one of the two at most, and JSON when neither is named.
#[derive(Debug, Args)]
#[group(multiple = false)]
pub(crate) struct GraphFormat {
    /// Print the graph as canonical JSON, the bytes of .buildl/graph.json (the default)
    #[arg(long)]
    pub json: bool,
    /// Print the graph in Graphviz DOT
    #[arg(long)]
    pub dot: bool,
}

/// The arguments of `query`.
#[derive(Debug, Args)]
pub(crate) struct QueryArgs {
    /// deps(TARGET) for what TARGET needs, or rdeps(TARGET) for what needs it; TARGET as //dir:name, :name or name
    #[arg(value_name = "EXPR")]
    pub expression: String,
    /// Print the answer as canonical JSON
    #[arg(long)]
    pub json: bool,
}

/// What `plan` and `build` share: which targets, under which settings.
#[derive(Debug, Args)]
pub(crate) struct Selection {
    /// Targets to build, as //dir:name, :name or name; every target when none is given
    #[arg(value_name = "TARGET")]
    pub targets: Vec<String>,
    /// Give a build setting a value for this run; may be repeated
    #[arg(long = "set", value_name = "NAME=VALUE", value_parser = parse_override)]
    pub overrides: Vec<Override>,
}

/// The arguments of `plan`.
#[derive(Debug, Args)]
pub(crate) struct PlanArgs {
    /// Which targets, under which settings.
    #[command(flatten)]
    pub selection: Selection,
    /// Print the plan as canonical JSON
    #[arg(long)]
    pub json: bool,
}

/// The arguments of `build`.
#[derive(Debug, Args)]
pub(crate) struct BuildArgs {
    /// Which targets, under which settings.
    #[command(flatten)]
    pub selection: Selection,
    /// After a failure, still build everything that does not depend on it
    #[arg(short = 'k', long)]
    pub keep_going: bool,
    /// Show one running action's output as it is printed
    #[arg(long)]
    pub stream: bool,
}

/// The arguments of `log`.
#[derive(Debug, Args)]
pub(crate) struct LogArgs {
    /// The target, as //dir:name, :name or name
    #[arg(value_name = "TARGET")]
    pub target: String,
    /// Which stream to print.
    #[command(flatten)]
    flags: StreamFlags,
}

/// The stream of a run `log` prints: one of the two at most, and both when neither is named.
#[derive(Debug, Args)]
#[group(multiple = false)]
pub(crate) struct StreamFlags {
    /// Print only what the run wrote to standard output
    #[arg(long)]
    pub stdout: bool,
    /// Print only what the run wrote to standard error, on standard output
    #[arg(long)]
    pub stderr: bool,
}

/// Which of a run's streams `log` prints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Streams {
    /// Standard output on standard output and standard error on standard error
    Both,
    /// Only what the run wrote to standard output
    Stdout,
    /// Only what the run wrote to standard error, on standard output
    Stderr,
}

impl LogArgs {
    /// The streams the command line asks for.
    pub(crate) const fn streams(&self) -> Streams {
        if self.flags.stdout {
            Streams::Stdout
        } else if self.flags.stderr {
            Streams::Stderr
        } else {
            Streams::Both
        }
    }
}

impl BuildArgs {
    /// What the build does once a target has failed.
    pub(crate) const fn on_failure(&self) -> OnFailure {
        if self.keep_going {
            OnFailure::KeepGoing
        } else {
            OnFailure::Stop
        }
    }

    /// Whether one running action's output is shown as it is printed.
    pub(crate) const fn live(&self) -> LiveOutput {
        if self.stream {
            LiveOutput::OneAction
        } else {
            LiveOutput::Off
        }
    }
}

/// One `--set NAME=VALUE`.
#[derive(Debug, Clone)]
pub(crate) struct Override {
    /// The setting.
    pub name: SettingName,
    /// The value it is given for this run.
    pub value: SettingValue,
}

/// Reads one `--set` argument, split at its first `=`: a value may hold more of them.
fn parse_override(raw: &str) -> Result<Override, String> {
    let (name, value) = raw
        .split_once('=')
        .ok_or_else(|| "expected NAME=VALUE".to_owned())?;
    Ok(Override {
        name: SettingName::parse(name).map_err(|error| error.to_string())?,
        value: SettingValue::parse(value).map_err(|error| error.to_string())?,
    })
}

/// The settings given a value for this run, by name.
///
/// # Errors
///
/// Returns a usage error when a setting is named twice: which of the two values is meant is
/// never guessed.
pub(crate) fn overrides(
    given: Vec<Override>,
) -> Result<BTreeMap<SettingName, SettingValue>, clap::Error> {
    let mut settings = BTreeMap::new();
    for Override { name, value } in given {
        match settings.entry(name) {
            Entry::Vacant(free) => {
                free.insert(value);
            }
            Entry::Occupied(taken) => {
                return Err(Cli::command().error(
                    ErrorKind::ArgumentConflict,
                    format!("setting {} is given more than once", taken.key()),
                ));
            }
        }
    }
    Ok(settings)
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        clippy::panic,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::iter;

    use buildl::buildl_core::{LiveOutput, OnFailure};
    use clap::error::ErrorKind;
    use clap::{CommandFactory, Parser};

    use super::{BuildArgs, Cli, Color, Command, LogArgs, PlanArgs, Streams, overrides};

    /// The command line `args` after the program's name, parsed.
    fn parsed(args: &[&str]) -> Cli {
        Cli::try_parse_from(iter::once(&"buildl").chain(args)).unwrap()
    }

    /// The kind of the error the command line `args` is refused with.
    fn refused(args: &[&str]) -> ErrorKind {
        Cli::try_parse_from(iter::once(&"buildl").chain(args))
            .unwrap_err()
            .kind()
    }

    /// The arguments of the `build` command line `args`.
    fn build(args: &[&str]) -> BuildArgs {
        match parsed(args).command {
            Command::Build(build) => build,
            other => panic!("expected build, got {other:?}"),
        }
    }

    /// The arguments of the `plan` command line `args`.
    fn plan(args: &[&str]) -> PlanArgs {
        match parsed(args).command {
            Command::Plan(plan) => plan,
            other => panic!("expected plan, got {other:?}"),
        }
    }

    /// The arguments of the `log` command line `args`.
    fn log(args: &[&str]) -> LogArgs {
        match parsed(args).command {
            Command::Log(log) => log,
            other => panic!("expected log, got {other:?}"),
        }
    }

    #[test]
    fn the_definition_is_consistent() {
        Cli::command().debug_assert();
    }

    #[test]
    fn each_command_parses() {
        assert!(matches!(parsed(&["check"]).command, Command::Check));
        assert!(matches!(parsed(&["graph"]).command, Command::Graph(_)));
        assert!(matches!(parsed(&["plan"]).command, Command::Plan(_)));
        assert!(matches!(parsed(&["build"]).command, Command::Build(_)));
        assert!(matches!(parsed(&["log", "app"]).command, Command::Log(_)));
    }

    #[test]
    fn log_streams_map_to_their_values() {
        assert_eq!(log(&["log", "app"]).streams(), Streams::Both);
        assert_eq!(log(&["log", "app", "--stdout"]).streams(), Streams::Stdout);
        assert_eq!(log(&["log", "--stderr", "app"]).streams(), Streams::Stderr);
        assert_eq!(log(&["log", "//lib:app"]).target, "//lib:app");
    }

    #[test]
    fn log_takes_one_stream_flag_and_one_target() {
        assert_eq!(
            refused(&["log", "app", "--stdout", "--stderr"]),
            ErrorKind::ArgumentConflict
        );
        assert_eq!(refused(&["log"]), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn targets_are_kept_as_written_and_in_order() {
        let args = plan(&["plan", "//lib:text", "app", ":docs"]);
        assert_eq!(args.selection.targets, ["//lib:text", "app", ":docs"]);
        assert_eq!(plan(&["plan"]).selection.targets, Vec::<String>::new());
    }

    #[test]
    fn set_may_be_repeated() {
        let args = plan(&["plan", "--set", "a=b", "--set", "c=d=e", "--set", "f="]);
        let settings = overrides(args.selection.overrides).unwrap();
        let settings: Vec<(&str, &str)> = settings
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect();
        assert_eq!(settings, [("a", "b"), ("c", "d=e"), ("f", "")]);
    }

    #[test]
    fn a_setting_given_twice_is_refused() {
        let args = plan(&["plan", "--set", "a=b", "--set", "a=c"]);
        let error = overrides(args.selection.overrides).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::ArgumentConflict);
        assert!(error.use_stderr());
        assert!(
            error
                .to_string()
                .starts_with("error: setting a is given more than once\n")
        );
    }

    #[test]
    fn set_without_a_value_part_is_refused() {
        let error = Cli::try_parse_from(["buildl", "plan", "--set", "novalue"]).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::ValueValidation);
        assert!(error.to_string().starts_with(
            "error: invalid value 'novalue' for '--set <NAME=VALUE>': expected NAME=VALUE\n"
        ));
    }

    #[test]
    fn a_setting_name_outside_its_grammar_is_refused() {
        assert_eq!(
            refused(&["plan", "--set", "bad name=x"]),
            ErrorKind::ValueValidation
        );
    }

    #[test]
    fn json_and_dot_exclude_each_other() {
        assert_eq!(
            refused(&["graph", "--json", "--dot"]),
            ErrorKind::ArgumentConflict
        );
        let Command::Graph(dot) = parsed(&["graph", "--dot"]).command else {
            panic!("expected graph");
        };
        assert!(dot.format.dot && !dot.format.json);
        let Command::Graph(neither) = parsed(&["graph"]).command else {
            panic!("expected graph");
        };
        assert!(!neither.format.dot && !neither.format.json);
    }

    #[test]
    fn check_takes_no_target() {
        assert_eq!(refused(&["check", "extra"]), ErrorKind::UnknownArgument);
        assert_eq!(refused(&["graph", "extra"]), ErrorKind::UnknownArgument);
        assert_eq!(
            refused(&["check", "--set", "a=b"]),
            ErrorKind::UnknownArgument
        );
    }

    #[test]
    fn keep_going_and_stream_are_flags() {
        let quiet = build(&["build"]);
        assert_eq!(quiet.on_failure(), OnFailure::Stop);
        assert_eq!(quiet.live(), LiveOutput::Off);

        let long = build(&["build", "--keep-going", "--stream"]);
        assert_eq!(long.on_failure(), OnFailure::KeepGoing);
        assert_eq!(long.live(), LiveOutput::OneAction);

        assert_eq!(build(&["build", "-k"]).on_failure(), OnFailure::KeepGoing);
        assert_eq!(build(&["build", "-k", "app"]).selection.targets, ["app"]);
        assert_eq!(
            refused(&["plan", "--keep-going"]),
            ErrorKind::UnknownArgument
        );
    }

    #[test]
    fn the_color_flag_follows_the_command() {
        assert_eq!(parsed(&["check"]).color, Color::Auto);
        assert_eq!(parsed(&["--color", "never", "check"]).color, Color::Never);
        assert_eq!(parsed(&["check", "--color", "always"]).color, Color::Always);
        assert_eq!(
            parsed(&["build", "-k", "--color", "never", "app"]).color,
            Color::Never
        );
        assert_eq!(
            refused(&["plan", "--color", "sometimes"]),
            ErrorKind::InvalidValue
        );
    }

    #[test]
    fn there_is_no_flag_for_a_start_directory() {
        assert_eq!(refused(&["-C", "/tmp", "plan"]), ErrorKind::UnknownArgument);
    }
}
