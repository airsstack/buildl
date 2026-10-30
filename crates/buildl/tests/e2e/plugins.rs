//! The plugins a workspace names, checked before any build file runs: a path plugin's
//! directory, and a git plugin's lock entry and stored files.
//!
//! The fixture `chain` is used for the check alone: the plugins it is given hold no build-file
//! code. The fixture `plugin_path` has a plugin that declares a target, built and refused.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::PathBuf;

use buildl::buildl_core::{
    CommitId, Directory, Error, GitUrl, Label, OutcomeClass, PluginName, Rev,
};
use buildl::plugins::{Lock, LockEntry, tree_digest};

use crate::common::{CHAIN_LABELS, Fixture, ended};

const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";
const URL: &str = "https://example.com/acme/go.git";

fn go() -> PluginName {
    PluginName::parse("go").unwrap()
}

/// A copy of `chain` whose manifest also names the git plugin `go`, pinned to `rev`.
fn naming_git_plugin(rev: &str) -> Fixture {
    let chain = Fixture::copy_of("chain", &["greet.sh"]);
    chain.append(
        "buildl.toml",
        format!("\n[plugins.go]\ngit = \"{URL}\"\nrev = \"{rev}\"\n").as_bytes(),
    );
    chain
}

/// Stores the plugin `go` as `rev` resolved to `COMMIT`, and locks what was stored. Returns the
/// store directory.
fn fetched(fixture: &Fixture, rev: &str) -> PathBuf {
    let store = fixture.state().dir().join("plugins/go").join(COMMIT);
    fs::create_dir_all(store.join("lib")).unwrap();
    fs::write(store.join("lib/go.lua"), b"return {}\n").unwrap();
    fs::write(
        store.join("extension.toml"),
        b"[extension]\nname = \"go\"\n",
    )
    .unwrap();
    let mut lock = Lock::default();
    lock.insert(
        go(),
        LockEntry {
            commit: CommitId::parse(COMMIT).unwrap(),
            dir: Directory::root(),
            git: GitUrl::parse(URL).unwrap(),
            rev: Rev::parse(rev).unwrap(),
            sha256: tree_digest(&go(), &store).unwrap(),
        },
    );
    lock.write(fixture.root(), &fixture.root().join(".buildl"))
        .unwrap();
    store
}

/// The label `raw` names, read by `fixture`'s workspace.
fn label(fixture: &Fixture, raw: &str) -> Label {
    fixture.open().label(raw).unwrap()
}

#[test]
fn a_workspace_without_plugins_is_unchanged() {
    let chain = Fixture::copy_of("chain", &["greet.sh"]);

    chain.open().check().unwrap();
    let outcomes = chain.build();

    assert_eq!(outcomes.len(), CHAIN_LABELS.len());
    assert!(
        outcomes
            .iter()
            .all(|outcome| outcome.class == OutcomeClass::Built),
        "{outcomes:?}"
    );
}

#[test]
fn a_fetched_git_plugin_is_accepted() {
    let chain = naming_git_plugin("v1.2.0");
    fetched(&chain, "v1.2.0");

    chain.open().check().unwrap();
    chain.build();
}

#[test]
fn a_path_plugin_that_is_there_is_accepted() {
    let chain = Fixture::copy_of("chain", &["greet.sh"]);
    chain.append(
        "buildl.toml",
        b"\n[plugins.local]\npath = \"tools/local\"\n",
    );
    fs::create_dir_all(chain.root().join("tools/local")).unwrap();

    chain.open().check().unwrap();
}

#[test]
fn a_missing_path_plugin_refuses_check() {
    let chain = Fixture::copy_of("chain", &["greet.sh"]);
    chain.append(
        "buildl.toml",
        b"\n[plugins.local]\npath = \"tools/local\"\n",
    );

    let error = chain.open().check().unwrap_err();

    let Error::PluginNotFetched { plugin, reason } = error else {
        unreachable!("a missing directory is not fetched, got {error:?}");
    };
    assert_eq!(plugin.to_string(), "local");
    assert!(reason.contains("tools/local"), "{reason}");
    assert!(matches!(
        chain.open().graph(),
        Err(Error::PluginNotFetched { .. })
    ));
    assert!(matches!(
        chain.open().doctor(),
        Err(Error::PluginNotFetched { .. })
    ));
    assert!(matches!(
        chain.try_plan(),
        Err(Error::PluginNotFetched { .. })
    ));
}

#[test]
fn an_unfetched_git_plugin_refuses_build_naming_fetch() {
    let chain = naming_git_plugin("v1.2.0");

    let error = chain.try_build().unwrap_err();

    let Error::PluginNotFetched { plugin, reason } = &error else {
        unreachable!("a plugin without a lock entry is not fetched, got {error:?}");
    };
    assert_eq!(*plugin, go());
    assert!(reason.contains("buildl plugins fetch"), "{reason}");
    assert_eq!(chain.files("out"), Vec::<String>::new());
}

