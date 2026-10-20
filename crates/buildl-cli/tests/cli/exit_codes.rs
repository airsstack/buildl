//! The status the process ends with, for each way a command can end, as the number a shell sees.

use std::ffi::OsStr;
use std::fs::{self, File};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::symlink;
use std::path::Path;

use crate::common::{append, code, manifest, program, run, stderr, stdout, workspace, write};

/// One target, `//:copy`, that copies `a.txt`.
const COPY: &str =
    "buildl.target(\"copy\", { inputs = { \"a.txt\" }, run = { \"cp\", \"$in\", \"$out\" } })\n";

/// What the binary ends with and writes to standard error when `args` is run at `root`, which
/// must write nothing to standard output.
fn failed(root: &Path, args: &[&str]) -> (i32, String) {
    let output = run(root, args, &[]);
    assert_eq!(stdout(&output), "", "{args:?}");
    (code(&output), stderr(&output))
}

#[test]
fn success_exits_zero() {
    let (_dir, root) = workspace(&[
        ("buildl.toml", &manifest(&["cp"])),
        ("build.lua", COPY),
        ("a.txt", "a\n"),
    ]);
    assert_eq!(failed(&root, &["build"]), (0, "1 built\n".to_owned()));
    assert_eq!(fs::read(root.join("out/copy")).unwrap(), b"a\n");
}

#[test]
fn an_action_failure_has_its_code() {
    let (_dir, root) = workspace(&[
        ("buildl.toml", &manifest(&["false"])),
        (
            "build.lua",
            "buildl.target(\"bad\", { run = { \"false\" } })\n",
        ),
    ]);
    assert_eq!(
        failed(&root, &["build"]),
        (
            1,
            concat!(
                "FAILED //:bad: action failure (exit status 1)\n",
                "    declared in build.lua\n",
                "    command: false\n",
                "1 failed\n",
            )
            .to_owned()
        )
    );
}

#[test]
fn an_action_a_signal_ended_is_an_action_failure() {
    let (_dir, root) = workspace(&[
        ("buildl.toml", &manifest(&["sh"])),
        (
            "build.lua",
            "buildl.target(\"killed\", { run = { \"sh\", \"-c\", \"kill -9 $$\" } })\n",
        ),
    ]);
    assert_eq!(
        failed(&root, &["build"]),
        (
            1,
            concat!(
                "FAILED //:killed: action failure (ended by signal 9)\n",
                "    declared in build.lua\n",
                "    command: sh -c \"kill -9 $$\"\n",
                "1 failed\n",
            )
            .to_owned()
        )
    );
}

#[test]
fn a_missing_output_is_an_action_failure() {
    let (_dir, root) = workspace(&[
        ("buildl.toml", &manifest(&["true"])),
        (
            "build.lua",
            "buildl.target(\"empty\", { run = { \"true\" } })\n",
        ),
    ]);
    assert_eq!(
        failed(&root, &["build"]),
        (
            1,
            concat!(
                "FAILED //:empty: action failure (declared output empty was not produced)\n",
                "    declared in build.lua\n",
                "    command: true\n",
                "1 failed\n",
            )
            .to_owned()
        )
    );
}

#[test]
fn an_output_that_cannot_be_kept_is_an_action_failure() {
    let (_dir, root) = workspace(&[
        ("buildl.toml", &manifest(&["sh"])),
        (
            "build.lua",
            "buildl.target(\"link\", { run = { \"sh\", \"-c\", \"/bin/ln -s nowhere out/link\" } })\n",
        ),
    ]);
    assert_eq!(
        failed(&root, &["build"]),
        (
            1,
            concat!(
                "FAILED //:link: action failure (output link cannot be kept: ",
                "it is or holds a symbolic link)\n",
                "    link is a symbolic link\n",
                "    declared in build.lua\n",
                "    command: sh -c \"/bin/ln -s nowhere out/link\"\n",
                "1 failed\n",
            )
            .to_owned()
        )
    );
}

