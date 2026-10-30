//! From the manifest's tables to the values buildl works with, and where a refusal is.
//!
//! Its own file because reading a table and understanding it are two steps: the tables hold
//! text and numbers, and each one becomes a value only through the type that checks it. A value
//! its type refuses is reported with the line and the column it was written at, which only the
//! step that still holds the manifest's text can say.
//!
//! Responsibilities: the four values a manifest is read into — [`entry`], [`workspace`],
//! [`declaration`] and [`plugins`] — with every default; [`position`]; and the error for a
//! document the parser refused, [`unparsed`].
//!
//! Non-responsibilities: the shape of the tables, and reading the file they come from.

use core::fmt;
use core::num::NonZeroU64;
use core::ops::Range;
use std::collections::{BTreeMap, BTreeSet};

use buildl_core::{
    ApproverMode, Ceiling, Diagnostic, Directory, EntryName, EnvName, Error, Execution, GitUrl,
    HeaderName, HostPath, InstanceName, Isolation, PlatformProperties, PlatformPropertyName,
    PluginCap, PluginName, PluginNetwork, PluginSource, PortName, Program, RemoteJobs,
    RemoteSettings, RemoteSettingsBuilder, RemoteUrl, Result, RetryCount, Rev, Timeout,
    TokenSource, Workers, Workspace,
};
use toml::Spanned;

use crate::manifest::anchor::table_written_at;
use crate::manifest::memory::memory_bytes;
use crate::manifest::root::MANIFEST_FILE;
use crate::manifest::schema::{
    ApproverTable, CeilingTable, DeclarationTable, ExecutionTable, PluginTable, Raw, RemoteTable,
    Tabular, TokenTable, WorkspaceTable,
};
use crate::manifest::toml_manifest::DeclarationSettings;

/// The file name of a build file when the manifest names none.
const DEFAULT_ENTRY: &str = "build.lua";

/// The directory outputs are placed under when the manifest names none.
const DEFAULT_OUT: &str = "out";

/// The directory buildl keeps its own files in. The manifest has no key for it.
const STATE: &str = ".buildl";

/// The one version of the manifest's own format this code reads.
const CONFIG: i64 = 1;

/// Why a `remote.token` with both or neither of `env` and `file` is refused.
const TOKEN_SOURCE: &str = "must name exactly one of env and file";

/// How a ceiling list writes the workspace root.
const ROOT: &str = ".";

/// The file name a build file has in every directory: `workspace.entry`, or `build.lua`.
pub(crate) fn entry(text: &str, table: &WorkspaceTable) -> Result<EntryName> {
    optional(text, "workspace.entry", table.entry.as_ref(), |raw| {
        EntryName::parse(raw.as_str())
    })?
    .map_or_else(|| EntryName::parse(DEFAULT_ENTRY), Ok)
}

/// The workspace the manifest describes: where its outputs and buildl's own files go, its
/// ceiling, how it wants its actions run, and which approver answers for what a build wants.
///
/// Nothing is granted that the manifest does not write: neither the output directory nor the
/// state directory is added to the ceiling. Every plugin the manifest names has an entry in
/// `plugins`, empty when the manifest gives it no cap.
pub(crate) fn workspace(text: &str, raw: &Raw) -> Result<Workspace> {
    unused(text, &raw.workspace)?;
    let state = Directory::parse(STATE)?;
    Ok(Workspace {
        out: out(text, raw.workspace.out.as_ref(), &state)?,
        state,
        ceiling: ceiling(text, &raw.ceiling)?,
        execution: execution(text, &raw.execution)?,
        approver: approver(text, &raw.approver)?,
        plugins: caps(text, &raw.plugins)?,
        remote: remote(text, raw.remote.as_ref())?,
    })
}

/// Where each plugin comes from: exactly one of `path`, `git` and `github`, a `rev` with the
/// last two, and with them an optional `dir`, the repository folder that holds the plugin.
///
/// A path plugin's directory may not be `workspace`'s output or state directory, hold either, or
/// lie inside either: buildl writes there.
pub(crate) fn plugins(
    text: &str,
    table: &BTreeMap<Spanned<String>, Tabular<PluginTable>>,
    workspace: &Workspace,
) -> Result<BTreeMap<PluginName, PluginSource>> {
    table
        .iter()
        .map(|(name, Tabular(plugin))| {
            Ok((
                plugin_name(text, name)?,
                source(text, name, plugin, workspace)?,
            ))
        })
        .collect()
}

