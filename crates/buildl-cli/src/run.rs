//! One run of the binary: a command line in, bytes on two streams and a status out.
//!
//! Its own file because this is the whole flow of a command, written over values: the
//! arguments, the directory the command was started in, what is known of the terminal, the
//! two streams and the console the approver asks on are all handed in. A test therefore drives
//! a command without a process.
//!
//! Standard output carries what the command was asked for and nothing else; standard error
//! carries everything about how the command went and, for `log`, the stored standard error of
//! the run it shows.
//!
//! Responsibilities: [`run`], and one function per command.
//!
//! Non-responsibilities: build logic and every rendering, which are `buildl`'s; the argument
//! surface, which is `cli`'s; and which status an ending gets, which is `exit`'s.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use buildl::approver::Console;
use buildl::buildl_core::json::canonical;
use buildl::buildl_core::{Clock, Error, Scope};
use buildl::clock::SystemClock;
use buildl::plugins::FetchStatus;
use buildl::remote::token_is_present;
use buildl::report::{
    Mode, Paint, Report, TerminalReporter, check_line, clean_line, damage, error_text, gc_line,
    graph_dot, header, not_run_lines, plan_text, query_text, verify_text, watch_error_line,
};
use buildl::watch::NotifyChanges;
use buildl::{RunOptions, Workspace};
use clap::error::ErrorKind;
use clap::{CommandFactory, Parser};

use crate::cli::{
    BuildArgs, Cli, Command, GraphArgs, LogArgs, PlanArgs, PluginsArgs, PluginsCommand, QueryArgs,
    Streams, TestArgs, VerifyArgs, overrides,
};
use crate::doctor_text::{RemoteReport, TokenState, doctor_text};
use crate::exit::Exit;
use crate::remote_switches::{RemoteSwitches, RemoteTable};
use crate::terminal::Terminal;

/// Why a command stopped before its end.
#[derive(Debug)]
enum Stop {
    /// The command line asked for something that cannot be done as written.
    Usage(clap::Error),
    /// The workspace, a build file or the builder failed.
    Failed(Box<Error>),
    /// What the command was asked for could not be written to standard output.
    Unwritten(io::Error),
}

impl From<clap::Error> for Stop {
    fn from(error: clap::Error) -> Self {
        Self::Usage(error)
    }
}

impl From<Error> for Stop {
    fn from(error: Error) -> Self {
        Self::Failed(Box::new(error))
    }
}

impl From<io::Error> for Stop {
    fn from(error: io::Error) -> Self {
        Self::Unwritten(error)
    }
}

/// Runs the command line `args`, whose first item is the program's name, and returns the
/// status the process ends with.
///
/// `start` is the directory the command was started in, as the process read it: the workspace
/// is the nearest one at or above it, and a target named without its directory is read against
/// it. `terminal` is what is known of where standard error goes, and `remote` what this
/// machine's variables say about the remote cache. What the command was asked for is written
/// to `stdout`, and everything about how it went to `stderr`. `console` is where a workspace
/// whose approver is `interactive` asks about what a build, a test run or a verify wants.
pub(crate) fn run<A, O, E, C>(
    args: A,
    start: io::Result<PathBuf>,
    terminal: &Terminal,
    remote: &RemoteSwitches,
    stdout: &mut O,
    stderr: &mut E,
    console: &mut C,
) -> Exit
where
    A: IntoIterator<Item: Into<OsString> + Clone>,
    O: Write,
    E: Write,
    C: Console,
{
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(error) => return told(&error, stdout, stderr),
    };
    let paint = terminal.paint(cli.color);
    let start = match start {
        Ok(start) => start,
        Err(error) => {
            return said(
                "the current directory cannot be read",
                &error,
                paint,
                stderr,
            );
        }
    };
    let ended = match cli.command {
        Command::Check => check(&start, stderr),
        Command::Graph(args) => graph(&start, &args, stdout, stderr),
        Command::Query(args) => query(&start, &args, stdout, stderr),
        Command::Plan(args) => plan(&start, args, stdout),
        Command::Build(args) => build(
            &start,
            args,
            remote,
            terminal.mode(),
            paint,
            stderr,
            console,
        ),
        Command::Watch(args) => watch(
            &start,
            args,
            remote,
            terminal.mode(),
            paint,
            stderr,
            console,
        ),
        Command::Test(args) => test(
            &start,
            args,
            remote,
            terminal.mode(),
            paint,
            stderr,
            console,
        ),
        Command::Verify(args) => verify(&start, args, paint, stderr, console),
        Command::Doctor => doctor(&start, remote, stdout, stderr),
        Command::Log(args) => log(&start, &args, stdout, stderr),
        Command::Clean => clean(&start, stderr),
        Command::Gc => gc(&start, stderr),
        Command::Plugins(args) => plugins(&start, &args, stderr),
    };
    match ended {
        Ok(exit) => exit,
        Err(Stop::Usage(error)) => told(&error, stdout, stderr),
        Err(Stop::Failed(error)) => {
            let _ = stderr.write_all(error_text(&error, paint).as_bytes());
            Exit::of_error(&error)
        }
        // Whoever read the output has stopped reading: there is nobody left to tell.
        Err(Stop::Unwritten(error)) if error.kind() == io::ErrorKind::BrokenPipe => Exit::Success,
        Err(Stop::Unwritten(error)) => {
            said("standard output cannot be written", &error, paint, stderr)
        }
    }
}