#[test]
fn a_timeout_is_an_infrastructure_failure() {
    let (_dir, root) = workspace(&[
        (
            "buildl.toml",
            "[ceiling]\nproc.run = [\"sleep\"]\nfs.read = [\".\"]\nfs.write = [\"out\", \".buildl\"]\n\n[execution]\nworkers = 1\ntimeout = \"200ms\"\nstrategy = \"bare\"\n",
        ),
        (
            "build.lua",
            "buildl.target(\"slow\", { run = { \"sleep\", \"30\" } })\n",
        ),
    ]);
    assert_eq!(
        failed(&root, &["build"]),
        (
            3,
            concat!(
                "FAILED //:slow: infrastructure failure (the command ran longer than the timeout)\n",
                "    declared in build.lua\n",
                "    command: sleep 30\n",
                "1 failed\n",
            )
            .to_owned()
        )
    );
}

#[test]
fn a_command_that_cannot_be_started_is_an_infrastructure_failure() {
    let (_dir, root) = workspace(&[
        (
            "buildl.toml",
            "[ceiling]\nproc.run = [\"tools/start\"]\nfs.read = [\".\"]\nfs.write = [\"out\", \".buildl\"]\n\n[execution]\nworkers = 1\nstrategy = \"bare\"\n",
        ),
        (
            "build.lua",
            "buildl.target(\"start\", { run = { \"tools/start\" } })\n",
        ),
    ]);
    // A script whose interpreter does not exist is found, and cannot be started.
    program(&root, "tools/start", "#!/nonexistent/interpreter\n");
    assert_eq!(
        failed(&root, &["build"]),
        (
            3,
            format!(
                concat!(
                    "FAILED //:start: infrastructure failure (the command could not be started)\n",
                    "    exec strategy: //:start: running {}: No such file or directory (os error 2)\n",
                    "    declared in build.lua\n",
                    "    command: tools/start\n",
                    "1 failed\n",
                ),
                root.join("tools/start").display()
            )
        )
    );
}

#[test]
fn a_bad_argument_is_a_usage_error() {
    let (_dir, root) = workspace(&[]);
    let (status, said) = failed(&root, &["bogus"]);
    assert_eq!(status, 2);
    assert!(
        said.starts_with("error: unrecognized subcommand 'bogus'\n"),
        "{said}"
    );
}

#[test]
fn no_command_is_a_usage_error() {
    let (_dir, root) = workspace(&[]);
    let (status, said) = failed(&root, &[]);
    assert_eq!(status, 2);
    assert!(
        said.starts_with("A sandboxed, deterministic build system\n"),
        "{said}"
    );
}

#[test]
fn an_unknown_target_is_a_usage_error() {
    let (_dir, root) = workspace(&[
        ("buildl.toml", &manifest(&["cp"])),
        (
            "build.lua",
            "buildl.target(\"lib\", { run = { \"cp\" } })\n",
        ),
    ]);
    for command in ["plan", "build"] {
        assert_eq!(
            failed(&root, &[command, "//:libs"]),
            (
                2,
                "error: target //:libs was asked for but is not declared (did you mean //:lib?)\n"
                    .to_owned()
            ),
            "{command}"
        );
    }
}

#[test]
fn a_query_in_neither_form_or_for_an_unknown_target_is_a_usage_error() {
    let (_dir, root) = workspace(&[
        ("buildl.toml", &manifest(&["cp"])),
        (
            "build.lua",
            "buildl.target(\"lib\", { run = { \"cp\" } })\n",
        ),
    ]);
    assert_eq!(
        failed(&root, &["query", "dep(//:lib)"]),
        (
            2,
            concat!(
                "error: invalid value 'dep(//:lib)' for '<EXPR>': ",
                "query `dep(//:lib)` is not deps(<target>) or rdeps(<target>)\n",
                "\n",
                "Usage: buildl [OPTIONS] <COMMAND>\n",
                "\n",
                "For more information, try '--help'.\n",
            )
            .to_owned()
        )
    );
    assert_eq!(
        failed(&root, &["query", "deps(//:libs)"]),
        (
            2,
            "error: target //:libs was asked for but is not declared (did you mean //:lib?)\n"
                .to_owned()
        )
    );
}