/// What the workspace lets each plugin have. A list or flag left out allows nothing.
fn caps(
    text: &str,
    table: &BTreeMap<Spanned<String>, Tabular<PluginTable>>,
) -> Result<BTreeMap<PluginName, PluginCap>> {
    table
        .iter()
        .map(|(name, Tabular(plugin))| {
            let name = plugin_name(text, name)?;
            let key = |field: &str| format!("plugins.{name}.{field}");
            let cap = PluginCap {
                run: set(text, &key("proc.run"), &plugin.proc.run, |raw| {
                    Program::parse(raw)
                })?,
                env: set(text, &key("env.read"), &plugin.env.read, |raw| {
                    EnvName::parse(raw)
                })?,
                network: match plugin.network.as_ref().map(|written| *written.get_ref()) {
                    Some(true) => PluginNetwork::Allowed,
                    Some(false) | None => PluginNetwork::Sealed,
                },
            };
            Ok((name, cap))
        })
        .collect()
}

/// The name a `[plugins.<name>]` header gives.
fn plugin_name(text: &str, written: &Spanned<String>) -> Result<PluginName> {
    PluginName::parse(written.get_ref().as_str())
        .map_err(|reason| refused(text, "plugins", written, &reason))
}

/// The source `plugin` names, whose table is written under `name`.
fn source(
    text: &str,
    name: &Spanned<String>,
    plugin: &PluginTable,
    workspace: &Workspace,
) -> Result<PluginSource> {
    let table = format!("plugins.{}", name.get_ref());
    let field = |field: &str| format!("{table}.{field}");
    match (&plugin.path, &plugin.git, &plugin.github) {
        (Some(path), None, None) => {
            if let Some(rev) = &plugin.rev {
                return Err(refused(
                    text,
                    &field("rev"),
                    rev,
                    &"a path plugin has no rev",
                ));
            }
            if let Some(dir) = &plugin.dir {
                return Err(refused(
                    text,
                    &field("dir"),
                    dir,
                    &"a path plugin has no dir",
                ));
            }
            plugin_directory(path.get_ref(), workspace)
                .map(PluginSource::Path)
                .map_err(|reason| refused(text, &field("path"), path, &reason))
        }
        (None, Some(git), None) => {
            let url = GitUrl::parse(git.get_ref().as_str())
                .map_err(|reason| refused(text, &field("git"), git, &reason))?;
            pinned(text, name, &table, plugin, url)
        }
        (None, None, Some(github)) => {
            let url = GitUrl::github(github.get_ref())
                .map_err(|reason| refused(text, &field("github"), github, &reason))?;
            pinned(text, name, &table, plugin, url)
        }
        _ => Err(refused(
            text,
            &table,
            name,
            &"must name exactly one of path, git and github",
        )),
    }
}

/// The git source at `url`, the `rev` `plugin` pins it to and the `dir` it sits in, the
/// repository root when `plugin` names none.
fn pinned(
    text: &str,
    name: &Spanned<String>,
    table: &str,
    plugin: &PluginTable,
    url: GitUrl,
) -> Result<PluginSource> {
    let rev = plugin.rev.as_ref().ok_or_else(|| {
        refused(
            text,
            table,
            name,
            &"a git plugin needs a rev, a tag or a full commit",
        )
    })?;
    let rev = Rev::parse(rev.get_ref())
        .map_err(|reason| refused(text, &format!("{table}.rev"), rev, &reason))?;
    let dir = plugin.dir.as_ref().map_or_else(
        || Ok(Directory::root()),
        |dir| {
            Directory::parse(dir.get_ref())
                .map_err(|reason| refused(text, &format!("{table}.dir"), dir, &reason))
        },
    )?;
    Ok(PluginSource::Git { url, rev, dir })
}

/// Why a path plugin's directory is refused.
enum PathFault {
    /// It is not a directory name.
    Name(Error),
    /// It is, holds or lies inside `directory`, which is the workspace's `what`.
    Overlaps {
        raw: String,
        what: &'static str,
        directory: Directory,
    },
}

impl fmt::Display for PathFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Name(error) => error.fmt(f),
            Self::Overlaps {
                raw,
                what,
                directory,
            } => f.write_str(&invalid(
                "plugin directory",
                raw,
                &format!("must not be, hold or lie inside the {what} {directory}"),
            )),
        }
    }
}

