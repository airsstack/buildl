//! What a build finds when the run before it was cut short, or when a state file is not what
//! buildl wrote.
//!
//! A build that is interrupted leaves one of four states behind, because content is stored
//! before the row that names it: content with no row, a row in the journal that was never
//! folded, a staging file, and an output a command was writing, which no row names. A worker
//! cannot be stopped at a chosen instant from inside a test, so each state is made from whole
//! builds.

use std::fs;

use buildl::buildl_core::{ActionCache, Error, OutcomeClass};
use buildl::cache::LocalCache;
use buildl::state::StateLock;

use crate::common::{CHAIN_LABELS, CHAIN_OUTPUTS, Fixture, ended};

/// A copy of the fixture `chain`, built once.
fn built_chain() -> Fixture {
    let chain = Fixture::copy_of("chain", &["greet.sh"]);
    chain.build();
    chain
}

/// Copies into `to`'s blob store every file of `from`'s that `to` does not hold, with its mode.
fn copy_missing_blobs(from: &Fixture, to: &Fixture) {
    for blob in from.files(".buildl/cas") {
        let copy = to.root().join(&blob);
        if !copy.exists() {
            fs::create_dir_all(copy.parent().unwrap()).unwrap();
            fs::copy(from.root().join(&blob), copy).unwrap();
        }
    }
}

#[test]
fn blobs_without_rows_are_not_trusted_and_the_build_finishes() {
    let whole = built_chain();
    let partial = Fixture::copy_of("chain", &["greet.sh"]);
    partial.build_targets(&["//lib:words"]);
    copy_missing_blobs(&whole, &partial);
    assert_eq!(partial.files(".buildl/cas"), whole.files(".buildl/cas"));
    assert_eq!(partial.recorded(), ["//lib:words"]);

    let outcomes = partial.build();

    assert_eq!(ended(&outcomes, &OutcomeClass::Cached), ["//lib:words"]);
    assert_eq!(
        ended(&outcomes, &OutcomeClass::Built),
        [
            "//:greet",
            "//:greeting",
            "//:report",
            "//:site",
            "//lib:assets"
        ]
    );
    assert_eq!(partial.snapshot().first_difference(&whole.snapshot()), None);
}

#[test]
fn a_partial_output_no_row_names_is_cleared_and_built_again() {
    let chain = Fixture::copy_of("chain", &["greet.sh"]);
    let out = chain.root().join("out");
    fs::create_dir_all(out.join("site")).unwrap();
    fs::create_dir_all(out.join("greeting")).unwrap();
    fs::write(out.join("report"), b"cut").unwrap();
    fs::write(out.join("site/stray.txt"), b"stray\n").unwrap();
    fs::write(out.join("greeting/stray.txt"), b"stray\n").unwrap();
    let clean = built_chain();

    let outcomes = chain.build();

    assert_eq!(ended(&outcomes, &OutcomeClass::Built), CHAIN_LABELS);
    assert_eq!(chain.files("out"), CHAIN_OUTPUTS);
    assert_eq!(chain.snapshot().first_difference(&clean.snapshot()), None);
}

#[test]
fn rows_left_in_the_journal_are_folded_by_the_next_run() {
    let chain = built_chain();
    let layout = chain.state();
    let folded = fs::read(layout.cache()).unwrap();
    let rows = chain.rows();
    fs::remove_file(layout.cache()).unwrap();
    {
        let lock = StateLock::acquire(chain.state()).unwrap();
        let mut cache = LocalCache::open(lock.layout()).unwrap();
        for (label, row) in rows {
            cache.record(label, row).unwrap();
        }
    }
    assert!(layout.journal().exists());
    assert!(!layout.cache().exists());

    let outcomes = chain.build();

    assert_eq!(ended(&outcomes, &OutcomeClass::Cached), CHAIN_LABELS);
    assert!(!layout.journal().exists());
    assert_eq!(fs::read(layout.cache()).unwrap(), folded);
}

#[test]
fn leftover_staging_files_are_swept_and_change_nothing() {
    let chain = built_chain();
    let first = chain.snapshot();
    let layout = chain.state();
    fs::create_dir_all(layout.blob_staging()).unwrap();
    fs::write(layout.blob_staging().join("blob"), b"cut").unwrap();
    fs::write(layout.dir().join(".tmp-x"), b"cut").unwrap();
    assert_eq!(chain.staged(), [".tmp-x", "cas/tmp/blob"]);

    let outcomes = chain.build();

    assert_eq!(ended(&outcomes, &OutcomeClass::Cached), CHAIN_LABELS);
    assert_eq!(first.first_difference(&chain.snapshot()), None);
    assert_eq!(chain.staged(), Vec::<String>::new());
}

/// The first half of `cache`.
fn cut_in_half(cache: &[u8]) -> Vec<u8> {
    cache[..cache.len() / 2].to_vec()
}

/// `cache` followed by one space.
fn space_appended(cache: &[u8]) -> Vec<u8> {
    [cache, b" "].concat()
}

/// `cache` with the first hex digit of its first row's key digest changed. The key's digest is
/// the one that follows the end of the key's components.
fn key_digest_changed(cache: &[u8]) -> Vec<u8> {
    let marker = b"}},\"digest\":\"";
    let digit = cache
        .windows(marker.len())
        .position(|window| window == marker)
        .unwrap()
        + marker.len();
    let mut changed = cache.to_vec();
    changed[digit] = if changed[digit] == b'0' { b'1' } else { b'0' };
    changed
}

/// Builds `chain`, puts what `damage` makes of its `cache.json` in the file's place, and holds
/// the workspace to refusing it: a build and a plan fail naming the file, the file is left as
/// it is, no action runs, and the two commands that read no state still work.
fn refused_after(damage: fn(&[u8]) -> Vec<u8>) {
    let chain = built_chain();
    let cache = chain.state().cache();
    let damaged = damage(&fs::read(&cache).unwrap());
    fs::write(&cache, &damaged).unwrap();

    for refused in [chain.try_build().map(drop), chain.try_plan().map(drop)] {
        match refused.unwrap_err() {
            Error::CorruptState { file, .. } => {
                assert!(file.ends_with(".buildl/cache.json"), "{}", file.display());
            }
            other => panic!("expected a corrupt state file, got {other:?}"),
        }
    }

    assert_eq!(fs::read(&cache).unwrap(), damaged);
    // An action that ran would have left a line in the log.
    assert_eq!(chain.log_lines().len(), 6);
    let workspace = chain.open();
    assert_eq!(workspace.check().unwrap().len(), 9);
    assert_eq!(workspace.graph().unwrap().len(), 6);
}

#[test]
fn a_corrupt_cache_is_refused() {
    refused_after(cut_in_half);
    refused_after(space_appended);
    refused_after(key_digest_changed);
}
