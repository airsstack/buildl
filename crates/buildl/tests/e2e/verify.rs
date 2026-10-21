//! Verifying a built workspace: its cached actions re-run in the input sandbox, and nothing
//! about the workspace changed by it.
//!
//! The fixture `verified` holds four targets, each there for one behaviour:
//!
//! - `plain` copies a file, and `copied` reads what `plain` built, so one dependency edge is
//!   re-run;
//! - `site` copies a directory, so a tree output is compared;
//! - `stamp` writes its process id, so it does not reproduce: the one mismatch.
//!
//! No two targets produce the same bytes, so removing the content of one target leaves every
//! other row whole.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::num::NonZeroUsize;
use std::path::PathBuf;

use buildl::buildl_core::{
    Check, Error, FileChange, Label, OutputDifference, OutputManifest, OutputName, PortName,
    Sample, Scope, Verification,
};

use crate::common::Fixture;

/// A copy of `verified` that has been built.
fn built() -> Fixture {
    let verified = Fixture::copy_of("verified", &[]);
    verified.build();
    verified
}

/// Verifies every target of `fixture`, sampled by `sample`.
fn verify(fixture: &Fixture, sample: Sample) -> Verification {
    fixture
        .open()
        .verify(&Scope::Workspace, &BTreeMap::new(), sample)
        .unwrap()
}

/// The label `raw` names.
fn label(raw: &str) -> Label {
    Label::parse(raw).unwrap()
}

/// The labels of the targets `verification` checked, in order.
fn labels(verification: &Verification) -> Vec<String> {
    verification
        .checked()
        .iter()
        .map(|(label, _)| label.to_string())
        .collect()
}

/// Every file below the output directory of `fixture` with its bytes.
fn outputs(fixture: &Fixture) -> BTreeMap<String, Vec<u8>> {
    fixture
        .files("out")
        .into_iter()
        .map(|relative| {
            let bytes = fs::read(fixture.root().join(&relative)).unwrap();
            (relative, bytes)
        })
        .collect()
}

/// Every file of the blob store of `fixture`, from the workspace root.
fn blobs(fixture: &Fixture) -> BTreeSet<String> {
    fixture.files(".buildl/cas").into_iter().collect()
}

/// The path of the store's blob for the digest written as `hex`.
fn blob(fixture: &Fixture, hex: &str) -> PathBuf {
    let (shard, rest) = hex.split_at(2);
    fixture.state().blobs().join(shard).join(rest)
}

/// The sample that holds only the one candidate with the lowest key digest.
const fn lowest_one() -> Sample {
    Sample::Lowest(NonZeroUsize::new(1).unwrap())
}

#[test]
fn verify_names_only_the_target_that_does_not_reproduce() {
    let verified = built();

    let verification = verify(&verified, Sample::All);

    let reproduced = |raw: &str| (label(raw), Check::Verified);
    assert_eq!(
        verification,
        Verification::new(
            vec![
                reproduced("//:copied"),
                reproduced("//:plain"),
                reproduced("//:site"),
                (
                    label("//:stamp"),
                    Check::Mismatched {
                        output: OutputName::parse("stamp").unwrap(),
                        difference: OutputDifference::File(FileChange::Bytes),
                    },
                ),
            ],
            0,
            0,
        )
    );
}

#[test]
fn verify_leaves_the_cache_and_the_log_as_they_were() {
    let verified = built();
    let cache = fs::read(verified.state().cache()).unwrap();
    let log = verified.log_lines();

    verify(&verified, Sample::All);

    assert_eq!(fs::read(verified.state().cache()).unwrap(), cache);
    assert_eq!(verified.log_lines(), log);
    assert!(!verified.state().journal().exists());
}

#[test]
fn verify_puts_cached_outputs_back_and_never_its_own() {
    let verified = built();
    let built_outputs = outputs(&verified);
    fs::remove_dir_all(verified.root().join("out")).unwrap();

    verify(&verified, Sample::All);

    assert_eq!(outputs(&verified), built_outputs);
    assert!(built_outputs.contains_key("out/stamp"), "{built_outputs:?}");
}