/// The directory a path plugin names, or why it cannot be one.
fn plugin_directory(
    raw: &str,
    workspace: &Workspace,
) -> core::result::Result<Directory, PathFault> {
    let path = Directory::parse(raw).map_err(PathFault::Name)?;
    for (what, directory) in [
        ("output directory", &workspace.out),
        ("state directory", &workspace.state),
    ] {
        if path.contains(directory) || directory.contains(&path) {
            return Err(PathFault::Overlaps {
                raw: raw.to_owned(),
                what,
                directory: directory.clone(),
            });
        }
    }
    Ok(path)
}

/// The ceilings the manifest sets for evaluating a build file, each `None` when it sets none.
pub(crate) fn declaration(text: &str, table: &DeclarationTable) -> Result<DeclarationSettings> {
    Ok(DeclarationSettings {
        memory_bytes: optional(text, "declaration.memory", table.memory.as_ref(), |raw| {
            memory_bytes(raw).map_err(|reason| invalid("memory size", raw, reason))
        })?,
        instructions: optional(
            text,
            "declaration.instructions",
            table.instructions.as_ref(),
            |written| count(*written),
        )?,
        walk_entries: optional(
            text,
            "declaration.walk_entries",
            table.walk_entries.as_ref(),
            |written| count(*written),
        )?,
    })
}

/// Checks the two keys of `[workspace]` that become no value: a name is not empty, and the
/// format version is the one this code reads.
fn unused(text: &str, table: &WorkspaceTable) -> Result<()> {
    optional(text, "workspace.name", table.name.as_ref(), |name| {
        if name.is_empty() {
            return Err(invalid("workspace name", name, "must not be empty"));
        }
        Ok(())
    })?;
    optional(text, "workspace.config", table.config.as_ref(), |version| {
        if *version != CONFIG {
            return Err(invalid("config version", version, "must be 1"));
        }
        Ok(())
    })?;
    Ok(())
}

/// The `[ceiling]` table as four sets. A list left out is empty: it allows nothing.
fn ceiling(text: &str, table: &CeilingTable) -> Result<Ceiling> {
    Ok(Ceiling {
        run: set(text, "ceiling.proc.run", &table.proc.run, |raw| {
            Program::parse(raw)
        })?,
        env: set(text, "ceiling.env.read", &table.env.read, |raw| {
            EnvName::parse(raw)
        })?,
        read: directories(text, "ceiling.fs.read", &table.fs.read)?,
        write: directories(text, "ceiling.fs.write", &table.fs.write)?,
    })
}

/// The `[execution]` table, each setting built through its own type and given that type's
/// default when it is left out.
fn execution(text: &str, table: &ExecutionTable) -> Result<Execution> {
    Ok(Execution {
        workers: optional(text, "execution.workers", table.workers.as_ref(), |count| {
            Workers::parse(&count.to_string())
        })?
        .unwrap_or_default(),
        timeout: optional(text, "execution.timeout", table.timeout.as_ref(), |raw| {
            Timeout::parse(raw)
        })?
        .unwrap_or_default(),
        strategy: optional(text, "execution.strategy", table.strategy.as_ref(), |raw| {
            Isolation::parse(raw)
        })?
        .unwrap_or_default(),
    })
}

/// The `[approver]` table: the mode `approver.default` names, or `manifest` when it is left out.
fn approver(text: &str, table: &ApproverTable) -> Result<ApproverMode> {
    Ok(
        optional(text, "approver.default", table.default.as_ref(), |raw| {
            ApproverMode::parse(raw)
        })?
        .unwrap_or_default(),
    )
}

/// The `[remote]` table: the cache the workspace names, or `None` when the manifest has no such
/// table. The table needs a `url`, and a `token` names exactly one of `env` and `file` and only
/// travels over `grpcs://`. The keys added for remote execution — `execute`, `jobs`,
/// `queue_timeout`, `retries` and `properties` — are each optional.
fn remote(text: &str, table: Option<&Tabular<RemoteTable>>) -> Result<Option<RemoteSettings>> {
    let Some(Tabular(table)) = table else {
        return Ok(None);
    };
    let Some(url) = table.url.as_ref() else {
        return Err(located(
            text,
            table_written_at(text, &["remote"]),
            "remote.url: is required when [remote] is present",
        ));
    };
    let url = RemoteUrl::parse(url.get_ref())
        .map_err(|reason| refused(text, "remote.url", url, &reason))?;
    let mut builder = RemoteSettings::builder(url);
    if let Some(instance) = optional(text, "remote.instance", table.instance.as_ref(), |raw| {
        InstanceName::parse(raw.as_str())
    })? {
        builder = builder.instance(instance);
    }
    if let Some(ca) = optional(text, "remote.ca", table.ca.as_ref(), |raw| {
        HostPath::parse(raw.as_str())
    })? {
        builder = builder.ca(ca);
    }
    let mut place = None;
    if let Some(Tabular(token)) = table.token.as_ref() {
        let (source, written) = token_source(text, token)?;
        place = Some(written.span());
        builder = builder.token(source);
    }
    if let Some(header) = optional(text, "remote.header", table.header.as_ref(), |raw| {
        HeaderName::parse(raw.as_str())
    })? {
        builder = builder.header(header);
    }
    if let Some(timeout) = optional(text, "remote.timeout", table.timeout.as_ref(), |raw| {
        Timeout::parse(raw)
    })? {
        builder = builder.timeout(timeout);
    }
    builder = execution_keys(text, table, builder)?;
    builder
        .build()
        .map(Some)
        .map_err(|reason| located(text, place, &format!("remote.token: {reason}")))
}