#[test]
fn a_locked_plugin_whose_store_is_missing_refuses_naming_fetch() {
    let chain = naming_git_plugin("v1.2.0");
    let store = fetched(&chain, "v1.2.0");
    fs::remove_dir_all(store).unwrap();

    let error = chain.open().check().unwrap_err();

    let Error::PluginNotFetched { reason, .. } = &error else {
        unreachable!("a missing store is not fetched, got {error:?}");
    };
    assert!(reason.contains("buildl plugins fetch"), "{reason}");
}

#[test]
fn a_stale_lock_entry_refuses() {
    let chain = naming_git_plugin("v1.2.0");
    fetched(&chain, "v1.2.0");
    chain.replace_line("buildl.toml", "rev = \"v1.2.0\"", "rev = \"v1.3.0\"");

    let error = chain.open().check().unwrap_err();

    let Error::PluginNotFetched { plugin, reason } = &error else {
        unreachable!("a lock out of date is not fetched, got {error:?}");
    };
    assert_eq!(*plugin, go());
    assert!(reason.contains("buildl plugins fetch"), "{reason}");

    chain.replace_line("buildl.toml", "rev = \"v1.3.0\"", "rev = \"v1.2.0\"");
    chain.replace_line(
        "buildl.toml",
        &format!("git = \"{URL}\""),
        "git = \"https://example.com/other/go.git\"",
    );
    assert!(matches!(
        chain.open().check(),
        Err(Error::PluginNotFetched { .. })
    ));
}

#[test]
fn an_edited_store_refuses_as_changed() {
    let chain = naming_git_plugin("v1.2.0");
    let store = fetched(&chain, "v1.2.0");
    fs::write(store.join("lib/go.lua"), b"return { edited = true }\n").unwrap();

    let error = chain.try_build().unwrap_err();

    let Error::PluginChanged { plugin, .. } = &error else {
        unreachable!("an edited store is changed, got {error:?}");
    };
    assert_eq!(*plugin, go());
    assert_eq!(chain.files("out"), Vec::<String>::new());
}

#[test]
fn a_link_added_to_the_store_refuses_as_changed() {
    let chain = naming_git_plugin("v1.2.0");
    let store = fetched(&chain, "v1.2.0");
    symlink("lib/go.lua", store.join("link")).unwrap();

    let error = chain.open().check().unwrap_err();

    let Error::PluginChanged { plugin, reason } = &error else {
        unreachable!("a store with a link is changed, got {error:?}");
    };
    assert_eq!(*plugin, go());
    assert!(reason.contains("link"), "{reason}");
}

#[test]
fn log_and_clean_do_not_check_plugins() {
    let logged = Fixture::copy_of("logged", &[]);
    logged.build();
    logged.append(
        "buildl.toml",
        format!("\n[plugins.go]\ngit = \"{URL}\"\nrev = \"v1.2.0\"\n").as_bytes(),
    );
    assert!(matches!(
        logged.open().check(),
        Err(Error::PluginNotFetched { .. })
    ));

    let shown = logged.open().log(&label(&logged, "//:hello")).unwrap();
    let cleaned = logged.open().clean().unwrap();

    assert_eq!(shown.stdout, b"said hello.txt\n");
    assert_eq!(cleaned, buildl::Cleaned::Removed);
}

#[test]
fn a_path_plugin_builds_and_caches() {
    let hello = Fixture::copy_of("plugin_path", &[]);

    let first = hello.build();

    assert_eq!(ended(&first, &OutcomeClass::Built), ["//:greet"]);
    assert_eq!(
        fs::read(hello.root().join("out/greet")).unwrap(),
        b"hello from a plugin\n"
    );

    let second = hello.build();

    assert_eq!(ended(&second, &OutcomeClass::Cached), ["//:greet"]);
}

#[test]
fn a_plugin_target_outside_its_cap_is_refused_before_anything_runs() {
    let hello = Fixture::copy_of("plugin_path", &[]);
    hello.replace_line(
        "buildl.toml",
        "proc.run = [\"cp\"]",
        "proc.run = [\"sort\"]",
    );

    let error = hello.try_build().unwrap_err();

    let Error::PluginRefused { violation } = &error else {
        unreachable!("a want outside the plugin's cap is refused, got {error:?}");
    };
    assert_eq!(violation.site.label.to_string(), "//:greet");
    assert_eq!(violation.plugin.name, PluginName::parse("hello").unwrap());
    assert_eq!(hello.files("out"), Vec::<String>::new());
}