#[test]
fn an_undeclared_setting_is_a_usage_error() {
    let (_dir, root) = workspace(&[
        ("buildl.toml", &manifest(&["cp"])),
        (
            "build.lua",
            "buildl.target(\"lib\", { run = { \"cp\" } })\n",
        ),
    ]);
    assert_eq!(
        failed(&root, &["plan", "--set", "optimise=3"]),
        (
            2,
            "error: setting optimise is overridden but not declared\n".to_owned()
        )
    );
}

#[test]
fn a_program_outside_the_ceiling_is_refused() {
    let (_dir, root) = workspace(&[
        (
            "buildl.toml",
            "[ceiling]\nfs.read = [\".\"]\nfs.write = [\"out\", \".buildl\"]\n\n[execution]\nstrategy = \"bare\"\n",
        ),
        (
            "build.lua",
            "buildl.target(\"app\", { run = { \"cc\" } })\n",
        ),
    ]);
    for command in ["plan", "build"] {
        assert_eq!(
            failed(&root, &[command]),
            (
                4,
                "error: build.lua: //:app: program cc is outside the ceiling (proc.run allows: nothing)\n"
                    .to_owned()
            ),
            "{command}"
        );
    }
}

#[test]
fn a_state_directory_outside_the_ceiling_is_refused_before_it_is_made() {
    let (_dir, root) = workspace(&[
        (
            "buildl.toml",
            "[ceiling]\nproc.run = [\"cp\"]\nfs.read = [\".\"]\nfs.write = [\"out\"]\n\n[execution]\nstrategy = \"bare\"\n",
        ),
        ("build.lua", COPY),
        ("a.txt", "a\n"),
    ]);
    assert_eq!(
        failed(&root, &["plan"]),
        (
            4,
            "error: directory .buildl is outside the ceiling (fs.write allows: out)\n".to_owned()
        )
    );
    assert!(!root.join(".buildl").exists());
}

#[test]
fn a_bad_build_file_fails_check() {
    let (_dir, root) = workspace(&[
        ("buildl.toml", &manifest(&["cp"])),
        ("build.lua", "buildl.target(\"a\", {\n"),
    ]);
    assert_eq!(
        failed(&root, &["check"]),
        (
            5,
            concat!(
                "error: build.lua: syntax error: lua error in build.lua: ",
                "syntax error: build.lua:2: unexpected symbol near <eof>\n",
            )
            .to_owned()
        )
    );
}

/// A command, the build files it reads besides a manifest, and the whole of what it says on
/// standard error.
type Mistake = (
    &'static str,
    &'static [(&'static str, &'static str)],
    &'static str,
);

