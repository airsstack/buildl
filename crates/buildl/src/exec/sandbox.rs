//! The strategy that runs an action in a directory of its own holding only what it declared.
//!
//! Its own file because it is where the three steps of running an action meet at the second
//! tier: making a directory outside the workspace and copying the action's inputs
//! into it, running the command there, and reporting what the command left in it. Waiting for a
//! process, walking an output, and the parts every tier shares have files of their own.
//!
//! Responsibilities: [`InputSandbox`] and [`ExecDir`].
//!
//! Non-responsibilities: confining the process once it runs. It can still reach any file by an
//! absolute path; what it is spared is finding an undeclared workspace file by a relative one.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use buildl_core::{
    ContentStore, ExecStrategy, Found, Isolation, LiveSink, Program, RawOutcome, ReadyAction,
    Result, Timeout,
};

use crate::exec::capture::capture;
use crate::exec::outcome::{ended, fault};
use crate::exec::run::{Started, collect_in, start};
use crate::search_path::SearchPath;
use crate::state::{Owner, Scratch, clear_path};

/// What an [`InputSandbox`] does with the real output paths in the workspace before it runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutPaths {
    /// Unlinks whatever stands at each output path and makes the directory above it, so an
    /// action that fails or runs out of time leaves nothing at its output paths.
    Cleared,
    /// Leaves the workspace's output directory as it is: nothing is unlinked or made there.
    Untouched,
}

/// Tier 2: a plain process in a directory of its own, outside the workspace.
///
/// The directory holds a copy of each file the action declares as an input, of every file of
/// each output of a dependency, and of the program the command names by a relative path. The
/// copies keep their mode and share nothing with the workspace, so the command can neither find
/// another workspace file by a relative path nor change one. The command runs there, started
/// as it is at the first tier, with the variables of the action and the ones this tier pins.
/// The directory has the same path on every run of the same target, and is removed when the
/// [`ExecDir`] that names it is dropped. Whether the real output paths in the workspace are
/// cleared first is the [`OutPaths`] it was made with; nothing else differs between the two.
#[derive(Debug)]
pub struct InputSandbox {
    root: PathBuf,
    scratch: Scratch,
    owner: Owner,
    timeout: Duration,
    search: SearchPath,
    out_paths: OutPaths,
}

impl InputSandbox {
    /// `root` is the absolute workspace root; `scratch` the directory the action directories are
    /// made in, which `owner` must own; `timeout` the longest one action may run; and `search`
    /// the search path the programs of a plan were fingerprinted through. `out_paths` says
    /// whether the real output paths are cleared before the action runs or left alone.
    #[must_use]
    pub const fn new(
        root: PathBuf,
        scratch: Scratch,
        owner: Owner,
        timeout: Timeout,
        search: SearchPath,
        out_paths: OutPaths,
    ) -> Self {
        Self {
            root,
            scratch,
            owner,
            timeout: timeout.as_duration(),
            search,
            out_paths,
        }
    }
}

/// The directory one action ran in. Dropping it removes the directory and everything in it; a
/// removal that fails is ignored.
#[derive(Debug)]
pub struct ExecDir {
    path: PathBuf,
}

impl ExecDir {
    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for ExecDir {
    fn drop(&mut self) {
        // The next command to take the state lock removes whatever this leaves.
        drop(clear_path(&self.path));
    }
}

impl ExecStrategy for InputSandbox {
    type Dir = ExecDir;

