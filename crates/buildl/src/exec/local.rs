//! The strategy the real ports name: the workspace's choice of how an action is run.
//!
//! Its own file because the choice is made once, from the manifest, and every action of a
//! build then runs under it: the pipeline is generic over one strategy type, so the several
//! ways to run an action have to meet in one. Each method does what its variant does and
//! nothing besides.
//!
//! Responsibilities: [`Local`], [`LocalDir`], and the one refusal text for a strategy this
//! version of buildl cannot run.
//!
//! Non-responsibilities: running an action, which [`Bare`] and [`InputSandbox`] do; and
//! deciding which strategy a workspace asks for, which its manifest does.

use buildl_core::{
    ContentStore, Diagnostic, Error, ExecStrategy, Found, Isolation, LiveSink, PortName,
    RawOutcome, ReadyAction, Result,
};

use crate::exec::bare::Bare;
use crate::exec::outcome::fault;
use crate::exec::sandbox::{ExecDir, InputSandbox};

/// The file a workspace's manifest is read from, as a refusal names it.
const MANIFEST_FILE: &str = "buildl.toml";

/// The strategy a workspace runs its actions under.
///
/// `Refused` stands for a strategy this version of buildl cannot run. It is made so that a
/// command that runs nothing, such as `plan`, still has every port; it prepares nothing, and
/// every method that would run an action returns the refusal.
#[derive(Debug)]
pub enum Local {
    /// Tier 1: a plain process in the workspace.
    Bare(Bare),
    /// Tier 2: a plain process in a directory of its own, holding only what the action declared.
    Sandbox(InputSandbox),
    /// A strategy this version of buildl cannot run.
    Refused(Isolation),
}

/// The directory one action runs in, as the strategy that prepared it knows it.
#[derive(Debug)]
pub enum LocalDir {
    /// The workspace itself, which needs no directory to be handed on.
    Bare(()),
    /// The directory of an action under the input sandbox.
    Sandbox(ExecDir),
}

impl Local {
    /// The error for a workspace that asks for `strategy`, which this version of buildl cannot
    /// run: it names the strategies it can, and the line to write instead.
    #[must_use]
    pub fn refusal(strategy: Isolation) -> Error {
        Error::Adapter {
            port: PortName::Manifest,
            diagnostic: Diagnostic::new(format!(
                "{MANIFEST_FILE}: [execution] strategy is \"{strategy}\", and this version of \
                 buildl runs \"{bare}\" and \"{sandbox}\" only: write strategy = \"{sandbox}\" \
                 under [execution]",
                bare = Isolation::Bare,
                sandbox = Isolation::Sandbox,
            )),
        }
    }
}

/// The error for a directory that another strategy prepared.
fn foreign(action: &ReadyAction) -> Error {
    fault(
        &action.label,
        "running the action",
        "its directory was prepared by another strategy",
    )
}

impl ExecStrategy for Local {
    type Dir = LocalDir;

    fn prepare(&self, action: &ReadyAction) -> Result<LocalDir> {
        match self {
            Self::Bare(bare) => bare.prepare(action).map(LocalDir::Bare),
            Self::Sandbox(sandbox) => sandbox.prepare(action).map(LocalDir::Sandbox),
            Self::Refused(strategy) => Err(Self::refusal(*strategy)),
        }
    }

    fn run(
        &self,
        dir: &LocalDir,
        action: &ReadyAction,
        live: Option<LiveSink<'_>>,
    ) -> Result<RawOutcome> {
        match (self, dir) {
            (Self::Bare(bare), LocalDir::Bare(nothing)) => bare.run(nothing, action, live),
            (Self::Sandbox(sandbox), LocalDir::Sandbox(exec)) => sandbox.run(exec, action, live),
            (Self::Refused(strategy), _) => Err(Self::refusal(*strategy)),
            _ => Err(foreign(action)),
        }
    }