#[test]
fn each_mistake_of_a_build_file_is_a_definition_error() {
    let cases: [Mistake; 9] = [
        (
            "check",
            &[(
                "build.lua",
                "buildl.target(\"a b\", { run = { \"cp\" } })\n",
            )],
            "error: build.lua: declaration #0: invalid name: invalid target name: \"a b\" \u{2014} may hold only ASCII letters, digits, '.', '-' and '_'\n",
        ),
        (
            "check",
            &[(
                "build.lua",
                "buildl.target(\"a\", { inputs = { \"x\" } })\n",
            )],
            "error: build.lua: declaration #0: a target names neither 'rule' nor 'run'\n",
        ),
        (
            "check",
            &[
                ("build.lua", "buildl.subdir(\"lib\")\n"),
                ("lib/a.txt", "a\n"),
            ],
            "error: no build file in //lib (requested by build.lua)\n",
        ),
        (
            "graph",
            &[(
                "build.lua",
                "buildl.target(\"a\", { run = { \"cp\" } })\nbuildl.target(\"a\", { run = { \"cp\" } })\n",
            )],
            "error: //:a is declared more than once: build.lua, build.lua\n",
        ),
        (
            "graph",
            &[(
                "build.lua",
                "buildl.target(\"app\", { deps = { \"lib\" }, run = { \"cp\" } })\nbuildl.target(\"libs\", { run = { \"cp\" } })\n",
            )],
            "error: build.lua: //:app: dep //:lib is not declared (did you mean //:libs?)\n",
        ),
        (
            "graph",
            &[(
                "build.lua",
                "buildl.target(\"app\", { run = { \"cp\", \"$opt:mode\" } })\n",
            )],
            "error: build.lua: //:app: setting mode is not declared\n",
        ),
        (
            "graph",
            &[
                (
                    "build.lua",
                    "buildl.subdir(\"lib\")\nbuildl.target(\"app\", { deps = { \"//lib:text\" }, run = { \"cp\" } })\nbuildl.target(\"docs\", { deps = { \"app\" }, run = { \"cp\" } })\n",
                ),
                (
                    "lib/build.lua",
                    "buildl.target(\"text\", { deps = { \"//:docs\" }, run = { \"cp\" } })\n",
                ),
            ],
            "error: dependency cycle: //:app (build.lua) -> //lib:text (lib/build.lua) -> //:docs (build.lua) -> //:app\n",
        ),
        (
            "build",
            &[
                (
                    "build.lua",
                    "buildl.subdir(\"lib\")\nbuildl.target(\"x\", { outputs = { \"lib/y\" }, run = { \"cp\" } })\n",
                ),
                (
                    "lib/build.lua",
                    "buildl.target(\"y\", { run = { \"cp\" } })\n",
                ),
            ],
            "error: out/lib/y: //:x (build.lua) and //lib:y (lib/build.lua) cannot both write here\n",
        ),
        (
            "build",
            &[("build.lua", "buildl.target(\"x\", { run = { \"$in\" } })\n")],
            "error: build.lua: //:x: the command names no program once expanded\n",
        ),
    ];
    for (command, files, expected) in cases {
        let (_dir, root) = workspace(&[("buildl.toml", &manifest(&["cp"]))]);
        for (path, text) in files {
            write(&root, path, text);
        }
        assert_eq!(
            failed(&root, &[command]),
            (5, expected.to_owned()),
            "{command} {files:?}"
        );
    }
}

#[test]
fn a_missing_input_is_a_build_file_mistake() {
    let (_dir, root) = workspace(&[("buildl.toml", &manifest(&["cp"])), ("build.lua", COPY)]);
    assert_eq!(
        failed(&root, &["plan"]),
        (
            5,
            "error: build.lua: //:copy: input a.txt does not exist\n".to_owned()
        )
    );
}

#[test]
fn an_input_that_is_a_directory_or_a_link_is_a_build_file_mistake() {
    let (_dir, root) = workspace(&[("buildl.toml", &manifest(&["cp"])), ("build.lua", COPY)]);
    fs::create_dir(root.join("a.txt")).unwrap();
    assert_eq!(
        failed(&root, &["plan"]),
        (
            5,
            "error: build.lua: //:copy: input a.txt is a directory; an input must be a regular file\n"
                .to_owned()
        )
    );

    fs::remove_dir(root.join("a.txt")).unwrap();
    fs::write(root.join("real.txt"), "a\n").unwrap();
    symlink("real.txt", root.join("a.txt")).unwrap();
    assert_eq!(
        failed(&root, &["plan"]),
        (
            5,
            "error: build.lua: //:copy: input a.txt is a symbolic link; an input must be a regular file\n"
                .to_owned()
        )
    );
}

