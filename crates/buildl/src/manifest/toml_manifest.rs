//! [`TomlManifest`], the [`Manifest`] read from a workspace's `buildl.toml`.
//!
//! Its own file because it is the adapter's entry point: it turns the manifest's text into the
//! three values the rest of buildl is assembled from, once, and refuses the whole file when any
//! part of it is wrong. A manifest is the ceiling on what a build may do, so nothing in it is
//! skipped and nothing is guessed.
//!
//! Responsibilities: [`TomlManifest`], its [`Manifest`] implementation, and
//! [`DeclarationSettings`].
//!
//! Non-responsibilities: the defaults for the ceilings a build file is evaluated under. A
//! ceiling the manifest leaves out stays absent here, and whoever evaluates build files supplies
//! its own default.

use core::fmt;
use core::num::NonZeroU64;
use std::fs;

use buildl_core::{Diagnostic, EntryName, Error, Manifest, PortName, Result, Workspace};

use crate::manifest::interpret;
use crate::manifest::root::{MANIFEST_FILE, WorkspaceRoot};
use crate::manifest::schema::Raw;

/// A workspace's `buildl.toml`, read and checked once.
///
/// Every table and every key is optional, and a manifest with nothing in it is valid: every
/// setting takes its default and the ceiling is empty, which allows nothing. A key this version
/// does not read is an error wherever it stands, and so is a value its type refuses; each is
/// reported on one line as `buildl.toml:<line>:<column>: <message>`.
///
/// # Examples
///
/// ```
/// use buildl::manifest::TomlManifest;
/// use buildl_core::{Isolation, Manifest};
///
/// let manifest = TomlManifest::parse(
///     r#"
/// [ceiling]
/// proc.run = ["cc"]
/// fs.write = ["out", ".buildl"]
///
/// [execution]
/// strategy = "bare"
/// "#,
/// )?;
/// let workspace = manifest.workspace()?;
/// assert_eq!(workspace.out.as_str(), "out");
/// assert_eq!(workspace.ceiling.run.len(), 1);
/// assert_eq!(workspace.execution.strategy, Isolation::Bare);
/// assert_eq!(manifest.entry().as_str(), "build.lua");
///
/// let typo = TomlManifest::parse("[ceiling]\nfs.wirte = [\"out\"]\n").unwrap_err();
/// assert_eq!(
///     typo.to_string(),
///     "manifest: buildl.toml:2:4: unknown field `wirte`, expected `read` or `write`"
/// );
/// # Ok::<(), buildl_core::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TomlManifest {
    workspace: Workspace,
    entry: EntryName,
    declaration: DeclarationSettings,
}

impl TomlManifest {
    /// Reads `buildl.toml` at `root`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Adapter`], naming [`PortName::Manifest`], when the file cannot be read,
    /// is not UTF-8 text, or is refused by [`TomlManifest::parse`].
    pub fn read(root: &WorkspaceRoot) -> Result<Self> {
        let bytes = fs::read(root.as_path().join(MANIFEST_FILE)).map_err(|error| unread(&error))?;
        let text = String::from_utf8(bytes).map_err(|_| unread(&"not UTF-8 text"))?;
        Self::parse(&text)
    }

    /// Reads a manifest from its text.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Adapter`](buildl_core::Error::Adapter), naming
    /// [`PortName::Manifest`](buildl_core::PortName::Manifest), when `text` is not TOML, holds a
    /// key this version does not read, holds a value of the wrong type, or holds a value its type
    /// refuses.
    pub fn parse(text: &str) -> Result<Self> {
        let raw: Raw = toml::from_str(text).map_err(|error| interpret::unparsed(text, &error))?;
        Ok(Self {
            entry: interpret::entry(text, &raw.workspace)?,
            workspace: interpret::workspace(text, &raw)?,
            declaration: interpret::declaration(text, &raw.declaration)?,
        })
    }

    /// The file name a build file has in every directory.
    #[must_use]
    pub const fn entry(&self) -> &EntryName {
        &self.entry
    }

    /// The declaration ceilings the manifest sets.
    #[must_use]
    pub const fn declaration(&self) -> DeclarationSettings {
        self.declaration
    }
}

impl Manifest for TomlManifest {
    /// The workspace as the manifest describes it. Never fails: the manifest was checked when it
    /// was read, and every call returns the same value.
    fn workspace(&self) -> Result<Workspace> {
        Ok(self.workspace.clone())
    }
}

