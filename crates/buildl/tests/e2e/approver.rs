//! The approver a workspace names, each mode driven through the facade: what it asks, what it
//! remembers, and what a refusal leaves behind.
//!
//! The approvals live in a directory of the fixture's own, outside the workspace, so no test
//! reads or writes the real home.

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;

use buildl::approver::NoTerminal;
use buildl::buildl_core::{Error, OutcomeClass, Sample, Scope};

use crate::common::{CHAIN_OUTPUTS, Fake, Fixture, ended};

/// A copy of `chain` whose approver is `mode`.
fn chain(mode: &str) -> Fixture {
    let chain = Fixture::copy_of("chain", &["greet.sh"]);
    chain.approver(mode);
    chain
}

/// A copy of `tested`, run in the input sandbox so it can be verified, and built once.
fn sandboxed_and_built() -> Fixture {
    let tested = Fixture::copy_of("tested", &[]);
    tested.replace_line(
        "buildl.toml",
        "strategy = \"bare\"",
        "strategy = \"sandbox\"",
    );
    tested.build();
    tested
}

#[test]
fn manifest_builds_without_asking() {
    let chain = chain("manifest");
    let mut console = Fake::answering(&[]);

    let outcomes = chain.try_build_asking(&mut console).unwrap();

    assert_eq!(outcomes.len(), 6);
    assert_eq!(console.prompts, Vec::<String>::new());
    assert!(!chain.approvals_file().exists());
}

#[test]
fn interactive_yes_builds_and_remembers() {
    let chain = chain("interactive");
    let mut console = Fake::answering(&["y"]);

    let outcomes = chain.try_build_asking(&mut console).unwrap();

    assert_eq!(ended(&outcomes, &OutcomeClass::Built).len(), 6);
    assert_eq!(console.prompts.len(), 1);
    let file = chain.approvals_file();
    assert!(file.is_file(), "{}", file.display());
    assert!(file.starts_with(chain.data()));
    assert!(!file.starts_with(chain.root()));
    assert!(
        chain
            .files(".")
            .iter()
            .all(|path| !path.contains("approved")),
        "the workspace holds an approvals file"
    );

    // What is remembered is not asked about again.
    let mut again = Fake::answering(&[]);
    let outcomes = chain.try_build_asking(&mut again).unwrap();
    assert_eq!(ended(&outcomes, &OutcomeClass::Cached).len(), 6);
    assert_eq!(again.prompts, Vec::<String>::new());
}

#[test]
fn interactive_asks_only_about_new_wants() {
    let chain = chain("interactive");
    chain
        .try_build_asking(&mut Fake::answering(&["y"]))
        .unwrap();
    // A target now reads one more variable, which the ceiling allows.
    chain.replace_line(
        "buildl.toml",
        "env.read = [\"BUILDL_FIXTURE_FLAVOR\"]",
        "env.read = [\"BUILDL_FIXTURE_FLAVOR\", \"X\"]",
    );
    chain.replace_line(
        "build.lua",
        "  env    = { \"BUILDL_FIXTURE_FLAVOR\" },",
        "  env    = { \"BUILDL_FIXTURE_FLAVOR\", \"X\" },",
    );
    let mut console = Fake::answering(&["y"]);

    chain.try_build_asking(&mut console).unwrap();

    assert_eq!(console.prompts.len(), 1);
    let prompt = &console.prompts[0];
    assert!(prompt.contains("read variables: X\n"), "{prompt}");
    assert!(!prompt.contains("run programs"), "{prompt}");
    assert!(!prompt.contains("write into"), "{prompt}");
}

