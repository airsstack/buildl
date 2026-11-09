//! The markers of the generated section of `build.lua`, and replacing what lies between them.
//!
//! [`find`] locates the lines between the begin marker and the end marker; [`replace`] swaps
//! them for new ones and leaves every other byte of the file as it was.

use std::fmt;

use buildl::cli::Exit;

/// What a line starts with to be the begin marker.
const BEGIN_PREFIX: &str = "-- cargo-buildl: begin generated";
/// What a line starts with to be the end marker.
const END_PREFIX: &str = "-- cargo-buildl: end generated";

/// The begin marker line as `cargo buildl init` writes it.
#[cfg_attr(not(test), expect(dead_code, reason = "written by cargo buildl init"))]
pub(crate) const BEGIN_LINE: &str =
    "-- cargo-buildl: begin generated. `cargo buildl sync` rewrites this part from Cargo.toml.";
/// The end marker line as `cargo buildl init` writes it.
#[cfg_attr(not(test), expect(dead_code, reason = "written by cargo buildl init"))]
pub(crate) const END_LINE: &str = "-- cargo-buildl: end generated";

/// Why the generated section could not be found, with the exit status that tells it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SectionError {
    exit: Exit,
    message: &'static str,
}

impl SectionError {
    /// The exit status of the command that failed: the build file is at fault.
    pub(crate) const fn exit(&self) -> Exit {
        self.exit
    }
}

impl fmt::Display for SectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message)
    }
}

impl std::error::Error for SectionError {}

const NO_MARKERS: SectionError = SectionError {
    exit: Exit::Definition,
    message: "sync: build.lua has no generated section; add the lines \
              \"-- cargo-buildl: begin generated\" and \"-- cargo-buildl: end generated\" \
              where it goes",
};

const MISPLACED_MARKERS: SectionError = SectionError {
    exit: Exit::Definition,
    message: "sync: build.lua must hold one begin line and, after it, one end line",
};

/// The byte range of the lines between the begin marker and the end marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Span {
    /// Where the line after the begin marker starts.
    start: usize,
    /// Where the end marker line starts.
    end: usize,
}

impl Span {
    /// The lines of `text` the span covers, each with its own line ending.
    ///
    /// `text` must be the text [`find`] searched.
    pub(crate) fn lines(self, text: &str) -> &str {
        &text[self.start..self.end]
    }
}

/// Finds the lines between the one begin marker and the one end marker that follows it.
///
/// A begin marker is a line starting with `-- cargo-buildl: begin generated` and an end marker
/// one starting with `-- cargo-buildl: end generated`; a `\r` before the line's end is ignored.
/// No markers at all, more than one of either, one without the other, and the end before the
/// begin are each an error, ending with [`Exit::Definition`].
pub(crate) fn find(text: &str) -> Result<Span, SectionError> {
    let mut begins = Vec::new();
    let mut ends = Vec::new();
    let mut offset = 0;
    for raw in text.split_inclusive('\n') {
        let line = raw.strip_suffix('\n').unwrap_or(raw);
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.starts_with(BEGIN_PREFIX) {
            begins.push(offset + raw.len());
        } else if line.starts_with(END_PREFIX) {
            ends.push(offset);
        }
        offset += raw.len();
    }
    match (begins.as_slice(), ends.as_slice()) {
        ([], []) => Err(NO_MARKERS),
        (&[start], &[end]) if start <= end => Ok(Span { start, end }),
        _ => Err(MISPLACED_MARKERS),
    }
}

/// What [`replace`] made of the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Replacement {
    /// The whole file, with the new section between the markers.
    pub(crate) text: String,
    /// Whether the section differed from what the file held.
    pub(crate) changed: bool,
}