    fn prepare(&self, action: &ReadyAction) -> Result<ExecDir> {
        let label = &action.label;
        if self.out_paths == OutPaths::Cleared {
            for output in &action.outputs {
                let path = self.root.join(output.as_str());
                // Unlinked, never written through: what an earlier build left at the path is a
                // link to a file the content store still holds.
                clear_path(&path)
                    .map_err(|error| fault(label, &format!("clearing {output}"), error))?;
                make_parent(&path).map_err(|error| {
                    fault(label, &format!("making the directory of {output}"), error)
                })?;
            }
        }
        self.scratch
            .claim(self.owner)
            .map_err(|error| fault(label, "claiming the scratch directory", error))?;
        let program = relative_program(action);
        if let Some(named) = program.as_ref().map(Program::as_str)
            && climbs(named)
        {
            return Err(fault(
                label,
                "preparing the command",
                format!(
                    "{named}: a program outside the action's directory cannot be copied in; \
                     name it by an absolute path or declare it"
                ),
            ));
        }
        let path = self.scratch.action(label);
        clear_path(&path)
            .map_err(|error| fault(label, "clearing the directory to run in", error))?;
        fs::create_dir(&path)
            .map_err(|error| fault(label, "making the directory to run in", error))?;
        let dir = ExecDir { path };
        let copier = Copier {
            root: &self.root,
            exec: dir.path(),
        };
        for input in &action.inputs {
            copier
                .file(Path::new(input.as_str()))
                .map_err(|error| fault(label, &format!("copying {input}"), error))?;
        }
        for dep in &action.deps {
            copier
                .tree(Path::new(dep.as_str()))
                .map_err(|error| fault(label, &format!("copying {dep}"), error))?;
        }
        if let Some(named) = program.as_ref().map(Program::as_str) {
            copier
                .program(Path::new(named))
                .map_err(|error| fault(label, &format!("copying {named}"), error))?;
        }
        for output in &action.outputs {
            make_parent(&dir.path().join(output.as_str())).map_err(|error| {
                fault(label, &format!("making the directory of {output}"), error)
            })?;
        }
        Ok(dir)
    }

    fn run(
        &self,
        dir: &ExecDir,
        action: &ReadyAction,
        live: Option<LiveSink<'_>>,
    ) -> Result<RawOutcome> {
        let Started {
            mut command, named, ..
        } = start(action, &self.search, dir.path())?;
        command.envs(
            Isolation::Sandbox
                .pinned()
                .iter()
                .map(|(name, value)| (name.as_str(), value.as_str())),
        );
        // The program is named as the command wrote it: where it was found may lie in the
        // directory the command ran in, and that is not for a log to say.
        let captured = capture(&mut command, self.timeout, live)
            .map_err(|error| fault(&action.label, &format!("running {named}"), error))?;
        Ok(RawOutcome {
            exit: ended(&captured),
            stdout: captured.stdout,
            stderr: captured.stderr,
        })
    }

    fn collect<C: ContentStore>(
        &self,
        dir: &ExecDir,
        action: &ReadyAction,
        cas: &C,
    ) -> Result<Vec<Found>> {
        collect_in(dir.path(), action, cas)
    }
}

/// Makes the directory that holds `path`.
fn make_parent(path: &Path) -> io::Result<()> {
    path.parent().map_or(Ok(()), fs::create_dir_all)
}

/// The program of the command when it is named by a relative path: the first argument, when it
/// is a valid program that holds a `/` and does not start with one.
fn relative_program(action: &ReadyAction) -> Option<Program> {
    let program = Program::of(action.command.arguments().first()?)?;
    let named = program.as_str();
    (named.contains('/') && !named.starts_with('/')).then_some(program)
}

/// Whether the relative path `named` leaves the directory it is relative to.
fn climbs(named: &str) -> bool {
    Path::new(named)
        .components()
        .any(|part| matches!(part, Component::ParentDir))
}

/// Where copies come from and go to: the workspace root and the directory an action runs in.
struct Copier<'a> {
    root: &'a Path,
    exec: &'a Path,
}

