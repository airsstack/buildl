//! A build asked for one target builds that target and what it needs, and nothing else.

use std::fs;

use buildl::buildl_core::OutcomeClass;

use crate::common::{CHAIN_GRAPH, Fixture, ended};

#[test]
fn a_named_target_builds_what_it_needs_and_nothing_else() {
    let chain = Fixture::copy_of("chain", &["greet.sh"]);

    let outcomes = chain.build_targets(&["//:site"]);

    assert_eq!(outcomes.len(), 2);
    assert_eq!(
        ended(&outcomes, &OutcomeClass::Built),
        ["//:site", "//lib:assets"]
    );
    assert_eq!(
        chain.files("out"),
        [
            "out/lib/assets/one.txt",
            "out/lib/assets/sub/two.txt",
            "out/site/one.txt",
            "out/site/sub/two.txt"
        ]
    );
    assert_eq!(chain.recorded(), ["//:site", "//lib:assets"]);
    // The graph the build leaves is the whole workspace's, not the part that was built.
    assert_eq!(
        fs::read_to_string(chain.state().graph()).unwrap(),
        CHAIN_GRAPH.trim_end()
    );
}

#[test]
fn a_named_alias_builds_the_target_behind_it() {
    let chain = Fixture::copy_of("chain", &["greet.sh"]);

    let outcomes = chain.build_targets(&["//:text"]);

    assert_eq!(outcomes.len(), 1);
    assert_eq!(ended(&outcomes, &OutcomeClass::Built), ["//lib:words"]);
    assert_eq!(chain.files("out"), ["out/lib/words"]);
}

#[test]
fn a_full_build_after_a_named_one_matches_a_clean_full_build() {
    let named_first = Fixture::copy_of("chain", &["greet.sh"]);
    named_first.build_targets(&["//:site"]);
    let clean = Fixture::copy_of("chain", &["greet.sh"]);
    clean.build();

    let outcomes = named_first.build();

    assert_eq!(
        ended(&outcomes, &OutcomeClass::Cached),
        ["//:site", "//lib:assets"]
    );
    assert_eq!(
        ended(&outcomes, &OutcomeClass::Built),
        ["//:greet", "//:greeting", "//:report", "//lib:words"]
    );
    assert_eq!(
        named_first.snapshot().first_difference(&clean.snapshot()),
        None
    );
}
