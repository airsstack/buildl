//! The status the process ends with, for each way a command can end, as the number a shell sees.

use std::ffi::OsStr;
use std::fs::{self, File};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::Command;

use buildl::approver::Approvals;

use crate::common::{
    append, code, data_home, manifest, program, run, run_at_home, stderr, stdout, workspace, write,
};

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
fn a_plugin_target_outside_its_cap_exits_4() {
    let (_dir, root) = workspace(&[
        (
            "buildl.toml",
            concat!(
                "[ceiling]\nproc.run = [\"cp\", \"sort\"]\nfs.read = [\".\"]\n",
                "fs.write = [\"out\", \".buildl\"]\n\n",
                "[execution]\nworkers = 1\nstrategy = \"bare\"\n\n",
                "[plugins.hello]\npath = \"plugins/hello\"\nproc.run = [\"cp\"]\n",
            ),
        ),
        ("build.lua", "buildl.use(\"hello\").greet(\"greet\")\n"),
        (
            "plugins/hello/extension.toml",
            "[extension]\nname = \"hello\"\nversion = \"0.1.0\"\nentry = \"main.lua\"\napi = 1\n\n[capabilities]\nproc.run = [\"cp\"]\n",
        ),
        (
            "plugins/hello/main.lua",
            "return { greet = function(name) buildl.target(name, { run = { \"sort\" } }) end }\n",
        ),
    ]);
    for command in ["plan", "build"] {
        let (status, said) = failed(&root, &[command]);
        assert_eq!(status, 4, "{command}: {said}");
        assert!(said.starts_with("error: "), "{command}: {said}");
        assert!(said.contains("hello"), "{command}: {said}");
    }
    assert!(!root.join("out").exists());
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
            "[ceiling]\nproc.run = [\"cp\"]\nfs.read = [\".\"]\nfs.write = [\"out\", \".buildl\"]\n\n[execution]\nstrategy = \"os-sandbox\"\n",
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
                "error: manifest: buildl.toml: [execution] strategy is \"os-sandbox\", ",
                "and this version of buildl runs \"bare\" and \"sandbox\" only: ",
                "write strategy = \"sandbox\" under [execution]\n",
            )
            .to_owned()
        )
    );
    assert!(!root.join("out").exists());
}

