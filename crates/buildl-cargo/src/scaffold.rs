//! The text of the files `cargo buildl init` writes: `buildl.toml`, `.gitignore` and `build.lua`.
//!
//! Each function renders one file from what `init` knows, so the same input always gives the
//! same bytes and none of them touches the disk.

use crate::section;

/// The tag of the `rust` plugin this release of `cargo-buildl` was tested with.
#[cfg_attr(not(test), expect(dead_code, reason = "written by cargo buildl init"))]
pub(crate) const RUST_PLUGIN_TAG: &str = "plugin-rust-v0.2.0";

/// The `.gitignore` lines `init` makes sure are there.
#[cfg_attr(not(test), expect(dead_code, reason = "written by cargo buildl init"))]
const IGNORED: [&str; 2] = ["/out/", "/.buildl/"];

/// The line that opens the part of `build.lua` that belongs to the engineer.
#[cfg_attr(not(test), expect(dead_code, reason = "written by cargo buildl init"))]
const OWN_TARGETS_LINE: &str =
    "-- Your own targets go below. `cargo buildl sync` never changes them.";

/// The text of `buildl.toml` for a workspace called `name`.
///
/// `name` is written as a TOML basic string, with `"` and `\` escaped. The `rust` plugin is
/// pinned to [`RUST_PLUGIN_TAG`].
#[cfg_attr(not(test), expect(dead_code, reason = "written by cargo buildl init"))]
pub(crate) fn buildl_toml(name: &str) -> String {
    let name = name.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        r#"[workspace]
name   = "{name}"
entry  = "build.lua"
out    = "out"
config = 1

[ceiling]
proc.run    = ["sh", "rustc"]
env.read    = ["HOME", "PATH"]
env.unkeyed = ["HOME", "PATH"]
fs.read     = ["."]
fs.write    = ["out", ".buildl"]

[plugins.rust]
github      = "airsstack/buildl"
rev         = "{RUST_PLUGIN_TAG}"
dir         = "plugins/rust"
proc.run    = ["sh", "rustc"]
env.read    = ["HOME", "PATH"]
env.unkeyed = ["HOME", "PATH"]
network     = true
"#
    )
}

/// The text of `.gitignore` once `/out/` and `/.buildl/` are in it.
///
/// `existing` is the file as it is now, or `None` when there is none. A line is added only when
/// no line of the file is exactly that; every byte already there is kept.
#[cfg_attr(not(test), expect(dead_code, reason = "written by cargo buildl init"))]
pub(crate) fn gitignore(existing: Option<&str>) -> String {
    let mut text = existing.unwrap_or_default().to_owned();
    for wanted in IGNORED {
        if text.lines().any(|line| line == wanted) {
            continue;
        }
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(wanted);
        text.push('\n');
    }
    text
}

/// The text of a new `build.lua`: the generated `section` between its markers, then a blank
/// line and a line telling the engineer where their own targets go.
///
/// Each line of `section` is written with a `\n` ending.
#[cfg_attr(not(test), expect(dead_code, reason = "written by cargo buildl init"))]
pub(crate) fn build_lua(section: &str) -> String {
    let mut text = String::with_capacity(section.len() + 256);
    text.push_str(section::BEGIN_LINE);
    text.push('\n');
    for line in section.lines() {
        text.push_str(line);
        text.push('\n');
    }
    text.push_str(section::END_LINE);
    text.push_str("\n\n");
    text.push_str(OWN_TARGETS_LINE);
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use super::*;

    #[test]
    fn buildl_toml_matches_the_spec() {
        let expected = "\
[workspace]
name   = \"demo\"
entry  = \"build.lua\"
out    = \"out\"
config = 1

[ceiling]
proc.run    = [\"sh\", \"rustc\"]
env.read    = [\"HOME\", \"PATH\"]
env.unkeyed = [\"HOME\", \"PATH\"]
fs.read     = [\".\"]
fs.write    = [\"out\", \".buildl\"]

[plugins.rust]
github      = \"airsstack/buildl\"
rev         = \"plugin-rust-v0.2.0\"
dir         = \"plugins/rust\"
proc.run    = [\"sh\", \"rustc\"]
env.read    = [\"HOME\", \"PATH\"]
env.unkeyed = [\"HOME\", \"PATH\"]
network     = true
";

        assert_eq!(buildl_toml("demo"), expected);
        assert_eq!(RUST_PLUGIN_TAG, "plugin-rust-v0.2.0");
    }

    #[test]
    fn a_name_with_a_quote_or_backslash_is_escaped() {
        let toml = buildl_toml(r#"a"b\c"#);

        assert!(
            toml.lines().any(|line| line == r#"name   = "a\"b\\c""#),
            "{toml}"
        );
    }

    #[test]
    fn gitignore_adds_only_missing_lines() {
        assert_eq!(gitignore(None), "/out/\n/.buildl/\n");
        assert_eq!(
            gitignore(Some("target\n/out/\n")),
            "target\n/out/\n/.buildl/\n"
        );
        assert_eq!(
            gitignore(Some("target")),
            "target\n/out/\n/.buildl/\n",
            "a last line without its ending gets one before the new lines"
        );
        let both = "/.buildl/\n/out/\nnode_modules\n";
        assert_eq!(gitignore(Some(both)), both);
        assert_eq!(
            gitignore(Some("out\n/out\n")),
            "out\n/out\n/out/\n/.buildl/\n",
            "only the exact line counts"
        );
    }

    #[test]
    fn build_lua_holds_both_markers() {
        let section = "local rust = buildl.use(\"rust\")\n\nrust.test(\"a-test\", { args = rust.package(\"a\") })\n";

        let text = build_lua(section);

        let span = section::find(&text).unwrap();
        assert_eq!(span.lines(&text), section);
        assert_eq!(
            text,
            format!(
                "{}\n{section}{}\n\n{}\n",
                section::BEGIN_LINE,
                section::END_LINE,
                "-- Your own targets go below. `cargo buildl sync` never changes them."
            )
        );
    }
}
