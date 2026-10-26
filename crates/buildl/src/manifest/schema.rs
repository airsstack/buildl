//! The tables of `buildl.toml`, as they are written.
//!
//! Its own file because what the file may hold is one decision, and it is made here: every
//! table and every key is optional, and a key no table declares is an error. The manifest is the
//! ceiling on what a build may do, so a misspelt key must not silently drop a grant. For the
//! same reason a table is read only from a table: left to itself, the derived reader also takes
//! an array, by position, and drops the entries it has no field for.
//!
//! Responsibilities: [`Raw`] and the table it holds for each section, every value kept with the
//! place it was written at.
//!
//! Non-responsibilities: what a value means. Each one is text or a number here; whether it
//! names a directory, a program or a length of time is decided where the tables are read into
//! values.

use core::fmt;
use core::marker::PhantomData;
use std::collections::BTreeMap;

use serde::Deserialize;
use serde::de::value::MapAccessDeserializer;
use serde::de::{Deserializer, MapAccess, Visitor};
use toml::Spanned;

/// Reads a table from a table and from nothing else.
fn table<'de, D, T>(deserializer: D) -> core::result::Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    deserializer.deserialize_map(TableOnly(PhantomData))
}

/// Takes a map of keys and values, and refuses every other form a value can have.
struct TableOnly<T>(PhantomData<T>);

impl<'de, T: Deserialize<'de>> Visitor<'de> for TableOnly<T> {
    type Value = T;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a table")
    }

    fn visit_map<A: MapAccess<'de>>(self, map: A) -> core::result::Result<T, A::Error> {
        T::deserialize(MapAccessDeserializer::new(map))
    }
}

/// A table kept in a map or a list: read from a table and from nothing else, as [`table`] does
/// for a field.
#[derive(Debug, Default)]
pub(crate) struct Tabular<T>(pub(crate) T);

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Tabular<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> core::result::Result<Self, D::Error> {
        table(deserializer).map(Self)
    }
}

/// The whole document.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Raw {
    /// `[workspace]`.
    #[serde(default, deserialize_with = "table")]
    pub(crate) workspace: WorkspaceTable,
    /// `[ceiling]`.
    #[serde(default, deserialize_with = "table")]
    pub(crate) ceiling: CeilingTable,
    /// `[declaration]`.
    #[serde(default, deserialize_with = "table")]
    pub(crate) declaration: DeclarationTable,
    /// `[execution]`.
    #[serde(default, deserialize_with = "table")]
    pub(crate) execution: ExecutionTable,
    /// `[approver]`.
    #[serde(default, deserialize_with = "table")]
    pub(crate) approver: ApproverTable,
    /// `[plugins.<name>]`, one table per plugin, each name with the place it was written at.
    #[serde(default, deserialize_with = "table")]
    pub(crate) plugins: BTreeMap<Spanned<String>, Tabular<PluginTable>>,
    /// `[remote]`. The table as a whole keeps no place: the parser has none to give an implicit
    /// parent, as in `[remote.token]`, so a refusal of the table is placed by reading the text again, in
    /// the `anchor` module.
    pub(crate) remote: Option<Tabular<RemoteTable>>,
}

/// `[workspace]`: what the workspace is called and where its files go.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkspaceTable {
    /// A name for people.
    pub(crate) name: Option<Spanned<String>>,
    /// The file name of every directory's build file.
    pub(crate) entry: Option<Spanned<String>>,
    /// The directory every output is placed under.
    pub(crate) out: Option<Spanned<String>>,
    /// The version of this file's own format.
    pub(crate) config: Option<Spanned<i64>>,
}

/// `[ceiling]`: the most any build of the workspace may be granted.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CeilingTable {
    /// `proc`: the programs.
    #[serde(default, deserialize_with = "table")]
    pub(crate) proc: ProcTable,
    /// `env`: the environment variables.
    #[serde(default, deserialize_with = "table")]
    pub(crate) env: EnvTable,
    /// `fs`: the directories.
    #[serde(default, deserialize_with = "table")]
    pub(crate) fs: FsTable,
}

/// `[ceiling.proc]`, and `proc` of a plugin.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProcTable {
    /// The programs a command may run.
    #[serde(default)]
    pub(crate) run: Vec<Spanned<String>>,
}

/// `[ceiling.env]`, and `env` of a plugin.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EnvTable {
    /// The environment variables a target may read.
    #[serde(default)]
    pub(crate) read: Vec<Spanned<String>>,
}