#[test]
fn interactive_asks_for_discovered_targets_only_about_what_discovery_did_not_cover() {
    let discovery = Fixture::copy_of("discovery", &[]);
    discovery.approver("interactive");
    // The targets declared from the discovery result read a variable the discovery target does
    // not.
    discovery.replace_line("buildl.toml", "env.read = []", "env.read = [\"X\"]");
    discovery.replace_line(
        "build.lua",
        "  b.target(name, { inputs = { \"item.txt\" }, run = { \"cp\", \"$in\", \"$out\" } })",
        "  b.target(name, { inputs = { \"item.txt\" }, env = { \"X\" }, run = { \"cp\", \"$in\", \"$out\" } })",
    );
    let mut console = Fake::answering(&["y", "y"]);

    let outcomes = discovery.try_build_asking(&mut console).unwrap();

    assert_eq!(ended(&outcomes, &OutcomeClass::Built).len(), 3);
    assert_eq!(console.prompts.len(), 2, "{:?}", console.prompts);
    let (first, second) = (&console.prompts[0], &console.prompts[1]);
    assert!(first.contains("run programs: cp\n"), "{first}");
    assert!(!first.contains("read variables"), "{first}");
    assert!(second.contains("read variables: X\n"), "{second}");
    assert!(!second.contains("run programs"), "{second}");
    assert!(!second.contains("write into"), "{second}");
}

#[test]
fn interactive_no_is_refused_and_builds_nothing() {
    let chain = chain("interactive");
    let mut console = Fake::answering(&["n"]);

    let error = chain.try_build_asking(&mut console).unwrap_err();

    assert!(matches!(error, Error::NotApproved { .. }), "{error:?}");
    assert!(error.to_string().starts_with("not approved"), "{error}");
    assert_eq!(chain.files("out"), Vec::<String>::new());
    assert_eq!(chain.recorded(), Vec::<String>::new());
    assert!(!chain.approvals_file().exists());
}

#[test]
fn interactive_with_no_terminal_is_refused_with_the_remedy() {
    let chain = chain("interactive");
    let mut console = Fake::nobody();

    let error = chain.try_build_asking(&mut console).unwrap_err();

    assert!(matches!(error, Error::NotApproved { .. }), "{error:?}");
    let text = error.to_string();
    assert!(text.contains("no terminal to ask on"), "{text}");
    assert!(text.contains("[approver] default = \"manifest\""), "{text}");
    assert_eq!(console.prompts, Vec::<String>::new());
    assert_eq!(chain.files("out"), Vec::<String>::new());
}

#[test]
fn a_corrupt_approvals_file_is_corrupt_state() {
    let chain = chain("interactive");
    let file = chain.approvals_file();
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, "not what buildl writes").unwrap();
    let mut console = Fake::answering(&["y"]);

    let error = chain.try_build_asking(&mut console).unwrap_err();

    assert!(matches!(error, Error::CorruptState { .. }), "{error:?}");
    assert!(error.to_string().contains(&file.display().to_string()));
    assert_eq!(console.prompts, Vec::<String>::new());
    assert_eq!(chain.files("out"), Vec::<String>::new());
}

#[test]
fn an_unwritable_approvals_file_after_yes_refuses_and_builds_nothing() {
    let chain = chain("interactive");
    let directory = chain.approvals_file().parent().unwrap().to_path_buf();
    fs::create_dir_all(&directory).unwrap();
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o500)).unwrap();
    let mut console = Fake::answering(&["y"]);

    let error = chain.try_build_asking(&mut console).unwrap_err();

    // Restore the mode first, so the directory can be removed whatever happens.
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(matches!(error, Error::Adapter { .. }), "{error:?}");
    assert!(error.to_string().starts_with("approver: "), "{error}");
    assert_eq!(chain.files("out"), Vec::<String>::new());
    assert_eq!(chain.recorded(), Vec::<String>::new());
}

#[test]
fn deny_refuses_build_test_and_verify() {
    let tested = sandboxed_and_built();
    tested.approver("deny");

    let built = tested.try_build().unwrap_err();
    let ran = tested.try_test_asking(&mut NoTerminal).unwrap_err();
    let verified = tested
        .open()
        .verify(
            &Scope::Workspace,
            &BTreeMap::new(),
            Sample::All,
            &mut NoTerminal,
        )
        .unwrap_err();

    for error in [built, ran, verified] {
        assert!(matches!(error, Error::NotApproved { .. }), "{error:?}");
        assert!(error.to_string().contains("deny"), "{error}");
    }
}

#[test]
fn plan_asks_nothing_under_deny() {
    let chain = chain("deny");

    let plan = chain.try_plan().unwrap();

    assert!(!plan.wanted().is_empty());
    assert!(
        CHAIN_OUTPUTS
            .iter()
            .all(|path| !chain.root().join(path).exists())
    );
}
