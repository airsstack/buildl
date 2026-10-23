//! What each command writes, byte for byte, and to which stream.
//!
//! Most tests here build one workspace: an application `//:app` linked from `//:main.o` and
//! `//lib:text`, its documentation `//:docs`, and an alias `//:default`. Its two programs are
//! scripts this file writes into a directory of their own, which is the whole search path the
//! binary is given: `cc`, which copies a source and complains about what the source asks it
//! to, and `mkdoc`, which prints two lines, the second not UTF-8.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use crate::common::{
    append, code, manifest, program, run, run_at_home, stderr, stdout, workspace, write,
};

/// The root build file of the workspace.
const BUILD: &str = r#"local b = buildl
b.subdir("lib")
b.option("mode", { default = "debug" })
b.alias("default", "app")
b.target("app", { deps = { "main.o", "//lib:text" }, run = { "cc", "$deps", "-o", "$out" } })
b.target("docs", { inputs = { "README" }, run = { "mkdoc", "--doc", "$in", "a b" } })
b.target("main.o", { inputs = { "main.c" }, run = { "cc", "-c", "$in", "-o", "$out", "$opt:mode" } })
"#;

/// The build file of `lib/`.
const LIB: &str = "buildl.target(\"text\", { inputs = { \"text.c\" }, run = { \"cc\", \"-c\", \"$in\", \"-o\", \"$out\" } })\n";

/// `cc -c <source> -o <output> [mode]` copies the source, warns about one that holds `unused`
/// and refuses one that holds `broken`; `cc <object> <object> -o <output>` joins the two.
const CC: &str = r#"#!/bin/sh
if [ "$1" = "-c" ]; then
    case "$(/bin/cat "$2")" in
    *broken*)
        echo "$2:3:1: error: expected ';'" >&2
        exit 1
        ;;
    *unused*) echo "$2:4:9: warning: unused variable 'n'" >&2 ;;
    esac
    /bin/cp "$2" "$4"
else
    /bin/cat "$1" "$2" > "$4"
fi
"#;

/// `mkdoc --doc <source> <title>` says what it wrote, the byte 0xE9 included, and copies the
/// source.
const MKDOC: &str = "#!/bin/sh\nprintf 'wrote 3 pages\\ncaf\\351\\n'\n/bin/cp \"$2\" out/docs\n";

/// The workspace, and the directory that holds its two programs.
struct Shop {
    _dir: tempfile::TempDir,
    root: PathBuf,
    _tools: tempfile::TempDir,
    bin: PathBuf,
}

impl Shop {
    fn new() -> Self {
        let (dir, root) = workspace(&[
            ("buildl.toml", &manifest(&["cc", "mkdoc"])),
            ("build.lua", BUILD),
            ("lib/build.lua", LIB),
            ("main.c", "int main(void) { int unused; return 0; }\n"),
            ("lib/text.c", "const char *text = \"text\";\n"),
            ("README", "three pages\n"),
        ]);
        let tools = tempfile::tempdir().unwrap();
        let bin = fs::canonicalize(tools.path()).unwrap();
        program(&bin, "cc", CC);
        program(&bin, "mkdoc", MKDOC);
        Self {
            _dir: dir,
            root,
            _tools: tools,
            bin,
        }
    }

    /// Runs the binary with `args` at the workspace root, its two programs the only ones it
    /// can find.
    fn run(&self, args: &[&str]) -> Output {
        run(&self.root, args, &[("PATH", self.bin.as_os_str())])
    }

    /// Runs the binary with `args` and returns its status, after checking that it wrote
    /// nothing to standard output, and what it wrote to standard error.
    fn built(&self, args: &[&str]) -> (i32, Vec<u8>) {
        let output = self.run(args);
        assert_eq!(stdout(&output), "", "{args:?}");
        (code(&output), output.stderr)
    }
}

#[test]
fn check_prints_one_line_on_stderr_and_nothing_on_stdout() {
    let shop = Shop::new();
    let output = shop.run(&["check"]);
    assert_eq!(code(&output), 0);
    assert_eq!(stderr(&output), "ok: 6 declarations in 2 build files\n");
    assert_eq!(stdout(&output), "");
}

