//! Workspaces to build: a temporary copy of a checked-in fixture, and what two builds of it
//! must agree on.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use buildl::Workspace;
use buildl::buildl_core::json::canonical;
use buildl::buildl_core::{
    ActionOutcome, CacheRow, Directory, Label, LiveOutput, OnFailure, OutcomeClass, Plan, Result,
    Scope,
};
use buildl::report::{Mode, Paint, Silent, TerminalReporter};
use buildl::state::StateLayout;

/// The graph of the fixture `chain` as canonical JSON, followed by one newline.
pub(crate) const CHAIN_GRAPH: &str = include_str!("golden/chain.graph.json");

/// The labels of the fixture `chain`, in order.
pub(crate) const CHAIN_LABELS: [&str; 6] = [
    "//:greet",
    "//:greeting",
    "//:report",
    "//:site",
    "//lib:assets",
    "//lib:words",
];

/// Every file `chain` builds, in order.
pub(crate) const CHAIN_OUTPUTS: [&str; 8] = [
    "out/greet",
    "out/greeting",
    "out/lib/assets/one.txt",
    "out/lib/assets/sub/two.txt",
    "out/lib/words",
    "out/report",
    "out/site/one.txt",
    "out/site/sub/two.txt",
];

/// A temporary copy of a checked-in fixture workspace.
pub(crate) struct Fixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
}

impl Fixture {
    /// Copies `tests/fixtures/<name>` into a new temporary directory.
    ///
    /// Each file named in `executables`, relative to the workspace root, is made executable.
    /// No other file is, whatever mode the checked-in file has: a copy is written from the
    /// bytes alone.
    pub(crate) fn copy_of(name: &str, executables: &[&str]) -> Self {
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name);
        assert!(source.is_dir(), "no fixture at {}", source.display());
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        for (relative, file) in files_below(&source, "") {
            let copy = root.join(relative);
            fs::create_dir_all(copy.parent().unwrap()).unwrap();
            fs::write(copy, fs::read(file).unwrap()).unwrap();
        }
        for relative in executables {
            fs::set_permissions(root.join(relative), fs::Permissions::from_mode(0o755)).unwrap();
        }
        Self { _dir: dir, root }
    }

    /// The workspace root: canonical and absolute.
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    /// Where the workspace keeps its state.
    pub(crate) fn state(&self) -> StateLayout {
        StateLayout::new(&self.root, &Directory::parse(".buildl").unwrap())
    }

    /// Adds `bytes` to the end of the file at `relative`. Every edit of an input goes through
    /// here, so every edit makes the file longer.
    pub(crate) fn append(&self, relative: &str, bytes: &[u8]) {
        OpenOptions::new()
            .append(true)
            .open(self.root.join(relative))
            .unwrap()
            .write_all(bytes)
            .unwrap();
    }

    /// Makes the one line of the file at `relative` that reads `from` read `to`.
    pub(crate) fn replace_line(&self, relative: &str, from: &str, to: &str) {
        let path = self.root.join(relative);
        let text = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(
            lines.iter().filter(|line| **line == from).count(),
            1,
            "{relative} holds the line {from:?} once"
        );
        let replaced: Vec<&str> = lines
            .into_iter()
            .map(|line| if line == from { to } else { line })
            .collect();
        fs::write(path, replaced.join("\n") + "\n").unwrap();
    }

    /// The path of every file below the directory at `relative`, from the workspace root, in
    /// order. A directory that is not there holds none.
    pub(crate) fn files(&self, relative: &str) -> Vec<String> {
        files_below(&self.root.join(relative), relative)
            .into_keys()
            .collect()
    }

    /// What a write that was cut short leaves behind: each staging file of the state directory,
    /// and everything in the staging directory of the blob store.
    pub(crate) fn staged(&self) -> Vec<String> {
        let layout = self.state();
        let mut staged: Vec<String> = names(layout.dir())
            .into_iter()
            .filter(|name| name.starts_with(".tmp-"))
            .collect();
        staged.extend(
            names(&layout.blob_staging())
                .into_iter()
                .map(|name| format!("cas/tmp/{name}")),
        );
        staged
    }

    /// The lines of the build log.
    pub(crate) fn log_lines(&self) -> Vec<String> {
        fs::read_to_string(self.state().log())
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    /// The label of every row of the action cache, in order.
    pub(crate) fn recorded(&self) -> Vec<String> {
        self.rows().keys().map(Label::to_string).collect()
    }

    /// The rows of the action cache, as `cache.json` holds them. No file is no row.
    pub(crate) fn rows(&self) -> BTreeMap<Label, CacheRow> {
        let Ok(bytes) = fs::read(self.state().cache()) else {
            return BTreeMap::new();
        };
        canonical::from_slice(&bytes).unwrap()
    }

    /// What two builds of this workspace must agree on.
    pub(crate) fn snapshot(&self) -> Snapshot {
        let layout = self.state();
        Snapshot {
            outputs: files_below(&self.root.join("out"), "out")
                .into_iter()
                .map(|(relative, file)| {
                    let executable = fs::metadata(&file).unwrap().permissions().mode() & 0o111 != 0;
                    (relative, (fs::read(file).unwrap(), executable))
                })
                .collect(),
            cache: fs::read(layout.cache()).unwrap(),
            graph: fs::read(layout.graph()).unwrap(),
            cas: files_below(&layout.blobs(), ".buildl/cas")
                .into_iter()
                .map(|(relative, file)| (relative, fs::read(file).unwrap()))
                .collect(),
        }
    }

    /// Opens the workspace from its root.
    pub(crate) fn open(&self) -> Workspace {
        Workspace::open(&self.root).unwrap()
    }

    /// Builds every target, stopping at the first failure and showing nothing.
    pub(crate) fn build(&self) -> Vec<ActionOutcome> {
        self.try_build().unwrap()
    }

    /// As [`Fixture::build`], for a build that may be refused.
    pub(crate) fn try_build(&self) -> Result<Vec<ActionOutcome>> {
        self.open().build(
            &Scope::Workspace,
            &BTreeMap::new(),
            OnFailure::Stop,
            LiveOutput::Off,
            &mut Silent,
        )
    }

    /// Plans a build of every target.
    pub(crate) fn try_plan(&self) -> Result<Plan> {
        self.open().plan(&Scope::Workspace, &BTreeMap::new())
    }

    /// Builds the targets `targets` name and what they need, and nothing else.
    pub(crate) fn build_targets(&self, targets: &[&str]) -> Vec<ActionOutcome> {
        let workspace = self.open();
        let named: BTreeSet<Label> = targets
            .iter()
            .map(|target| workspace.label(target).unwrap())
            .collect();
        workspace
            .build(
                &Scope::Targets(named),
                &BTreeMap::new(),
                OnFailure::Stop,
                LiveOutput::Off,
                &mut Silent,
            )
            .unwrap()
    }

    /// Builds every target with a reporter that writes plain text, and returns what it wrote
    /// beside the outcomes.
    pub(crate) fn build_shown(&self, on_failure: OnFailure) -> (Vec<ActionOutcome>, String) {
        let mut shown = Vec::new();
        let outcomes = {
            let mut reporter = TerminalReporter::new(&mut shown, Mode::Plain, Paint::Plain);
            self.open()
                .build(
                    &Scope::Workspace,
                    &BTreeMap::new(),
                    on_failure,
                    LiveOutput::Off,
                    &mut reporter,
                )
                .unwrap()
        };
        (outcomes, String::from_utf8(shown).unwrap())
    }
}

