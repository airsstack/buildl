//! The command line: its help, its refusals, and what each argument reaches.

use std::ffi::OsStr;
use std::fs;
use std::io;
use std::process::{Command, Stdio};

use crate::common::{append, code, manifest, path_with, program, run, stderr, stdout, workspace};

/// One target, `//:copy`, that copies `a.txt`.
const COPY: &str =
    "buildl.target(\"copy\", { inputs = { \"a.txt\" }, run = { \"cp\", \"$in\", \"$out\" } })\n";

/// One test target, `//:pass`, that prints a line and passes.
const PASS: &str = "buildl.test(\"pass\", { run = { \"sh\", \"-c\", \"echo ok\" } })\n";

/// The help of the `--color` option, as every command's help shows it.
const COLOR: &str = concat!(
    "      --color <WHEN>\n",
    "          When to colour what is printed\n",
    "\n",
    "          Possible values:\n",
    "          - auto:   Colour when standard error is a terminal and `NO_COLOR` is unset\n",
    "          - always: Always colour\n",
    "          - never:  Never colour\n",
    "          \n",
    "          [default: auto]\n",
    "\n",
);

/// The help of `--help` and `--version`, as every command's help ends.
const HELP_AND_VERSION: &str = concat!(
    "  -h, --help\n",
    "          Print help (see a summary with '-h')\n",
    "\n",
    "  -V, --version\n",
    "          Print version\n",
);

/// The help of the arguments `plan`, `build` and `test` share.
const SELECTION: &str = concat!(
    "Arguments:\n",
    "  [TARGET]...\n",
    "          Targets, as //dir:name, :name or name; with none, plan and build take every target but tests, and test every test\n",
    "\n",
);

/// The help of `--set`.
const SET: &str = concat!(
    "      --set <NAME=VALUE>\n",
    "          Give a build setting a value for this run; may be repeated\n",
    "\n",
);

/// The help of the argument of `log`.
const LOG_TARGET: &str = concat!(
    "Arguments:\n",
    "  <TARGET>\n",
    "          The target, as //dir:name, :name or name\n",
    "\n",
);

/// The help of `--stdout` and `--stderr`.
const STREAMS: &str = concat!(
    "      --stdout\n",
    "          Print only what the run wrote to standard output\n",
    "\n",
    "      --stderr\n",
    "          Print only what the run wrote to standard error, on standard output\n",
    "\n",
);

/// The help of the flags only `test` has.
const TEST_FLAGS: &str = concat!(
    "      --stop\n",
    "          Start nothing new after the first failure\n",
    "\n",
    "      --stream\n",
    "          Show one running action's output as it is printed\n",
    "\n",
    "      --rerun\n",
    "          Run the selected tests even when nothing they depend on changed\n",
    "\n",
    "      --verbose\n",
    "          Show what every test printed, not only what a failing one did\n",
    "\n",
);

/// The help of the remote cache flags `build` and `test` share.
const REMOTE_FLAGS: &str = concat!(
    "      --remote-upload\n",
    "          Upload what this run produced to the remote cache\n",
    "\n",
    "      --remote-strict\n",
    "          Fail the run when a call to the remote cache fails, instead of warning and building locally\n",
    "\n",
    "      --no-remote\n",
    "          Do not use the remote cache in this run, whatever the variables say\n",
    "\n",
);

/// What the binary writes to standard output for `args`, which it must answer with success and
/// nothing on standard error.
fn answered(args: &[&str]) -> String {
    let (_dir, root) = workspace(&[]);
    let output = run(&root, args, &[]);
    assert_eq!(code(&output), 0, "{args:?}");
    assert_eq!(stderr(&output), "", "{args:?}");
    stdout(&output)
}

/// What the binary writes to standard error for `args`, which it must refuse as a usage error
/// with nothing on standard output.
fn refused(args: &[&str]) -> String {
    let (_dir, root) = workspace(&[]);
    let output = run(&root, args, &[]);
    assert_eq!(code(&output), 2, "{args:?}");
    assert_eq!(stdout(&output), "", "{args:?}");
    stderr(&output)
}