impl Copier<'_> {
    /// Copies the file at `relative` below the root to the same place below the exec directory,
    /// mode kept, making the directories above it. A copy already there is left alone.
    fn file(&self, relative: &Path) -> io::Result<()> {
        let target = self.exec.join(relative);
        if fs::symlink_metadata(&target).is_ok() {
            return Ok(());
        }
        make_parent(&target)?;
        fs::copy(self.root.join(relative), target).map(drop)
    }

    /// Copies what is at `relative` below the root to the same place below the exec directory:
    /// the file itself, or every regular file below a directory. A link, a pipe and the like
    /// are not copied.
    fn tree(&self, relative: &Path) -> io::Result<()> {
        let mut pending = vec![relative.to_path_buf()];
        while let Some(next) = pending.pop() {
            let kind = fs::symlink_metadata(self.root.join(&next))?.file_type();
            if kind.is_file() {
                self.file(&next)?;
            } else if kind.is_dir() {
                fs::create_dir_all(self.exec.join(&next))?;
                for entry in fs::read_dir(self.root.join(&next))? {
                    pending.push(next.join(entry?.file_name()));
                }
            }
        }
        Ok(())
    }

    /// Copies the program at `relative` below the root unless it is already in the exec
    /// directory. A program that is not in the workspace is left for the run to report as not
    /// found; any other failure to look at it is the caller's.
    fn program(&self, relative: &Path) -> io::Result<()> {
        match fs::symlink_metadata(self.root.join(relative)) {
            Ok(_) => self.file(relative),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
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
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    use buildl_core::{
        Argument, Command, Digest, Directory, EnvName, EnvValue, Error, ExecStrategy, Exit,
        FileMode, Found, FoundEntry, FoundKind, Label, NetworkAccess, OutputName, OutputPath,
        PortName, ReadyAction, SourcePath, StoredFile, TargetName, Timeout,
    };

    use super::{InputSandbox, OutPaths};
    use crate::exec::memory_store::MemoryStore;
    use crate::search_path::SearchPath;
    use crate::state::{Owner, Scratch};

    /// A workspace root in a directory of its own, by its canonical path, with a scratch for it
    /// whose temporary base is a sibling directory. The directory above both is the test's too.
    struct Fixture {
        _dir: tempfile::TempDir,
        root: PathBuf,
        scratch: Scratch,
        owner: Owner,
    }

    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(dir.path()).unwrap();
        let root = base.join("ws");
        let temp = base.join("tmp");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&temp).unwrap();
        Fixture {
            owner: Owner::of(&fs::metadata(&root).unwrap()),
            scratch: Scratch::new(&temp, &root),
            root,
            _dir: dir,
        }
    }

    impl Fixture {
        fn sandbox(&self) -> InputSandbox {
            self.sandbox_with(patient(), system())
        }

        fn sandbox_with(&self, timeout: Timeout, search: SearchPath) -> InputSandbox {
            InputSandbox::new(
                self.root.clone(),
                self.scratch.clone(),
                self.owner,
                timeout,
                search,
                OutPaths::Cleared,
            )
        }

        fn untouched(&self) -> InputSandbox {
            InputSandbox::new(
                self.root.clone(),
                self.scratch.clone(),
                self.owner,
                patient(),
                system(),
                OutPaths::Untouched,
            )
        }

        /// A file of the workspace at `relative`, holding `bytes`.
        fn write(&self, relative: &str, bytes: &str) {
            let path = self.root.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
        }
    }

    /// Where the system keeps the tools these tests run.
    fn system() -> SearchPath {
        SearchPath::new(["/bin", "/usr/bin"].map(PathBuf::from))
    }

    /// Longer than any command of these tests runs when nothing is wrong.
    fn patient() -> Timeout {
        Timeout::parse("60s").unwrap()
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
        }
    }

    /// The action that runs `script` through the system's `sh`.
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

    fn source(path: &str) -> SourcePath {
        SourcePath::parse(path).unwrap()
    }

    /// Every file and directory below `top`, as paths relative to it, sorted.
    fn listing(top: &Path) -> Vec<String> {
        let mut found = Vec::new();
        let mut pending = vec![PathBuf::new()];
        while let Some(next) = pending.pop() {
            for entry in fs::read_dir(top.join(&next)).unwrap() {
                let relative = next.join(entry.unwrap().file_name());
                if top.join(&relative).is_dir() {
                    pending.push(relative.clone());
                }
                found.push(relative.to_str().unwrap().to_owned());
            }
        }
        found.sort();
        found
    }

    fn adapter(error: &Error) -> bool {
        matches!(
            error,
            Error::Adapter {
                port: PortName::ExecStrategy,
                ..
            }
        )
    }

    #[test]
    fn prepare_clears_the_real_output_paths() {
        let fixture = fixture();
        fixture.write("out/lib/file", "from an earlier build");
        fixture.write("out/lib/tree/old", "old");
        let mut action = sh("true");
        action.outputs = vec![
            output("lib", "file"),
            output("lib", "tree"),
            output("fresh/deep", "made"),
        ];

        let _dir = fixture.sandbox().prepare(&action).unwrap();

        for output in &action.outputs {
            assert!(
                fs::symlink_metadata(fixture.root.join(output.as_str())).is_err(),
                "{output} still holds something"
            );
        }
        assert!(fixture.root.join("out/fresh/deep").is_dir());
    }

    #[test]
    fn an_untouched_sandbox_leaves_the_real_output_paths_alone() {
        let fixture = fixture();
        fixture.write("out/lib/file", "from an earlier build");
        fixture.write("out/lib/tree/old", "old");
        let mut action = sh("true");
        action.outputs = vec![
            output("lib", "file"),
            output("lib", "tree"),
            output("fresh/deep", "made"),
        ];

        let _dir = fixture.untouched().prepare(&action).unwrap();

        assert_eq!(
            fs::read(fixture.root.join("out/lib/file")).unwrap(),
            b"from an earlier build"
        );
        assert_eq!(
            fs::read(fixture.root.join("out/lib/tree/old")).unwrap(),
            b"old"
        );
        assert!(!fixture.root.join("out/fresh").exists());
    }

    #[test]
    fn an_untouched_sandbox_still_copies_inputs_and_dependency_outputs() {
        let fixture = fixture();
        fixture.write("a.txt", "a");
        fixture.write("secret.txt", "secret");
        fixture.write("out/lib/tree/x", "x");
        fixture.write("out/made/result", "from an earlier build");
        let mut action = sh("true");
        action.inputs = vec![source("a.txt")];
        action.deps = vec![output("lib", "tree")];
        action.outputs = vec![output("made", "result")];

        let dir = fixture.untouched().prepare(&action).unwrap();

        assert_eq!(
            listing(dir.path()),
            [
                "a.txt",
                "out",
                "out/lib",
                "out/lib/tree",
                "out/lib/tree/x",
                "out/made",
            ]
        );
        assert_eq!(fs::read(dir.path().join("a.txt")).unwrap(), b"a");
        assert_eq!(fs::read(dir.path().join("out/lib/tree/x")).unwrap(), b"x");
    }

    #[test]
    fn only_declared_inputs_and_dependency_outputs_are_present() {
        let fixture = fixture();
        fixture.write("a.txt", "a");
        fixture.write("secret.txt", "secret");
        fixture.write("out/lib/tree/x", "x");
        fixture.write("out/lib/tree/sub/y", "y");
        let mut action = sh("true");
        action.inputs = vec![source("a.txt")];
        action.deps = vec![output("lib", "tree")];
        action.outputs = vec![output("made", "result")];

        let dir = fixture.sandbox().prepare(&action).unwrap();

        assert_eq!(
            listing(dir.path()),
            [
                "a.txt",
                "out",
                "out/lib",
                "out/lib/tree",
                "out/lib/tree/sub",
                "out/lib/tree/sub/y",
                "out/lib/tree/x",
                "out/made",
            ]
        );
        assert_eq!(
            fs::read(dir.path().join("out/lib/tree/sub/y")).unwrap(),
            b"y"
        );
    }

    #[test]
    fn copies_keep_the_mode_and_are_not_links() {
        let fixture = fixture();
        fixture.write("tool.sh", "#!/bin/sh\n");
        fs::set_permissions(fixture.root.join("tool.sh"), Permissions::from_mode(0o755)).unwrap();
        fixture.write("data.txt", "data");
        fixture.write("out/lib/dep", "dep");
        let mut action = sh("true");
        action.inputs = vec![source("tool.sh"), source("data.txt")];
        action.deps = vec![output("lib", "dep")];

        let dir = fixture.sandbox().prepare(&action).unwrap();

        for relative in ["tool.sh", "data.txt", "out/lib/dep"] {
            let original = fs::metadata(fixture.root.join(relative)).unwrap();
            let copy = fs::symlink_metadata(dir.path().join(relative)).unwrap();
            assert!(copy.is_file(), "{relative} is not a regular file");
            assert_ne!(copy.ino(), original.ino(), "{relative} shares its inode");
            assert_eq!(copy.mode() & 0o777, original.mode() & 0o777, "{relative}");
        }
        let mode = fs::metadata(dir.path().join("tool.sh")).unwrap().mode();
        assert_eq!(mode & 0o777, 0o755);
    }

    #[test]
    fn the_exec_dir_is_outside_the_workspace_and_stable() {
        let fixture = fixture();
        let action = sh("true");

        let first = fixture.sandbox().prepare(&action).unwrap();
        let first_path = first.path().to_path_buf();
        drop(first);
        let second = fixture.sandbox().prepare(&action).unwrap();

        assert!(!first_path.starts_with(&fixture.root));
        assert!(first_path.starts_with(fixture.scratch.dir()));
        assert_eq!(first_path, second.path());
    }

    #[test]
    fn a_written_out_relative_program_is_copied_in() {
        let fixture = fixture();
        fixture.write("tools/run.sh", "#!/bin/sh\n");
        fs::set_permissions(
            fixture.root.join("tools/run.sh"),
            Permissions::from_mode(0o755),
        )
        .unwrap();
        fixture.write("tools/other.sh", "#!/bin/sh\n");

        let dir = fixture
            .sandbox()
            .prepare(&action(&["tools/run.sh", "arg"]))
            .unwrap();

        assert_eq!(listing(dir.path()), ["tools", "tools/run.sh"]);
    }

    #[test]
    fn a_relative_program_absent_from_the_workspace_is_not_found_at_run() {
        let fixture = fixture();
        let sandbox = fixture.sandbox();
        let action = action(&["tools/missing.sh"]);

        let dir = sandbox.prepare(&action).unwrap();
        let failed = sandbox.run(&dir, &action, None).unwrap_err();

        assert!(adapter(&failed));
        assert_eq!(
            failed.to_string(),
            "exec strategy: //lib:probe: starting the command: the program tools/missing.sh was \
             not found"
        );
    }

    #[test]
    fn a_program_that_cannot_be_looked_at_fails_the_prepare() {
        let fixture = fixture();
        // A file stands where the directory of the program has to be.
        fixture.write("tools", "in the way");

        let failed = fixture
            .sandbox()
            .prepare(&action(&["tools/run.sh"]))
            .unwrap_err();

        assert!(adapter(&failed));
        let said = failed.to_string();
        assert!(said.contains("copying tools/run.sh"), "{said}");
        assert!(
            !said.contains(fixture.scratch.dir().to_str().unwrap()),
            "{said}"
        );
    }

    #[test]
    fn an_input_below_a_dependency_directory_is_copied_once() {
        let fixture = fixture();
        // Placed outputs are read-only; a second copy onto the first would be refused.
        fixture.write("out/lib/tree/x", "x");
        fixture.write("out/lib/tree/y", "y");
        for relative in ["out/lib/tree/x", "out/lib/tree/y"] {
            fs::set_permissions(fixture.root.join(relative), Permissions::from_mode(0o444))
                .unwrap();
        }
        let mut action = sh("true");
        action.inputs = vec![source("out/lib/tree/x")];
        action.deps = vec![output("lib", "tree")];

        let dir = fixture.sandbox().prepare(&action).unwrap();

        assert_eq!(
            listing(dir.path()),
            [
                "out",
                "out/lib",
                "out/lib/tree",
                "out/lib/tree/x",
                "out/lib/tree/y"
            ]
        );
    }

    #[test]
    fn a_program_climbing_out_is_refused() {
        let fixture = fixture();

        let failed = fixture
            .sandbox()
            .prepare(&action(&["../outside/tool"]))
            .unwrap_err();

        assert!(adapter(&failed));
        let said = failed.to_string();
        assert!(said.contains("../outside/tool"), "{said}");
        assert!(
            said.contains("name it by an absolute path or declare it"),
            "{said}"
        );
        assert!(
            !said.contains(fixture.scratch.dir().to_str().unwrap()),
            "{said}"
        );
    }

    #[test]
    fn a_failed_copy_names_the_workspace_path_only() {
        let fixture = fixture();
        let mut action = sh("true");
        action.inputs = vec![source("missing.txt")];

        let failed = fixture.sandbox().prepare(&action).unwrap_err();

        assert!(adapter(&failed));
        let said = failed.to_string();
        assert!(said.contains("missing.txt"), "{said}");
        assert!(
            !said.contains(fixture.scratch.dir().to_str().unwrap()),
            "{said}"
        );
        // The directory made for the action is gone with the failure.
        assert!(!fixture.scratch.action(&action.label).exists());
    }

    #[test]
    fn a_scratch_that_cannot_be_claimed_fails_the_prepare() {
        let fixture = fixture();
        fs::create_dir(fixture.scratch.dir()).unwrap();
        fs::set_permissions(fixture.scratch.dir(), Permissions::from_mode(0o755)).unwrap();

        let failed = fixture.sandbox().prepare(&sh("true")).unwrap_err();

        assert!(adapter(&failed));
        let said = failed.to_string();
        assert!(said.contains("mode 0755"), "{said}");
        assert!(
            !said.contains(fixture.scratch.dir().to_str().unwrap()),
            "{said}"
        );
    }

    #[test]
    fn the_dir_is_gone_after_drop() {
        let fixture = fixture();
        fixture.write("a.txt", "a");
        let mut action = sh("true");
        action.inputs = vec![source("a.txt")];

        let dir = fixture.sandbox().prepare(&action).unwrap();
        let path = dir.path().to_path_buf();
        assert!(path.is_dir());
        drop(dir);

        assert!(!path.exists());
    }

    /// An executable file of the workspace at `relative`.
    fn executable(fixture: &Fixture, relative: &str, script: &str) {
        fixture.write(relative, script);
        fs::set_permissions(fixture.root.join(relative), Permissions::from_mode(0o755)).unwrap();
    }

    fn plain(bytes: &[u8]) -> StoredFile {
        StoredFile {
            digest: Digest::of(bytes),
            mode: FileMode::Plain,
        }
    }

    #[test]
    fn an_undeclared_workspace_file_is_absent_to_the_command() {
        let fixture = fixture();
        fixture.write("secret.txt", "secret");
        fixture.write("declared.txt", "declared");
        let sandbox = fixture.sandbox();
        let mut seen = sh("/bin/cat secret.txt");
        let dir = sandbox.prepare(&seen).unwrap();

        let raw = sandbox.run(&dir, &seen, None).unwrap();

        assert!(
            matches!(raw.exit, Exit::Exited(code) if code != 0),
            "{:?}",
            raw.exit
        );
        assert_eq!(raw.stdout, Vec::<u8>::new());
        // The same command finds a file the action declares.
        seen.inputs = vec![source("declared.txt")];
        seen.command = sh("/bin/cat declared.txt").command;
        let dir = sandbox.prepare(&seen).unwrap();
        let raw = sandbox.run(&dir, &seen, None).unwrap();
        assert_eq!(raw.exit, Exit::Exited(0));
        assert_eq!(raw.stdout, b"declared");
    }

    #[test]
    fn the_command_runs_in_the_exec_dir() {
        let fixture = fixture();
        let sandbox = fixture.sandbox();
        let action = action(&["pwd"]);
        let dir = sandbox.prepare(&action).unwrap();

        let raw = sandbox.run(&dir, &action, None).unwrap();

        assert_eq!(raw.exit, Exit::Exited(0));
        let printed = String::from_utf8(raw.stdout).unwrap();
        assert_eq!(
            Path::new(printed.trim_end()),
            fixture.scratch.action(&action.label)
        );
        assert!(Path::new(printed.trim_end()).starts_with(fixture.scratch.dir()));
    }

    #[test]
    fn a_dependency_program_runs_from_the_exec_dir() {
        let fixture = fixture();
        executable(&fixture, "out/lib/greet", "#!/bin/sh\necho $0\n");
        let sandbox = fixture.sandbox();
        let mut action = action(&["out/lib/greet"]);
        action.deps = vec![output("lib", "greet")];
        let dir = sandbox.prepare(&action).unwrap();

        let raw = sandbox.run(&dir, &action, None).unwrap();

        assert_eq!(raw.exit, Exit::Exited(0));
        let printed = String::from_utf8(raw.stdout).unwrap();
        assert!(
            Path::new(printed.trim_end()).starts_with(fixture.scratch.dir()),
            "{printed}"
        );
        assert!(printed.trim_end().ends_with("out/lib/greet"), "{printed}");
    }

    #[test]
    fn the_child_sees_declared_and_pinned_variables_only() {
        let fixture = fixture();
        let sandbox = fixture.sandbox();
        let mut declared = action(&["env"]);
        declared.env.insert(
            EnvName::parse("FOO").unwrap(),
            EnvValue::parse("bar").unwrap(),
        );
        let dir = sandbox.prepare(&declared).unwrap();

        let raw = sandbox.run(&dir, &declared, None).unwrap();

        assert_eq!(raw.exit, Exit::Exited(0));
        let printed = String::from_utf8(raw.stdout).unwrap();
        let mut lines: Vec<&str> = printed.lines().collect();
        lines.sort_unstable();
        assert_eq!(
            lines,
            [
                "FOO=bar",
                "LC_ALL=C",
                "SOURCE_DATE_EPOCH=315532800",
                "TZ=UTC"
            ]
        );
    }

    #[test]
    fn outputs_are_collected_from_the_exec_dir() {
        let fixture = fixture();
        let sandbox = fixture.sandbox();
        let store = MemoryStore::new();
        let mut action =
            sh("echo hi > out/lib/file; mkdir out/lib/tree; echo 3 > out/lib/tree/inside");
        action.outputs = vec![
            output("lib", "file"),
            output("lib", "tree"),
            output("lib", "absent"),
        ];
        let dir = sandbox.prepare(&action).unwrap();

        let raw = sandbox.run(&dir, &action, None).unwrap();
        let found = sandbox.collect(&dir, &action, &store).unwrap();

        assert_eq!(raw.exit, Exit::Exited(0));
        assert_eq!(
            found,
            [
                Found::File(plain(b"hi\n")),
                Found::Tree(vec![FoundEntry {
                    path: "inside".to_owned(),
                    kind: FoundKind::File(plain(b"3\n")),
                }]),
                Found::Nothing,
            ]
        );
        // The command wrote in its own directory; the real output paths hold nothing.
        for output in &action.outputs {
            assert!(
                fs::symlink_metadata(fixture.root.join(output.as_str())).is_err(),
                "{output} was written in the workspace"
            );
        }
    }

    #[test]
    fn a_store_that_fails_fails_the_collect_with_its_own_error() {
        let fixture = fixture();
        let sandbox = fixture.sandbox();
        let mut action = sh("echo hi > out/lib/file");
        action.outputs = vec![output("lib", "file")];
        let dir = sandbox.prepare(&action).unwrap();
        sandbox.run(&dir, &action, None).unwrap();

        let refused = sandbox
            .collect(&dir, &action, &MemoryStore::broken())
            .unwrap_err();

        assert_eq!(refused.to_string(), "content store: the store is broken");
    }

    #[test]
    fn a_missing_program_names_it_as_written() {
        let fixture = fixture();
        let sandbox = fixture.sandbox_with(patient(), SearchPath::new([]));
        let action = sh("true");
        let dir = sandbox.prepare(&action).unwrap();

        let failed = sandbox.run(&dir, &action, None).unwrap_err();

        assert!(adapter(&failed));
        let said = failed.to_string();
        assert_eq!(
            said,
            "exec strategy: //lib:probe: starting the command: the program sh was not found"
        );
        assert!(!said.contains(fixture.scratch.dir().to_str().unwrap()));
    }

    #[test]
    fn a_program_that_cannot_start_is_named_as_written() {
        let fixture = fixture();
        // Executable, but its interpreter is missing: starting it fails with the system's own
        // error.
        executable(&fixture, "not-a-program", "#!/no/such/interpreter\n");
        let broken = fixture.root.join("not-a-program");
        let sandbox = fixture.sandbox();
        let action = action(&[broken.to_str().unwrap()]);
        let dir = sandbox.prepare(&action).unwrap();

        let failed = sandbox.run(&dir, &action, None).unwrap_err();

        assert!(adapter(&failed));
        let said = failed.to_string();
        assert!(
            said.starts_with(&format!(
                "exec strategy: //lib:probe: running {}: ",
                broken.to_str().unwrap()
            )),
            "{said}"
        );
        assert!(
            !said.contains(fixture.scratch.dir().to_str().unwrap()),
            "{said}"
        );
    }

    #[test]
    fn an_action_past_its_timeout_is_stopped() {
        let fixture = fixture();
        let sandbox = fixture.sandbox_with(Timeout::parse("300ms").unwrap(), system());
        let action = sh("exec /bin/sleep 30");
        let dir = sandbox.prepare(&action).unwrap();
        let started = Instant::now();

        let raw = sandbox.run(&dir, &action, None).unwrap();

        assert_eq!(raw.exit, Exit::TimedOut);
        // Stopped at the timeout, not when the command would have ended by itself.
        assert!(started.elapsed() < Duration::from_secs(10));
    }
}
