//! A target whose output is what its command prints, built in a real workspace.
//!
//! The fixture `stdout_output` has a target, `//:greeting`, and a discovery target, `//:pkgs`,
//! each declared with `stdout = true` and each running `printf` and nothing else, so what an
//! output holds is exactly what its command printed. `//:pkgs` prints two JSON values back to
//! back; the build file reads them with `buildl.decode_all` and declares `//:alpha` and
//! `//:beta` from them.

use std::fs;

use buildl::buildl_core::OutcomeClass;

use crate::common::{Fixture, ended};

/// What `//:greeting` prints.
const GREETING: &[u8] = b"hello\n";

/// What `//:pkgs` prints: two JSON objects, one to a line.
const PACKAGES: &[u8] = b"{\"name\":\"alpha\"}\n{\"name\":\"beta\"}\n";

/// The targets the build file declares from `//:pkgs`'s output, one per value
/// `buildl.decode_all` reads from it, each with what it prints.
const DECLARED: [(&str, &[u8]); 2] = [("alpha", b"alpha\n"), ("beta", b"beta\n")];

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
        ["//:alpha", "//:beta", "//:greeting", "//:pkgs"]
    );
    assert_eq!(
        copy.files("out"),
        ["out/alpha", "out/beta", "out/greeting", "out/pkgs.json"]
    );
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
        ["//:alpha", "//:beta", "//:greeting", "//:pkgs"]
    );
}

#[test]
fn a_stdout_stream_declares_one_target_per_value() {
    let copy = fixture(None);

    let built = copy.build();

    assert_eq!(
        ended(&built, &OutcomeClass::Built),
        ["//:alpha", "//:beta", "//:greeting", "//:pkgs"]
    );
    let workspace = copy.open();
    let (graph, missing) = workspace.graph().unwrap();
    assert_eq!(graph.len(), 4);
    assert_eq!(missing, Vec::new());
    for (name, printed) in DECLARED {
        let label = format!("//:{name}");
        assert!(
            graph.id_of(&workspace.label(&label).unwrap()).is_some(),
            "{label}"
        );
        assert_eq!(
            fs::read(copy.root().join("out").join(name)).unwrap(),
            printed,
            "{label}"
        );
    }
}

#[test]
fn the_read_back_is_byte_identical_in_a_second_checkout() {
    let first = fixture(None);
    let second = fixture(None);

    first.build();
    second.build();

    for (name, _) in DECLARED {
        assert_eq!(
            fs::read(first.root().join("out").join(name)).unwrap(),
            fs::read(second.root().join("out").join(name)).unwrap(),
            "{name}"
        );
    }
    assert_eq!(first.snapshot().first_difference(&second.snapshot()), None);
}