/// `[ceiling.fs]`.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FsTable {
    /// The directories inputs may come from.
    #[serde(default)]
    pub(crate) read: Vec<Spanned<String>>,
    /// The directories a build may write under.
    #[serde(default)]
    pub(crate) write: Vec<Spanned<String>>,
}

/// `[declaration]`: the ceilings one build file is evaluated under.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DeclarationTable {
    /// The memory ceiling, written with a unit.
    pub(crate) memory: Option<Spanned<String>>,
    /// The instruction ceiling.
    pub(crate) instructions: Option<Spanned<i64>>,
    /// The ceiling on directory entries a source listing may visit.
    pub(crate) walk_entries: Option<Spanned<i64>>,
}

/// `[execution]`: how actions are run.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExecutionTable {
    /// How many actions run at once; `0` is one per core.
    pub(crate) workers: Option<Spanned<i64>>,
    /// How long one action may run, written with a unit.
    pub(crate) timeout: Option<Spanned<String>>,
    /// How strongly actions are isolated.
    pub(crate) strategy: Option<Spanned<String>>,
}

/// `[approver]`: who approves what a build wants.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ApproverTable {
    /// The mode that answers: `manifest`, `interactive` or `deny`.
    pub(crate) default: Option<Spanned<String>>,
}

/// `[remote]`: the remote cache the workspace shares results with.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RemoteTable {
    /// The cache's address, `grpc://` or `grpcs://`.
    pub(crate) url: Option<Spanned<String>>,
    /// The instance name the cache's storage is addressed under.
    pub(crate) instance: Option<Spanned<String>>,
    /// A certificate authority file to trust.
    pub(crate) ca: Option<Spanned<String>>,
    /// Where the token sent with every request comes from.
    pub(crate) token: Option<Tabular<TokenTable>>,
    /// The header the token is sent in.
    pub(crate) header: Option<Spanned<String>>,
    /// How long one request may take, written with a unit.
    pub(crate) timeout: Option<Spanned<String>>,
}

/// `token` of `[remote]`: the environment variable or the file the token is read from.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TokenTable {
    /// The environment variable holding the token.
    pub(crate) env: Option<Spanned<String>>,
    /// The file holding the token.
    pub(crate) file: Option<Spanned<String>>,
}