#[test]
fn check_writes_nothing() {
    let shop = Shop::new();
    for args in [&["check"][..], &["graph"][..], &["graph", "--dot"][..]] {
        assert_eq!(code(&shop.run(args)), 0, "{args:?}");
    }
    assert!(!shop.root.join("out").exists());
    assert!(!shop.root.join(".buildl").exists());
}

#[test]
fn graph_json_equals_the_graph_file() {
    let shop = Shop::new();
    assert_eq!(shop.built(&["build"]).0, 0);
    let file = fs::read(shop.root.join(".buildl/graph.json")).unwrap();
    for args in [&["graph"][..], &["graph", "--json"][..]] {
        let output = shop.run(args);
        assert_eq!(code(&output), 0, "{args:?}");
        assert_eq!(output.stdout.last(), Some(&b'\n'), "{args:?}");
        assert_eq!(
            output.stdout[..output.stdout.len() - 1],
            file[..],
            "{args:?}"
        );
        assert_eq!(stderr(&output), "", "{args:?}");
    }
}

#[test]
fn query_prints_one_label_a_line_for_deps_and_rdeps() {
    let shop = Shop::new();
    for (expression, expected) in [
        ("deps(//:app)", "//:main.o\n//lib:text\n"),
        ("deps(default)", "//:main.o\n//lib:text\n"),
        ("rdeps(//lib:text)", "//:app\n"),
        ("rdeps(//:docs)", ""),
    ] {
        let output = shop.run(&["query", expression]);
        assert_eq!(code(&output), 0, "{expression}");
        assert_eq!(stdout(&output), expected, "{expression}");
        assert_eq!(stderr(&output), "", "{expression}");
    }
    assert!(!shop.root.join("out").exists());
    assert!(!shop.root.join(".buildl").exists());
}

#[test]
fn query_json_is_the_canonical_array_and_one_newline() {
    let shop = Shop::new();
    for (expression, expected) in [
        ("deps(//:app)", "[\"//:main.o\",\"//lib:text\"]\n"),
        ("rdeps(//:main.o)", "[\"//:app\"]\n"),
        ("deps(//:docs)", "[]\n"),
    ] {
        let output = shop.run(&["query", "--json", expression]);
        assert_eq!(code(&output), 0, "{expression}");
        assert_eq!(stdout(&output), expected, "{expression}");
        assert_eq!(stderr(&output), "", "{expression}");
    }
}

#[test]
fn graph_dot_is_stable() {
    let shop = Shop::new();
    let first = shop.run(&["graph", "--dot"]);
    assert_eq!(code(&first), 0);
    assert_eq!(
        stdout(&first),
        concat!(
            "digraph buildl {\n",
            "  \"//:app\";\n",
            "  \"//:docs\";\n",
            "  \"//:main.o\";\n",
            "  \"//lib:text\";\n",
            "  \"//:app\" -> \"//:main.o\";\n",
            "  \"//:app\" -> \"//lib:text\";\n",
            "  \"//:default\" [shape=plaintext];\n",
            "  \"//:default\" -> \"//:app\" [style=dashed];\n",
            "}\n",
        )
    );
    assert_eq!(shop.run(&["graph", "--dot"]).stdout, first.stdout);
}

#[test]
fn plan_says_why_each_target_would_run() {
    let shop = Shop::new();
    let never = shop.run(&["plan"]);
    assert_eq!(code(&never), 0);
    assert_eq!(
        stdout(&never),
        concat!(
            "run    //:app: never built\n",
            "run    //:docs: never built\n",
            "run    //:main.o: never built\n",
            "run    //lib:text: never built\n",
            "4 to run, 0 conditional, 0 clean\n",
        )
    );

    assert_eq!(shop.built(&["build"]).0, 0);
    append(&shop.root, "main.c", "// grown\n");
    let output = shop.run(&["plan"]);
    assert_eq!(code(&output), 0);
    assert_eq!(
        stdout(&output),
        concat!(
            "maybe  //:app: runs only if //:main.o produces something else\n",
            "clean  //:docs\n",
            "run    //:main.o: input main.c changed\n",
            "clean  //lib:text\n",
            "1 to run, 1 conditional, 2 clean\n",
        )
    );
    assert_eq!(stderr(&output), "");
}

