//! The strategy that runs an action as a plain process in the workspace root.
//!
//! Its own file because it is where the three steps of running an action meet at the first tier:
//! clearing the paths the action writes, running its command, and reporting what the command
//! left. Each step is short here; waiting for a process, walking an output, and the parts every
//! tier shares have files of their own.
//!
//! Responsibilities: [`Bare`].

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use buildl_core::{
    ContentStore, ExecStrategy, Found, LiveSink, RawOutcome, ReadyAction, Result, Timeout,
};

use crate::exec::capture::capture;
use crate::exec::outcome::{ended, fault};
use crate::exec::run::{Started, collect_in, start};
use crate::search_path::SearchPath;
use crate::state::clear_path;

/// Tier 1: a plain process in the workspace root.
///
/// The command is started as an argument vector, never through a shell, with an empty
/// standard input; its program is the file the search path it was handed leads to; and its
/// environment holds exactly the variables of the action, with nothing of this process's own,
/// not even `PATH`. A command that runs longer than the timeout is stopped; what it started is
/// not. Nothing is confined: the process can read and write whatever the user can.
#[derive(Debug)]
pub struct Bare {
    root: PathBuf,
    timeout: Duration,
    search: SearchPath,
}

impl Bare {
    /// `root` is the absolute workspace root; `timeout` the longest one action may run; and
    /// `search` the search path the programs of a plan were fingerprinted through, so that the
    /// file run is the file fingerprinted.
    #[must_use]
    pub const fn new(root: PathBuf, timeout: Timeout, search: SearchPath) -> Self {
        Self {
            root,
            timeout: timeout.as_duration(),
            search,
        }
    }
}

impl ExecStrategy for Bare {
    /// Nothing: an action runs in the workspace root, and its command already names its paths
    /// relative to it, so no directory travels from `prepare` to `run`.
    type Dir = ();

    fn prepare(&self, action: &ReadyAction) -> Result<()> {
        for output in &action.outputs {
            let path = self.root.join(output.as_str());
            if let Some(directory) = path.parent() {
                fs::create_dir_all(directory).map_err(|error| {
                    fault(
                        &action.label,
                        &format!("making the directory of {output}"),
                        error,
                    )
                })?;
            }
            // Unlinked, never written through: what an earlier build left at the path is a
            // link to a file the content store still holds.
            clear_path(&path)
                .map_err(|error| fault(&action.label, &format!("clearing {output}"), error))?;
        }
        Ok(())
    }

    fn run(&self, (): &(), action: &ReadyAction, live: Option<LiveSink<'_>>) -> Result<RawOutcome> {
        let Started {
            mut command,
            program,
            ..
        } = start(action, &self.search, &self.root)?;
        let captured = capture(&mut command, self.timeout, live).map_err(|error| {
            fault(
                &action.label,
                &format!("running {}", program.display()),
                error,
            )
        })?;
        Ok(RawOutcome {
            exit: ended(&captured),
            stdout: captured.stdout,
            stderr: captured.stderr,
        })
    }

    fn collect<C: ContentStore>(
        &self,
        (): &(),
        action: &ReadyAction,
        cas: &C,
    ) -> Result<Vec<Found>> {
        collect_in(&self.root, action, cas)
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeMap;
    use std::fs::{self, Permissions};
    use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    use buildl_core::{
        Argument, Channel, Command, Digest, Directory, EnvName, EnvValue, Error, ExecStrategy,
        Exit, FileMode, Found, FoundEntry, FoundKind, Label, NetworkAccess, OutputName, OutputPath,
        OutputSource, PortName, RawOutcome, ReadyAction, StoredFile, TargetName, Timeout,
    };

    use super::Bare;
    use crate::exec::memory_store::MemoryStore;
    use crate::search_path::SearchPath;

    /// A workspace root in a directory of its own, by its canonical path. The directory above
    /// it is the test's too, for what has to lie outside the workspace.
    fn workspace() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap().join("ws");
        fs::create_dir(&root).unwrap();
        (dir, root)
    }