#[test]
fn a_sample_of_one_checks_the_same_target_twice_in_a_row() {
    let verified = built();

    let first = verify(&verified, lowest_one());
    let second = verify(&verified, lowest_one());

    assert_eq!(first.checked().len(), 1, "{first:?}");
    assert_eq!(first.not_sampled(), 3);
    assert_eq!(first.not_cached(), 0);
    assert_eq!(labels(&first), labels(&second));
    assert_eq!(second.not_sampled(), 3);
}

#[test]
fn a_target_never_built_is_counted_not_cached() {
    let verified = built();
    verified.append(
        "build.lua",
        b"b.target(\"fresh\", { inputs = { \"a.txt\" }, run = { \"cp\", \"$in\", \"$out\" } })\n",
    );

    let verification = verify(&verified, Sample::All);

    assert_eq!(verification.not_cached(), 1);
    assert_eq!(verification.not_sampled(), 0);
    assert_eq!(
        labels(&verification),
        ["//:copied", "//:plain", "//:site", "//:stamp"]
    );
}

#[test]
fn a_dependent_of_a_target_whose_content_is_gone_is_skipped() {
    let verified = built();
    let plain = label("//:plain");
    let manifest = verified.rows()[&plain].outputs()[0];
    let manifest_bytes = fs::read(blob(&verified, &manifest.to_string())).unwrap();
    let OutputManifest::File(stored) = OutputManifest::from_bytes(&manifest_bytes).unwrap() else {
        panic!("plain's output is a file");
    };
    let content = stored.digest.to_string();
    fs::remove_file(blob(&verified, &content)).unwrap();

    let verification = verify(&verified, Sample::All);

    assert_eq!(verification.not_cached(), 1);
    assert_eq!(labels(&verification), ["//:copied", "//:site", "//:stamp"]);
    let checks: BTreeMap<String, &Check> = verification
        .checked()
        .iter()
        .map(|(label, check)| (label.to_string(), check))
        .collect();
    assert_eq!(
        checks["//:copied"],
        &Check::Skipped { because: plain },
        "{verification:?}"
    );
    assert_eq!(checks["//:site"], &Check::Verified);
    assert!(
        matches!(checks["//:stamp"], Check::Mismatched { .. }),
        "{verification:?}"
    );
}

#[test]
fn verify_is_refused_under_bare_and_os_sandbox() {
    for (strategy, reason) in [
        (
            "bare",
            "and verify re-runs actions in the input sandbox only",
        ),
        (
            "os-sandbox",
            "and this version of buildl runs \"bare\" and \"sandbox\" only",
        ),
    ] {
        let verified = Fixture::copy_of("verified", &[]);
        verified.append(
            "buildl.toml",
            format!("strategy = \"{strategy}\"\n").as_bytes(),
        );

        let error = verified
            .open()
            .verify(&Scope::Workspace, &BTreeMap::new(), Sample::All)
            .unwrap_err();

        assert!(
            matches!(
                &error,
                Error::Adapter {
                    port: PortName::Manifest,
                    ..
                }
            ),
            "{error:?}"
        );
        assert_eq!(
            error.to_string(),
            format!(
                "manifest: buildl.toml: [execution] strategy is \"{strategy}\", {reason}: \
                 write strategy = \"sandbox\" under [execution]"
            )
        );
        // Refused before the state directory was taken, so nothing was made.
        assert!(!verified.root().join(".buildl").exists());
        assert!(!verified.root().join("out").exists());
    }
}

#[test]
fn gc_after_verify_removes_what_the_reruns_stored() {
    let verified = built();
    let before = blobs(&verified);

    verify(&verified, Sample::All);
    let during = blobs(&verified);
    verified.open().gc().unwrap();

    assert_ne!(during, before, "the re-run of stamp stored its own bytes");
    assert_eq!(blobs(&verified), before);
}

/// The scratch directory goes with the lock; each exec dir's own removal is the sandbox's, and
/// its unit tests prove it.
#[test]
fn verify_leaves_no_scratch_directory() {
    let verified = built();

    verify(&verified, Sample::All);

    assert!(!verified.scratch().dir().exists());
}