/// `[plugins.<name>]`: where one plugin comes from, and what the workspace lets it have.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PluginTable {
    /// A workspace-relative directory holding the plugin.
    pub(crate) path: Option<Spanned<String>>,
    /// The URL of a git repository holding the plugin.
    pub(crate) git: Option<Spanned<String>>,
    /// A GitHub repository, written `owner/repo`.
    pub(crate) github: Option<Spanned<String>>,
    /// The tag or full commit of the repository the plugin is pinned to.
    pub(crate) rev: Option<Spanned<String>>,
    /// `proc`: the programs the plugin's targets may run.
    #[serde(default, deserialize_with = "table")]
    pub(crate) proc: ProcTable,
    /// `env`: the environment variables the plugin's targets may read.
    #[serde(default, deserialize_with = "table")]
    pub(crate) env: EnvTable,
    /// Whether the plugin's targets may reach the network.
    pub(crate) network: Option<Spanned<bool>>,
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use super::Raw;

    #[test]
    fn a_document_with_no_table_leaves_every_key_out() {
        for text in ["", "# nothing\n\n", "[workspace]\n[ceiling]\n"] {
            let raw: Raw = toml::from_str(text).unwrap();
            assert!(raw.workspace.name.is_none(), "{text:?}");
            assert!(raw.workspace.entry.is_none(), "{text:?}");
            assert!(raw.workspace.out.is_none(), "{text:?}");
            assert!(raw.workspace.config.is_none(), "{text:?}");
            assert!(raw.ceiling.proc.run.is_empty(), "{text:?}");
            assert!(raw.ceiling.env.read.is_empty(), "{text:?}");
            assert!(raw.ceiling.fs.read.is_empty(), "{text:?}");
            assert!(raw.ceiling.fs.write.is_empty(), "{text:?}");
            assert!(raw.declaration.memory.is_none(), "{text:?}");
            assert!(raw.declaration.instructions.is_none(), "{text:?}");
            assert!(raw.declaration.walk_entries.is_none(), "{text:?}");
            assert!(raw.execution.workers.is_none(), "{text:?}");
            assert!(raw.execution.timeout.is_none(), "{text:?}");
            assert!(raw.execution.strategy.is_none(), "{text:?}");
            assert!(raw.approver.default.is_none(), "{text:?}");
            assert!(raw.plugins.is_empty(), "{text:?}");
            assert!(raw.remote.is_none(), "{text:?}");
        }
    }

    #[test]
    fn a_value_keeps_the_place_it_was_written_at() {
        let text = "[workspace]\nout = \"gen\"\n\n[ceiling]\nproc.run = [\"cc\", \"ld\"]\n\n[execution]\nworkers = 4\n";
        let raw: Raw = toml::from_str(text).unwrap();

        let out = raw.workspace.out.unwrap();
        assert_eq!(out.get_ref(), "gen");
        assert_eq!(&text[out.span()], "\"gen\"");

        let run: Vec<&str> = raw
            .ceiling
            .proc
            .run
            .iter()
            .map(|program| &text[program.span()])
            .collect();
        assert_eq!(run, ["\"cc\"", "\"ld\""]);

        let workers = raw.execution.workers.unwrap();
        assert_eq!(*workers.get_ref(), 4);
        assert_eq!(&text[workers.span()], "4");
    }

    #[test]
    fn a_table_is_read_from_a_table_and_from_nothing_else() {
        let cases = [
            (
                "workspace = 3\n",
                "invalid type: integer `3`, expected a table",
            ),
            (
                "ceiling = \"open\"\n",
                "invalid type: string \"open\", expected a table",
            ),
            (
                "declaration = true\n",
                "invalid type: boolean `true`, expected a table",
            ),
            (
                "execution = [4, \"1s\", \"bare\"]\n",
                "invalid type: sequence, expected a table",
            ),
            (
                "approver = \"deny\"\n",
                "invalid type: string \"deny\", expected a table",
            ),
            (
                "remote = \"grpcs://c\"\n",
                "invalid type: string \"grpcs://c\", expected a table",
            ),
            (
                "[remote]\ntoken = \"secret\"\n",
                "invalid type: string \"secret\", expected a table",
            ),
            (
                "[[remote]]\nurl = \"grpcs://c\"\n",
                "invalid type: sequence, expected a table",
            ),
            (
                "[remote]\ntoken = [\"T\", \"t\"]\n",
                "invalid type: sequence, expected a table",
            ),
            (
                "plugins = \"go\"\n",
                "invalid type: string \"go\", expected a table",
            ),
            (
                "[plugins]\ngo = \"x\"\n",
                "invalid type: string \"x\", expected a table",
            ),
            (
                "[[plugins.go]]\ngit = \"x\"\n",
                "invalid type: sequence, expected a table",
            ),
            (
                "[ceiling]\nproc = [[\"cc\"], [\"ld\"]]\n",
                "invalid type: sequence, expected a table",
            ),
            (
                "[ceiling]\nenv = [[\"HOME\"]]\n",
                "invalid type: sequence, expected a table",
            ),
            (
                "[ceiling]\nfs = [[\"src\"], [\"out\"]]\n",
                "invalid type: sequence, expected a table",
            ),
        ];
        for (text, expected) in cases {
            let error = toml::from_str::<Raw>(text).unwrap_err();
            assert_eq!(error.message(), expected, "{text:?}");
        }
    }

    #[test]
    fn every_table_refuses_a_key_it_does_not_declare() {
        let cases = [
            ("[approver]\nmode = \"deny\"\n", "mode"),
            ("approvers = 1\n", "approvers"),
            ("[workspace]\nroot = \".\"\n", "root"),
            ("[ceiling]\nnet.connect = []\n", "net"),
            ("[ceiling.proc]\nexec = []\n", "exec"),
            ("[ceiling.env]\nwrite = []\n", "write"),
            ("[ceiling.fs]\nwirte = []\n", "wirte"),
            ("[declaration]\ntime = \"1s\"\n", "time"),
            ("[execution]\nworker = 4\n", "worker"),
            ("[plugins.go]\nbranch = \"main\"\n", "branch"),
            ("[plugins.go.proc]\nexec = []\n", "exec"),
            ("[plugins.go.env]\nwrite = []\n", "write"),
            ("[remote]\nurll = \"grpcs://c\"\n", "urll"),
            ("[remote.token]\nenvs = \"T\"\n", "envs"),
        ];
        for (text, key) in cases {
            let error = toml::from_str::<Raw>(text).unwrap_err();
            assert!(
                error
                    .message()
                    .starts_with(&format!("unknown field `{key}`")),
                "{text:?}: {}",
                error.message()
            );
        }
    }
}