/// The keys of `[remote]` added for remote execution, passed to `builder`; one left out leaves
/// the builder's default.
fn execution_keys(
    text: &str,
    table: &RemoteTable,
    mut builder: RemoteSettingsBuilder,
) -> Result<RemoteSettingsBuilder> {
    if let Some(execute) = table.execute.as_ref() {
        builder = builder.execute(*execute.get_ref());
    }
    if let Some(jobs) = optional(text, "remote.jobs", table.jobs.as_ref(), |count| {
        RemoteJobs::from_integer(*count)
    })? {
        builder = builder.jobs(jobs);
    }
    if let Some(timeout) = optional(
        text,
        "remote.queue_timeout",
        table.queue_timeout.as_ref(),
        |raw| Timeout::parse(raw),
    )? {
        builder = builder.queue_timeout(timeout);
    }
    if let Some(retries) = optional(text, "remote.retries", table.retries.as_ref(), |count| {
        RetryCount::from_integer(*count)
    })? {
        builder = builder.retries(retries);
    }
    if let Some(written) = table.properties.as_ref() {
        let mut properties = BTreeMap::new();
        for (name, value) in written {
            let parsed = PlatformPropertyName::parse(name.get_ref().as_str())
                .map_err(|reason| refused(text, "remote.properties", name, &reason))?;
            properties.insert(parsed, value.clone());
        }
        builder = builder.properties(PlatformProperties::new(properties));
    }
    Ok(builder)
}

/// Where the token is read from: the one of `env` and `file` `token` names, and the value of
/// that key, which is where a refusal of the token is placed.
///
/// A token naming both is placed at the first of the two written; one naming neither has no
/// value to be placed at, and is placed where the `token` table is first written.
fn token_source<'a>(
    text: &str,
    token: &'a TokenTable,
) -> Result<(TokenSource, &'a Spanned<String>)> {
    match (&token.env, &token.file) {
        (Some(env), None) => EnvName::parse(env.get_ref().as_str())
            .map(|name| (TokenSource::Env(name), env))
            .map_err(|reason| refused(text, "remote.token.env", env, &reason)),
        (None, Some(file)) => HostPath::parse(file.get_ref().as_str())
            .map(|path| (TokenSource::File(path), file))
            .map_err(|reason| refused(text, "remote.token.file", file, &reason)),
        (Some(env), Some(file)) => Err(refused(
            text,
            "remote.token",
            if env.span().start < file.span().start {
                env
            } else {
                file
            },
            &TOKEN_SOURCE,
        )),
        (None, None) => Err(located(
            text,
            table_written_at(text, &["remote", "token"]),
            &format!("remote.token: {TOKEN_SOURCE}"),
        )),
    }
}

/// A ceiling of the declaration phase: a whole number, at least 1.
fn count(written: i64) -> core::result::Result<NonZeroU64, String> {
    u64::try_from(written)
        .ok()
        .and_then(NonZeroU64::new)
        .ok_or_else(|| invalid("count", &written, "must be at least 1"))
}

/// The line and the column of the byte at `offset` in `text`, both counted from 1, the column
/// in characters.
///
/// An offset past the end is the end, and one inside a character is that character.
pub(crate) fn position(text: &str, offset: usize) -> (usize, usize) {
    let mut end = offset.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let before = text.get(..end).unwrap_or(text);
    let line = before.matches('\n').count() + 1;
    let column = before
        .rsplit('\n')
        .next()
        .map_or(0, |last| last.chars().count())
        + 1;
    (line, column)
}

/// What the parser refused: text that is not TOML, a key no table declares, or a value of the
/// wrong type. The parser's own message, on one line.
pub(crate) fn unparsed(text: &str, error: &toml::de::Error) -> Error {
    located(text, error.span(), &error.message().replace('\n', ": "))
}