#[test]
fn the_version_is_the_crates() {
    let version = concat!("buildl ", env!("CARGO_PKG_VERSION"), "\n");
    assert_eq!(answered(&["--version"]), version);
}

#[test]
fn help_names_every_command_and_flag() {
    assert_eq!(
        answered(&["--help"]),
        [
            "A sandboxed, deterministic build system\n",
            "\n",
            "Usage: buildl [OPTIONS] <COMMAND>\n",
            "\n",
            "Commands:\n",
            "  check    Evaluate every build file twice and run nothing\n",
            "  graph    Print the target graph\n",
            "  query    Ask what a target needs, or what needs it\n",
            "  plan     Say what a build would run, and why\n",
            "  build    Build targets and what they need\n",
            "  test     Run test targets and what they need\n",
            "  verify   Re-run cached actions and name those whose outputs differ\n",
            "  doctor   Print the workspace's ceiling, approver, run settings and what its build wants\n",
            "  log      Show what a target printed when it last ran\n",
            "  clean    Remove the output directory\n",
            "  gc       Drop rows of targets no longer declared and remove what nothing needs\n",
            "  plugins  Work with the plugins `buildl.toml` names\n",
            "  help     Print this message or the help of the given subcommand(s)\n",
            "\n",
            "Options:\n",
            COLOR,
            HELP_AND_VERSION,
        ]
        .concat()
    );
}

#[test]
fn each_command_has_its_help() {
    let cases = [
        (
            "check",
            [
                "Evaluate every build file twice and run nothing\n\n",
                "Usage: buildl check [OPTIONS]\n\n",
                "Options:\n",
                COLOR,
                HELP_AND_VERSION,
            ]
            .concat(),
        ),
        (
            "doctor",
            [
                "Print the workspace's ceiling, approver, run settings and what its build wants\n\n",
                "Usage: buildl doctor [OPTIONS]\n\n",
                "Options:\n",
                COLOR,
                HELP_AND_VERSION,
            ]
            .concat(),
        ),
        (
            "graph",
            [
                "Print the target graph\n\n",
                "Usage: buildl graph [OPTIONS]\n\n",
                "Options:\n",
                COLOR,
                "      --json\n",
                "          Print the graph as canonical JSON, the bytes of .buildl/graph.json (the default)\n",
                "\n",
                "      --dot\n",
                "          Print the graph in Graphviz DOT\n",
                "\n",
                HELP_AND_VERSION,
            ]
            .concat(),
        ),
        (
            "query",
            [
                "Ask what a target needs, or what needs it\n\n",
                "Usage: buildl query [OPTIONS] <EXPR>\n\n",
                "Arguments:\n",
                "  <EXPR>\n",
                "          deps(TARGET) for what TARGET needs, or rdeps(TARGET) for what needs it; TARGET as //dir:name, :name or name\n",
                "\n",
                "Options:\n",
                COLOR,
                "      --json\n",
                "          Print the answer as canonical JSON\n",
                "\n",
                HELP_AND_VERSION,
            ]
            .concat(),
        ),
        (
            "log",
            [
                "Show what a target printed when it last ran\n\n",
                "Usage: buildl log [OPTIONS] <TARGET>\n\n",
                LOG_TARGET,
                "Options:\n",
                COLOR,
                STREAMS,
                HELP_AND_VERSION,
            ]
            .concat(),
        ),
        (
            "verify",
            [
                "Re-run cached actions and name those whose outputs differ\n\n",
                "Usage: buildl verify [OPTIONS] [TARGET]...\n\n",
                "Arguments:\n",
                "  [TARGET]...\n",
                "          Targets, as //dir:name, :name or name; with none, every target but tests\n",
                "\n",
                "Options:\n",
                COLOR,
                SET,
                "      --sample <N>\n",
                "          Re-run only the N cached actions whose keys sort lowest: the same ones while their keys stay the same\n",
                "\n",
                HELP_AND_VERSION,
            ]
            .concat(),
        ),
    ];
    for (command, expected) in cases.into_iter().chain(selecting_helps()) {
        assert_eq!(answered(&[command, "--help"]), expected, "{command}");
    }
}