/// The `[declaration]` table: each ceiling the manifest sets, `None` for one it leaves out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DeclarationSettings {
    /// The memory ceiling for evaluating one build file, in bytes.
    pub memory_bytes: Option<NonZeroU64>,
    /// The instruction ceiling for evaluating one build file.
    pub instructions: Option<NonZeroU64>,
    /// The ceiling on directory entries one build file's source listings may visit.
    pub walk_entries: Option<NonZeroU64>,
}

/// A manifest that could not be read as text at all, as the manifest's error.
fn unread(reason: &dyn fmt::Display) -> Error {
    Error::Adapter {
        port: PortName::Manifest,
        diagnostic: Diagnostic::new(format!("{MANIFEST_FILE}: {reason}")),
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use core::num::NonZeroU64;
    use std::collections::BTreeSet;
    use std::fs;

    use buildl_core::{
        Directory, EnvName, Error, Execution, Isolation, Manifest, PortName, Program, Timeout,
        Workers,
    };

    use super::{DeclarationSettings, TomlManifest};
    use crate::manifest::root::WorkspaceRoot;

    /// The manifest the design shows, without the table for approving grants.
    const DESIGN_EXAMPLE: &str = r#"[workspace]
name    = "myproject"
entry   = "build.lua"
out     = "out"
config  = 1                # buildl config version

[ceiling]                  # maximum authority any build may be granted
proc.run = ["cc", "ld", "go", "cargo", "bun", "docker", "protoc"]
env.read = ["HOME", "PATH", "GOCACHE", "CARGO_HOME"]
fs.read  = ["."]           # workspace-relative; expanded by the host, never by Lua
fs.write = ["out", ".buildl", "target"]

[declaration]              # ceilings for the Lua phase — it only declares
memory       = "16MB"
instructions = 10_000_000
walk_entries = 100_000     # directory entries b.sources() may visit per build file

[execution]
workers  = 0               # 0 = number of cores
timeout  = "10m"           # per action
strategy = "sandbox"       # bare | sandbox | os-sandbox | container
"#;

    fn dir(text: &str) -> Directory {
        Directory::parse(text).unwrap()
    }

    fn refusal(text: &str) -> String {
        TomlManifest::parse(text).unwrap_err().to_string()
    }

    #[test]
    fn the_design_example_reads_into_a_workspace() {
        let manifest = TomlManifest::parse(DESIGN_EXAMPLE).unwrap();
        let workspace = manifest.workspace().unwrap();

        assert_eq!(workspace.out, dir("out"));
        assert_eq!(workspace.state, dir(".buildl"));
        assert_eq!(
            workspace.ceiling.run,
            ["cc", "ld", "go", "cargo", "bun", "docker", "protoc"]
                .into_iter()
                .map(|program| Program::parse(program).unwrap())
                .collect::<BTreeSet<_>>()
        );
        assert_eq!(
            workspace.ceiling.env,
            ["HOME", "PATH", "GOCACHE", "CARGO_HOME"]
                .into_iter()
                .map(|name| EnvName::parse(name).unwrap())
                .collect::<BTreeSet<_>>()
        );
        assert_eq!(workspace.ceiling.read, BTreeSet::from([Directory::root()]));
        assert_eq!(
            workspace.ceiling.write,
            BTreeSet::from([dir("out"), dir(".buildl"), dir("target")])
        );
        assert_eq!(
            workspace.execution,
            Execution {
                workers: Workers::PerCore,
                timeout: Timeout::parse("10m").unwrap(),
                strategy: Isolation::Sandbox,
            }
        );
        assert_eq!(manifest.entry().as_str(), "build.lua");
        assert_eq!(
            manifest.declaration(),
            DeclarationSettings {
                memory_bytes: NonZeroU64::new(16 * 1024 * 1024),
                instructions: NonZeroU64::new(10_000_000),
                walk_entries: NonZeroU64::new(100_000),
            }
        );
        assert_eq!(manifest.workspace().unwrap(), workspace);
    }

    #[test]
    fn an_empty_file_gives_every_default() {
        for text in [
            "",
            "# nothing\n\n",
            "[workspace]\n[ceiling]\n[declaration]\n[execution]\n",
        ] {
            let manifest = TomlManifest::parse(text).unwrap();
            let workspace = manifest.workspace().unwrap();
            assert_eq!(workspace.out, dir("out"), "{text:?}");
            assert_eq!(workspace.state, dir(".buildl"), "{text:?}");
            assert!(workspace.ceiling.run.is_empty(), "{text:?}");
            assert!(workspace.ceiling.env.is_empty(), "{text:?}");
            assert!(workspace.ceiling.read.is_empty(), "{text:?}");
            assert!(workspace.ceiling.write.is_empty(), "{text:?}");
            assert_eq!(workspace.execution, Execution::default(), "{text:?}");
            assert_eq!(workspace.execution.workers, Workers::PerCore, "{text:?}");
            assert_eq!(
                workspace.execution.timeout,
                Timeout::parse("10m").unwrap(),
                "{text:?}"
            );
            assert_eq!(workspace.execution.strategy, Isolation::Sandbox, "{text:?}");
            assert_eq!(manifest.entry().as_str(), "build.lua", "{text:?}");
            assert_eq!(
                manifest.declaration(),
                DeclarationSettings::default(),
                "{text:?}"
            );
        }
    }

    #[test]
    fn a_key_that_is_written_replaces_its_default() {
        let manifest = TomlManifest::parse(
            r#"[workspace]
entry = "rules.lua"
out = "gen/files"

[declaration]
memory = "1GB"
instructions = 5
walk_entries = 7

[execution]
workers = 4
timeout = "90s"
strategy = "bare"
"#,
        )
        .unwrap();
        let workspace = manifest.workspace().unwrap();
        assert_eq!(workspace.out, dir("gen/files"));
        assert_eq!(workspace.state, dir(".buildl"));
        assert_eq!(
            workspace.execution,
            Execution {
                workers: Workers::of(4),
                timeout: Timeout::parse("90s").unwrap(),
                strategy: Isolation::Bare,
            }
        );
        assert_eq!(manifest.entry().as_str(), "rules.lua");
        assert_eq!(
            manifest.declaration(),
            DeclarationSettings {
                memory_bytes: NonZeroU64::new(1024 * 1024 * 1024),
                instructions: NonZeroU64::new(5),
                walk_entries: NonZeroU64::new(7),
            }
        );
    }

    #[test]
    fn neither_the_output_nor_the_state_directory_is_granted_unwritten() {
        let manifest = TomlManifest::parse("[ceiling]\nfs.write = [\"target\"]\n").unwrap();
        assert_eq!(
            manifest.workspace().unwrap().ceiling.write,
            BTreeSet::from([dir("target")])
        );
    }

    #[test]
    fn an_unknown_key_is_refused_at_its_position() {
        let cases = [
            (
                "[workspace]\nout = \"out\"\n\n[approver]\ndefault = \"interactive\"\n",
                "manifest: buildl.toml:4:2: unknown field `approver`, expected one of `workspace`, `ceiling`, `declaration`, `execution`",
            ),
            (
                "[ceiling]\nfs.read = [\".\"]\nfs.wirte = [\"out\"]\n",
                "manifest: buildl.toml:3:4: unknown field `wirte`, expected `read` or `write`",
            ),
            (
                "[ceiling]\nnet.connect = [\"example.com\"]\n",
                "manifest: buildl.toml:2:1: unknown field `net`, expected one of `proc`, `env`, `fs`",
            ),
            (
                "[execution]\nworker = 4\n",
                "manifest: buildl.toml:2:1: unknown field `worker`, expected one of `workers`, `timeout`, `strategy`",
            ),
            (
                "out = \"out\"\n",
                "manifest: buildl.toml:1:1: unknown field `out`, expected one of `workspace`, `ceiling`, `declaration`, `execution`",
            ),
        ];
        for (text, expected) in cases {
            assert_eq!(refusal(text), expected, "{text:?}");
        }
    }

    #[test]
    fn a_value_of_the_wrong_type_is_refused_at_its_position() {
        let cases = [
            (
                "[execution]\nworkers = \"4\"\n",
                "manifest: buildl.toml:2:11: invalid type: string \"4\", expected i64",
            ),
            (
                "[ceiling]\nproc.run = \"cc\"\n",
                "manifest: buildl.toml:2:12: invalid type: string \"cc\", expected a sequence",
            ),
            (
                "[execution]\ntimeout = 10\n",
                "manifest: buildl.toml:2:11: invalid type: integer `10`, expected a string",
            ),
            (
                "[execution]\nworkers = 1.5\n",
                "manifest: buildl.toml:2:11: invalid type: floating point `1.5`, expected i64",
            ),
            (
                "[ceiling]\nproc.run = [\"cc\", 7]\n",
                "manifest: buildl.toml:2:19: invalid type: integer `7`, expected a string",
            ),
            (
                "ceiling = 3\n",
                "manifest: buildl.toml:1:11: invalid type: integer `3`, expected a table",
            ),
            (
                "execution = [4, \"1s\", \"bare\"]\n",
                "manifest: buildl.toml:1:13: invalid type: sequence, expected a table",
            ),
            (
                "[ceiling]\nproc = [[\"cc\"], [\"ld\"]]\n",
                "manifest: buildl.toml:2:8: invalid type: sequence, expected a table",
            ),
            (
                "[declaration]\nmemory = 16\n",
                "manifest: buildl.toml:2:10: invalid type: integer `16`, expected a string",
            ),
            (
                "[declaration]\ninstructions = 99999999999999999999\n",
                "manifest: buildl.toml:2:16: number too large to fit in target type",
            ),
        ];
        for (text, expected) in cases {
            assert_eq!(refusal(text), expected, "{text:?}");
        }
    }

    #[test]
    fn a_value_its_type_rejects_names_the_key_and_the_reason() {
        let cases = [
            (
                "[execution]\nworkers = 2\ntimeout = \"10\"\n",
                "manifest: buildl.toml:3:11: execution.timeout: invalid timeout: \"10\" — must end in one of the units ms, s, m and h",
            ),
            (
                "[execution]\nstrategy = \"docker\"\n",
                "manifest: buildl.toml:2:12: execution.strategy: invalid execution strategy: \"docker\" — must be one of bare, sandbox, os-sandbox and container",
            ),
            (
                "[execution]\nworkers = -1\n",
                "manifest: buildl.toml:2:11: execution.workers: invalid worker count: \"-1\" — must be a whole number, written in digits only",
            ),
            (
                "[workspace]\nconfig = 2\n",
                "manifest: buildl.toml:2:10: workspace.config: invalid config version: 2 — must be 1",
            ),
            (
                "[ceiling]\nenv.read = [\"HOME\", \"MY-VAR\"]\n",
                "manifest: buildl.toml:2:21: ceiling.env.read: invalid environment variable name: \"MY-VAR\" — may hold only ASCII letters, digits and '_'",
            ),
            (
                "[ceiling]\nfs.read = [\"../x\"]\n",
                "manifest: buildl.toml:2:12: ceiling.fs.read: invalid directory: \"../x\" — must not contain a '.' or '..' segment",
            ),
            (
                "[ceiling]\nfs.write = [\"\"]\n",
                "manifest: buildl.toml:2:13: ceiling.fs.write: invalid directory: \"\" — must not be empty; the workspace root is written \".\"",
            ),
            (
                "[ceiling]\nproc.run = [\"cc\", \"$CC\"]\n",
                "manifest: buildl.toml:2:19: ceiling.proc.run: invalid program: \"$CC\" — must not hold '$'",
            ),
            (
                "[workspace]\nname = \"\"\n",
                "manifest: buildl.toml:2:8: workspace.name: invalid workspace name: \"\" — must not be empty",
            ),
            (
                "[workspace]\nentry = \"lua/build.lua\"\n",
                "manifest: buildl.toml:2:9: workspace.entry: invalid entry file name: \"lua/build.lua\" — may hold only ASCII letters, digits, '.', '-' and '_'",
            ),
            (
                "[workspace]\nout = \"\"\n",
                "manifest: buildl.toml:2:7: workspace.out: invalid output directory: \"\" — must not be the workspace root",
            ),
            (
                "[workspace]\nout = \".buildl/out\"\n",
                "manifest: buildl.toml:2:7: workspace.out: invalid output directory: \".buildl/out\" — must not be the state directory .buildl, hold it or lie inside it",
            ),
            (
                "[declaration]\nmemory = \"16\"\n",
                "manifest: buildl.toml:2:10: declaration.memory: invalid memory size: \"16\" — must end in one of the units B, KB, MB and GB",
            ),
            (
                "[declaration]\nmemory = \"0MB\"\n",
                "manifest: buildl.toml:2:10: declaration.memory: invalid memory size: \"0MB\" — must not be zero",
            ),
            (
                "[declaration]\ninstructions = 0\n",
                "manifest: buildl.toml:2:16: declaration.instructions: invalid count: 0 — must be at least 1",
            ),
            (
                "[declaration]\nwalk_entries = -3\n",
                "manifest: buildl.toml:2:16: declaration.walk_entries: invalid count: -3 — must be at least 1",
            ),
        ];
        for (text, expected) in cases {
            assert_eq!(refusal(text), expected, "{text:?}");
        }
    }

    #[test]
    fn the_three_spellings_of_a_table_read_alike() {
        let dotted = "[ceiling]\nproc.run = [\"cc\"]\nenv.read = [\"HOME\"]\nfs.read = [\".\"]\nfs.write = [\"out\", \".buildl\"]\n";
        let headers = "[ceiling.proc]\nrun = [\"cc\"]\n\n[ceiling.env]\nread = [\"HOME\"]\n\n[ceiling.fs]\nread = [\".\"]\nwrite = [\"out\", \".buildl\"]\n";
        let inline = "ceiling = { proc = { run = [\"cc\"] }, env = { read = [\"HOME\"] }, fs = { read = [\".\"], write = [\"out\", \".buildl\"] } }\n";

        let expected = TomlManifest::parse(dotted).unwrap();
        let ceiling = expected.workspace().unwrap().ceiling;
        assert_eq!(ceiling.run.len(), 1);
        assert_eq!(ceiling.env.len(), 1);
        assert_eq!(ceiling.read, BTreeSet::from([Directory::root()]));
        assert_eq!(ceiling.write, BTreeSet::from([dir("out"), dir(".buildl")]));

        assert_eq!(TomlManifest::parse(headers).unwrap(), expected);
        assert_eq!(TomlManifest::parse(inline).unwrap(), expected);
    }

    #[test]
    fn every_failure_is_an_adapter_error_naming_the_manifest() {
        let texts = [
            "[workspace\n",
            "[workspace]\nout = \"a\"\nout = \"b\"\n",
            "[approver]\ndefault = \"interactive\"\n",
            "[execution]\nworkers = \"4\"\n",
            "[execution]\ntimeout = \"10\"\n",
            "[workspace]\nconfig = 2\n",
        ];
        for text in texts {
            let error = TomlManifest::parse(text).unwrap_err();
            assert!(
                matches!(
                    error,
                    Error::Adapter {
                        port: PortName::Manifest,
                        ..
                    }
                ),
                "{text:?}: {error:?}"
            );
            let shown = error.to_string();
            assert!(shown.starts_with("manifest: buildl.toml:"), "{shown}");
            assert!(!shown.contains('\n'), "{shown}");
        }
    }

    #[test]
    fn a_manifest_is_read_from_the_root_it_was_found_at() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("lib/text")).unwrap();
        fs::write(dir.path().join("buildl.toml"), DESIGN_EXAMPLE).unwrap();

        let root = WorkspaceRoot::find(&dir.path().join("lib/text")).unwrap();
        assert_eq!(
            TomlManifest::read(&root).unwrap(),
            TomlManifest::parse(DESIGN_EXAMPLE).unwrap()
        );
    }

    #[test]
    fn a_file_that_is_not_utf8_or_cannot_be_read_is_refused() {
        let not_text = tempfile::tempdir().unwrap();
        fs::write(
            not_text.path().join("buildl.toml"),
            b"[workspace]\nname = \"caf\xE9\"\n",
        )
        .unwrap();
        let root = WorkspaceRoot::find(not_text.path()).unwrap();
        let error = TomlManifest::read(&root).unwrap_err();
        assert!(matches!(
            error,
            Error::Adapter {
                port: PortName::Manifest,
                ..
            }
        ));
        assert_eq!(error.to_string(), "manifest: buildl.toml: not UTF-8 text");

        let not_a_file = tempfile::tempdir().unwrap();
        let path = not_a_file.path().join("buildl.toml");
        fs::create_dir(&path).unwrap();
        let root = WorkspaceRoot::find(not_a_file.path()).unwrap();
        let error = TomlManifest::read(&root).unwrap_err();
        assert!(matches!(
            error,
            Error::Adapter {
                port: PortName::Manifest,
                ..
            }
        ));
        assert_eq!(
            error.to_string(),
            format!("manifest: buildl.toml: {}", fs::read(&path).unwrap_err())
        );
    }
}