#[test]
fn plan_names_every_difference_of_a_target() {
    let shop = Shop::new();
    assert_eq!(shop.built(&["build"]).0, 0);
    append(&shop.root, "main.c", "// grown\n");
    append(&shop.bin, "cc", "# a newer compiler\n");
    let output = shop.run(&["plan", "--set", "mode=release"]);
    assert_eq!(code(&output), 0);
    assert_eq!(
        stdout(&output),
        concat!(
            "run    //:app: tool cc changed\n",
            "clean  //:docs\n",
            "run    //:main.o: input main.c changed; tool cc changed; setting mode changed\n",
            "run    //lib:text: tool cc changed\n",
            "3 to run, 0 conditional, 1 clean\n",
        )
    );
}

#[test]
fn plan_json_is_one_canonical_line() {
    let shop = Shop::new();
    let first = shop.run(&["plan", "--json"]);
    assert_eq!(code(&first), 0);
    let text = stdout(&first);
    assert!(
        text.starts_with(concat!(
            "{\"attribution\":{\"env\":{},\"network\":{},\"programs\":{}},",
            "\"settings\":{\"mode\":\"debug\"},\"targets\":{\"//:app\":{"
        )),
        "{text}"
    );
    assert!(text.ends_with("}\n"), "{text}");
    assert_eq!(text.lines().count(), 1);
    assert_eq!(shop.run(&["plan", "--json"]).stdout, first.stdout);
}

#[test]
fn build_prints_one_block_per_action_and_a_summary() {
    let shop = Shop::new();
    assert_eq!(
        shop.built(&["build"]),
        (
            0,
            [
                &b"--- //:docs\n"[..],
                b"wrote 3 pages\n",
                b"caf\xe9\n",
                b"--- //:main.o\n",
                b"main.c:4:9: warning: unused variable 'n'\n",
                b"4 built\n",
            ]
            .concat()
        )
    );
}

#[test]
fn a_cache_hit_shows_what_the_action_printed() {
    let shop = Shop::new();
    assert_eq!(shop.built(&["build"]).0, 0);
    assert_eq!(
        shop.built(&["build"]),
        (
            0,
            [
                &b"--- //:docs (cached)\n"[..],
                b"wrote 3 pages\n",
                b"caf\xe9\n",
                b"--- //:main.o (cached)\n",
                b"main.c:4:9: warning: unused variable 'n'\n",
                b"4 cached\n",
            ]
            .concat()
        )
    );
}

/// The workspace after a full build, with `//:docs` unable to start and `//lib:text` broken.
fn broken() -> Shop {
    let shop = Shop::new();
    assert_eq!(shop.built(&["build"]).0, 0);
    program(&shop.bin, "mkdoc", "#!/nonexistent/interpreter\n");
    append(&shop.root, "lib/text.c", "broken\n");
    shop
}

/// What a build of [`broken`] reports about `//:docs`.
fn docs_failed(bin: &Path) -> String {
    format!(
        concat!(
            "FAILED //:docs: infrastructure failure (the command could not be started)\n",
            "    exec strategy: //:docs: running {}: No such file or directory (os error 2)\n",
            "    declared in build.lua\n",
            "    command: mkdoc --doc README \"a b\"\n",
        ),
        bin.join("mkdoc").display()
    )
}

#[test]
fn a_failure_report_names_the_class_the_file_and_the_command() {
    let shop = broken();
    let (status, said) = shop.built(&["build", "--keep-going"]);
    assert_eq!(status, 3);
    assert_eq!(
        String::from_utf8(said).unwrap(),
        [
            "--- //:main.o (cached)\n",
            "main.c:4:9: warning: unused variable 'n'\n",
            &docs_failed(&shop.bin),
            "--- //lib:text\n",
            "lib/text.c:3:1: error: expected ';'\n",
            "FAILED //lib:text: action failure (exit status 1)\n",
            "    declared in lib/build.lua\n",
            "    command: cc -c lib/text.c -o out/lib/text\n",
            "2 failed, 1 skipped, 1 cached\n",
        ]
        .concat()
    );
}