#[test]
fn an_unknown_manifest_key_is_refused_at_its_position() {
    let cases = [
        (
            "[ceiling]\nfs.wirte = [\"out\"]\n",
            "error: manifest: buildl.toml:2:4: unknown field `wirte`, expected `read` or `write`\n",
        ),
        (
            "[execution]\nworkers = \"four\"\n",
            "error: manifest: buildl.toml:2:11: invalid type: string \"four\", expected i64\n",
        ),
        (
            "[execution]\ntimeout = \"10\"\n",
            "error: manifest: buildl.toml:2:11: execution.timeout: invalid timeout: \"10\" \u{2014} must end in one of the units ms, s, m and h\n",
        ),
    ];
    for (manifest, expected) in cases {
        let (_dir, root) = workspace(&[("buildl.toml", manifest), ("build.lua", "")]);
        assert_eq!(
            failed(&root, &["check"]),
            (5, expected.to_owned()),
            "{manifest}"
        );
    }
}

#[test]
fn a_strategy_that_cannot_be_run_fails_build_and_not_plan() {
    let (_dir, root) = workspace(&[
        (
            "buildl.toml",
            "[ceiling]\nproc.run = [\"cp\"]\nfs.read = [\".\"]\nfs.write = [\"out\", \".buildl\"]\n",
        ),
        ("build.lua", COPY),
        ("a.txt", "a\n"),
    ]);
    let planned = run(&root, &["plan"], &[]);
    assert_eq!(code(&planned), 0);
    assert_eq!(
        failed(&root, &["build"]),
        (
            5,
            concat!(
                "error: manifest: buildl.toml: [execution] strategy is \"sandbox\" ",
                "(the default when it is not written), and this version of buildl runs \"bare\" only: ",
                "write strategy = \"bare\" under [execution]\n",
            )
            .to_owned()
        )
    );
    assert!(!root.join("out").exists());
}

#[test]
fn a_corrupt_cache_is_refused() {
    let (_dir, root) = workspace(&[
        ("buildl.toml", &manifest(&["cp"])),
        ("build.lua", COPY),
        ("a.txt", "a\n"),
    ]);
    assert_eq!(code(&run(&root, &["build"], &[])), 0);
    let cache = root.join(".buildl/cache.json");
    let length = fs::read(&cache).unwrap().len();
    append(&root, ".buildl/cache.json", " ");
    let expected = format!(
        concat!(
            "error: {file} is not a file buildl wrote: ",
            "JSON is not in canonical form: it differs from it at byte {length}\n",
            "    remove {file} and run the command again: every target is then built anew\n",
        ),
        file = cache.display(),
        length = length
    );
    for command in ["plan", "build"] {
        assert_eq!(
            failed(&root, &[command]),
            (6, expected.clone()),
            "{command}"
        );
    }
    // A command that reads only the build files is not stopped by it.
    assert_eq!(code(&run(&root, &["check"], &[])), 0);
}

#[test]
fn a_program_that_is_not_found_is_an_infrastructure_failure() {
    let (_dir, root) = workspace(&[
        ("buildl.toml", &manifest(&["cc"])),
        (
            "build.lua",
            "buildl.target(\"app\", { run = { \"cc\" } })\n",
        ),
    ]);
    let nowhere = tempfile::tempdir().unwrap();
    let output = run(&root, &["plan"], &[("PATH", nowhere.path().as_os_str())]);
    assert_eq!(code(&output), 3);
    assert_eq!(
        stderr(&output),
        "error: build.lua: //:app: program cc was not found\n"
    );
}

