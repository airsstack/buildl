//! `fetch`: pinning git plugins in `buildl.lock` and storing their files, from repositories the
//! tests build in temporary directories and reach over `file://`.
//!
//! The machine's own `git` is run, as `buildl plugins fetch` runs it. A machine with none fails
//! these tests rather than skipping them.

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::Command;

use buildl::buildl_core::json::canonical;
use buildl::buildl_core::{Directory, Error, GitUrl, PluginName, PluginSource, Result, Rev};
use buildl::plugins::{FetchStatus, Fetched, Lock, check, fetch, tree_digest};
use buildl::search_path::SearchPath;

use crate::common::Fixture;

fn plugin(raw: &str) -> PluginName {
    PluginName::parse(raw).unwrap()
}

/// A git repository, in a temporary directory, that holds one plugin's files.
struct Source {
    _dir: tempfile::TempDir,
    path: PathBuf,
}

impl Source {
    /// A repository whose first commit holds a complete plugin, tagged `v1.0.0`.
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = fs::canonicalize(dir.path()).unwrap();
        let source = Self { _dir: dir, path };
        source.git(&["init", "-q", "-b", "main"]);
        fs::create_dir(source.path.join("lib")).unwrap();
        fs::write(source.path.join("lib/go.lua"), "return {}\n").unwrap();
        source.git(&["add", "lib"]);
        source.commit("extension.toml", "[extension]\nname = \"go\"\n");
        source.git(&["tag", "v1.0.0"]);
        source
    }

    /// A repository whose first commit holds a complete plugin in each of the folders `dirs`,
    /// each with a `lib/go.lua` that names its folder, and `outside.txt` at the root, tagged
    /// `v1.0.0`.
    fn in_folders(dirs: &[&str]) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = fs::canonicalize(dir.path()).unwrap();
        let source = Self { _dir: dir, path };
        source.git(&["init", "-q", "-b", "main"]);
        for folder in dirs {
            let at = source.path.join(folder);
            fs::create_dir_all(at.join("lib")).unwrap();
            fs::write(at.join("lib/go.lua"), format!("return \"{folder}\"\n")).unwrap();
            fs::write(at.join("extension.toml"), "[extension]\nname = \"go\"\n").unwrap();
        }
        fs::write(source.path.join("outside.txt"), "not the plugin\n").unwrap();
        source.git(&["add", "."]);
        source.git(&["commit", "-q", "-m", "plugin"]);
        source.git(&["tag", "v1.0.0"]);
        source
    }

    /// Commits a symbolic link `name` to `target`, tags the commit `v1.1.0` and returns it.
    fn commit_link(&self, name: &str, target: &Path) -> String {
        symlink(target, self.path.join(name)).unwrap();
        self.git(&["add", name]);
        self.git(&["commit", "-q", "-m", "link"]);
        self.git(&["tag", "v1.1.0"]);
        self.head()
    }

    /// Runs `git args` in the repository and returns what it printed to standard output.
    fn git(&self, args: &[&str]) -> String {
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
            .current_dir(&self.path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    /// Commits the file `name` holding `content`, and returns the new commit.
    fn commit(&self, name: &str, content: &str) -> String {
        fs::write(self.path.join(name), content).unwrap();
        self.git(&["add", name]);
        self.git(&["commit", "-q", "-m", name]);
        self.head()
    }

    fn head(&self) -> String {
        self.git(&["rev-parse", "HEAD"])
    }

    fn url(&self) -> String {
        format!("file://{}", self.path.display())
    }
}

/// The sources of a workspace that names `plugin` as `source` at `rev`.
fn named(plugin: &str, source: &Source, rev: &str) -> BTreeMap<PluginName, PluginSource> {
    named_in(plugin, source, rev, "")
}

/// The sources of a workspace that names `plugin` as the folder `dir` of `source` at `rev`.
fn named_in(
    plugin: &str,
    source: &Source,
    rev: &str,
    dir: &str,
) -> BTreeMap<PluginName, PluginSource> {
    BTreeMap::from([(
        self::plugin(plugin),
        PluginSource::Git {
            url: GitUrl::parse(source.url()).unwrap(),
            rev: Rev::parse(rev).unwrap(),
            dir: Directory::parse(dir).unwrap(),
        },
    )])
}

/// A workspace whose manifest names `go` as `source` at `rev`.
fn workspace(source: &Source, rev: &str) -> Fixture {
    let chain = Fixture::copy_of("chain", &["greet.sh"]);
    chain.append(
        "buildl.toml",
        format!(
            "\n[plugins.go]\ngit = \"{}\"\nrev = \"{rev}\"\n",
            source.url()
        )
        .as_bytes(),
    );
    chain
}

/// Fetches `sources` into `fixture`, finding `git` on this process's `PATH`.
fn run(fixture: &Fixture, sources: &BTreeMap<PluginName, PluginSource>) -> Result<Fetched> {
    fetch(
        fixture.root(),
        &fixture.state(),
        sources,
        &SearchPath::from_process(),
    )
}

/// The names in the directory `dir`, in order. A directory that is not there holds none.
fn names(dir: &Path) -> Vec<String> {
    let Ok(listing) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = listing
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

fn lock_file(fixture: &Fixture) -> PathBuf {
    fixture.root().join("buildl.lock")
}

/// What `fetch` reported, as `(plugin, commit, status)`.
fn reported(fetched: &Fetched) -> Vec<(String, String, FetchStatus)> {
    fetched
        .iter()
        .map(|one| (one.plugin.to_string(), one.commit.to_string(), one.status))
        .collect()
}

#[test]
fn first_fetch_writes_the_lock_and_the_store() {
    let source = Source::new();
    let commit = source.head();
    let fixture = workspace(&source, "v1.0.0");

    let fetched = run(&fixture, &named("go", &source, "v1.0.0")).unwrap();

    assert_eq!(
        reported(&fetched),
        [("go".to_owned(), commit.clone(), FetchStatus::Fetched)]
    );
    let store = fixture.state().plugins().join("go").join(&commit);
    assert_eq!(
        fs::read_to_string(store.join("lib/go.lua")).unwrap(),
        "return {}\n"
    );
    assert_eq!(
        names(&store),
        ["extension.toml", "lib"],
        "no .git directory is kept"
    );
    assert_eq!(names(&fixture.state().plugins()), ["go"]);
    assert_eq!(
        names(&fixture.state().plugins().join("go")),
        [commit.as_str()]
    );

    let lock = Lock::read(fixture.root()).unwrap();
    let entry = lock.get(&plugin("go")).unwrap();
    assert_eq!(entry.commit.as_str(), commit);
    assert_eq!(entry.git.as_str(), source.url());
    assert_eq!(entry.rev.to_string(), "v1.0.0");
    assert_eq!(entry.sha256, tree_digest(&plugin("go"), &store).unwrap());
}

#[test]
fn a_second_fetch_changes_nothing() {
    let source = Source::new();
    let commit = source.head();
    let fixture = workspace(&source, "v1.0.0");
    let sources = named("go", &source, "v1.0.0");
    run(&fixture, &sources).unwrap();
    let bytes = fs::read(lock_file(&fixture)).unwrap();
    let modified = fs::metadata(lock_file(&fixture))
        .unwrap()
        .modified()
        .unwrap();

    let fetched = run(&fixture, &sources).unwrap();

    assert_eq!(
        reported(&fetched),
        [("go".to_owned(), commit, FetchStatus::Unchanged)]
    );
    assert_eq!(fs::read(lock_file(&fixture)).unwrap(), bytes);
    assert_eq!(
        fs::metadata(lock_file(&fixture))
            .unwrap()
            .modified()
            .unwrap(),
        modified
    );
}

#[test]
fn a_fetch_that_has_nothing_to_fetch_needs_no_git() {
    let source = Source::new();
    let fixture = workspace(&source, "v1.0.0");
    let sources = named("go", &source, "v1.0.0");
    run(&fixture, &sources).unwrap();

    let no_git = SearchPath::new([PathBuf::from("/no/such/directory")]);
    let fetched = fetch(fixture.root(), &fixture.state(), &sources, &no_git).unwrap();

    assert_eq!(fetched.iter().count(), 1);
}

#[test]
fn a_fetch_without_git_is_a_fetch_error() {
    let source = Source::new();
    let fixture = workspace(&source, "v1.0.0");

    let no_git = SearchPath::new([PathBuf::from("/no/such/directory")]);
    let error = fetch(
        fixture.root(),
        &fixture.state(),
        &named("go", &source, "v1.0.0"),
        &no_git,
    )
    .unwrap_err();

    let Error::PluginFetch { plugin, .. } = error else {
        unreachable!("no git is a fetch error, got {error:?}");
    };
    assert_eq!(plugin.to_string(), "go");
    assert!(!lock_file(&fixture).exists());
}

#[test]
fn a_moved_tag_is_refused_as_changed() {
    let source = Source::new();
    let locked = source.head();
    let fixture = workspace(&source, "v1.0.0");
    let sources = named("go", &source, "v1.0.0");
    run(&fixture, &sources).unwrap();
    let bytes = fs::read(lock_file(&fixture)).unwrap();
    // The store is not there, as in a fresh checkout of a workspace whose lock is committed,
    // so the tag is read from the repository again.
    fs::remove_dir_all(fixture.state().plugins()).unwrap();
    source.commit("later.txt", "later");
    source.git(&["tag", "-f", "v1.0.0"]);

    let error = run(&fixture, &sources).unwrap_err();

    let Error::PluginChanged { plugin, reason } = error else {
        unreachable!("a moved tag is a change, got {error:?}");
    };
    assert_eq!(plugin.to_string(), "go");
    assert!(reason.contains(&locked), "{reason}");
    assert_eq!(fs::read(lock_file(&fixture)).unwrap(), bytes);
    assert!(
        names(&fixture.state().plugins()).is_empty(),
        "nothing is left in the store"
    );
}

#[test]
fn a_changed_rev_refetches_and_prunes_the_old_store() {
    let source = Source::new();
    let first = source.head();
    let fixture = workspace(&source, "v1.0.0");
    run(&fixture, &named("go", &source, "v1.0.0")).unwrap();
    let second = source.commit("later.txt", "later");
    source.git(&["tag", "v2.0.0"]);

    let fetched = run(&fixture, &named("go", &source, "v2.0.0")).unwrap();

    assert_eq!(
        reported(&fetched),
        [("go".to_owned(), second.clone(), FetchStatus::Fetched)]
    );
    let go = fixture.state().plugins().join("go");
    assert_eq!(names(&go), [second.as_str()], "the first commit is pruned");
    assert_ne!(first, second);
    let lock = Lock::read(fixture.root()).unwrap();
    let entry = lock.get(&plugin("go")).unwrap();
    assert_eq!(entry.commit.as_str(), second);
    assert_eq!(entry.rev.to_string(), "v2.0.0");
}

#[test]
fn a_plugin_the_workspace_no_longer_names_is_dropped_from_lock_and_store() {
    let source = Source::new();
    let fixture = workspace(&source, "v1.0.0");
    run(&fixture, &named("go", &source, "v1.0.0")).unwrap();

    let fetched = run(&fixture, &BTreeMap::new()).unwrap();

    assert_eq!(fetched.iter().count(), 0);
    assert_eq!(Lock::read(fixture.root()).unwrap(), Lock::default());
    assert!(
        names(&fixture.state().plugins()).is_empty(),
        "the store is empty"
    );
}

#[test]
fn a_path_plugin_is_not_fetched() {
    let fixture = Fixture::copy_of("chain", &["greet.sh"]);
    let sources = BTreeMap::from([(
        plugin("local"),
        PluginSource::Path(buildl::buildl_core::Directory::parse("tools/local").unwrap()),
    )]);

    let fetched = run(&fixture, &sources).unwrap();

    assert_eq!(fetched.iter().count(), 0);
    assert!(!lock_file(&fixture).exists());
}

#[test]
fn a_damaged_store_is_restored_without_touching_the_lock() {
    let source = Source::new();
    let commit = source.head();
    let fixture = workspace(&source, "v1.0.0");
    let sources = named("go", &source, "v1.0.0");
    run(&fixture, &sources).unwrap();
    let bytes = fs::read(lock_file(&fixture)).unwrap();
    let store = fixture.state().plugins().join("go").join(&commit);
    fs::write(store.join("lib/go.lua"), "return { hacked = true }\n").unwrap();

    let fetched = run(&fixture, &sources).unwrap();

    assert_eq!(
        reported(&fetched),
        [("go".to_owned(), commit, FetchStatus::Fetched)]
    );
    assert_eq!(
        fs::read_to_string(store.join("lib/go.lua")).unwrap(),
        "return {}\n"
    );
    assert_eq!(fs::read(lock_file(&fixture)).unwrap(), bytes);
}

#[test]
fn a_link_in_the_plugin_is_refused() {
    let source = Source::new();
    symlink("/etc/hosts", source.path.join("hosts")).unwrap();
    source.git(&["add", "hosts"]);
    source.git(&["commit", "-q", "-m", "link"]);
    source.git(&["tag", "v1.1.0"]);
    let fixture = workspace(&source, "v1.1.0");

    let error = run(&fixture, &named("go", &source, "v1.1.0")).unwrap_err();

    let Error::PluginTree { plugin, path } = error else {
        unreachable!("a link in a plugin is a tree error, got {error:?}");
    };
    assert_eq!(plugin.to_string(), "go");
    assert!(path.ends_with("hosts"), "{}", path.display());
    assert!(!lock_file(&fixture).exists());
    assert!(
        names(&fixture.state().plugins()).is_empty(),
        "nothing is left in the store"
    );
}

#[test]
fn the_lock_is_canonical_json() {
    let go = Source::new();
    let zed = Source::new();
    let fixture = workspace(&go, "v1.0.0");
    let mut sources = named("zed", &zed, "v1.0.0");
    sources.extend(named("go", &go, "v1.0.0"));

    run(&fixture, &sources).unwrap();

    let bytes = fs::read(lock_file(&fixture)).unwrap();
    let lock = Lock::read(fixture.root()).unwrap();
    assert_eq!(bytes, canonical::to_vec(&lock).unwrap());
    let text = String::from_utf8(bytes).unwrap();
    assert!(
        text.find("\"go\"").unwrap() < text.find("\"zed\"").unwrap(),
        "{text}"
    );
    let keys = ["\"commit\"", "\"dir\"", "\"git\"", "\"rev\"", "\"sha256\""];
    let at: Vec<usize> = keys.iter().map(|key| text.find(key).unwrap()).collect();
    assert!(at.windows(2).all(|pair| pair[0] < pair[1]), "{text}");
    assert!(text.contains(r#""dir":"""#), "{text}");
    assert!(!text.contains(' ') && !text.contains('\n'), "{text}");
}

#[test]
fn after_fetch_check_finds_the_plugin() {
    let source = Source::new();
    let commit = source.head();
    let fixture = workspace(&source, "v1.0.0");
    let sources = named("go", &source, "v1.0.0");
    run(&fixture, &sources).unwrap();

    let lock = Lock::read(fixture.root()).unwrap();
    let checked = check(fixture.root(), &fixture.state(), &sources, &lock).unwrap();

    assert_eq!(
        checked.get(&plugin("go")).unwrap(),
        fixture.state().plugins().join("go").join(commit)
    );
    fixture.open().check().unwrap();
}

#[test]
fn a_plugin_pinned_by_commit_is_fetched_and_locked() {
    let source = Source::new();
    let pinned = source.head();
    source.commit("later.txt", "later");
    let fixture = workspace(&source, &pinned);
    let sources = named("go", &source, &pinned);

    let fetched = run(&fixture, &sources).unwrap();

    assert_eq!(
        reported(&fetched),
        [("go".to_owned(), pinned.clone(), FetchStatus::Fetched)]
    );
    let store = fixture.state().plugins().join("go").join(&pinned);
    assert_eq!(names(&store), ["extension.toml", "lib"]);
    let lock = Lock::read(fixture.root()).unwrap();
    let entry = lock.get(&plugin("go")).unwrap();
    assert_eq!(entry.commit.as_str(), pinned);
    assert_eq!(entry.rev.to_string(), pinned);
    let again = run(&fixture, &sources).unwrap();
    assert_eq!(
        reported(&again),
        [("go".to_owned(), pinned, FetchStatus::Unchanged)]
    );
}

#[test]
fn a_leftover_staging_directory_is_not_part_of_the_plugin() {
    let source = Source::new();
    let commit = source.head();
    let fixture = workspace(&source, "v1.0.0");
    let leftover = fixture.state().plugins().join("fetching~go");
    fs::create_dir_all(&leftover).unwrap();
    fs::write(leftover.join("stray"), "left by an interrupted fetch").unwrap();

    run(&fixture, &named("go", &source, "v1.0.0")).unwrap();

    let store = fixture.state().plugins().join("go").join(&commit);
    assert_eq!(names(&store), ["extension.toml", "lib"]);
    assert_eq!(names(&fixture.state().plugins()), ["go"]);
}

#[test]
fn a_plugin_that_holds_no_file_is_refused_naming_it() {
    let source = Source::new();
    source.git(&["rm", "-q", "-r", "lib", "extension.toml"]);
    source.git(&["commit", "-q", "--allow-empty", "-m", "empty"]);
    source.git(&["tag", "v2.0.0"]);
    let fixture = workspace(&source, "v2.0.0");

    let error = run(&fixture, &named("go", &source, "v2.0.0")).unwrap_err();

    let Error::PluginNotFetched { plugin, reason } = error else {
        unreachable!("a plugin with no file is not fetched, got {error:?}");
    };
    assert_eq!(plugin.to_string(), "go");
    assert!(reason.contains("v2.0.0"), "{reason}");
    assert!(reason.contains("no file"), "{reason}");
    assert!(!lock_file(&fixture).exists());
    assert!(
        names(&fixture.state().plugins()).is_empty(),
        "nothing is left in the store"
    );
}

#[test]
fn a_plugin_with_the_longest_name_is_fetched() {
    let source = Source::new();
    let name = "a".repeat(246);
    let fixture = workspace(&source, "v1.0.0");

    let fetched = run(&fixture, &named(&name, &source, "v1.0.0")).unwrap();

    assert_eq!(fetched.iter().count(), 1);
    assert_eq!(names(&fixture.state().plugins()), [name]);
}

#[test]
fn a_subfolder_plugin_is_stored_without_the_rest_of_the_repository() {
    let source = Source::in_folders(&["plugins/go"]);
    let commit = source.head();
    let fixture = workspace(&source, "v1.0.0");

    let fetched = run(&fixture, &named_in("go", &source, "v1.0.0", "plugins/go")).unwrap();

    assert_eq!(
        reported(&fetched),
        [("go".to_owned(), commit.clone(), FetchStatus::Fetched)]
    );
    let store = fixture.state().plugins().join("go").join(&commit);
    assert_eq!(names(&store), ["extension.toml", "lib"]);
    assert_eq!(names(&store.join("lib")), ["go.lua"]);
    assert_eq!(names(&fixture.state().plugins()), ["go"]);
    assert_eq!(
        names(&fixture.state().plugins().join("go")),
        [commit.as_str()]
    );
}

#[test]
fn a_subfolder_plugin_is_digested_alone() {
    let source = Source::in_folders(&["plugins/go"]);
    let commit = source.head();
    let fixture = workspace(&source, "v1.0.0");

    run(&fixture, &named_in("go", &source, "v1.0.0", "plugins/go")).unwrap();

    let store = fixture.state().plugins().join("go").join(&commit);
    let lock = Lock::read(fixture.root()).unwrap();
    let entry = lock.get(&plugin("go")).unwrap();
    assert_eq!(entry.dir.as_str(), "plugins/go");
    assert_eq!(entry.sha256, tree_digest(&plugin("go"), &store).unwrap());
}

#[test]
fn a_missing_folder_is_refused_naming_it() {
    let source = Source::in_folders(&["plugins/go"]);
    let commit = source.head();
    let fixture = workspace(&source, "v1.0.0");

    let error = run(&fixture, &named_in("go", &source, "v1.0.0", "nope")).unwrap_err();

    let Error::PluginNotFetched { plugin, reason } = error else {
        unreachable!("a missing folder is not fetched, got {error:?}");
    };
    assert_eq!(plugin.to_string(), "go");
    assert!(reason.contains("v1.0.0"), "{reason}");
    assert!(reason.contains(&commit), "{reason}");
    assert!(reason.contains("has no folder nope"), "{reason}");
    assert!(!lock_file(&fixture).exists());
    assert!(
        names(&fixture.state().plugins()).is_empty(),
        "nothing is left in the store"
    );
}

#[test]
fn a_folder_that_is_a_link_is_refused() {
    let source = Source::in_folders(&["real"]);
    source.commit_link("alias", Path::new("real"));
    let fixture = workspace(&source, "v1.1.0");

    let error = run(&fixture, &named_in("go", &source, "v1.1.0", "alias")).unwrap_err();

    let Error::PluginNotFetched { plugin, reason } = error else {
        unreachable!("a link is not a folder, got {error:?}");
    };
    assert_eq!(plugin.to_string(), "go");
    assert!(reason.contains("has no folder alias"), "{reason}");
    assert!(!lock_file(&fixture).exists());
    assert!(
        names(&fixture.state().plugins()).is_empty(),
        "nothing is left in the store"
    );
}

#[test]
fn a_folder_under_a_linked_parent_is_refused() {
    let outside = tempfile::tempdir().unwrap();
    let elsewhere = fs::canonicalize(outside.path()).unwrap();
    fs::create_dir(elsewhere.join("c")).unwrap();
    fs::write(
        elsewhere.join("c/extension.toml"),
        "[extension]\nname = \"go\"\n",
    )
    .unwrap();
    let source = Source::new();
    source.commit_link("plugins", &elsewhere);
    let fixture = workspace(&source, "v1.1.0");

    let error = run(&fixture, &named_in("go", &source, "v1.1.0", "plugins/c")).unwrap_err();

    let Error::PluginNotFetched { reason, .. } = error else {
        unreachable!("a folder behind a link is not fetched, got {error:?}");
    };
    assert!(reason.contains("has no folder plugins/c"), "{reason}");
    assert_eq!(
        names(&elsewhere.join("c")),
        ["extension.toml"],
        "nothing outside the staging folder is moved"
    );
    assert!(!lock_file(&fixture).exists());
    assert!(
        names(&fixture.state().plugins()).is_empty(),
        "nothing is left in the store"
    );
}

#[test]
fn a_changed_dir_refetches() {
    let source = Source::in_folders(&["a", "b"]);
    let commit = source.head();
    let fixture = workspace(&source, "v1.0.0");
    run(&fixture, &named_in("go", &source, "v1.0.0", "a")).unwrap();
    let first = Lock::read(fixture.root()).unwrap();
    assert_eq!(first.get(&plugin("go")).unwrap().dir.as_str(), "a");
    let first_sha256 = first.get(&plugin("go")).unwrap().sha256;

    let fetched = run(&fixture, &named_in("go", &source, "v1.0.0", "b")).unwrap();

    assert_eq!(
        reported(&fetched),
        [("go".to_owned(), commit.clone(), FetchStatus::Fetched)]
    );
    let lock = Lock::read(fixture.root()).unwrap();
    let entry = lock.get(&plugin("go")).unwrap();
    assert_eq!(entry.dir.as_str(), "b", "the row holds the new folder");
    let store = fixture.state().plugins().join("go").join(&commit);
    assert_eq!(names(&store), ["extension.toml", "lib"]);
    assert_eq!(
        fs::read_to_string(store.join("lib/go.lua")).unwrap(),
        "return \"b\"\n",
        "the files stored are those of the new folder"
    );
    assert_eq!(entry.sha256, tree_digest(&plugin("go"), &store).unwrap());
    assert_ne!(
        entry.sha256, first_sha256,
        "the digest is of the new folder"
    );
}