#[test]
fn without_keep_going_the_build_stops_at_its_first_failure() {
    let shop = broken();
    let (status, said) = shop.built(&["build"]);
    assert_eq!(status, 3);
    // With one worker `//:docs` fails first, so `//lib:text` is never begun and is skipped
    // with `//:app`.
    assert_eq!(
        String::from_utf8(said).unwrap(),
        [
            "--- //:main.o (cached)\n",
            "main.c:4:9: warning: unused variable 'n'\n",
            &docs_failed(&shop.bin),
            "1 failed, 2 skipped, 1 cached\n",
        ]
        .concat()
    );
}

#[test]
fn stream_shows_the_action_output() {
    let (_dir, root) = workspace(&[
        ("buildl.toml", &manifest(&["sh"])),
        (
            "build.lua",
            concat!(
                "buildl.target(\"loud\", { run = { \"sh\", \"-c\", \"echo one; echo two; exit 3\" } })\n",
                "buildl.target(\"partial\", { run = { \"sh\", \"-c\", \"printf partial; echo > out/partial\" } })\n",
            ),
        ),
    ]);
    let output = run(&root, &["build", "--stream", "--keep-going"], &[]);
    assert_eq!(code(&output), 1);
    assert_eq!(
        stderr(&output),
        concat!(
            "--- //:loud\n",
            "one\n",
            "two\n",
            "FAILED //:loud: action failure (exit status 3)\n",
            "    declared in build.lua\n",
            "    command: sh -c \"echo one; echo two; exit 3\"\n",
            "--- //:partial\n",
            "partial\n",
            "1 failed, 1 built\n",
        )
    );
    assert_eq!(stdout(&output), "");
}

#[test]
fn colour_is_written_only_when_asked_for() {
    let (_dir, root) = workspace(&[
        ("buildl.toml", &manifest(&["false"])),
        (
            "build.lua",
            "buildl.target(\"bad\", { run = { \"false\" } })\n",
        ),
    ]);
    let terminal = [("TERM", OsStr::new("xterm-256color"))];
    let plain = run(&root, &["build"], &terminal);
    assert!(!plain.stderr.contains(&0x1b), "{}", stderr(&plain));

    let asked = run(
        &root,
        &["build", "--color", "always"],
        &[("NO_COLOR", OsStr::new("1"))],
    );
    assert_eq!(code(&asked), 1);
    assert!(
        stderr(&asked)
            .starts_with("\x1b[1;31mFAILED\x1b[0m //:bad: action failure (exit status 1)\n"),
        "{}",
        stderr(&asked)
    );
}

/// A workspace whose one target, `//:both`, prints a line on each stream, built once.
fn logged() -> (tempfile::TempDir, PathBuf) {
    let (dir, root) = workspace(&[
        ("buildl.toml", &manifest(&["sh"])),
        (
            "build.lua",
            "buildl.target(\"both\", { run = { \"sh\", \"-c\", \"echo out; echo err >&2\" }, outputs = {} })\n",
        ),
    ]);
    assert_eq!(code(&run(&root, &["build"], &[])), 0);
    (dir, root)
}

/// Whether `line` is `//:both: built at ` and a UTC time to the second, as the header of a
/// last run is written, with its newline.
fn is_header(line: &str) -> bool {
    let Some(time) = line
        .strip_prefix("//:both: built at ")
        .and_then(|rest| rest.strip_suffix(" UTC\n"))
    else {
        return false;
    };
    let shape = "dddd-dd-dd dd:dd:dd";
    time.len() == shape.len()
        && time
            .chars()
            .zip(shape.chars())
            .all(|(got, want)| match want {
                'd' => got.is_ascii_digit(),
                other => got == other,
            })
}

#[test]
fn log_prints_each_stream_on_its_own_channel() {
    let (_dir, root) = logged();
    let output = run(&root, &["log", "both"], &[]);
    assert_eq!(code(&output), 0);
    assert_eq!(stdout(&output), "out\n");
    let said = stderr(&output);
    let (header, rest) = said.split_once('\n').unwrap();
    assert!(is_header(&format!("{header}\n")), "{said}");
    assert_eq!(rest, "err\n");
}