    /// Where the system keeps the tools these tests run.
    fn system() -> SearchPath {
        SearchPath::new(["/bin", "/usr/bin"].map(PathBuf::from))
    }

    /// Longer than any command of these tests runs when nothing is wrong.
    fn patient() -> Timeout {
        Timeout::parse("60s").unwrap()
    }

    fn bare(root: &Path) -> Bare {
        Bare::new(root.to_path_buf(), patient(), system())
    }

    /// The action of `//lib:probe` that runs `arguments`, with no variable and no output.
    fn action(arguments: &[&str]) -> ReadyAction {
        let arguments = arguments
            .iter()
            .map(|argument| Argument::parse(*argument).unwrap())
            .collect();
        ReadyAction {
            label: Label::parse("//lib:probe").unwrap(),
            command: Command::new(arguments).unwrap(),
            description: None,
            env: BTreeMap::new(),
            inputs: Vec::new(),
            deps: Vec::new(),
            outputs: Vec::new(),
            network: NetworkAccess::Sealed,
            output_source: OutputSource::Command,
        }
    }

    /// The action that runs `script` through the system's `sh`. A tool the script names is
    /// written by its path.
    fn sh(script: &str) -> ReadyAction {
        action(&["sh", "-c", script])
    }

    /// The path `out/<directory>/<name>`.
    fn output(directory: &str, name: &str) -> OutputPath {
        let label = Label::new(
            Directory::parse(directory).unwrap(),
            TargetName::parse("probe").unwrap(),
        );
        OutputPath::of(
            &Directory::parse("out").unwrap(),
            &label,
            &OutputName::parse(name).unwrap(),
        )
    }

    fn plain(bytes: &[u8]) -> StoredFile {
        StoredFile {
            digest: Digest::of(bytes),
            mode: FileMode::Plain,
        }
    }

    #[test]
    fn prepare_makes_the_directory_and_leaves_nothing_at_the_path() {
        let (_dir, root) = workspace();
        let outside = root.parent().unwrap().join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("keep"), "untouched").unwrap();
        // What an earlier build left: a file that is a link to the store's read-only blob, a
        // tree, and a link to a directory outside the workspace.
        let blob = outside.join("blob");
        fs::write(&blob, "held by the store").unwrap();
        fs::set_permissions(&blob, Permissions::from_mode(0o444)).unwrap();
        fs::create_dir_all(root.join("out/lib/tree/deep")).unwrap();
        fs::write(root.join("out/lib/tree/deep/old"), "old").unwrap();
        fs::hard_link(&blob, root.join("out/lib/file")).unwrap();
        symlink(&outside, root.join("out/lib/link")).unwrap();
        let mut action = sh("true");
        action.outputs = vec![
            output("lib", "file"),
            output("lib", "tree"),
            output("lib", "link"),
            output("fresh/deep", "made"),
        ];

        bare(&root).prepare(&action).unwrap();