/// The directories the ceiling list `key` names, as a set: `.` is the workspace root, and a
/// directory named twice is held once.
fn directories(text: &str, key: &str, entries: &[Spanned<String>]) -> Result<BTreeSet<Directory>> {
    set(text, key, entries, directory)
}

/// The directory outputs are placed under: the one `written`, or `out` when the manifest names
/// none. It is never the workspace root, and never `state`, a directory holding it or one
/// inside it.
fn out(text: &str, written: Option<&Spanned<String>>, state: &Directory) -> Result<Directory> {
    let named = optional(text, "workspace.out", written, |raw| {
        let out = Directory::parse(raw.as_str()).map_err(|error| error.to_string())?;
        if out.is_root() {
            return Err(invalid(
                "output directory",
                raw,
                "must not be the workspace root",
            ));
        }
        if out.contains(state) || state.contains(&out) {
            return Err(invalid(
                "output directory",
                raw,
                &format!("must not be the state directory {state}, hold it or lie inside it"),
            ));
        }
        Ok(out)
    })?;
    named.map_or_else(|| Directory::parse(DEFAULT_OUT), Ok)
}

/// The directory one entry of a ceiling list names.
fn directory(raw: &str) -> core::result::Result<Directory, String> {
    match raw {
        ROOT => Ok(Directory::root()),
        "" => Err(invalid(
            "directory",
            &raw,
            "must not be empty; the workspace root is written \".\"",
        )),
        other => Directory::parse(other).map_err(|error| error.to_string()),
    }
}

/// Every entry of the list `key`, read by `read`, as a set.
fn set<T: Ord, E: fmt::Display>(
    text: &str,
    key: &str,
    entries: &[Spanned<String>],
    read: impl Fn(&str) -> core::result::Result<T, E>,
) -> Result<BTreeSet<T>> {
    entries
        .iter()
        .map(|entry| read(entry.get_ref()).map_err(|reason| refused(text, key, entry, &reason)))
        .collect()
}

/// The value `written` for `key`, read by `read`; `None` when the manifest leaves the key out.
fn optional<W, T, E: fmt::Display>(
    text: &str,
    key: &str,
    written: Option<&Spanned<W>>,
    read: impl FnOnce(&W) -> core::result::Result<T, E>,
) -> Result<Option<T>> {
    written
        .map(|value| read(value.get_ref()).map_err(|reason| refused(text, key, value, &reason)))
        .transpose()
}

/// Why a value is refused, in the words every name's own refusal uses: what was written, then
/// the reason.
fn invalid(kind: &str, value: &dyn fmt::Debug, reason: &str) -> String {
    format!("invalid {kind}: {value:?} — {reason}")
}

/// The manifest's error for a `value` of `key` that is refused for `reason`, at the place the
/// value was written.
fn refused<W>(text: &str, key: &str, value: &Spanned<W>, reason: &dyn fmt::Display) -> Error {
    located(text, Some(value.span()), &format!("{key}: {reason}"))
}