/// Writes what the argument parser has to say: help and the version to `stdout`, with success;
/// a refusal to `stderr`, as a usage error.
fn told<O: Write, E: Write>(error: &clap::Error, stdout: &mut O, stderr: &mut E) -> Exit {
    // The rendered text carries no colour sequence, whatever the stream is.
    let text = error.render();
    if error.use_stderr() {
        let _ = write!(stderr, "{text}");
        Exit::Usage
    } else {
        let _ = write!(stdout, "{text}");
        Exit::Success
    }
}

/// Writes that the machine failed the command, as `error: <what>: <error>`, and returns the
/// status of a failure that is not the user's.
fn said<E: Write>(what: &str, error: &io::Error, paint: Paint, stderr: &mut E) -> Exit {
    let _ = writeln!(stderr, "{}: {what}: {error}", paint.failure("error"));
    Exit::Infrastructure
}

/// Writes `bytes`, what a command was asked for, to `stdout`.
fn product<O: Write>(stdout: &mut O, bytes: &[u8]) -> io::Result<()> {
    stdout.write_all(bytes)?;
    stdout.flush()
}

/// The targets a command line names, each read as a person writes it: against the directory
/// the workspace was opened from when it names none. No target is the whole workspace.
fn scope(workspace: &Workspace, targets: &[String]) -> Result<Scope, clap::Error> {
    if targets.is_empty() {
        return Ok(Scope::Workspace);
    }
    let labels = targets
        .iter()
        .map(|raw| {
            workspace.label(raw).map_err(|error| {
                Cli::command().error(
                    ErrorKind::ValueValidation,
                    format!("invalid value '{raw}' for '[TARGET]...': {error}"),
                )
            })
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    Ok(Scope::Targets(labels))
}

/// `check`: evaluates every build file twice, and says on `stderr` how many declarations were
/// found and which discovery targets have not run.
fn check<E: Write>(start: &Path, stderr: &mut E) -> Result<Exit, Stop> {
    let loaded = Workspace::open(start)?.check()?;
    let _ = stderr.write_all(check_line(&loaded.declarations).as_bytes());
    let _ = stderr.write_all(not_run_lines(&loaded.missing).as_bytes());
    Ok(Exit::Success)
}

/// `clean`: removes the output directory, and says on `stderr` that it did or that it held
/// nothing.
fn clean<E: Write>(start: &Path, stderr: &mut E) -> Result<Exit, Stop> {
    let workspace = Workspace::open(start)?;
    let cleaned = workspace.clean()?;
    let _ = stderr.write_all(clean_line(&cleaned, workspace.out()).as_bytes());
    Ok(Exit::Success)
}

/// `gc`: drops the cache rows of targets no longer declared and removes what nothing needs, and
/// says on `stderr` how many rows, blobs and bytes that was.
fn gc<E: Write>(start: &Path, stderr: &mut E) -> Result<Exit, Stop> {
    let collected = Workspace::open(start)?.gc()?;
    let _ = stderr.write_all(gc_line(&collected).as_bytes());
    Ok(Exit::Success)
}

/// `plugins`: runs the command under it.
fn plugins<E: Write>(start: &Path, args: &PluginsArgs, stderr: &mut E) -> Result<Exit, Stop> {
    match args.command {
        PluginsCommand::Fetch => fetch_plugins(start, stderr),
    }
}

/// `plugins fetch`: pins every git plugin in `buildl.lock` and stores its files, and says on
/// `stderr`, for each in name order, `fetched` or `unchanged`, its name and the commit it is
/// pinned to.
fn fetch_plugins<E: Write>(start: &Path, stderr: &mut E) -> Result<Exit, Stop> {
    let fetched = Workspace::open(start)?.fetch_plugins()?;
    for plugin in fetched.iter() {
        let status = match plugin.status {
            FetchStatus::Fetched => "fetched",
            FetchStatus::Unchanged => "unchanged",
        };
        let _ = writeln!(stderr, "{status} {} {}", plugin.plugin, plugin.commit);
    }
    Ok(Exit::Success)
}

/// `graph`: writes the target graph to `stdout`, as canonical JSON and a newline, or as DOT, and
/// says on `stderr` which discovery targets have not run.
fn graph<O: Write, E: Write>(
    start: &Path,
    args: &GraphArgs,
    stdout: &mut O,
    stderr: &mut E,
) -> Result<Exit, Stop> {
    let (graph, missing) = Workspace::open(start)?.graph()?;
    let _ = stderr.write_all(not_run_lines(&missing).as_bytes());
    let text = if args.format.dot {
        graph_dot(&graph).into_bytes()
    } else {
        let mut json = canonical::to_vec(&graph)?;
        json.push(b'\n');
        json
    };
    product(stdout, &text)?;
    Ok(Exit::Success)
}

/// `query`: writes to `stdout` the targets the expression reaches, one label a line, or as
/// canonical JSON and a newline. An empty answer is a success. An expression that is neither
/// `deps(...)` nor `rdeps(...)`, or whose target fails the label grammar, is a mistake of the
/// command line, not of the workspace. Which discovery targets have not run is said on `stderr`.
fn query<O: Write, E: Write>(
    start: &Path,
    args: &QueryArgs,
    stdout: &mut O,
    stderr: &mut E,
) -> Result<Exit, Stop> {
    let workspace = Workspace::open(start)?;
    let asked = workspace.query_of(&args.expression).map_err(|error| {
        Cli::command().error(
            ErrorKind::ValueValidation,
            format!("invalid value '{}' for '<EXPR>': {error}", args.expression),
        )
    })?;
    let (answer, missing) = workspace.query(&asked)?;
    let _ = stderr.write_all(not_run_lines(&missing).as_bytes());
    let text = if args.json {
        let mut json = canonical::to_vec(&answer)?;
        json.push(b'\n');
        json
    } else {
        query_text(&answer).into_bytes()
    };
    product(stdout, &text)?;
    Ok(Exit::Success)
}

/// `doctor`: writes to `stdout` what the workspace allows and what its build wants, and the
/// remote cache it names with what `remote` says of this machine. Wants outside the ceiling or
/// not approved are reported, and the command succeeded. Which discovery targets have not run
/// is said on `stderr`. It connects to no remote cache; a named token is read only to see that
/// a build could, and is never shown.
fn doctor<O: Write, E: Write>(
    start: &Path,
    remote: &RemoteSwitches,
    stdout: &mut O,
    stderr: &mut E,
) -> Result<Exit, Stop> {
    let (doctor, missing) = Workspace::open(start)?.doctor()?;
    let _ = stderr.write_all(not_run_lines(&missing).as_bytes());
    let token = match doctor.checkup().workspace().remote.as_ref() {
        Some(settings) if settings.token().is_none() => TokenState::Unnamed,
        Some(settings) if token_is_present(settings, doctor.root()) => TokenState::Set,
        _ => TokenState::Missing,
    };
    let report = RemoteReport {
        token,
        upload: remote.upload_switch(),
        strict: remote.strict_switch(),
    };
    product(stdout, doctor_text(&doctor, &report).as_bytes())?;
    Ok(Exit::Success)
}

/// `plan`: writes to `stdout` what a build would run and why, as lines for a person, or as
/// canonical JSON and a newline. Whatever would run, the command succeeded.
fn plan<O: Write>(start: &Path, args: PlanArgs, stdout: &mut O) -> Result<Exit, Stop> {
    let settings = overrides(args.selection.overrides)?;
    let workspace = Workspace::open(start)?;
    let scope = scope(&workspace, &args.selection.targets)?;
    let plan = workspace.plan(&scope, &settings)?;
    let text = if args.json {
        let mut json = canonical::to_vec(&plan)?;
        json.push(b'\n');
        json
    } else {
        plan_text(&plan).into_bytes()
    };
    product(stdout, &text)?;
    Ok(Exit::Success)
}

/// The `[remote]` table of `workspace`, as the switches need to know it.
fn remote_table(workspace: &Workspace) -> RemoteTable<'_> {
    workspace
        .remote_settings()
        .map_or(RemoteTable::Absent, RemoteTable::Named)
}

/// `build`: builds, showing on `stderr` how it goes, and returns the status of the most severe
/// way a target ended. The remote cache is used as `remote` and the flags say.
fn build<E: Write, C: Console>(
    start: &Path,
    args: BuildArgs,
    remote: &RemoteSwitches,
    mode: Mode,
    paint: Paint,
    stderr: &mut E,
    console: &mut C,
) -> Result<Exit, Stop> {
    let (on_failure, live) = (args.on_failure(), args.live());
    let settings = overrides(args.selection.overrides)?;
    let workspace = Workspace::open(start)?;
    let policy = remote.resolve(
        &args.remote,
        remote_table(&workspace),
        workspace.local_workers(),
    )?;
    let scope = scope(&workspace, &args.selection.targets)?;
    let mut reporter = TerminalReporter::new(&mut *stderr, mode, paint, Report::Build);
    let built = workspace.build(
        &scope,
        &settings,
        RunOptions::new(on_failure, live).with_remote(policy),
        &mut reporter,
        console,
    );
    if built.is_err() {
        // The build ended before its summary: whatever is said next starts on a row of its own.
        reporter.close();
    }
    Ok(Exit::of_outcomes(&built?))
}

/// `watch`: builds as `build` does, then again each time a file the build reads changes, until
/// the file watcher fails or the watch cannot read its layout before the first build, and returns
/// the status of that error after writing it to `stderr`.
fn watch<E: Write, C: Console>(
    start: &Path,
    args: BuildArgs,
    remote: &RemoteSwitches,
    mode: Mode,
    paint: Paint,
    stderr: &mut E,
    console: &mut C,
) -> Result<Exit, Stop> {
    let (on_failure, live) = (args.on_failure(), args.live());
    let settings = overrides(args.selection.overrides)?;
    let workspace = Workspace::open(start)?;
    let policy = remote.resolve(
        &args.remote,
        remote_table(&workspace),
        workspace.local_workers(),
    )?;
    workspace.ensure_sandboxed_remote(policy)?;
    let scope = scope(&workspace, &args.selection.targets)?;
    let mut changes = match NotifyChanges::start(workspace.root(), SystemClock.now()) {
        Ok(changes) => changes,
        Err(error) => return Ok(watch_ended(&error, paint, stderr)),
    };
    let mut reporter = TerminalReporter::new(&mut *stderr, mode, paint, Report::Build);
    let error = workspace.watch(
        &mut changes,
        &scope,
        &settings,
        RunOptions::new(on_failure, live).with_remote(policy),
        &mut reporter,
        console,
    );
    // The loop returns after closing its own row; this guarantees the error starts on a row of
    // its own whatever the reporter was left holding.
    reporter.close();
    Ok(watch_ended(&error, paint, stderr))
}

/// Writes the error that ended a watch to `stderr` and returns the status it ends the process
/// with.
fn watch_ended<E: Write>(error: &Error, paint: Paint, stderr: &mut E) -> Exit {
    let _ = stderr.write_all(watch_error_line(error, paint).as_bytes());
    Exit::of_error(error)
}

/// `test`: builds what the selected tests need and runs them, showing on `stderr` how it goes
/// and ending with a line that counts the tests, and returns the status of the most severe way a
/// target ended.
fn test<E: Write, C: Console>(
    start: &Path,
    args: TestArgs,
    remote: &RemoteSwitches,
    mode: Mode,
    paint: Paint,
    stderr: &mut E,
    console: &mut C,
) -> Result<Exit, Stop> {
    let (on_failure, live, rerun) = (args.on_failure(), args.live(), args.rerun());
    let report = Report::Test {
        blocks: args.blocks(),
        rerun,
    };
    let settings = overrides(args.selection.overrides)?;
    let workspace = Workspace::open(start)?;
    let policy = remote.resolve(
        &args.remote,
        remote_table(&workspace),
        workspace.local_workers(),
    )?;
    let scope = scope(&workspace, &args.selection.targets)?;
    let mut reporter = TerminalReporter::new(&mut *stderr, mode, paint, report);
    let tested = workspace.test(
        &scope,
        &settings,
        RunOptions::new(on_failure, live).with_remote(policy),
        rerun,
        &mut reporter,
        console,
    );
    if tested.is_err() {
        // The run ended before its summary: whatever is said next starts on a row of its own.
        reporter.close();
    }
    Ok(Exit::of_outcomes(&tested?))
}

/// `verify`: re-runs the cached actions of the selected targets in the input sandbox and says on
/// `stderr` which reproduced their cached outputs and which did not, and returns the status of
/// the most severe way a target ended. Nothing is recorded.
fn verify<E: Write, C: Console>(
    start: &Path,
    args: VerifyArgs,
    paint: Paint,
    stderr: &mut E,
    console: &mut C,
) -> Result<Exit, Stop> {
    let sample = args.sample();
    let settings = overrides(args.overrides)?;
    let workspace = Workspace::open(start)?;
    let scope = scope(&workspace, &args.targets)?;
    let verification = workspace.verify(&scope, &settings, sample, console)?;
    let _ = stderr.write_all(verify_text(&verification, paint).as_bytes());
    Ok(Exit::of_verification(&verification))
}

/// `log`: writes to `stderr` the header of the target's last logged run and a warning of any
/// damaged line of the log, then the bytes that run printed as stored: standard output to
/// `stdout` and standard error to `stderr`, or the one stream asked for to `stdout`. A target
/// that fails the label grammar is a mistake of the command line.
fn log<O: Write, E: Write>(
    start: &Path,
    args: &LogArgs,
    stdout: &mut O,
    stderr: &mut E,
) -> Result<Exit, Stop> {
    let workspace = Workspace::open(start)?;
    let label = workspace.label(&args.target).map_err(|error| {
        Cli::command().error(
            ErrorKind::ValueValidation,
            format!("invalid value '{}' for '<TARGET>': {error}", args.target),
        )
    })?;
    let shown = workspace.log(&label)?;
    let _ = stderr.write_all(header(&shown.last).as_bytes());
    if let Some(warning) = damage(&shown.last.damaged) {
        let _ = stderr.write_all(warning.as_bytes());
    }
    match args.streams() {
        Streams::Both => {
            // The stored standard error is shown whether or not standard output can be written.
            let written = product(stdout, &shown.stdout);
            let _ = stderr.write_all(&shown.stderr);
            written?;
        }
        Streams::Stdout => product(stdout, &shown.stdout)?,
        Streams::Stderr => product(stdout, &shown.stderr)?,
    }
    Ok(Exit::Success)
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeMap;
    use std::fs;
    use std::io::{self, Write};
    use std::iter;
    use std::path::{Path, PathBuf};

    use buildl::Workspace;
    use buildl::approver::NoTerminal;
    use buildl::buildl_core::json::canonical;
    use buildl::buildl_core::{Diagnostic, Error, PortName, Scope};
    use buildl::report::{Paint, error_text};
    use clap::Parser;

    use super::{Stop, run, told, watch_ended};
    use crate::cli::Cli;
    use crate::exit::Exit;
    use crate::remote_switches::RemoteSwitches;
    use crate::terminal::Terminal;

    /// The manifest text up to the `strategy` line: allows a build to write its outputs and its
    /// state, to read every file, and to run `cp`, one action at a time.
    macro_rules! manifest_head {
        () => {
            "[ceiling]\nproc.run = [\"cp\"]\nfs.read = [\".\"]\nfs.write = [\"out\", \".buildl\"]\n\n[execution]\nworkers = 1\n"
        };
    }

    /// A manifest that allows a build to write its outputs and its state, to read every file,
    /// and to run `cp`, one action at a time.
    const MANIFEST: &str = concat!(manifest_head!(), "strategy = \"bare\"\n");

    /// A manifest like [`MANIFEST`] that runs actions in the input sandbox.
    const SANDBOX_MANIFEST: &str = concat!(manifest_head!(), "strategy = \"sandbox\"\n");

    /// One target, `//:copy`, that copies `a.txt`.
    const COPY: &str = "buildl.target(\"copy\", { inputs = { \"a.txt\" }, run = { \"cp\", \"$in\", \"$out\" } })\n";

    /// A manifest like [`MANIFEST`] that allows `sh` instead of `cp`.
    const SH_MANIFEST: &str = "[ceiling]\nproc.run = [\"sh\"]\nfs.read = [\".\"]\nfs.write = [\"out\", \".buildl\"]\n\n[execution]\nworkers = 1\nstrategy = \"bare\"\n";

    /// One target, `//:both`, that prints a line on each stream.
    const BOTH: &str = "buildl.target(\"both\", { run = { \"sh\", \"-c\", \"echo out; echo err >&2\" }, outputs = {} })\n";

    /// A temporary workspace holding each of `files`, and its canonical root.
    fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        for (path, text) in files {
            let path = root.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        (dir, root)
    }

    /// The workspace of [`COPY`] under [`MANIFEST`].
    fn copying() -> (tempfile::TempDir, PathBuf) {
        workspace(&[
            ("buildl.toml", MANIFEST),
            ("build.lua", COPY),
            ("a.txt", "a\n"),
        ])
    }

    /// A process whose standard error is a pipe.
    const fn piped() -> Terminal {
        Terminal {
            interactive: false,
            term: None,
            no_color: None,
        }
    }

    /// A process whose standard error is a terminal that takes colour, with `NO_COLOR` as
    /// given.
    fn on_a_terminal(no_color: Option<&str>) -> Terminal {
        Terminal {
            interactive: true,
            term: Some("xterm".into()),
            no_color: no_color.map(Into::into),
        }
    }

    /// What one run ended with and wrote.
    struct Ran {
        exit: Exit,
        stdout: String,
        stderr: String,
    }

    /// Runs `buildl` with `args` in `start`, as a process on `terminal` would.
    fn ran_on(terminal: &Terminal, start: &Path, args: &[&str]) -> Ran {
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let exit = run(
            iter::once(&"buildl").chain(args),
            Ok(start.to_path_buf()),
            terminal,
            &RemoteSwitches::default(),
            &mut stdout,
            &mut stderr,
            &mut NoTerminal,
        );
        Ran {
            exit,
            stdout: String::from_utf8(stdout).unwrap(),
            stderr: String::from_utf8(stderr).unwrap(),
        }
    }

    /// Runs `buildl` with `args` in `start`, with standard error a pipe.
    fn ran(start: &Path, args: &[&str]) -> Ran {
        ran_on(&piped(), start, args)
    }

    /// A stream every write to which fails the way its kind says.
    struct Failing(io::ErrorKind);

    impl Write for Failing {
        fn write(&mut self, _bytes: &[u8]) -> io::Result<usize> {
            Err(io::Error::from(self.0))
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::from(self.0))
        }
    }

    /// Runs `buildl graph` in `start` with a standard output that fails with `kind`, and
    /// returns the status and what standard error was sent.
    fn graph_into(kind: io::ErrorKind, start: &Path) -> (Exit, String) {
        let mut stderr = Vec::new();
        let exit = run(
            ["buildl", "graph"],
            Ok(start.to_path_buf()),
            &piped(),
            &RemoteSwitches::default(),
            &mut Failing(kind),
            &mut stderr,
            &mut NoTerminal,
        );
        (exit, String::from_utf8(stderr).unwrap())
    }

    #[test]
    fn a_remote_flag_in_a_workspace_without_a_remote_table_is_a_usage_error() {
        let (_dir, root) = copying();

        let build = ran(&root, &["build", "--remote-upload"]);
        let test = ran(&root, &["test", "--remote-strict"]);

        assert_eq!(build.exit, Exit::Usage, "{}", build.stderr);
        assert_eq!(test.exit, Exit::Usage, "{}", test.stderr);
        assert!(build.stderr.contains("[remote]"), "{}", build.stderr);
        assert!(!root.join("out").exists(), "something was built");
    }

    #[test]
    fn verify_writes_its_report_to_stderr() {
        let (_dir, root) = workspace(&[
            ("buildl.toml", SANDBOX_MANIFEST),
            ("build.lua", COPY),
            ("a.txt", "a\n"),
        ]);
        assert_eq!(ran(&root, &["build"]).exit, Exit::Success);

        let verified = ran(&root, &["verify"]);

        assert_eq!(verified.exit, Exit::Success);
        assert_eq!(verified.stdout, "");
        assert_eq!(
            verified.stderr,
            "verified //:copy\n1 verified, 0 mismatched, 0 failed, 0 skipped, 0 not cached, 0 not sampled\n"
        );
    }

    #[test]
    fn help_and_version_go_to_the_stdout_writer_and_exit_zero() {
        let (_dir, root) = copying();
        for (args, first_line) in [
            (&["--help"][..], "A sandboxed, deterministic build system\n"),
            (
                &["plan", "--help"][..],
                "Say what a build would run, and why\n",
            ),
            (
                &["build", "--help"][..],
                "Build targets and what they need\n",
            ),
            (&["--version"][..], "buildl 0.1.0\n"),
        ] {
            let ran = ran(&root, args);
            assert_eq!(ran.exit, Exit::Success, "{args:?}");
            assert!(
                ran.stdout.starts_with(first_line),
                "{args:?}: {}",
                ran.stdout
            );
            assert_eq!(ran.stderr, "", "{args:?}");
        }
    }

    #[test]
    fn a_usage_error_goes_to_the_stderr_writer_and_exits_two() {
        let (_dir, root) = copying();
        for (args, first_line) in [
            (&["bogus"][..], "error: unrecognized subcommand 'bogus'\n"),
            (&[][..], "A sandboxed, deterministic build system\n"),
            (
                &["graph", "--json", "--dot"][..],
                "error: the argument '--json' cannot be used with '--dot'\n",
            ),
            (
                &["build", "--set", "a=b", "--set", "a=c"][..],
                "error: setting a is given more than once\n",
            ),
            (
                &["plan", "--set", "a=b", "--set", "a=c"][..],
                "error: setting a is given more than once\n",
            ),
        ] {
            let ran = ran(&root, args);
            assert_eq!(ran.exit, Exit::Usage, "{args:?}");
            assert!(
                ran.stderr.starts_with(first_line),
                "{args:?}: {}",
                ran.stderr
            );
            assert_eq!(ran.stdout, "", "{args:?}");
        }
        assert!(!root.join(".buildl").exists());
    }

    #[test]
    fn a_target_that_fails_its_grammar_is_a_usage_error() {
        let (_dir, root) = copying();
        for args in [
            &["plan", "a b"][..],
            &["plan", "--json", "copy", "a b"][..],
            &["build", "a b"][..],
        ] {
            let ran = ran(&root, args);
            assert_eq!(ran.exit, Exit::Usage, "{args:?}");
            assert_eq!(
                ran.stderr,
                concat!(
                    "error: invalid value 'a b' for '[TARGET]...': ",
                    "invalid target name: \"a b\" \u{2014} ",
                    "may hold only ASCII letters, digits, '.', '-' and '_'\n",
                    "\n",
                    "Usage: buildl [OPTIONS] <COMMAND>\n",
                    "\n",
                    "For more information, try '--help'.\n",
                ),
                "{args:?}"
            );
            assert_eq!(ran.stdout, "", "{args:?}");
        }
        // Nothing was built, and nothing was made to build it in.
        assert!(!root.join("out").exists());
        assert!(!root.join(".buildl").exists());
    }

    #[test]
    fn check_writes_one_line_to_stderr_and_nothing_to_stdout() {
        let (_dir, root) = copying();
        let ran = ran(&root, &["check"]);
        assert_eq!(ran.exit, Exit::Success);
        assert_eq!(ran.stderr, "ok: 1 declaration in 1 build file\n");
        assert_eq!(ran.stdout, "");
    }

    #[test]
    fn graph_writes_the_canonical_bytes_and_one_newline_to_stdout() {
        let (_dir, root) = copying();
        let (graph, _) = Workspace::open(&root).unwrap().graph().unwrap();
        let expected = format!("{}\n", canonical::to_string(&graph).unwrap());
        for args in [&["graph"][..], &["graph", "--json"][..]] {
            let ran = ran(&root, args);
            assert_eq!(ran.exit, Exit::Success, "{args:?}");
            assert_eq!(ran.stdout, expected, "{args:?}");
            assert_eq!(ran.stderr, "", "{args:?}");
        }
    }

    #[test]
    fn graph_dot_writes_the_drawing_to_stdout() {
        let (_dir, root) = copying();
        let ran = ran(&root, &["graph", "--dot"]);
        assert_eq!(ran.exit, Exit::Success);
        assert_eq!(ran.stdout, "digraph buildl {\n  \"//:copy\";\n}\n");
        assert_eq!(ran.stderr, "");
    }

    #[test]
    fn query_writes_the_answer_to_stdout_in_either_form() {
        let (_dir, root) = copying();
        // //:copy depends on nothing: an empty answer is a success, and says nothing.
        let ran_text = ran(&root, &["query", "deps(//:copy)"]);
        assert_eq!(ran_text.exit, Exit::Success);
        assert_eq!(ran_text.stdout, "");
        assert_eq!(ran_text.stderr, "");
        let ran_json = ran(&root, &["query", "--json", "rdeps(copy)"]);
        assert_eq!(ran_json.exit, Exit::Success);
        assert_eq!(ran_json.stdout, "[]\n");
        assert_eq!(ran_json.stderr, "");
        assert!(!root.join(".buildl").exists());
    }

    #[test]
    fn a_query_in_neither_form_is_a_usage_error() {
        let (_dir, root) = copying();
        let ran = ran(&root, &["query", "dep(//:copy)"]);
        assert_eq!(ran.exit, Exit::Usage);
        assert_eq!(ran.stdout, "");
        assert_eq!(
            ran.stderr,
            concat!(
                "error: invalid value 'dep(//:copy)' for '<EXPR>': ",
                "query `dep(//:copy)` is not deps(<target>) or rdeps(<target>)\n",
                "\n",
                "Usage: buildl [OPTIONS] <COMMAND>\n",
                "\n",
                "For more information, try '--help'.\n",
            )
        );
    }

    #[test]
    fn a_query_whose_target_fails_its_grammar_is_a_usage_error() {
        let (_dir, root) = copying();
        let ran = ran(&root, &["query", "deps(//:)"]);
        assert_eq!(ran.exit, Exit::Usage);
        assert_eq!(ran.stdout, "");
        assert_eq!(
            ran.stderr,
            concat!(
                "error: invalid value 'deps(//:)' for '<EXPR>': ",
                "invalid target name: \"\" \u{2014} must not be empty\n",
                "\n",
                "Usage: buildl [OPTIONS] <COMMAND>\n",
                "\n",
                "For more information, try '--help'.\n",
            )
        );
    }

    #[test]
    fn a_query_for_an_undeclared_target_is_a_usage_error() {
        let (_dir, root) = copying();
        let ran = ran(&root, &["query", "rdeps(//:cpy)"]);
        assert_eq!(ran.exit, Exit::Usage);
        assert_eq!(ran.stdout, "");
        assert_eq!(
            ran.stderr,
            "error: target //:cpy was asked for but is not declared (did you mean //:copy?)\n"
        );
    }

    #[test]
    fn log_writes_the_stored_streams_to_their_writers() {
        let (_dir, root) = copying();
        assert_eq!(ran(&root, &["build"]).exit, Exit::Success);
        let ran = ran(&root, &["log", "copy"]);
        assert_eq!(ran.exit, Exit::Success);
        assert_eq!(ran.stdout, "");
        assert!(
            ran.stderr.starts_with("//:copy: built at "),
            "{}",
            ran.stderr
        );
        assert!(ran.stderr.ends_with(" UTC\n"), "{}", ran.stderr);
        assert_eq!(ran.stderr.lines().count(), 1, "{}", ran.stderr);
    }

    #[test]
    fn log_writes_stored_stderr_when_stdout_is_closed() {
        let (_dir, root) = workspace(&[("buildl.toml", SH_MANIFEST), ("build.lua", BOTH)]);
        assert_eq!(ran(&root, &["build"]).exit, Exit::Success);
        let mut stderr = Vec::new();
        let exit = run(
            ["buildl", "log", "both"],
            Ok(root),
            &piped(),
            &RemoteSwitches::default(),
            &mut Failing(io::ErrorKind::BrokenPipe),
            &mut stderr,
            &mut NoTerminal,
        );
        assert_eq!(exit, Exit::Success);
        let stderr = String::from_utf8(stderr).unwrap();
        assert!(stderr.starts_with("//:both: built at "), "{stderr}");
        assert!(stderr.ends_with(" UTC\nerr\n"), "{stderr}");
    }

    #[test]
    fn log_warns_of_damaged_lines() {
        let (_dir, root) = copying();
        assert_eq!(ran(&root, &["build"]).exit, Exit::Success);
        let log = root.join(".buildl/log.jsonl");
        let before = fs::read_to_string(&log).unwrap();
        let line = before.lines().count() + 1;
        fs::write(&log, format!("{before}junk\n")).unwrap();
        let ran = ran(&root, &["log", "copy"]);
        assert_eq!(ran.exit, Exit::Success);
        let (header, warning) = ran.stderr.split_once('\n').unwrap();
        assert!(header.starts_with("//:copy: built at "), "{}", ran.stderr);
        assert_eq!(
            warning,
            format!("log.jsonl: 1 damaged line skipped (line {line})\n")
        );
    }

    #[test]
    fn plan_writes_its_lines_to_stdout() {
        let (_dir, root) = copying();
        let ran = ran(&root, &["plan"]);
        assert_eq!(ran.exit, Exit::Success);
        assert_eq!(
            ran.stdout,
            "run    //:copy: never built\n  run programs: cp\n  \
             write into:   .buildl, out\n1 to run, 0 conditional, 0 clean\n"
        );
        assert_eq!(ran.stderr, "");
    }

    #[test]
    fn plan_json_writes_the_canonical_bytes_and_one_newline_to_stdout() {
        let (_dir, root) = copying();
        let ran = ran(&root, &["plan", "--json", "copy"]);
        let plan = Workspace::open(&root)
            .unwrap()
            .plan(&Scope::Workspace, &BTreeMap::new())
            .unwrap();
        assert_eq!(ran.exit, Exit::Success);
        assert_eq!(
            ran.stdout,
            format!("{}\n", canonical::to_string(&plan).unwrap())
        );
        assert_eq!(ran.stderr, "");
    }

    #[test]
    fn a_build_of_nothing_says_so_on_stderr() {
        let (_dir, root) = workspace(&[("buildl.toml", MANIFEST), ("build.lua", "")]);
        let ran = ran(&root, &["build"]);
        assert_eq!(ran.exit, Exit::Success);
        assert_eq!(ran.stderr, "nothing to build\n");
        assert_eq!(ran.stdout, "");
    }

    #[test]
    fn a_test_run_of_a_workspace_without_tests_says_so_on_stderr() {
        let (_dir, root) = copying();
        let ran = ran(&root, &["test"]);
        assert_eq!(ran.exit, Exit::Success);
        assert_eq!(ran.stderr, "no tests\n");
        assert_eq!(ran.stdout, "");
    }

    #[test]
    fn an_error_is_rendered_on_stderr_with_the_code_of_its_variant() {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        for command in ["check", "graph", "plan", "build"] {
            let ran = ran(&root, &[command]);
            assert_eq!(ran.exit, Exit::Definition, "{command}");
            assert_eq!(
                ran.stderr,
                format!(
                    "error: manifest: no buildl.toml in {} or any directory above it\n",
                    root.display()
                ),
                "{command}"
            );
            assert_eq!(ran.stdout, "", "{command}");
        }
    }

    #[test]
    fn a_build_that_ends_in_an_error_says_so_once() {
        // The manifest names a strategy this buildl cannot run.
        let (_dir, root) = workspace(&[
            ("buildl.toml", "[execution]\nstrategy = \"os-sandbox\"\n"),
            ("build.lua", COPY),
        ]);
        let ran = ran(&root, &["build"]);
        assert_eq!(ran.exit, Exit::Definition);
        assert!(
            ran.stderr
                .starts_with("error: manifest: buildl.toml: [execution] strategy is "),
            "{}",
            ran.stderr
        );
        assert_eq!(ran.stderr.lines().count(), 1);
        assert_eq!(ran.stdout, "");
    }

    #[test]
    fn an_undeclared_target_and_an_undeclared_setting_are_usage_errors() {
        let (_dir, root) = copying();
        let target = ran(&root, &["plan", "copies"]);
        assert_eq!(target.exit, Exit::Usage);
        assert_eq!(
            target.stderr,
            "error: target //:copies was asked for but is not declared (did you mean //:copy?)\n"
        );
        let setting = ran(&root, &["plan", "--set", "optimise=3"]);
        assert_eq!(setting.exit, Exit::Usage);
        assert_eq!(
            setting.stderr,
            "error: setting optimise is overridden but not declared\n"
        );
    }

    #[test]
    fn a_closed_stdout_is_success_and_silent() {
        let (_dir, root) = copying();
        let (exit, stderr) = graph_into(io::ErrorKind::BrokenPipe, &root);
        assert_eq!(exit, Exit::Success);
        assert_eq!(stderr, "");
    }

    #[test]
    fn a_product_that_cannot_be_written_is_an_infrastructure_failure() {
        let (_dir, root) = copying();
        let (exit, stderr) = graph_into(io::ErrorKind::StorageFull, &root);
        assert_eq!(exit, Exit::Infrastructure);
        assert!(
            stderr.starts_with("error: standard output cannot be written: "),
            "{stderr}"
        );
    }

    #[test]
    fn a_start_directory_that_cannot_be_read_is_an_infrastructure_failure() {
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let exit = run(
            ["buildl", "check"],
            Err(io::Error::from(io::ErrorKind::NotFound)),
            &piped(),
            &RemoteSwitches::default(),
            &mut stdout,
            &mut stderr,
            &mut NoTerminal,
        );
        assert_eq!(exit, Exit::Infrastructure);
        let stderr = String::from_utf8(stderr).unwrap();
        assert!(
            stderr.starts_with("error: the current directory cannot be read: "),
            "{stderr}"
        );
        assert_eq!(stdout, b"");
    }

    #[test]
    fn help_needs_no_start_directory() {
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let exit = run(
            ["buildl", "--version"],
            Err(io::Error::from(io::ErrorKind::NotFound)),
            &piped(),
            &RemoteSwitches::default(),
            &mut stdout,
            &mut stderr,
            &mut NoTerminal,
        );
        assert_eq!(exit, Exit::Success);
        assert_eq!(stdout, b"buildl 0.1.0\n");
        assert_eq!(stderr, b"");
    }

    #[test]
    fn an_error_is_coloured_on_a_terminal_unless_no_color_is_set() {
        let dir = tempfile::tempdir().unwrap();
        let painted = "\x1b[1;31merror\x1b[0m: manifest: ";

        let coloured = ran_on(&on_a_terminal(None), dir.path(), &["check"]);
        assert!(coloured.stderr.starts_with(painted), "{}", coloured.stderr);

        let quiet = ran_on(&on_a_terminal(Some("1")), dir.path(), &["check"]);
        assert!(
            quiet.stderr.starts_with("error: manifest: "),
            "{}",
            quiet.stderr
        );

        let never = ran_on(
            &on_a_terminal(None),
            dir.path(),
            &["--color", "never", "check"],
        );
        assert!(
            never.stderr.starts_with("error: manifest: "),
            "{}",
            never.stderr
        );

        let always = ran(dir.path(), &["check", "--color", "always"]);
        assert!(always.stderr.starts_with(painted), "{}", always.stderr);
    }

    #[test]
    fn what_the_argument_parser_says_is_never_coloured() {
        let error = Cli::try_parse_from(["buildl", "bogus"]).unwrap_err();
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        assert_eq!(told(&error, &mut stdout, &mut stderr), Exit::Usage);
        assert!(!stderr.contains(&0x1b));
        assert_eq!(stdout, b"");
    }

    #[test]
    fn a_watch_that_ends_says_why_and_exits_with_the_error_status() {
        let error = Error::Adapter {
            port: PortName::Changes,
            diagnostic: Diagnostic::new("the watcher stopped"),
        };
        let mut stderr = Vec::new();
        let exit = watch_ended(&error, Paint::Plain, &mut stderr);
        let expected = format!("watch: {}", error_text(&error, Paint::Plain));
        assert_eq!(String::from_utf8(stderr).unwrap(), expected);
        assert_eq!(exit, Exit::of_error(&error));
        assert_eq!(exit, Exit::Infrastructure);
    }

    #[test]
    fn each_way_a_command_stops_is_kept_as_what_it_is() {
        let usage = Cli::try_parse_from(["buildl", "bogus"]).unwrap_err();
        assert!(matches!(Stop::from(usage), Stop::Usage(_)));
        assert!(matches!(Stop::from(Error::ForeignPlan), Stop::Failed(_)));
        let unwritten = io::Error::from(io::ErrorKind::BrokenPipe);
        assert!(matches!(Stop::from(unwritten), Stop::Unwritten(_)));
    }
}