        for output in &action.outputs {
            assert!(
                fs::symlink_metadata(root.join(output.as_str())).is_err(),
                "{output} still holds something"
            );
        }
        assert!(root.join("out/fresh/deep").is_dir());
        // The blob was unlinked from, never written through, and nothing was followed.
        let held = fs::metadata(&blob).unwrap();
        assert_eq!(fs::read(&blob).unwrap(), b"held by the store");
        assert_eq!(held.nlink(), 1);
        assert_eq!(held.permissions().mode() & 0o777, 0o444);
        assert_eq!(fs::read(outside.join("keep")).unwrap(), b"untouched");
    }

    #[test]
    fn an_output_whose_directory_cannot_be_made_fails_the_prepare() {
        let (_dir, root) = workspace();
        // A file stands where the directory of the output has to be.
        fs::create_dir(root.join("out")).unwrap();
        fs::write(root.join("out/lib"), "in the way").unwrap();
        let mut action = sh("true");
        action.outputs = vec![output("lib", "file")];

        let failed = bare(&root).prepare(&action).unwrap_err();

        assert!(matches!(
            failed,
            Error::Adapter {
                port: PortName::ExecStrategy,
                ..
            }
        ));
        let said = failed.to_string();
        assert!(
            said.starts_with("exec strategy: //lib:probe: making the directory of out/lib/file: "),
            "{said}"
        );
    }

    #[test]
    fn the_command_runs_in_the_workspace_root() {
        let (_dir, root) = workspace();

        let raw = bare(&root).run(&(), &action(&["pwd"]), None).unwrap();

        assert_eq!(raw.exit, Exit::Exited(0));
        assert_eq!(
            String::from_utf8(raw.stdout).unwrap(),
            format!("{}\n", root.display())
        );
    }

    #[test]
    fn an_exit_status_is_reported_as_it_is() {
        let (_dir, root) = workspace();

        let raw = bare(&root)
            .run(&(), &sh("echo out; echo err >&2; exit 3"), None)
            .unwrap();

        assert_eq!(
            raw,
            RawOutcome {
                exit: Exit::Exited(3),
                stdout: b"out\n".to_vec(),
                stderr: b"err\n".to_vec(),
            }
        );
    }

    #[test]
    fn an_action_killed_by_a_signal_is_reported_signaled() {
        let (_dir, root) = workspace();

        let raw = bare(&root).run(&(), &sh("kill -SEGV $$"), None).unwrap();

        assert_eq!(raw.exit, Exit::Signaled(11));
    }

    #[test]
    fn a_command_that_cannot_be_started_is_an_adapter_error() {
        let (_dir, root) = workspace();
        // No process can be started in a directory that does not exist.
        let nowhere = bare(&root.join("gone"));

        let failed = nowhere.run(&(), &sh("true"), None).unwrap_err();

        assert!(matches!(
            failed,
            Error::Adapter {
                port: PortName::ExecStrategy,
                ..
            }
        ));
        let said = failed.to_string();
        assert!(
            said.starts_with("exec strategy: //lib:probe: running /bin/sh: "),
            "{said}"
        );
    }

    #[test]
    fn collect_reports_one_entry_per_output_in_order() {
        let (_dir, root) = workspace();
        let store = MemoryStore::new();
        fs::create_dir_all(root.join("out/lib/tree")).unwrap();
        fs::write(root.join("out/lib/second"), "2").unwrap();
        fs::write(root.join("out/lib/tree/inside"), "3").unwrap();
        let mut action = sh("true");
        action.outputs = vec![
            output("lib", "first"),
            output("lib", "second"),
            output("lib", "tree"),
        ];

        let found = bare(&root).collect(&(), &action, &store).unwrap();

        assert_eq!(
            found,
            [
                Found::Nothing,
                Found::File(plain(b"2")),
                Found::Tree(vec![FoundEntry {
                    path: "inside".to_owned(),
                    kind: FoundKind::File(plain(b"3")),
                }]),
            ]
        );
    }

    #[test]
    fn a_store_that_fails_fails_the_collect_with_its_own_error() {
        let (_dir, root) = workspace();
        fs::create_dir_all(root.join("out/lib")).unwrap();
        fs::write(root.join("out/lib/file"), "x").unwrap();
        let mut action = sh("true");
        action.outputs = vec![output("lib", "file")];

        let refused = bare(&root)
            .collect(&(), &action, &MemoryStore::broken())
            .unwrap_err();

        // The store's own error, not one this strategy made.
        assert_eq!(refused.to_string(), "content store: the store is broken");
    }

    #[test]
    fn the_located_file_is_the_one_run() {
        let (_dir, root) = workspace();
        // The same name in two directories of the search path. Each is the system's shell under
        // another name, and a shell handed only a script says the name it was started by.
        let first = root.parent().unwrap().join("first");
        let second = root.parent().unwrap().join("second");
        for directory in [&first, &second] {
            fs::create_dir(directory).unwrap();
            symlink("/bin/sh", directory.join("tool")).unwrap();
        }
        let search = SearchPath::new([first.clone(), second]);

        let raw = Bare::new(root, patient(), search)
            .run(&(), &action(&["tool", "-c", "echo $0"]), None)
            .unwrap();

        assert_eq!(raw.exit, Exit::Exited(0));
        assert_eq!(
            String::from_utf8(raw.stdout).unwrap(),
            format!("{}\n", first.join("tool").display())
        );
    }

    #[test]
    fn a_program_that_is_not_located_is_an_adapter_error() {
        let (_dir, root) = workspace();
        let nowhere_to_look = Bare::new(root, patient(), SearchPath::new([]));

        let failed = nowhere_to_look.run(&(), &sh("true"), None).unwrap_err();

        assert!(matches!(
            failed,
            Error::Adapter {
                port: PortName::ExecStrategy,
                ..
            }
        ));
        assert_eq!(
            failed.to_string(),
            "exec strategy: //lib:probe: starting the command: the program sh was not found"
        );
    }

    #[test]
    fn the_child_sees_only_the_declared_variables() {
        let (_dir, root) = workspace();
        let mut declared = action(&["env"]);
        declared.env.insert(
            EnvName::parse("FOO").unwrap(),
            EnvValue::parse("bar").unwrap(),
        );

        let raw = bare(&root).run(&(), &declared, None).unwrap();

        // `env` prints every variable it was started with: the one the action declares, and
        // none of this process's own.
        assert_eq!(raw.exit, Exit::Exited(0));
        assert_eq!(String::from_utf8(raw.stdout).unwrap(), "FOO=bar\n");
    }

    #[test]
    fn an_action_past_its_timeout_is_stopped_and_reported_timed_out() {
        let (_dir, root) = workspace();
        let impatient = Bare::new(root, Timeout::parse("300ms").unwrap(), system());
        let started = Instant::now();

        let raw = impatient.run(&(), &sh("exec /bin/sleep 30"), None).unwrap();

        assert_eq!(raw.exit, Exit::TimedOut);
        // Stopped at the timeout, not when the command would have ended by itself.
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn a_live_sink_is_handed_each_chunk_before_the_command_ends() {
        let (_dir, root) = workspace();
        let heard = root.join("heard");
        let seen = Mutex::new(Vec::new());
        // The command writes a line, then waits for the file `heard` before it writes more.
        // The sink makes that file when it is handed something, so the command can only end
        // when its first line reached the sink while it was still running.
        let live = |channel: Channel, bytes: &[u8]| {
            seen.lock().unwrap().push((channel, bytes.to_vec()));
            fs::write(&heard, "").unwrap();
        };
        let waits = sh(
            "echo one; while [ ! -e heard ]; do /bin/sleep 0.05; done; echo two; echo three >&2",
        );
        let strategy = Bare::new(root, Timeout::parse("20s").unwrap(), system());

        let raw = strategy.run(&(), &waits, Some(&live)).unwrap();

        assert_eq!(raw.exit, Exit::Exited(0));
        assert_eq!(raw.stdout, b"one\ntwo\n");
        assert_eq!(raw.stderr, b"three\n");
        let seen = seen.into_inner().unwrap();
        assert_eq!(seen.first(), Some(&(Channel::Stdout, b"one\n".to_vec())));
        // For each output, what the sink was handed, joined in order, is what the result holds.
        let joined = |wanted: Channel| -> Vec<u8> {
            seen.iter()
                .filter(|(channel, _)| *channel == wanted)
                .flat_map(|(_, bytes)| bytes.iter().copied())
                .collect()
        };
        assert_eq!(joined(Channel::Stdout), raw.stdout);
        assert_eq!(joined(Channel::Stderr), raw.stderr);
    }
}