/// Puts `section` between the markers of `text`, keeping both marker lines and every byte
/// outside them.
///
/// Each line of `section` is written with a `\n` ending. `changed` is false when those lines
/// are the ones already there, byte for byte. A file whose markers [`find`] refuses is refused
/// the same way.
pub(crate) fn replace(text: &str, section: &str) -> Result<Replacement, SectionError> {
    let span = find(text)?;
    let mut lines = String::with_capacity(section.len() + 1);
    for line in section.lines() {
        lines.push_str(line);
        lines.push('\n');
    }
    let changed = span.lines(text) != lines;
    let mut out = String::with_capacity(text.len() - (span.end - span.start) + lines.len());
    out.push_str(&text[..span.start]);
    out.push_str(&lines);
    out.push_str(&text[span.end..]);
    Ok(Replacement { text: out, changed })
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use super::*;

    const NO_MARKERS_TEXT: &str = "sync: build.lua has no generated section; add the lines \
        \"-- cargo-buildl: begin generated\" and \"-- cargo-buildl: end generated\" where it goes";
    const MISPLACED_TEXT: &str =
        "sync: build.lua must hold one begin line and, after it, one end line";

    fn file(before: &str, body: &str, after: &str) -> String {
        format!("{before}{BEGIN_LINE}\n{body}{END_LINE}\n{after}")
    }

    #[test]
    fn replace_keeps_every_byte_outside_the_markers() {
        let before = "-- mine\r\nlocal x = 1\r\n\r\n";
        let after = "\r\n-- also mine\r\nlocal y = 2";
        let text = file(before, "old\n", after);

        let replaced = replace(&text, "new one\nnew two\n").unwrap();

        assert!(replaced.changed);
        assert_eq!(
            replaced.text,
            file(before, "new one\nnew two\n", after),
            "text outside the markers, CRLF included, is unchanged"
        );
    }

    #[test]
    fn replace_keeps_marker_lines_as_written() {
        let text = "-- cargo-buildl: begin generated by hand\r\nold\r\n-- cargo-buildl: end generated here\r\ntail";

        let replaced = replace(text, "new").unwrap();

        assert_eq!(
            replaced.text,
            "-- cargo-buildl: begin generated by hand\r\nnew\n-- cargo-buildl: end generated here\r\ntail"
        );
    }

    #[test]
    fn free_text_after_the_marker_prefix_is_a_marker() {
        let text = "a\n-- cargo-buildl: begin generated whatever you like\nbody\n-- cargo-buildl: end generated, thanks\nb\n";

        let span = find(text).unwrap();

        assert_eq!(span.lines(text), "body\n");
    }

    #[test]
    fn crlf_markers_are_markers() {
        let text = "-- cargo-buildl: begin generated\r\nbody\r\n-- cargo-buildl: end generated\r\n";

        assert_eq!(find(text).unwrap().lines(text), "body\r\n");
    }

    #[test]
    fn an_empty_section_sits_between_adjacent_markers() {
        let text = file("", "", "");

        let replaced = replace(&text, "a\n").unwrap();

        assert!(replaced.changed);
        assert_eq!(replaced.text, file("", "a\n", ""));
    }

    #[test]
    fn no_markers_is_refused() {
        let error = find("local x = 1\n").unwrap_err();

        assert_eq!(error.to_string(), NO_MARKERS_TEXT);
        assert_eq!(error.exit(), Exit::Definition);
    }

    #[test]
    fn two_begins_is_refused() {
        let text = format!("{BEGIN_LINE}\n{BEGIN_LINE}\n{END_LINE}\n");

        assert_eq!(find(&text).unwrap_err().to_string(), MISPLACED_TEXT);
    }

    #[test]
    fn two_ends_is_refused() {
        let text = format!("{BEGIN_LINE}\n{END_LINE}\n{END_LINE}\n");

        assert_eq!(find(&text).unwrap_err().to_string(), MISPLACED_TEXT);
    }

    #[test]
    fn a_lone_begin_is_refused() {
        let text = format!("{BEGIN_LINE}\nbody\n");

        assert_eq!(find(&text).unwrap_err().to_string(), MISPLACED_TEXT);
    }

    #[test]
    fn a_lone_end_is_refused() {
        let text = format!("body\n{END_LINE}\n");

        assert_eq!(find(&text).unwrap_err().to_string(), MISPLACED_TEXT);
    }

    #[test]
    fn end_before_begin_is_refused() {
        let text = format!("{END_LINE}\n{BEGIN_LINE}\n");

        assert_eq!(find(&text).unwrap_err().to_string(), MISPLACED_TEXT);
    }

    #[test]
    fn same_section_is_up_to_date() {
        let text = file(
            "-- mine\n",
            "local rust = buildl.use(\"rust\")\n",
            "-- tail\n",
        );

        let replaced = replace(&text, "local rust = buildl.use(\"rust\")\n").unwrap();

        assert!(!replaced.changed);
        assert_eq!(replaced.text, text);
    }
}