#[test]
fn log_stdout_prints_only_stdout() {
    let (_dir, root) = logged();
    let output = run(&root, &["log", "both", "--stdout"], &[]);
    assert_eq!(code(&output), 0);
    assert_eq!(stdout(&output), "out\n");
    assert!(is_header(&stderr(&output)), "{}", stderr(&output));
}

#[test]
fn log_stderr_prints_stored_stderr_on_stdout() {
    let (_dir, root) = logged();
    let output = run(&root, &["log", "both", "--stderr"], &[]);
    assert_eq!(code(&output), 0);
    assert_eq!(stdout(&output), "err\n");
    assert!(is_header(&stderr(&output)), "{}", stderr(&output));
}

#[test]
fn log_header_is_on_stderr() {
    let (_dir, root) = logged();
    let said = stderr(&run(&root, &["log", "both"], &[]));
    let header = said.lines().next().unwrap();
    assert!(is_header(&format!("{header}\n")), "{said}");
}

/// Two test targets, `//:pass`, which prints `ok` and passes, and `//:fail`, which prints `bad`
/// and fails.
const TESTS: &str = r#"buildl.test("pass", { run = { "sh", "-c", "echo ok" } })
buildl.test("fail", { run = { "sh", "-c", "echo bad; exit 1" } })
"#;

/// A workspace of [`TESTS`] under a manifest that allows `sh`.
fn tests() -> (tempfile::TempDir, PathBuf) {
    workspace(&[("buildl.toml", &manifest(&["sh"])), ("build.lua", TESTS)])
}

/// The block `buildl build fail` prints for `//:fail`: what it wrote to standard error without
/// the summary line that ends it.
fn failing_block() -> String {
    let (_dir, root) = tests();
    let said = stderr(&run(&root, &["build", "fail"], &[]));
    said.strip_suffix("1 failed\n").unwrap().to_owned()
}

#[test]
fn test_prints_only_failing_tests_and_the_test_line() {
    let (_dir, root) = tests();
    let output = run(&root, &["test"], &[]);
    assert_eq!(stdout(&output), "");
    let said = stderr(&output);
    assert_eq!(
        said,
        [failing_block(), "tests: 1 passed, 1 failed\n".to_owned()].concat()
    );
    assert!(!said.contains("ok"), "{said}");
}

#[test]
fn test_verbose_prints_every_test() {
    let (_dir, root) = tests();
    let output = run(&root, &["test", "--verbose"], &[]);
    assert_eq!(stdout(&output), "");
    assert_eq!(
        stderr(&output),
        [
            failing_block(),
            "--- //:pass\nok\n".to_owned(),
            "tests: 1 passed, 1 failed\n".to_owned()
        ]
        .concat()
    );
}

#[test]
fn test_stop_starts_nothing_after_the_first_failure() {
    let (_dir, root) = tests();
    let output = run(&root, &["test", "--stop"], &[]);
    assert_eq!(code(&output), 1);
    let said = stderr(&output);
    assert!(said.ends_with("tests: 1 failed, 1 skipped\n"), "{said}");
    assert!(!said.contains("ok"), "{said}");
}

#[test]
fn test_stream_shows_a_passing_test_as_it_runs() {
    let (_dir, root) = tests();
    let output = run(&root, &["test", "--stream", "pass"], &[]);
    assert_eq!(code(&output), 0);
    assert_eq!(stdout(&output), "");
    assert_eq!(stderr(&output), "--- //:pass\nok\ntests: 1 passed\n");
}

#[test]
fn test_rerun_line() {
    let (_dir, root) = tests();
    assert_eq!(code(&run(&root, &["test", "pass"], &[])), 0);
    let output = run(&root, &["test", "--rerun", "pass"], &[]);
    assert_eq!(code(&output), 0);
    assert_eq!(stdout(&output), "");
    assert_eq!(stderr(&output), "tests (rerun): 1 passed\n");
}

