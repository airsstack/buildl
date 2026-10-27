//! A target whose output is what its command prints, built in a real workspace.
//!
//! The fixture `stdout_output` has a target, `//:greeting`, and a discovery target, `//:pkgs`,
//! each declared with `stdout = true` and each running `printf` and nothing else, so what an
//! output holds is exactly what its command printed.

use std::fs;

use buildl::buildl_core::OutcomeClass;

use crate::common::{Fixture, ended};

/// What `//:greeting` prints.
const GREETING: &[u8] = b"hello\n";

/// What `//:pkgs` prints: two JSON objects, one to a line.
const PACKAGES: &[u8] = b"{\"name\":\"alpha\"}\n{\"name\":\"beta\"}\n";

/// A copy of `stdout_output` that runs at `strategy`, or at the default one when there is none.
fn fixture(strategy: Option<&str>) -> Fixture {
    let copy = Fixture::copy_of("stdout_output", &[]);
    if let Some(strategy) = strategy {
        copy.append(
            "buildl.toml",
            format!("strategy = \"{strategy}\"\n").as_bytes(),
        );
    }
    copy
}

/// Builds `copy` and holds the files the outputs became to what the commands printed.
fn built_with_the_printed_bytes(copy: &Fixture) {
    let built = copy.build();
    assert_eq!(
        ended(&built, &OutcomeClass::Built),
        ["//:greeting", "//:pkgs"]
    );
    assert_eq!(copy.files("out"), ["out/greeting", "out/pkgs.json"]);
    assert_eq!(
        fs::read(copy.root().join("out/greeting")).unwrap(),
        GREETING
    );
    assert_eq!(
        fs::read(copy.root().join("out/pkgs.json")).unwrap(),
        PACKAGES
    );
}

#[test]
fn a_stdout_output_is_built_twice_byte_identical() {
    let first = fixture(Some("sandbox"));
    let second = fixture(Some("sandbox"));

    built_with_the_printed_bytes(&first);
    built_with_the_printed_bytes(&second);

    assert_eq!(first.snapshot().first_difference(&second.snapshot()), None);
    assert_eq!(first.staged(), Vec::<String>::new());
}

#[test]
fn the_same_under_bare() {
    let bare = fixture(Some("bare"));
    let second_bare = fixture(Some("bare"));
    let sandboxed = fixture(Some("sandbox"));

    built_with_the_printed_bytes(&bare);
    built_with_the_printed_bytes(&second_bare);
    built_with_the_printed_bytes(&sandboxed);

    assert_eq!(
        bare.snapshot().first_difference(&second_bare.snapshot()),
        None
    );

    for output in ["out/greeting", "out/pkgs.json"] {
        assert_eq!(
            fs::read(bare.root().join(output)).unwrap(),
            fs::read(sandboxed.root().join(output)).unwrap(),
            "{output}"
        );
    }
}

#[test]
fn log_still_shows_what_a_stdout_target_printed() {
    let copy = fixture(None);
    copy.build();
    let workspace = copy.open();

    let shown = workspace
        .log(&workspace.label("//:greeting").unwrap())
        .unwrap();

    assert_eq!(shown.last.run.class, OutcomeClass::Built);
    assert_eq!(shown.stdout, GREETING);
    assert_eq!(shown.stderr, b"");
}

#[test]
fn a_second_build_is_served_from_the_cache() {
    let copy = fixture(None);
    copy.build();

    let again = copy.build();

    assert_eq!(
        ended(&again, &OutcomeClass::Cached),
        ["//:greeting", "//:pkgs"]
    );
}
