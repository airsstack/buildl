//! From the manifest's tables to the values buildl works with, and where a refusal is.
//!
//! Its own file because reading a table and understanding it are two steps: the tables hold
//! text and numbers, and each one becomes a value only through the type that checks it. A value
//! its type refuses is reported with the line and the column it was written at, which only the
//! step that still holds the manifest's text can say.
//!
//! Responsibilities: the three values a manifest is read into — [`entry`], [`workspace`] and
//! [`declaration`] — with every default; [`position`]; and the error for a document the parser
//! refused, [`unparsed`].
//!
//! Non-responsibilities: the shape of the tables, and reading the file they come from.

use core::fmt;
use core::num::NonZeroU64;
use core::ops::Range;
use std::collections::BTreeSet;

use buildl_core::{
    ApproverMode, Ceiling, Diagnostic, Directory, EntryName, EnvName, Error, Execution, Isolation,
    PortName, Program, Result, Timeout, Workers, Workspace,
};
use toml::Spanned;

use crate::manifest::memory::memory_bytes;
use crate::manifest::root::MANIFEST_FILE;
use crate::manifest::schema::{
    CeilingTable, DeclarationTable, ExecutionTable, Raw, WorkspaceTable,
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
/// ceiling, and how it wants its actions run.
///
/// Nothing is granted that the manifest does not write: neither the output directory nor the
/// state directory is added to the ceiling.
pub(crate) fn workspace(text: &str, raw: &Raw) -> Result<Workspace> {
    unused(text, &raw.workspace)?;
    let state = Directory::parse(STATE)?;
    Ok(Workspace {
        out: out(text, raw.workspace.out.as_ref(), &state)?,
        state,
        ceiling: ceiling(text, &raw.ceiling)?,
        execution: execution(text, &raw.execution)?,
        approver: ApproverMode::Manifest,
    })
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

    use buildl_core::Directory;

    use super::{directories, located, out, position, unparsed};
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
}
