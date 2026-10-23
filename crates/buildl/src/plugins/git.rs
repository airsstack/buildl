//! Running the system `git` to fetch one revision of a plugin's repository.
//!
//! Its own file because fetching is a short sequence of `git` processes whose only shared rule
//! is how one is started and how its failure is read: the engineer's own `git` is run, with
//! their configuration, and anything it prints to standard error becomes the diagnostic of the
//! error. `git` is found through the [`SearchPath`] the caller holds, as a tool of a build is.
//!
//! Responsibilities: [`Git`] and its five operations.
//!
//! Non-responsibilities: where the checkout is kept, what is done with its files, and the lock.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use buildl_core::{CommitId, Diagnostic, Error, GitUrl, PluginName, Program, Result, Rev};

use crate::search_path::SearchPath;

/// The name `git` is looked for by.
const GIT: &str = "git";

/// The environment variables `git` reads to find the repository it acts on: those
/// `git rev-parse --local-env-vars` prints. They are removed from every `git` that is run, so
/// it acts on the repository in the directory it is given, whatever process started buildl.
const REPOSITORY_ENV: [&str; 16] = [
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_CONFIG",
    "GIT_CONFIG_PARAMETERS",
    "GIT_CONFIG_COUNT",
    "GIT_OBJECT_DIRECTORY",
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_IMPLICIT_WORK_TREE",
    "GIT_GRAFT_FILE",
    "GIT_INDEX_FILE",
    "GIT_NO_REPLACE_OBJECTS",
    "GIT_REPLACE_REF_BASE",
    "GIT_PREFIX",
    "GIT_INTERNAL_SUPER_PREFIX",
    "GIT_SHALLOW_FILE",
    "GIT_COMMON_DIR",
];

/// The system `git`, run to fetch one plugin.
///
/// A value belongs to one plugin, which every error it returns names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Git {
    program: PathBuf,
    plugin: PluginName,
}

impl Git {
    /// The `git` that `search` leads to, for fetching `plugin`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PluginFetch`] when no executable file named `git` is on `search`.
    pub fn locate(plugin: &PluginName, search: &SearchPath) -> Result<Self> {
        let name = Program::parse(GIT)?;
        // A bare name is searched for, so the base directory is never consulted.
        search
            .locate(&name, Path::new("/"))
            .map(|program| Self {
                program,
                plugin: plugin.clone(),
            })
            .ok_or_else(|| Error::PluginFetch {
                plugin: plugin.clone(),
                diagnostic: Diagnostic::new("git was not found on PATH"),
            })
    }

    /// Makes `dir`, which must exist, an empty repository.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PluginFetch`] when `git` cannot be started or exits unsuccessfully.
    pub fn init(&self, dir: &Path) -> Result<()> {
        self.run(dir, &["init", "-q"]).map(drop)
    }

    /// Fetches `rev` of the repository at `url` into the repository in `dir`, at depth 1, and
    /// leaves it as `FETCH_HEAD`.
    ///
    /// A tag is fetched as `refs/tags/<tag>`, so a branch of that name is never taken for it;
    /// a commit is fetched by its id.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PluginFetch`] when `git` cannot be started or exits unsuccessfully,
    /// carrying what it printed: the repository is unreachable, the tag is not there, or the
    /// host does not serve a commit that no ref names.
    pub fn fetch_rev(&self, dir: &Path, url: &GitUrl, rev: &Rev) -> Result<()> {
        let refspec = match rev {
            Rev::Tag(tag) => format!("refs/tags/{tag}"),
            Rev::Commit(commit) => commit.to_string(),
        };
        self.run(
            dir,
            &[
                "fetch",
                "-q",
                "--depth",
                "1",
                "--end-of-options",
                url.as_str(),
                &refspec,
            ],
        )
        .map(drop)
    }

    /// Checks out `FETCH_HEAD` in `dir`, detached.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PluginFetch`] when `git` cannot be started or exits unsuccessfully.
    pub fn checkout_fetched(&self, dir: &Path) -> Result<()> {
        self.run(
            dir,
            &[
                "-c",
                "advice.detachedHead=false",
                "checkout",
                "-q",
                "FETCH_HEAD",
            ],
        )
        .map(drop)
    }

    /// The commit checked out in `dir`; for an annotated tag, the commit it points at.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PluginFetch`] when `git` cannot be started, exits unsuccessfully or
    /// prints something that is not a full commit id.
    pub fn head_commit(&self, dir: &Path) -> Result<CommitId> {
        let printed = self.run(dir, &["rev-parse", "HEAD"])?;
        CommitId::parse(printed.trim()).map_err(|_| {
            self.refused(format!(
                "git rev-parse HEAD printed {:?}, which is not a commit id",
                printed.trim()
            ))
        })
    }