#[test]
fn a_value_that_is_not_unicode_fails_the_plan() {
    let (_dir, root) = workspace(&[
        (
            "buildl.toml",
            "[ceiling]\nproc.run = [\"cp\"]\nenv.read = [\"BUILDL_FIXTURE_FLAVOR\"]\nfs.read = [\".\"]\nfs.write = [\"out\", \".buildl\"]\n\n[execution]\nstrategy = \"bare\"\n",
        ),
        (
            "build.lua",
            "buildl.target(\"flavor\", { inputs = { \"a.txt\" }, env = { \"BUILDL_FIXTURE_FLAVOR\" }, run = { \"cp\", \"$in\", \"$out\" } })\n",
        ),
        ("a.txt", "a\n"),
    ]);
    let output = run(
        &root,
        &["plan"],
        &[("BUILDL_FIXTURE_FLAVOR", OsStr::from_bytes(b"caf\xe9"))],
    );
    assert_eq!(code(&output), 3);
    assert_eq!(
        stderr(&output),
        "error: host: environment variable BUILDL_FIXTURE_FLAVOR does not hold valid Unicode\n"
    );
}

#[test]
fn a_workspace_another_buildl_holds_is_refused() {
    let (_dir, root) = workspace(&[
        ("buildl.toml", &manifest(&["cp"])),
        ("build.lua", COPY),
        ("a.txt", "a\n"),
    ]);
    fs::create_dir(root.join(".buildl")).unwrap();
    let lock = File::create(root.join(".buildl/lock")).unwrap();
    lock.try_lock().unwrap();
    let expected = format!(
        "error: action cache: {} is in use by another buildl\n",
        root.join(".buildl/lock").display()
    );
    for command in ["plan", "build"] {
        assert_eq!(
            failed(&root, &[command]),
            (3, expected.clone()),
            "{command}"
        );
    }
    assert_eq!(code(&run(&root, &["check"], &[])), 0);
    drop(lock);
    assert_eq!(code(&run(&root, &["build"], &[])), 0);
}

#[test]
fn clean_and_gc_exit_0() {
    let (_dir, root) = workspace(&[
        ("buildl.toml", &manifest(&["cp"])),
        ("build.lua", COPY),
        ("a.txt", "a\n"),
    ]);
    assert_eq!(code(&run(&root, &["build"], &[])), 0);
    for command in ["clean", "gc"] {
        let output = run(&root, &[command], &[]);
        assert_eq!(code(&output), 0, "{command}");
        assert_eq!(stdout(&output), "", "{command}");
    }
}

#[test]
fn clean_and_gc_exit_0_without_a_strategy_line() {
    let (_dir, root) = workspace(&[
        (
            "buildl.toml",
            "[ceiling]\nproc.run = [\"cp\"]\nfs.read = [\".\"]\nfs.write = [\"out\", \".buildl\"]\n",
        ),
        ("build.lua", COPY),
        ("a.txt", "a\n"),
    ]);
    for command in ["clean", "gc"] {
        let output = run(&root, &[command], &[]);
        assert_eq!(code(&output), 0, "{command}");
    }
}

#[test]
fn clean_and_gc_exit_3_while_the_lock_is_held() {
    let (_dir, root) = workspace(&[
        ("buildl.toml", &manifest(&["cp"])),
        ("build.lua", COPY),
        ("a.txt", "a\n"),
    ]);
    fs::create_dir(root.join(".buildl")).unwrap();
    let lock = File::create(root.join(".buildl/lock")).unwrap();
    lock.try_lock().unwrap();
    let expected = format!(
        "error: action cache: {} is in use by another buildl\n",
        root.join(".buildl/lock").display()
    );
    for command in ["clean", "gc"] {
        assert_eq!(
            failed(&root, &[command]),
            (3, expected.clone()),
            "{command}"
        );
    }
    drop(lock);
    assert_eq!(code(&run(&root, &["clean"], &[])), 0);
}