/// The help of each command that takes a selection of targets, by command.
fn selecting_helps() -> [(&'static str, String); 3] {
    [
        (
            "plan",
            [
                "Say what a build would run, and why\n\n",
                "Usage: buildl plan [OPTIONS] [TARGET]...\n\n",
                SELECTION,
                "Options:\n",
                COLOR,
                SET,
                "      --json\n",
                "          Print the plan as canonical JSON\n",
                "\n",
                HELP_AND_VERSION,
            ]
            .concat(),
        ),
        (
            "build",
            [
                "Build targets and what they need\n\n",
                "Usage: buildl build [OPTIONS] [TARGET]...\n\n",
                SELECTION,
                "Options:\n",
                COLOR,
                SET,
                "  -k, --keep-going\n",
                "          After a failure, still build everything that does not depend on it\n",
                "\n",
                "      --stream\n",
                "          Show one running action's output as it is printed\n",
                "\n",
                REMOTE_FLAGS,
                HELP_AND_VERSION,
            ]
            .concat(),
        ),
        (
            "test",
            [
                "Run test targets and what they need\n\n",
                "Usage: buildl test [OPTIONS] [TARGET]...\n\n",
                SELECTION,
                "Options:\n",
                COLOR,
                SET,
                TEST_FLAGS,
                REMOTE_FLAGS,
                HELP_AND_VERSION,
            ]
            .concat(),
        ),
    ]
}

#[test]
fn test_rejects_keep_going() {
    let (_dir, root) = workspace(&[]);
    let output = run(&root, &["test", "-k"], &[]);
    assert_eq!(code(&output), 2);
    assert_eq!(stdout(&output), "");
}

#[test]
fn test_accepts_stop_stream_rerun_verbose() {
    let (_dir, root) = workspace(&[("buildl.toml", &manifest(&["sh"])), ("build.lua", PASS)]);
    let output = run(
        &root,
        &["test", "--stop", "--stream", "--rerun", "--verbose"],
        &[],
    );
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(stdout(&output), "");
}

#[test]
fn log_rejects_both_stream_flags() {
    let (_dir, root) = workspace(&[]);
    let output = run(&root, &["log", "copy", "--stdout", "--stderr"], &[]);
    assert_eq!(code(&output), 2);
    assert_eq!(stdout(&output), "");
    assert!(
        stderr(&output)
            .starts_with("error: the argument '--stdout' cannot be used with '--stderr'\n"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn log_reads_a_relative_target() {
    let (_dir, root) = workspace(&[
        ("buildl.toml", &manifest(&["cp"])),
        ("build.lua", "buildl.subdir(\"lib\")\n"),
        (
            "lib/build.lua",
            "buildl.target(\"text\", { inputs = { \"text.c\" }, run = { \"cp\", \"$in\", \"$out\" } })\n",
        ),
        ("lib/text.c", "text\n"),
    ]);
    assert_eq!(code(&run(&root, &["build"], &[])), 0);
    let output = run(&root.join("lib"), &["log", "text"], &[]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert!(
        stderr(&output).starts_with("//lib:text: built at "),
        "{}",
        stderr(&output)
    );
}

#[test]
fn log_rejects_a_bad_label_with_exit_2() {
    let (_dir, root) = workspace(&[("buildl.toml", &manifest(&["sh"])), ("build.lua", "")]);
    let output = run(&root, &["log", "//lib"], &[]);
    assert_eq!(code(&output), 2);
    assert_eq!(stdout(&output), "");
    assert!(
        stderr(&output).starts_with("error: invalid value '//lib' for '<TARGET>'"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn no_command_prints_the_short_help_on_stderr() {
    assert_eq!(
        refused(&[]),
        concat!(
            "A sandboxed, deterministic build system\n",
            "\n",
            "Usage: buildl [OPTIONS] <COMMAND>\n",
            "\n",
            "Commands:\n",
            "  check    Evaluate every build file twice and run nothing\n",
            "  graph    Print the target graph\n",
            "  query    Ask what a target needs, or what needs it\n",
            "  plan     Say what a build would run, and why\n",
            "  build    Build targets and what they need\n",
            "  test     Run test targets and what they need\n",
            "  verify   Re-run cached actions and name those whose outputs differ\n",
            "  doctor   Print the workspace's ceiling, approver, run settings and what its build wants\n",
            "  log      Show what a target printed when it last ran\n",
            "  clean    Remove the output directory\n",
            "  gc       Drop rows of targets no longer declared and remove what nothing needs\n",
            "  plugins  Work with the plugins `buildl.toml` names\n",
            "  help     Print this message or the help of the given subcommand(s)\n",
            "\n",
            "Options:\n",
            "      --color <WHEN>  When to colour what is printed [default: auto] [possible values: auto, always, never]\n",
            "  -h, --help          Print help (see more with '--help')\n",
            "  -V, --version       Print version\n",
        )
    );
}

#[test]
fn a_refused_command_line_says_why_and_how_to_get_help() {
    let top = "\nUsage: buildl [OPTIONS] <COMMAND>\n\nFor more information, try '--help'.\n";
    let cases = [
        (
            &["build", "--set", "a=b", "--set", "a=c"][..],
            format!("error: setting a is given more than once\n{top}"),
        ),
        (
            &["build", "--set", "novalue"][..],
            concat!(
                "error: invalid value 'novalue' for '--set <NAME=VALUE>': expected NAME=VALUE\n",
                "\n",
                "For more information, try '--help'.\n",
            )
            .to_owned(),
        ),
        (
            &["graph", "--json", "--dot"][..],
            concat!(
                "error: the argument '--json' cannot be used with '--dot'\n",
                "\n",
                "Usage: buildl graph --json\n",
                "\n",
                "For more information, try '--help'.\n",
            )
            .to_owned(),
        ),
        (
            &["check", "extra"][..],
            concat!(
                "error: unexpected argument 'extra' found\n",
                "\n",
                "Usage: buildl check [OPTIONS]\n",
                "\n",
                "For more information, try '--help'.\n",
            )
            .to_owned(),
        ),
        (
            &["-C", "/tmp", "plan"][..],
            format!("error: unexpected argument '-C' found\n{top}"),
        ),
        (
            &["build", "--keep-goin"][..],
            concat!(
                "error: unexpected argument '--keep-goin' found\n",
                "\n",
                "  tip: a similar argument exists: '--keep-going'\n",
                "\n",
                "Usage: buildl build --keep-going [TARGET]...\n",
                "\n",
                "For more information, try '--help'.\n",
            )
            .to_owned(),
        ),
        (
            &["query"][..],
            concat!(
                "error: the following required arguments were not provided:\n",
                "  <EXPR>\n",
                "\n",
                "Usage: buildl query <EXPR>\n",
                "\n",
                "For more information, try '--help'.\n",
            )
            .to_owned(),
        ),
        (
            &["bogus"][..],
            format!("error: unrecognized subcommand 'bogus'\n{top}"),
        ),
    ];
    for (args, expected) in cases {
        assert_eq!(refused(args), expected, "{args:?}");
    }
}

#[test]
fn set_reaches_the_plan() {
    let (_dir, root) = workspace(&[
        ("buildl.toml", &manifest(&["cp"])),
        (
            "build.lua",
            concat!(
                "buildl.option(\"from\", { default = \"a.txt\" })\n",
                "buildl.option(\"level\", { default = \"1\" })\n",
                "buildl.target(\"copy\", { inputs = { \"a.txt\", \"b.txt\" }, run = { \"cp\", \"$opt:from\", \"$out\" } })\n",
            ),
        ),
        ("a.txt", "a\n"),
        ("b.txt", "b\n"),
    ]);
    assert_eq!(code(&run(&root, &["build"], &[])), 0);
    let settings = ["--set", "from=b.txt", "--set", "level=2"];
    let output = run(&root, &[&["plan"][..], &settings].concat(), &[]);
    assert_eq!(code(&output), 0);
    assert_eq!(
        stdout(&output),
        "run    //:copy: setting from changed\n  run programs: cp\n  \
         write into:   .buildl, out\n1 to run, 0 conditional, 0 clean\n"
    );
    let built = run(&root, &[&["build"][..], &settings].concat(), &[]);
    assert_eq!(code(&built), 0);
    assert_eq!(fs::read(root.join("out/copy")).unwrap(), b"b\n");
}

#[test]
fn keep_going_reaches_the_build() {
    let (_dir, root) = workspace(&[
        ("buildl.toml", &manifest(&["cp", "false"])),
        (
            "build.lua",
            concat!(
                "local b = buildl\n",
                "b.target(\"bad\",    { run = { \"false\" } })\n",
                "b.target(\"after\",  { deps = { \"bad\" },   inputs = { \"x.txt\" }, run = { \"cp\", \"$in\", \"$out\" } })\n",
                "b.target(\"after2\", { deps = { \"after\" }, run = { \"cp\", \"$deps\", \"$out\" } })\n",
                "b.target(\"good\",   { inputs = { \"x.txt\" }, run = { \"cp\", \"$in\", \"$out\" } })\n",
                "b.target(\"good2\",  { deps = { \"good\" },  run = { \"cp\", \"$deps\", \"$out\" } })\n",
            ),
        ),
        ("x.txt", "x\n"),
    ]);
    let failed = concat!(
        "FAILED //:bad: action failure (exit status 1)\n",
        "    declared in build.lua\n",
        "    command: false\n",
    );
    let first = run(&root, &["build", "--keep-going"], &[]);
    assert_eq!(code(&first), 1);
    assert_eq!(
        stderr(&first),
        format!("{failed}1 failed, 2 skipped, 2 built\n")
    );
    assert_eq!(fs::read(root.join("out/good2")).unwrap(), b"x\n");

    // A failure is never recorded, so the failed target runs again.
    let second = run(&root, &["build", "-k"], &[]);
    assert_eq!(code(&second), 1);
    assert_eq!(
        stderr(&second),
        format!("{failed}1 failed, 2 skipped, 2 cached\n")
    );
}

#[test]
fn an_env_value_change_rebuilds_the_target_that_reads_it() {
    let (_dir, root) = workspace(&[
        (
            "buildl.toml",
            "[ceiling]\nproc.run = [\"cp\"]\nenv.read = [\"BUILDL_FIXTURE_FLAVOR\"]\nfs.read = [\".\"]\nfs.write = [\"out\", \".buildl\"]\n\n[execution]\nworkers = 1\nstrategy = \"bare\"\n",
        ),
        (
            "build.lua",
            "buildl.target(\"flavor\", { inputs = { \"a.txt\" }, env = { \"BUILDL_FIXTURE_FLAVOR\" }, run = { \"cp\", \"$in\", \"$out\" } })\n",
        ),
        ("a.txt", "a\n"),
    ]);
    let flavor = |value: &'static str| [("BUILDL_FIXTURE_FLAVOR", OsStr::new(value))];
    let steps: [(&[(&str, &OsStr)], &str); 4] = [
        (&[], "1 built\n"),
        (&flavor("vanilla"), "1 built\n"),
        (&flavor("vanilla"), "1 cached\n"),
        (&flavor(""), "1 built\n"),
    ];
    for (variables, expected) in steps {
        let output = run(&root, &["build"], variables);
        assert_eq!(code(&output), 0, "{variables:?}");
        assert_eq!(stderr(&output), expected, "{variables:?}");
    }
}

#[test]
fn a_changed_tool_is_the_reason_its_targets_run() {
    let (_dir, root) = workspace(&[
        ("buildl.toml", &manifest(&["mytool"])),
        (
            "build.lua",
            "buildl.target(\"made\", { inputs = { \"a.txt\" }, run = { \"mytool\", \"$in\", \"$out\" } })\n",
        ),
        ("a.txt", "a\n"),
    ]);
    let tools = tempfile::tempdir().unwrap();
    let bin = fs::canonicalize(tools.path()).unwrap();
    program(&bin, "mytool", "#!/bin/sh\n/bin/cp \"$1\" \"$2\"\n");
    let path = path_with(&bin);
    let search = [("PATH", path.as_os_str())];

    assert_eq!(code(&run(&root, &["build"], &search)), 0);
    append(&bin, "mytool", "# a newer release\n");
    let output = run(&root, &["plan"], &search);
    assert_eq!(code(&output), 0);
    assert_eq!(
        stdout(&output),
        "run    //:made: tool mytool changed\n  run programs: mytool\n  \
         write into:   .buildl, out\n1 to run, 0 conditional, 0 clean\n"
    );
}

#[test]
fn the_workspace_is_found_from_a_subdirectory() {
    let (_dir, root) = workspace(&[
        ("buildl.toml", &manifest(&["cp"])),
        ("build.lua", "buildl.subdir(\"lib\")\n"),
        (
            "lib/build.lua",
            "buildl.target(\"words\", { inputs = { \"words.txt\" }, run = { \"cp\", \"$in\", \"$out\" } })\n",
        ),
        ("lib/words.txt", "pear\n"),
        ("lib/other/kept.txt", ""),
    ]);
    let from_root = run(&root, &["graph"], &[]);
    let lib = root.join("lib");
    let from_lib = run(&lib, &["graph"], &[]);
    assert_eq!(code(&from_lib), 0);
    assert_eq!(from_lib.stdout, from_root.stdout);

    let built = run(&lib, &["build", "words"], &[]);
    assert_eq!(code(&built), 0);
    assert_eq!(stderr(&built), "1 built\n");
    assert_eq!(fs::read(root.join("out/lib/words")).unwrap(), b"pear\n");
    assert!(!lib.join("out").exists());
    assert!(!lib.join(".buildl").exists());

    // A target named without its directory is read against where the command started.
    let from_lib = run(&lib, &["query", "deps(words)"], &[]);
    assert_eq!(code(&from_lib), 0);
    assert_eq!(stdout(&from_lib), "");
    let rooted = run(&root, &["query", "rdeps(//lib:words)"], &[]);
    assert_eq!(code(&rooted), 0);
    assert_eq!(stdout(&rooted), "");
    // `//lib:words` is 3 edits from `//:words`, past the suggestion limit, so none is made.
    let bare = run(&root, &["query", "deps(words)"], &[]);
    assert_eq!(code(&bare), 2);
    assert_eq!(stdout(&bare), "");
    assert_eq!(
        stderr(&bare),
        "error: target //:words was asked for but is not declared\n"
    );
}

#[test]
fn clean_and_gc_take_no_arguments() {
    for command in ["clean", "gc"] {
        let said = refused(&[command, "x"]);
        assert!(
            said.starts_with("error: unexpected argument 'x' found\n"),
            "{command}: {said}"
        );
    }
}

#[test]
fn a_closed_stdout_is_success_and_silent() {
    let (_dir, root) = workspace(&[("buildl.toml", &manifest(&["cp"])), ("build.lua", COPY)]);
    let (reader, writer) = io::pipe().unwrap();
    // Nobody reads what the command is asked for: the pipe's only reader is gone before it
    // starts.
    drop(reader);
    let output = Command::new(env!("CARGO_BIN_EXE_buildl"))
        .arg("graph")
        .current_dir(&root)
        .stdout(Stdio::from(writer))
        .stderr(Stdio::piped())
        .output()
        .unwrap();
    assert_eq!(code(&output), 0);
    assert_eq!(stderr(&output), "");
}