/// The manifest's error for `message`, with the line and the column `span` starts at when the
/// place is known.
fn located(text: &str, span: Option<Range<usize>>, message: &str) -> Error {
    let place = span.map_or_else(String::new, |span| {
        let (line, column) = position(text, span.start);
        format!(":{line}:{column}")
    });
    Error::Adapter {
        port: PortName::Manifest,
        diagnostic: Diagnostic::new(format!("{MANIFEST_FILE}{place}: {message}")),
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use buildl_core::{
        Directory, EnvName, HeaderName, HostPath, InstanceName, PlatformProperties,
        PlatformPropertyName, RemoteJobs, RemoteSettings, RemoteUrl, RetryCount, Timeout,
        TokenSource,
    };

    use std::collections::BTreeMap;

    use super::{directories, located, out, position, unparsed, workspace};
    use crate::manifest::schema::Raw;

    fn raw(text: &str) -> Raw {
        toml::from_str(text).unwrap()
    }

    fn dir(text: &str) -> Directory {
        Directory::parse(text).unwrap()
    }

    #[test]
    fn a_byte_offset_becomes_a_line_and_a_column_in_characters() {
        let text = "# é\nkey = \"ü\" # ok\n\nlast";
        let cases = [
            (0, (1, 1)),
            (2, (1, 3)),
            (3, (1, 3)),
            (4, (1, 4)),
            (5, (2, 1)),
            (11, (2, 7)),
            (14, (2, 9)),
            (21, (3, 1)),
            (22, (4, 1)),
            (26, (4, 5)),
            (99, (4, 5)),
        ];
        for (offset, expected) in cases {
            assert_eq!(position(text, offset), expected, "offset {offset}");
        }
        assert_eq!(position("", 0), (1, 1));
    }

    #[test]
    fn what_the_parser_refuses_is_one_line_at_its_position() {
        let cases = [
            (
                "[workspace\nout = \"a\"\n",
                "manifest: buildl.toml:1:11: invalid table header: expected `.`, `]`",
            ),
            (
                "# é\n[workspace]\nout = \"a\"\nout = \"b\"\n",
                "manifest: buildl.toml:4:1: duplicate key `out` in table `workspace`",
            ),
        ];
        for (text, expected) in cases {
            let error = toml::from_str::<Raw>(text).unwrap_err();
            assert_eq!(unparsed(text, &error).to_string(), expected, "{text:?}");
        }
    }

    #[test]
    fn a_failure_with_no_place_names_the_file_alone() {
        assert_eq!(
            located("", None, "not UTF-8 text").to_string(),
            "manifest: buildl.toml: not UTF-8 text"
        );
    }

    #[test]
    fn dot_is_the_root_and_empty_text_is_refused() {
        let text = "[ceiling]\nfs.read = [\".\", \"src\", \"lib/text\", \"src\"]\n";
        let read = directories(text, "ceiling.fs.read", &raw(text).ceiling.fs.read).unwrap();
        assert_eq!(
            read.into_iter().collect::<Vec<_>>(),
            [Directory::root(), dir("lib/text"), dir("src")]
        );

        let cases = [
            (
                "[ceiling]\nfs.write = [\"out\", \"\"]\n",
                "manifest: buildl.toml:2:20: ceiling.fs.write: invalid directory: \"\" — must not be empty; the workspace root is written \".\"",
            ),
            (
                "[ceiling]\nfs.write = [\"../x\"]\n",
                "manifest: buildl.toml:2:13: ceiling.fs.write: invalid directory: \"../x\" — must not contain a '.' or '..' segment",
            ),
            (
                "[ceiling.fs]\nwrite = [\n  \"out\",\n  \"/tmp\",\n]\n",
                "manifest: buildl.toml:4:3: ceiling.fs.write: invalid directory: \"/tmp\" — must not start or end with '/'",
            ),
        ];
        for (text, expected) in cases {
            let error =
                directories(text, "ceiling.fs.write", &raw(text).ceiling.fs.write).unwrap_err();
            assert_eq!(error.to_string(), expected, "{text:?}");
        }
    }

    #[test]
    fn out_may_not_be_the_root_or_overlap_the_state_directory() {
        let state = dir(".buildl");
        assert_eq!(out("", None, &state).unwrap(), dir("out"));
        let text = "[workspace]\nout = \"gen/files\"\n";
        assert_eq!(
            out(text, raw(text).workspace.out.as_ref(), &state).unwrap(),
            dir("gen/files")
        );

        let overlap = "must not be the state directory .buildl, hold it or lie inside it";
        let cases = [
            (
                "",
                "invalid output directory: \"\" — must not be the workspace root".to_owned(),
            ),
            (
                ".buildl",
                format!("invalid output directory: \".buildl\" — {overlap}"),
            ),
            (
                ".buildl/out",
                format!("invalid output directory: \".buildl/out\" — {overlap}"),
            ),
            (
                ".",
                "invalid directory: \".\" — must not contain a '.' or '..' segment".to_owned(),
            ),
            (
                "../x",
                "invalid directory: \"../x\" — must not contain a '.' or '..' segment".to_owned(),
            ),
        ];
        for (written, reason) in cases {
            let text = format!("[workspace]\nout = {written:?}\n");
            let error = out(&text, raw(&text).workspace.out.as_ref(), &state).unwrap_err();
            assert_eq!(
                error.to_string(),
                format!("manifest: buildl.toml:2:7: workspace.out: {reason}"),
                "{written:?}"
            );
        }
    }

    /// The error `workspace` reports for `text`, which must be refused.
    fn refusal(text: &str) -> String {
        workspace(text, &raw(text)).unwrap_err().to_string()
    }

    #[test]
    fn no_remote_table_means_no_remote() {
        for text in ["", "[workspace]\nout = \"gen\"\n"] {
            assert_eq!(
                workspace(text, &raw(text)).unwrap().remote,
                None,
                "{text:?}"
            );
        }
    }

    #[test]
    fn a_full_remote_table_is_read() {
        let text = "[remote]\n\
            url      = \"grpcs://cache.example.com:443\"\n\
            instance = \"ci\"\n\
            ca       = \"certs/ca.pem\"\n\
            token    = { env = \"BUILDL_REMOTE_TOKEN\" }\n\
            header   = \"authorization\"\n\
            timeout  = \"30s\"\n";
        let expected =
            RemoteSettings::builder(RemoteUrl::parse("grpcs://cache.example.com:443").unwrap())
                .instance(InstanceName::parse("ci").unwrap())
                .ca(HostPath::parse("certs/ca.pem").unwrap())
                .token(TokenSource::Env(
                    EnvName::parse("BUILDL_REMOTE_TOKEN").unwrap(),
                ))
                .header(HeaderName::parse("authorization").unwrap())
                .timeout(Timeout::parse("30s").unwrap())
                .build()
                .unwrap();
        assert_eq!(workspace(text, &raw(text)).unwrap().remote, Some(expected));

        let by_file = "[remote]\nurl = \"grpcs://c\"\ntoken.file = \"/run/secrets/cache\"\n";
        let remote = workspace(by_file, &raw(by_file)).unwrap().remote.unwrap();
        assert_eq!(
            remote.token(),
            Some(&TokenSource::File(
                HostPath::parse("/run/secrets/cache").unwrap()
            ))
        );
    }

    #[test]
    fn a_remote_table_without_url_is_refused_at_its_header() {
        let missing = "remote.url: is required when [remote] is present";
        let cases = [
            ("[remote]\ninstance = \"ci\"\n", "1:1"),
            ("[workspace]\n\n[remote]\n", "3:1"),
            ("[remote.token]\nenv = \"T\"\n", "1:2"),
            ("# c\nremote.instance = \"ci\"\n", "2:1"),
            ("remote = {}\n", "1:1"),
        ];
        for (text, place) in cases {
            assert_eq!(
                refusal(text),
                format!("manifest: buildl.toml:{place}: {missing}"),
                "{text:?}"
            );
        }
    }

    #[test]
    fn an_unknown_remote_key_is_refused_with_its_position() {
        let text = "[workspace]\n\n[remote]\nurll = \"grpcs://c\"\n";
        let error = toml::from_str::<Raw>(text).unwrap_err();
        assert_eq!(
            unparsed(text, &error).to_string(),
            "manifest: buildl.toml:4:1: unknown field `urll`, expected one of `url`, `instance`, `ca`, `token`, `header`, `timeout`, `execute`, `jobs`, `queue_timeout`, `retries`, `properties`"
        );
    }

    #[test]
    fn a_token_with_env_and_file_is_refused() {
        let text = "[remote]\nurl = \"grpcs://c\"\ntoken = { env = \"T\", file = \"t\" }\n";
        assert_eq!(
            refusal(text),
            "manifest: buildl.toml:3:17: remote.token: must name exactly one of env and file"
        );
    }

    #[test]
    fn a_token_with_neither_is_refused_where_the_token_is_written() {
        let cases = [
            ("[remote]\nurl = \"grpcs://c\"\ntoken = {}\n", "3:1"),
            (
                "[remote]\nurl = \"grpcs://c\"\ntimeout = \"5s\"\n\n[remote.token]\n",
                "5:1",
            ),
        ];
        for (text, place) in cases {
            assert_eq!(
                refusal(text),
                format!(
                    "manifest: buildl.toml:{place}: remote.token: must name exactly one of env and file"
                ),
                "{text:?}"
            );
        }
    }

    #[test]
    fn a_token_with_a_grpc_url_is_refused_with_its_position() {
        let text = "[remote]\nurl = \"grpc://cache:50051\"\ntoken = { env = \"T\" }\n";
        assert_eq!(
            refusal(text),
            "manifest: buildl.toml:3:17: remote.token: a token would be sent in clear text to grpc://cache:50051; use a grpcs:// URL"
        );
    }

    #[test]
    fn a_bad_timeout_is_refused() {
        let text = "[remote]\nurl = \"grpcs://c\"\ntimeout = \"30\"\n";
        let message = refusal(text);
        assert!(
            message.starts_with("manifest: buildl.toml:3:11: remote.timeout: "),
            "{message}"
        );
    }

    #[test]
    fn every_refused_remote_value_is_named_with_its_place() {
        let cases = [
            (
                "[remote]\nurl = \"https://cache.example.com\"\n",
                "2:7: remote.url: invalid remote cache URL: \"https://cache.example.com\" — must start with grpc:// or grpcs://",
            ),
            (
                "[remote]\nurl = \"grpcs://c\"\ninstance = \"a//b\"\n",
                "3:12: remote.instance: ",
            ),
            (
                "[remote]\nurl = \"grpcs://c\"\nca = \"../ca.pem\"\n",
                "3:6: remote.ca: ",
            ),
            (
                "[remote]\nurl = \"grpcs://c\"\nheader = \"te\"\n",
                "3:10: remote.header: ",
            ),
            (
                "[remote]\nurl = \"grpcs://c\"\ntoken.env = \"not valid\"\n",
                "3:13: remote.token.env: ",
            ),
            (
                "[remote]\nurl = \"grpcs://c\"\ntoken.file = \"\"\n",
                "3:14: remote.token.file: ",
            ),
        ];
        for (text, expected) in cases {
            let message = refusal(text);
            assert!(
                message.starts_with(&format!("manifest: buildl.toml:{expected}")),
                "{text:?}: {message}"
            );
        }
    }

    #[test]
    fn the_execution_keys_are_read() {
        let text = "[remote]\n\
            url           = \"grpcs://cache.example.com\"\n\
            execute       = true\n\
            jobs          = 16\n\
            queue_timeout = \"10m\"\n\
            retries       = 0\n\
            properties    = { pool = \"large\", arch = \"wide\" }\n";
        let expected =
            RemoteSettings::builder(RemoteUrl::parse("grpcs://cache.example.com").unwrap())
                .execute(true)
                .jobs(RemoteJobs::from_integer(16).unwrap())
                .queue_timeout(Timeout::parse("10m").unwrap())
                .retries(RetryCount::from_integer(0).unwrap())
                .properties(PlatformProperties::new(BTreeMap::from([
                    (
                        PlatformPropertyName::parse("pool").unwrap(),
                        "large".to_owned(),
                    ),
                    (
                        PlatformPropertyName::parse("arch").unwrap(),
                        "wide".to_owned(),
                    ),
                ])))
                .build()
                .unwrap();
        assert_eq!(workspace(text, &raw(text)).unwrap().remote, Some(expected));

        let table = "[remote]\nurl = \"grpcs://c\"\n\n[remote.properties]\npool = \"large\"\n";
        let remote = workspace(table, &raw(table)).unwrap().remote.unwrap();
        let listed: Vec<_> = remote
            .properties()
            .iter()
            .map(|(name, value)| (name.as_str(), value))
            .collect();
        assert_eq!(listed, [("pool", "large")]);
    }

    #[test]
    fn the_execution_keys_default_when_absent() {
        let text = "[remote]\nurl = \"grpcs://c\"\n";
        let remote = workspace(text, &raw(text)).unwrap().remote.unwrap();
        let expected = RemoteSettings::builder(RemoteUrl::parse("grpcs://c").unwrap())
            .build()
            .unwrap();
        assert_eq!(remote, expected);
        assert!(!remote.execute());
        assert_eq!(remote.jobs().get(), 128);
        assert_eq!(remote.queue_timeout(), Timeout::parse("1h").unwrap());
        assert_eq!(remote.retries().get(), 5);
        assert!(remote.properties().is_empty());
    }

    #[test]
    fn a_bad_execution_key_is_named_with_its_place() {
        let cases = [
            (
                "[remote]\nurl = \"grpcs://c\"\njobs = 0\n",
                "3:8: remote.jobs: ",
            ),
            (
                "[remote]\nurl = \"grpcs://c\"\nretries = 11\n",
                "3:11: remote.retries: ",
            ),
            (
                "[remote]\nurl = \"grpcs://c\"\nqueue_timeout = \"0s\"\n",
                "3:17: remote.queue_timeout: ",
            ),
            (
                "[remote]\nurl = \"grpcs://c\"\nproperties = { OSFamily = \"x\" }\n",
                "3:16: remote.properties: ",
            ),
        ];
        for (text, expected) in cases {
            let message = refusal(text);
            assert!(
                message.starts_with(&format!("manifest: buildl.toml:{expected}")),
                "{text:?}: {message}"
            );
        }
    }

    #[test]
    fn a_non_string_property_is_refused() {
        let text = "[remote]\nurl = \"grpcs://c\"\nproperties = { pool = 3 }\n";
        let error = toml::from_str::<Raw>(text).unwrap_err();
        let message = unparsed(text, &error).to_string();
        assert!(
            message.starts_with("manifest: buildl.toml:3:23")
                && message.contains("invalid type")
                && message.contains("expected a string"),
            "{message}"
        );
    }
}