    /// Runs `git args` in `dir` and returns what it printed to standard output.
    ///
    /// The engineer's `git` configuration applies, but the variables that point `git` at a
    /// repository do not. The messages are fixed to the C locale so the diagnostic does not
    /// vary with it. `GIT_TERMINAL_PROMPT=0` makes `git` fail rather than ask for a credential
    /// on the terminal, and standard input is closed. A program `git` starts that opens the
    /// terminal itself, such as `ssh` asking for a passphrase, may still ask.
    fn run(&self, dir: &Path, args: &[&str]) -> Result<String> {
        let mut command = Command::new(&self.program);
        command.args(args).current_dir(dir);
        for name in REPOSITORY_ENV {
            command.env_remove(name);
        }
        let output = command
            .env("LC_ALL", "C")
            .env("GIT_TERMINAL_PROMPT", "0")
            .stdin(Stdio::null())
            .output()
            .map_err(|error| {
                self.refused(format!("could not run {}: {error}", self.program.display()))
            })?;
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
        }
        let said = String::from_utf8_lossy(&output.stderr);
        let said = said.trim();
        Err(self.refused(if said.is_empty() {
            format!("git {} exited with {}", args.join(" "), output.status)
        } else {
            said.to_owned()
        }))
    }

    /// The error for this plugin with `diagnostic` as its message.
    fn refused(&self, diagnostic: String) -> Error {
        Error::PluginFetch {
            plugin: self.plugin.clone(),
            diagnostic: Diagnostic::new(diagnostic),
        }
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        clippy::panic,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use buildl_core::{Error, GitUrl, PluginName, Rev};

    use super::Git;
    use crate::search_path::SearchPath;

    fn plugin() -> PluginName {
        PluginName::parse("go").unwrap()
    }

    /// The `git` of this machine. A machine with none fails the test that asks.
    fn git() -> Git {
        Git::locate(&plugin(), &SearchPath::from_process()).unwrap()
    }

    /// A repository, in a temporary directory, that tests build history in.
    struct Source {
        _dir: tempfile::TempDir,
        path: PathBuf,
    }

    impl Source {
        /// An empty repository whose branch is `main`.
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let path = fs::canonicalize(dir.path()).unwrap();
            let source = Self { _dir: dir, path };
            source.git(&["init", "-q", "-b", "main"]);
            source
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

        /// Commits a file `name` holding `content`, and returns the new commit.
        fn commit(&self, name: &str, content: &str) -> String {
            fs::write(self.path.join(name), content).unwrap();
            self.git(&["add", name]);
            self.git(&["commit", "-q", "-m", name]);
            self.git(&["rev-parse", "HEAD"])
        }

        fn url(&self) -> GitUrl {
            GitUrl::parse(format!("file://{}", self.path.display())).unwrap()
        }
    }

    /// An empty directory to fetch into.
    fn target() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn rev(raw: &str) -> Rev {
        Rev::parse(raw).unwrap()
    }

    /// What the fetch error `result` holds says, after checking it names the plugin.
    fn refusal<T: core::fmt::Debug>(result: buildl_core::Result<T>) -> String {
        match result.unwrap_err() {
            Error::PluginFetch {
                plugin: named,
                diagnostic,
            } => {
                assert_eq!(named, plugin());
                diagnostic.to_string()
            }
            other => panic!("not a fetch error: {other}"),
        }
    }

    #[test]
    fn fetching_a_tag_checks_out_its_commit() {
        let source = Source::new();
        source.commit("first.txt", "one");
        let tagged = source.commit("plugin.txt", "two");
        source.git(&["tag", "v1"]);
        source.commit("later.txt", "three");
        let (dir, git) = (target(), git());

        git.init(dir.path()).unwrap();
        git.fetch_rev(dir.path(), &source.url(), &rev("v1"))
            .unwrap();
        git.checkout_fetched(dir.path()).unwrap();

        assert_eq!(git.head_commit(dir.path()).unwrap().as_str(), tagged);
        assert_eq!(
            fs::read_to_string(dir.path().join("plugin.txt")).unwrap(),
            "two"
        );
        assert!(!dir.path().join("later.txt").exists());
    }

    #[test]
    fn fetching_a_commit_checks_it_out() {
        let source = Source::new();
        let wanted = source.commit("plugin.txt", "one");
        source.commit("later.txt", "two");
        let (dir, git) = (target(), git());

        git.init(dir.path()).unwrap();
        git.fetch_rev(dir.path(), &source.url(), &rev(&wanted))
            .unwrap();
        git.checkout_fetched(dir.path()).unwrap();

        assert_eq!(git.head_commit(dir.path()).unwrap().as_str(), wanted);
        assert!(dir.path().join("plugin.txt").is_file());
        assert!(!dir.path().join("later.txt").exists());
    }

    #[test]
    fn a_branch_name_is_not_found_as_a_tag() {
        let source = Source::new();
        source.commit("plugin.txt", "one");
        let (dir, git) = (target(), git());

        git.init(dir.path()).unwrap();
        let said = refusal(git.fetch_rev(dir.path(), &source.url(), &rev("main")));

        assert!(said.contains("refs/tags/main"), "{said}");
    }

    #[test]
    fn an_annotated_tag_resolves_to_its_commit() {
        let source = Source::new();
        let tagged = source.commit("plugin.txt", "one");
        source.git(&["tag", "-a", "v2", "-m", "release"]);
        let tag_object = source.git(&["rev-parse", "v2"]);
        assert_ne!(tag_object, tagged, "the tag must be an object of its own");
        let (dir, git) = (target(), git());

        git.init(dir.path()).unwrap();
        git.fetch_rev(dir.path(), &source.url(), &rev("v2"))
            .unwrap();
        git.checkout_fetched(dir.path()).unwrap();

        assert_eq!(git.head_commit(dir.path()).unwrap().as_str(), tagged);
    }

    /// A `Git` that runs `env`, so that `run` prints the environment `git` would be given.
    fn env_printer() -> Git {
        Git {
            program: PathBuf::from("/usr/bin/env"),
            plugin: plugin(),
        }
    }

    #[test]
    fn git_is_run_in_the_c_locale_and_told_not_to_prompt() {
        let dir = target();

        let printed = env_printer().run(dir.path(), &[]).unwrap();

        let lines: Vec<&str> = printed.lines().collect();
        assert!(lines.contains(&"LC_ALL=C"), "{printed}");
        assert!(lines.contains(&"GIT_TERMINAL_PROMPT=0"), "{printed}");
    }

    /// Set on the child process that runs `foreign_repository_in_the_environment_child`: the
    /// directory to fetch into, a newline, then the url of the repository to fetch from.
    const CHILD: &str = "BUILDL_TEST_FETCH_CHILD";

    /// The body of the child process of
    /// `a_repository_named_by_the_environment_is_not_the_one_fetched_into`. Run on its own, with
    /// `CHILD` unset, it does nothing.
    #[test]
    fn foreign_repository_in_the_environment_child() {
        let Ok(arguments) = std::env::var(CHILD) else {
            return;
        };
        let (dir, source) = arguments.split_once('\n').unwrap();
        let (dir, git) = (Path::new(dir), git());

        git.init(dir).unwrap();
        git.fetch_rev(dir, &GitUrl::parse(source).unwrap(), &rev("v1"))
            .unwrap();
        git.checkout_fetched(dir).unwrap();

        assert!(dir.join("plugin.txt").is_file());
    }

    #[test]
    fn a_repository_named_by_the_environment_is_not_the_one_fetched_into() {
        let source = Source::new();
        source.commit("plugin.txt", "one");
        source.git(&["tag", "v1"]);
        let foreign = Source::new();
        foreign.commit("mine.txt", "mine");
        let before = foreign.git(&["status", "--porcelain"]);
        let dir = target();
        // A process whose environment names the foreign repository, as a git hook's does.
        let child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "plugins::git::tests::foreign_repository_in_the_environment_child",
            ])
            .env(CHILD, format!("{}\n{}", dir.path().display(), source.url()))
            .env("GIT_DIR", foreign.path.join(".git"))
            .env("GIT_WORK_TREE", &foreign.path)
            .env("GIT_INDEX_FILE", foreign.path.join(".git/index"))
            .output()
            .unwrap();

        assert!(
            child.status.success(),
            "{}{}",
            String::from_utf8_lossy(&child.stdout),
            String::from_utf8_lossy(&child.stderr)
        );
        assert!(dir.path().join(".git").is_dir());
        assert!(!foreign.path.join("plugin.txt").exists());
        assert!(!foreign.path.join(".git/FETCH_HEAD").exists());
        assert_eq!(foreign.git(&["status", "--porcelain"]), before);
    }

    #[test]
    fn git_missing_from_path_is_a_fetch_error() {
        let empty = tempfile::tempdir().unwrap();
        let search = SearchPath::new([fs::canonicalize(empty.path()).unwrap()]);

        let said = refusal(Git::locate(&plugin(), &search));

        assert!(said.contains("git was not found on PATH"), "{said}");
    }

    #[test]
    fn an_unreachable_repository_carries_what_git_said() {
        let (dir, git) = (target(), git());
        let url = GitUrl::parse(format!("file://{}", dir.path().join("absent").display())).unwrap();

        git.init(dir.path()).unwrap();
        let said = refusal(git.fetch_rev(dir.path(), &url, &rev("v1")));

        assert!(said.contains("absent"), "{said}");
    }

    #[test]
    fn a_checkout_where_nothing_was_fetched_is_a_fetch_error() {
        let (dir, git) = (target(), git());

        git.init(dir.path()).unwrap();
        let said = refusal(git.checkout_fetched(dir.path()));

        assert!(said.contains("FETCH_HEAD"), "{said}");
    }
}