/// What two builds must agree on, keyed by path from the workspace root.
///
/// It holds every file under the output directory with its executable bit, the action cache,
/// the graph, and every file of the blob store. The build log and the stat cache are left out:
/// both hold times.
#[derive(PartialEq, Eq)]
pub(crate) struct Snapshot {
    outputs: BTreeMap<String, (Vec<u8>, bool)>,
    cache: Vec<u8>,
    graph: Vec<u8>,
    cas: BTreeMap<String, Vec<u8>>,
}

impl Snapshot {
    /// What the first difference from `other` is, in words; `None` when there is none. Outputs
    /// are compared first, then the action cache, the graph and the blob store.
    pub(crate) fn first_difference(&self, other: &Self) -> Option<String> {
        if let Some(path) = first_unlike(&self.outputs, &other.outputs) {
            return Some(format!("{path} differs"));
        }
        if self.cache != other.cache {
            return Some(".buildl/cache.json differs".to_owned());
        }
        if self.graph != other.graph {
            return Some(".buildl/graph.json differs".to_owned());
        }
        first_unlike(&self.cas, &other.cas).map(|path| format!("{path} differs"))
    }
}

/// The first key, in order, that the two maps do not hold alike: one lacks it, or its values
/// differ.
fn first_unlike<'a, V: PartialEq>(
    left: &'a BTreeMap<String, V>,
    right: &'a BTreeMap<String, V>,
) -> Option<&'a String> {
    left.keys()
        .chain(right.keys())
        .filter(|key| left.get(*key) != right.get(*key))
        .min()
}

/// The labels of the outcomes that ended as `class`, in order.
pub(crate) fn ended(outcomes: &[ActionOutcome], class: &OutcomeClass) -> Vec<String> {
    let labels: BTreeSet<String> = outcomes
        .iter()
        .filter(|outcome| outcome.class == *class)
        .map(|outcome| outcome.label.to_string())
        .collect();
    labels.into_iter().collect()
}

/// Every file below `dir`, keyed by `prefix` joined to its path below `dir`. A directory that
/// is not there holds none. The map puts the files in order, whatever order the system lists
/// them in.
fn files_below(dir: &Path, prefix: &str) -> BTreeMap<String, PathBuf> {
    let mut found = BTreeMap::new();
    for name in names(dir) {
        let path = dir.join(&name);
        let relative = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        if path.is_dir() {
            found.extend(files_below(&path, &relative));
        } else {
            found.insert(relative, path);
        }
    }
    found
}

/// The name of everything in `dir`, in order. A directory that is not there holds nothing.
fn names(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}
