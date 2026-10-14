//! What builds of a fixture leave on disk, compared byte for byte, and the build log, compared
//! once its times are dropped.

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;

use buildl::buildl_core::{Digest, OutcomeClass};

use crate::common::{CHAIN_GRAPH, CHAIN_LABELS, CHAIN_OUTPUTS, Fixture, ended};

/// The keys of a line of the build log that hold a time.
const TIMES: [&str; 3] = ["at", "finished", "started"];

/// Two copies of the fixture `name`, each built once from clean: the first by four workers, as
/// the fixture says, and the second by one.
fn built_twice(name: &str, executables: &[&str]) -> (Fixture, Fixture) {
    let four = Fixture::copy_of(name, executables);
    let one = Fixture::copy_of(name, executables);
    one.replace_line("buildl.toml", "workers  = 4", "workers  = 1");
    for copy in [&four, &one] {
        let outcomes = copy.build();
        assert_eq!(
            ended(&outcomes, &OutcomeClass::Built).len(),
            outcomes.len(),
            "{name}: every target was built"
        );
    }
    (four, one)
}

/// Builds the fixture `name` twice as [`built_twice`] does, holds the two copies to the same
/// bytes with nothing left half-written, and returns the copy four workers built.
fn identical(name: &str, executables: &[&str]) -> Fixture {
    let (four, one) = built_twice(name, executables);
    assert_eq!(four.snapshot().first_difference(&one.snapshot()), None);
    for copy in [&four, &one] {
        assert!(!copy.state().journal().exists());
        assert_eq!(copy.staged(), Vec::<String>::new());
    }
    four
}

/// The top-level fields of one line of the build log, each key with its value as written.
///
/// A line is one object in canonical form: no space outside a string, and keys that need no
/// escape. The line is cut at each comma that is outside a string and outside a nested value.
fn fields(line: &str) -> BTreeMap<String, String> {
    let inner = line
        .strip_prefix('{')
        .and_then(|rest| rest.strip_suffix('}'))
        .unwrap();
    let mut fields = BTreeMap::new();
    let (mut start, mut depth, mut in_string, mut escaped) = (0, 0_u32, false, false);
    // One comma is added at the end, so that the last field is cut like the others.
    for (at, byte) in inner.bytes().chain(*b",").enumerate() {
        if in_string {
            match byte {
                _ if escaped => escaped = false,
                b'\\' => escaped = true,
                b'"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'{' | b'[' => depth += 1,
            b'}' | b']' => depth -= 1,
            b',' if depth == 0 => {
                let (key, value) = inner[start..at].split_once(':').unwrap();
                fields.insert(key.trim_matches('"').to_owned(), value.to_owned());
                start = at + 1;
            }
            _ => {}
        }
    }
    fields
}

/// The build log of `copy` with every time dropped: what is left of each line, by the label
/// the line is for.
fn log_without_times(copy: &Fixture) -> BTreeMap<String, BTreeMap<String, String>> {
    copy.log_lines()
        .iter()
        .map(|line| {
            let mut fields = fields(line);
            for time in TIMES {
                assert!(fields.remove(time).is_some(), "a line holds {time:?}");
            }
            (fields["label"].trim_matches('"').to_owned(), fields)
        })
        .collect()
}

#[test]
fn the_graph_file_matches_its_golden() {
    let chain = Fixture::copy_of("chain", &["greet.sh"]);
    chain.build();

    let graph = fs::read_to_string(chain.state().graph()).unwrap();

    assert_eq!(graph, CHAIN_GRAPH.trim_end());
}

#[test]
fn equal_bytes_placed_plain_and_executable_keep_one_blob_and_its_twin() {
    let (four, one) = built_twice("twins", &["echo.sh"]);
    for copy in [&four, &one] {
        let placed = ["out/runnable", "out/text", "out/text-after"].map(|output| {
            let path = copy.root().join(output);
            let executable = fs::metadata(&path).unwrap().permissions().mode() & 0o111 != 0;
            (fs::read(path).unwrap(), executable)
        });
        let bytes = b"#!/bin/sh\necho twins\n".to_vec();
        assert_eq!(
            placed,
            [
                (bytes.clone(), true),
                (bytes.clone(), false),
                (bytes.clone(), false)
            ]
        );

        let digest = Digest::of(&bytes).to_string();
        let blob = format!(".buildl/cas/{}/{}", &digest[..2], &digest[2..]);
        let held = copy.files(".buildl/cas");
        assert!(held.contains(&blob), "the blob is held: {held:?}");
        assert!(
            held.contains(&format!("{blob}.x")),
            "its twin is held: {held:?}"
        );
    }
}

#[test]
fn two_clean_builds_in_two_directories_are_byte_identical() {
    let chain = identical("chain", &["greet.sh"]);
    assert_eq!(chain.files("out"), CHAIN_OUTPUTS);
    assert_eq!(
        fs::read(chain.root().join("out/report")).unwrap(),
        b"apple\nfig\npear\nwords:\nzzz\n"
    );

    let twins = identical("twins", &["echo.sh"]);
    assert_eq!(
        twins.files("out"),
        ["out/runnable", "out/text", "out/text-after"]
    );
}

#[test]
fn two_logs_agree_once_times_are_dropped() {
    let (four, one) = built_twice("chain", &["greet.sh"]);

    let (four, one) = (log_without_times(&four), log_without_times(&one));

    assert_eq!(four.keys().collect::<Vec<_>>(), CHAIN_LABELS);
    assert_eq!(
        four["//:greet"].keys().collect::<Vec<_>>(),
        [
            "class",
            "command",
            "diagnostic",
            "file",
            "key",
            "label",
            "outputs",
            "stderr",
            "stdout"
        ]
    );
    assert_eq!(four, one);
}