    fn collect<C: ContentStore>(
        &self,
        dir: &LocalDir,
        action: &ReadyAction,
        cas: &C,
    ) -> Result<Vec<Found>> {
        match (self, dir) {
            (Self::Bare(bare), LocalDir::Bare(nothing)) => bare.collect(nothing, action, cas),
            (Self::Sandbox(sandbox), LocalDir::Sandbox(exec)) => sandbox.collect(exec, action, cas),
            (Self::Refused(strategy), _) => Err(Self::refusal(*strategy)),
            _ => Err(foreign(action)),
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
    use std::fs;
    use std::path::{Path, PathBuf};

    use buildl_core::{
        Argument, Command, Directory, Error, ExecStrategy, Exit, Isolation, Label, NetworkAccess,
        OutputName, OutputPath, PortName, ReadyAction, TargetName, Timeout,
    };

    use super::{Local, LocalDir};
    use crate::exec::bare::Bare;
    use crate::exec::memory_store::MemoryStore;
    use crate::exec::sandbox::{InputSandbox, OutPaths};
    use crate::search_path::SearchPath;
    use crate::state::{Owner, Scratch};

    /// A workspace root in a directory of its own, by its canonical path, with a scratch for it
    /// whose temporary base is a sibling directory.
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
        fn bare(&self) -> Local {
            Local::Bare(Bare::new(self.root.clone(), patient(), system()))
        }

        fn sandbox(&self) -> Local {
            Local::Sandbox(InputSandbox::new(
                self.root.clone(),
                self.scratch.clone(),
                self.owner,
                patient(),
                system(),
                OutPaths::Cleared,
            ))
        }
    }

    fn system() -> SearchPath {
        SearchPath::new(["/bin", "/usr/bin"].map(PathBuf::from))
    }

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

    fn output() -> OutputPath {
        let label = Label::new(
            Directory::parse("lib").unwrap(),
            TargetName::parse("probe").unwrap(),
        );
        OutputPath::of(
            &Directory::parse("out").unwrap(),
            &label,
            &OutputName::parse("file").unwrap(),
        )
    }

    fn adapter(error: &Error, port: PortName) -> bool {
        matches!(error, Error::Adapter { port: found, .. } if *found == port)
    }

    #[test]
    fn bare_runs_in_the_workspace_and_sandbox_in_its_exec_dir() {
        let fixture = fixture();
        let action = action(&["pwd"]);

        let bare = fixture.bare();
        let dir = bare.prepare(&action).unwrap();
        let raw = bare.run(&dir, &action, None).unwrap();
        assert_eq!(raw.exit, Exit::Exited(0));
        let printed = String::from_utf8(raw.stdout).unwrap();
        assert_eq!(Path::new(printed.trim_end()), fixture.root);

        let sandbox = fixture.sandbox();
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
    fn a_dir_of_the_other_variant_is_an_adapter_error() {
        let fixture = fixture();
        let action = action(&["pwd"]);
        let store = MemoryStore::new();
        let sandbox_dir = fixture.sandbox().prepare(&action).unwrap();
        let bare_dir = LocalDir::Bare(());

        let bare = fixture.bare();
        let failed = bare.run(&sandbox_dir, &action, None).unwrap_err();
        assert!(adapter(&failed, PortName::ExecStrategy), "{failed:?}");
        assert_eq!(
            failed.to_string(),
            "exec strategy: //lib:probe: running the action: its directory was prepared by \
             another strategy"
        );
        let failed = bare.collect(&sandbox_dir, &action, &store).unwrap_err();
        assert!(adapter(&failed, PortName::ExecStrategy), "{failed:?}");

        let sandbox = fixture.sandbox();
        let failed = sandbox.run(&bare_dir, &action, None).unwrap_err();
        assert!(adapter(&failed, PortName::ExecStrategy), "{failed:?}");
        let failed = sandbox.collect(&bare_dir, &action, &store).unwrap_err();
        assert!(adapter(&failed, PortName::ExecStrategy), "{failed:?}");
    }

    #[test]
    fn refused_prepares_nothing_and_returns_the_refusal() {
        let mut action = action(&["pwd"]);
        action.outputs = vec![output()];
        let refused = Local::Refused(Isolation::OsSandbox);
        let text = "manifest: buildl.toml: [execution] strategy is \"os-sandbox\", and this \
                    version of buildl runs \"bare\" and \"sandbox\" only: write strategy = \
                    \"sandbox\" under [execution]";

        let failed = refused.prepare(&action).unwrap_err();

        assert!(adapter(&failed, PortName::Manifest), "{failed:?}");
        assert_eq!(failed.to_string(), text);
        // Handed a directory anyway, it still returns the refusal and runs nothing.
        let failed = refused.run(&LocalDir::Bare(()), &action, None).unwrap_err();
        assert_eq!(failed.to_string(), text);
        let failed = refused
            .collect(&LocalDir::Bare(()), &action, &MemoryStore::new())
            .unwrap_err();
        assert_eq!(failed.to_string(), text);
    }
}