#[test]
fn clean_exits_4_when_the_ceiling_leaves_out_the_output_directory() {
    let (_dir, root) = workspace(&[
        (
            "buildl.toml",
            "[ceiling]\nproc.run = [\"cp\"]\nfs.read = [\".\"]\nfs.write = [\".buildl\"]\n\n[execution]\nstrategy = \"bare\"\n",
        ),
        ("build.lua", COPY),
        ("a.txt", "a\n"),
    ]);
    let (status, said) = failed(&root, &["clean"]);
    assert_eq!(status, 4);
    assert!(
        said.starts_with("error: directory out is outside the ceiling"),
        "{said}"
    );
}

#[test]
fn a_state_directory_that_cannot_be_used_is_an_infrastructure_failure() {
    let (_dir, root) = workspace(&[
        ("buildl.toml", &manifest(&["cp"])),
        ("build.lua", COPY),
        ("a.txt", "a\n"),
        (".buildl/log.jsonl/kept", ""),
    ]);
    let (status, said) = failed(&root, &["build"]);
    assert_eq!(status, 3);
    assert_eq!(
        said,
        format!(
            "error: event log: appending to {}: Is a directory (os error 21)\n",
            root.join(".buildl/log.jsonl").display()
        )
    );
}

/// The workspace of [`COPY`], not built.
fn copying() -> (tempfile::TempDir, std::path::PathBuf) {
    workspace(&[
        ("buildl.toml", &manifest(&["cp"])),
        ("build.lua", COPY),
        ("a.txt", "a\n"),
    ])
}

/// The workspace of [`COPY`], built once.
fn built_copying() -> (tempfile::TempDir, std::path::PathBuf) {
    let (dir, root) = copying();
    assert_eq!(code(&run(&root, &["build"], &[])), 0);
    (dir, root)
}

#[test]
fn log_of_a_target_that_never_ran_exits_2() {
    let (_dir, root) = copying();
    assert_eq!(
        failed(&root, &["log", "copy"]),
        (
            2,
            "error: target //:copy has no run in the log\n".to_owned()
        )
    );
}

#[test]
fn log_with_a_blob_removed_exits_3() {
    let (_dir, root) = built_copying();
    fs::remove_dir_all(root.join(".buildl/cas")).unwrap();
    assert_eq!(
        failed(&root, &["log", "copy"]),
        (
            3,
            "error: target //:copy: the output of its last logged run is no longer in the store\n"
                .to_owned()
        )
    );
}

#[test]
fn log_with_a_broken_build_file_exits_0() {
    let (_dir, root) = built_copying();
    write(&root, "build.lua", "this is not lua\n");
    assert_eq!(code(&run(&root, &["log", "copy"], &[])), 0);
}

/// Two test targets, `//:pass` and `//:fail`, which fails.
const TESTS: &str = r#"buildl.test("pass", { run = { "sh", "-c", "echo ok" } })
buildl.test("fail", { run = { "sh", "-c", "echo bad; exit 1" } })
"#;

#[test]
fn test_with_a_failing_test_exits_1() {
    let (_dir, root) = workspace(&[("buildl.toml", &manifest(&["sh"])), ("build.lua", TESTS)]);
    let output = run(&root, &["test"], &[]);
    assert_eq!(stdout(&output), "");
    assert_eq!(code(&output), 1, "{}", stderr(&output));
}

#[test]
fn test_naming_a_build_target_exits_2() {
    let (_dir, root) = workspace(&[
        ("buildl.toml", &manifest(&["cp"])),
        ("build.lua", COPY),
        ("a.txt", "a\n"),
    ]);
    assert_eq!(
        failed(&root, &["test", "copy"]),
        (
            2,
            "error: target //:copy was asked for as a test, but //:copy is declared with b.target in build.lua\n"
                .to_owned()
        )
    );
}

#[test]
fn test_with_no_tests_exits_0() {
    let (_dir, root) = workspace(&[
        ("buildl.toml", &manifest(&["cp"])),
        ("build.lua", COPY),
        ("a.txt", "a\n"),
    ]);
    assert_eq!(failed(&root, &["test"]), (0, "no tests\n".to_owned()));
    assert!(!root.join("out/copy").exists());
}