#[test]
fn a_workspace_without_a_strategy_line_builds() {
    let (_dir, root) = workspace(&[
        (
            "buildl.toml",
            "[ceiling]\nproc.run = [\"cp\"]\nfs.read = [\".\"]\nfs.write = [\"out\", \".buildl\"]\n",
        ),
        ("build.lua", COPY),
        ("a.txt", "a\n"),
    ]);

    let built = run(&root, &["build"], &[]);

    assert_eq!(code(&built), 0, "{}", stderr(&built));
    assert_eq!(fs::read(root.join("out/copy")).unwrap(), b"a\n");
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
fn a_pinned_variable_exits_5() {
    let manifest = |execution: &str| {
        format!(
            "[ceiling]\nproc.run = [\"cp\"]\nenv.read = [\"TZ\"]\nfs.read = [\".\"]\nfs.write = [\"out\", \".buildl\"]\n{execution}"
        )
    };
    let (_dir, root) = workspace(&[
        ("buildl.toml", &manifest("")),
        (
            "build.lua",
            "buildl.target(\"app\", { inputs = { \"a.txt\" }, env = { \"TZ\" }, run = { \"cp\", \"$in\", \"$out\" } })\n",
        ),
        ("a.txt", "a\n"),
    ]);
    assert_eq!(
        failed(&root, &["plan"]),
        (
            5,
            "error: build.lua: //:app: env TZ is set to UTC by the sandbox and cannot be declared\n"
                .to_owned()
        )
    );

    write(
        &root,
        "buildl.toml",
        &manifest("\n[execution]\nstrategy = \"bare\"\n"),
    );
    assert_eq!(code(&run(&root, &["plan"], &[])), 0);
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
fn copying() -> (tempfile::TempDir, PathBuf) {
    workspace(&[
        ("buildl.toml", &manifest(&["cp"])),
        ("build.lua", COPY),
        ("a.txt", "a\n"),
    ])
}

/// The workspace of [`COPY`], built once.
fn built_copying() -> (tempfile::TempDir, PathBuf) {
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

/// A workspace that copies `a.txt` and whose approver is `mode`, and a home for the process
/// that runs in it, the home as an absolute path.
fn approved_by(mode: &str) -> (tempfile::TempDir, PathBuf, tempfile::TempDir, PathBuf) {
    let manifest = format!("{}\n[approver]\ndefault = \"{mode}\"\n", manifest(&["cp"]));
    let (dir, root) = workspace(&[
        ("buildl.toml", &manifest),
        ("build.lua", COPY),
        ("a.txt", "a\n"),
    ]);
    let home_dir = tempfile::tempdir().unwrap();
    let home = fs::canonicalize(home_dir.path()).unwrap();
    (dir, root, home_dir, home)
}

#[test]
fn interactive_with_piped_stdin_exits_four_with_the_remedy() {
    let (_dir, root, _home_dir, home) = approved_by("interactive");

    let output = run_at_home(&root, &["build"], &home);

    assert_eq!(stdout(&output), "");
    assert_eq!(code(&output), 4, "{}", stderr(&output));
    let text = stderr(&output);
    assert!(text.contains("no terminal to ask on"), "{text}");
    assert!(text.contains("[approver] default = \"manifest\""), "{text}");
    assert!(
        text.contains("  run programs: cp\n  write into:   .buildl, out\n"),
        "{text}"
    );
    assert!(!root.join("out").exists());
    assert!(!data_home(&home).exists(), "nothing was remembered");
}

#[test]
fn deny_exits_four() {
    let (_dir, root, _home_dir, home) = approved_by("deny");

    let output = run_at_home(&root, &["build"], &home);

    assert_eq!(stdout(&output), "");
    assert_eq!(code(&output), 4, "{}", stderr(&output));
    assert!(
        stderr(&output).contains("the workspace's approver is deny"),
        "{}",
        stderr(&output)
    );
    assert!(!root.join("out").exists());
}

#[test]
fn a_corrupt_approvals_file_exits_six() {
    let (_dir, root, _home_dir, home) = approved_by("interactive");
    let file = Approvals::new(&data_home(&home), &root)
        .file()
        .to_path_buf();
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, "not what buildl writes").unwrap();

    let output = run_at_home(&root, &["build"], &home);

    assert_eq!(stdout(&output), "");
    assert_eq!(code(&output), 6, "{}", stderr(&output));
    let text = stderr(&output);
    assert!(text.contains(&file.display().to_string()), "{text}");
    assert!(!root.join("out").exists());
}

#[test]
fn doctor_exits_zero_with_refused_wants() {
    let (_dir, root) = workspace(&[
        ("buildl.toml", &manifest(&["cp"])),
        (
            "build.lua",
            "buildl.target(\"fetch\", { run = { \"curl\", \"$out\" } })\n",
        ),
    ]);

    let output = run(&root, &["doctor"], &[]);

    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert!(
        stdout(&output).contains("  run curl       REFUSED  -\n"),
        "{}",
        stdout(&output)
    );
    assert_eq!(stderr(&output), "");
}

#[test]
fn doctor_exits_five_on_an_output_conflict() {
    let (_dir, root) = workspace(&[
        ("buildl.toml", &manifest(&["cp"])),
        (
            "build.lua",
            "buildl.target(\"a\", { outputs = { \"x\" }, run = { \"cp\" } })\nbuildl.target(\"b\", { outputs = { \"x\" }, run = { \"cp\" } })\n",
        ),
    ]);

    assert_eq!(
        failed(&root, &["doctor"]),
        (
            5,
            "error: out/x: //:a (build.lua) and //:b (build.lua) cannot both write here\n"
                .to_owned()
        )
    );
}

#[test]
fn doctor_exits_six_on_a_corrupt_approvals_file() {
    let (_dir, root, _home_dir, home) = approved_by("interactive");
    let file = Approvals::new(&data_home(&home), &root)
        .file()
        .to_path_buf();
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, "not what buildl writes").unwrap();

    let output = run_at_home(&root, &["doctor"], &home);

    assert_eq!(stdout(&output), "");
    assert_eq!(code(&output), 6, "{}", stderr(&output));
    let text = stderr(&output);
    assert!(text.contains(&file.display().to_string()), "{text}");
}

#[test]
fn doctor_exits_five_on_a_build_file_error() {
    let (_dir, root) = workspace(&[
        ("buildl.toml", &manifest(&["cp"])),
        ("build.lua", "buildl.target(\"a\", {\n"),
    ]);
    assert_eq!(
        failed(&root, &["doctor"]),
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

/// A git repository, in a temporary directory, holding the files of one plugin named `name`
/// that declares a target copying `hello.txt`, tagged `v1.0.0`; and the commit the tag names.
///
/// The machine's own `git` builds it: a machine with none fails the test that asks.
fn plugin_repository(name: &str) -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = fs::canonicalize(dir.path()).unwrap();
    write(
        &path,
        "extension.toml",
        &format!(
            "[extension]\nname = \"{name}\"\nversion = \"0.1.0\"\nentry = \"main.lua\"\napi = 1\n\n[capabilities]\nproc.run = [\"cp\"]\n"
        ),
    );
    write(
        &path,
        "main.lua",
        concat!(
            "local module = {}\n",
            "function module.greet(name)\n",
            "  buildl.target(name, { inputs = { \"hello.txt\" }, run = { \"cp\", \"$in\", \"$out\" } })\n",
            "end\n",
            "return module\n",
        ),
    );
    git_in(&path, &["init", "-q", "-b", "main"]);
    git_in(&path, &["add", "."]);
    git_in(&path, &["commit", "-q", "-m", "plugin"]);
    git_in(&path, &["tag", "v1.0.0"]);
    let commit = git_in(&path, &["rev-parse", "HEAD"]);
    (dir, commit)
}

/// Runs `git args` in the repository at `path` and returns what it printed to standard output.
fn git_in(path: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "tag.gpgsign=false",
        ])
        .args(args)
        .current_dir(path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

/// The `file://` address of the repository `dir` holds.
fn repository_url(dir: &tempfile::TempDir) -> String {
    format!("file://{}", fs::canonicalize(dir.path()).unwrap().display())
}

/// A workspace whose build file uses each of `plugins`, a plugin named as given and fetched
/// from the repository given, at the tag `v1.0.0`.
fn workspace_using(plugins: &[(&str, &tempfile::TempDir)]) -> (tempfile::TempDir, PathBuf) {
    let tables: Vec<String> = plugins
        .iter()
        .map(|(name, repository)| {
            format!(
                "\n[plugins.{name}]\ngit = \"{}\"\nrev = \"v1.0.0\"\nproc.run = [\"cp\"]\n",
                repository_url(repository)
            )
        })
        .collect();
    let toml = manifest(&["cp"]) + &tables.concat();
    let lua: Vec<String> = plugins
        .iter()
        .map(|(name, _)| format!("buildl.use(\"{name}\").greet(\"{name}-out\")\n"))
        .collect();
    let lua = lua.concat();
    workspace(&[
        ("buildl.toml", &toml),
        ("build.lua", &lua),
        ("hello.txt", "hello\n"),
    ])
}

#[test]
fn plugins_fetch_exits_0_and_reports_each_plugin() {
    let (zeta_repository, zeta) = plugin_repository("zeta");
    let (alpha_repository, alpha) = plugin_repository("alpha");
    let (_dir, root) = workspace_using(&[("zeta", &zeta_repository), ("alpha", &alpha_repository)]);

    let first = run(&root, &["plugins", "fetch"], &[]);
    assert_eq!(code(&first), 0, "{}", stderr(&first));
    assert_eq!(stdout(&first), "");
    assert_eq!(
        stderr(&first),
        format!("fetched alpha {alpha}\nfetched zeta {zeta}\n")
    );
    assert!(root.join("buildl.lock").is_file());

    let second = run(&root, &["plugins", "fetch"], &[]);
    assert_eq!(code(&second), 0, "{}", stderr(&second));
    assert_eq!(stdout(&second), "");
    assert_eq!(
        stderr(&second),
        format!("unchanged alpha {alpha}\nunchanged zeta {zeta}\n")
    );
}

#[test]
fn plugins_fetch_with_no_git_exits_3() {
    let (repository, _commit) = plugin_repository("hello");
    let (_dir, root) = workspace_using(&[("hello", &repository)]);
    let nowhere = tempfile::tempdir().unwrap();

    let output = run(
        &root,
        &["plugins", "fetch"],
        &[("PATH", nowhere.path().as_os_str())],
    );

    assert_eq!(stdout(&output), "");
    assert_eq!(code(&output), 3, "{}", stderr(&output));
    let text = stderr(&output);
    assert!(text.starts_with("error: "), "{text}");
    assert!(text.contains("hello"), "{text}");
    assert!(!root.join("buildl.lock").exists());
}

#[test]
fn plugins_fetch_of_a_plugin_holding_a_link_exits_5() {
    let (repository, _commit) = plugin_repository("hello");
    let path = fs::canonicalize(repository.path()).unwrap();
    symlink("/etc/hosts", path.join("hosts")).unwrap();
    git_in(&path, &["add", "hosts"]);
    git_in(&path, &["commit", "-q", "-m", "link"]);
    git_in(&path, &["tag", "-f", "v1.0.0"]);
    let (_dir, root) = workspace_using(&[("hello", &repository)]);

    let output = run(&root, &["plugins", "fetch"], &[]);

    assert_eq!(stdout(&output), "");
    assert_eq!(code(&output), 5, "{}", stderr(&output));
    let text = stderr(&output);
    assert!(text.starts_with("error: "), "{text}");
    assert!(text.contains("hello") && text.contains("hosts"), "{text}");
    assert!(!root.join("buildl.lock").exists());
}

#[test]
fn plugins_fetch_of_a_tag_that_moved_exits_6() {
    let (repository, locked) = plugin_repository("hello");
    let path = fs::canonicalize(repository.path()).unwrap();
    let (_dir, root) = workspace_using(&[("hello", &repository)]);
    let fetched = run(&root, &["plugins", "fetch"], &[]);
    assert_eq!(code(&fetched), 0, "{}", stderr(&fetched));
    let lock = fs::read(root.join("buildl.lock")).unwrap();
    // The store is gone, as in a fresh checkout of a workspace whose lock is committed.
    fs::remove_dir_all(root.join(".buildl/plugins")).unwrap();
    write(&path, "later.txt", "later");
    git_in(&path, &["add", "later.txt"]);
    git_in(&path, &["commit", "-q", "-m", "later"]);
    git_in(&path, &["tag", "-f", "v1.0.0"]);

    let output = run(&root, &["plugins", "fetch"], &[]);

    assert_eq!(stdout(&output), "");
    assert_eq!(code(&output), 6, "{}", stderr(&output));
    let text = stderr(&output);
    assert!(text.starts_with("error: "), "{text}");
    assert!(text.contains("hello") && text.contains(&locked), "{text}");
    assert_eq!(fs::read(root.join("buildl.lock")).unwrap(), lock);
}

#[test]
fn build_after_fetch_succeeds_offline() {
    let (repository, _commit) = plugin_repository("hello");
    let (_dir, root) = workspace_using(&[("hello", &repository)]);
    let fetched = run(&root, &["plugins", "fetch"], &[]);
    assert_eq!(code(&fetched), 0, "{}", stderr(&fetched));

    drop(repository);
    let built = run(&root, &["build"], &[]);

    assert_eq!(code(&built), 0, "{}", stderr(&built));
    assert_eq!(fs::read(root.join("out/hello-out")).unwrap(), b"hello\n");
}

/// One target, `//:listing`, whose output is what `printf` prints.
const LISTING: &str =
    "buildl.target(\"listing\", { run = { \"printf\", \"one\\ntwo\\n\" }, stdout = true })\n";

#[test]
fn a_stdout_target_with_two_outputs_exits_5() {
    let (_dir, root) = workspace(&[
        ("buildl.toml", &manifest(&["printf"])),
        (
            "build.lua",
            "buildl.target(\"listing\", { outputs = { \"a\", \"b\" }, run = { \"printf\", \"x\" }, stdout = true })\n",
        ),
    ]);
    let (status, said) = failed(&root, &["build"]);
    assert_eq!(status, 5, "{said}");
    assert_eq!(
        said,
        "error: build.lua: declaration #0: invalid stdout: target //:listing declares 2 outputs; a target whose output is its stdout declares exactly one\n"
    );
    assert!(!root.join("out").exists());
}

#[test]
fn a_stdout_target_whose_command_writes_the_path_exits_1() {
    let (_dir, root) = workspace(&[
        ("buildl.toml", &manifest(&["sh"])),
        (
            "build.lua",
            "buildl.target(\"listing\", { run = { \"sh\", \"-c\", \"printf x > \\\"$1\\\"; printf y\", \"sh\", \"$out\" }, stdout = true })\n",
        ),
    ]);
    let (status, said) = failed(&root, &["build"]);
    assert_eq!(status, 1, "{said}");
    assert!(
        said.contains("output listing is written from stdout, and the command wrote it too"),
        "{said}"
    );
}

#[test]
fn log_shows_what_a_stdout_target_printed() {
    let (_dir, root) = workspace(&[
        ("buildl.toml", &manifest(&["printf"])),
        ("build.lua", LISTING),
    ]);
    assert_eq!(code(&run(&root, &["build"], &[])), 0);
    assert_eq!(fs::read(root.join("out/listing")).unwrap(), b"one\ntwo\n");

    let output = run(&root, &["log", "listing"], &[]);

    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(stdout(&output), "one\ntwo\n");
}