/// A workspace of two targets, `//:hello` and `//:quiet` that needs it, not built.
fn pair() -> (tempfile::TempDir, PathBuf) {
    workspace(&[
        ("buildl.toml", &manifest(&["cp"])),
        (
            "build.lua",
            "buildl.target(\"hello\", { inputs = { \"hello.txt\" }, run = { \"cp\", \"$in\", \"$out\" } })\nbuildl.target(\"quiet\", { deps = { \"hello\" }, run = { \"cp\", \"$deps\", \"$out\" } })\n",
        ),
        ("hello.txt", "hello\n"),
    ])
}

/// Whether `line` is what `gc` says after dropping one row: a count of blobs and a count of
/// bytes it removed, each in the plural or the singular, and a newline.
fn is_gc_line(line: &str) -> bool {
    let Some(rest) = line.strip_prefix("gc: dropped 1 row, removed ") else {
        return false;
    };
    let Some(rest) = rest
        .strip_suffix(" bytes)\n")
        .or_else(|| rest.strip_suffix(" byte)\n"))
    else {
        return false;
    };
    let Some((blobs, bytes)) = rest.split_once(" (") else {
        return false;
    };
    let digits = |text: &str| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit());
    blobs
        .strip_suffix(" blobs")
        .or_else(|| blobs.strip_suffix(" blob"))
        .is_some_and(digits)
        && digits(bytes)
}

#[test]
fn clean_says_it_removed_out_and_prints_nothing_on_stdout() {
    let (_dir, root) = pair();
    assert_eq!(code(&run(&root, &["build"], &[])), 0);
    let output = run(&root, &["clean"], &[]);
    assert_eq!(code(&output), 0);
    assert_eq!(stdout(&output), "");
    assert_eq!(stderr(&output), "clean: removed out\n");
    assert!(!root.join("out").exists());
}

#[test]
fn clean_without_out_says_it_holds_nothing() {
    let (_dir, root) = pair();
    let output = run(&root, &["clean"], &[]);
    assert_eq!(code(&output), 0);
    assert_eq!(stdout(&output), "");
    assert_eq!(stderr(&output), "clean: out holds nothing\n");
}

#[test]
fn gc_reports_rows_blobs_and_bytes() {
    let (_dir, root) = pair();
    assert_eq!(code(&run(&root, &["build"], &[])), 0);
    write(
        &root,
        "build.lua",
        "buildl.target(\"hello\", { inputs = { \"hello.txt\" }, run = { \"cp\", \"$in\", \"$out\" } })\n",
    );
    let output = run(&root, &["gc"], &[]);
    assert_eq!(code(&output), 0);
    assert_eq!(stdout(&output), "");
    assert!(is_gc_line(&stderr(&output)), "{}", stderr(&output));
}

#[test]
fn doctor_prints_the_workspace_authority() {
    let manifest = format!(
        "{}\n[approver]\ndefault = \"interactive\"\n",
        manifest(&["cp"])
    );
    let (_dir, root) = workspace(&[
        ("buildl.toml", &manifest),
        (
            "build.lua",
            concat!(
                "buildl.target(\"copy\", { inputs = { \"a.txt\" }, run = { \"cp\", \"$in\", \"$out\" } })\n",
                "buildl.target(\"fetch\", { run = { \"curl\", \"-o\", \"$out\" }, network = true })\n",
            ),
        ),
        ("a.txt", "a\n"),
    ]);
    let home_dir = tempfile::tempdir().unwrap();
    let home = fs::canonicalize(home_dir.path()).unwrap();

    let output = run_at_home(&root, &["doctor"], &home);

    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(stderr(&output), "");
    assert_eq!(
        stdout(&output),
        format!(
            concat!(
                "workspace  {}\n",
                "approver   interactive\n",
                "execution  workers 1, timeout 10m, strategy bare\n",
                "ceiling\n",
                "  proc.run  cp\n",
                "  env.read  -\n",
                "  fs.read   .\n",
                "  fs.write  .buildl, out\n",
                "wants               ceiling  approved\n",
                "  run cp            allowed  no\n",
                "  run curl          REFUSED  -\n",
                "  write .buildl     allowed  no\n",
                "  write out         allowed  no\n",
                "  network //:fetch  -        no\n",
            ),
            root.display()
        )
    );
}
