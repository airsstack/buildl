//! The workspace-relative path one output of one target lives at.
//!
//! Its own file because an output's name says only what a target calls it. Where it lives is a
//! second value, decided from three others: the directory the workspace puts outputs in, the
//! directory of the target's label, and the name. Giving that its own type keeps a path nobody
//! computed from being handed to an executor.
//!
//! Responsibilities: [`OutputPath`], its one constructor, and its rendering.
//!
//! Non-responsibilities: whether two targets were given the same path, which needs the whole
//! graph; and the filesystem. Nothing here asks whether the file exists.

use core::fmt;

use serde::Serialize;

use crate::types::{Directory, Label, OutputName};

/// The workspace-relative path one output of one target lives at, such as `out/lib/text`.
///
/// Built only by [`OutputPath::of`], so every path held is one the layout rule produced. There
/// is no `parse` and no `Deserialize`: nothing reads one back.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct OutputPath(String);

impl OutputPath {
    /// The path of the output `name` of the target `label`, in a workspace whose outputs go
    /// under `out`.
    ///
    /// The three texts are joined by `/`: the output directory, the directory of the label, and
    /// the name. A root directory contributes nothing, so a target declared at the workspace
    /// root puts its outputs directly under `out`.
    #[must_use]
    pub fn of(out: &Directory, label: &Label, name: &OutputName) -> Self {
        let segments = [out.as_str(), label.directory().as_str(), name.as_str()];
        let present: Vec<&str> = segments
            .into_iter()
            .filter(|segment| !segment.is_empty())
            .collect();
        Self(present.join("/"))
    }

    /// The path as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OutputPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use super::OutputPath;
    use crate::types::{Directory, Label, OutputName};

    fn path(out: &str, label: &str, name: &str) -> OutputPath {
        OutputPath::of(
            &Directory::parse(out).unwrap(),
            &Label::parse(label).unwrap(),
            &OutputName::parse(name).unwrap(),
        )
    }

    #[test]
    fn joins_the_output_directory_the_labels_directory_and_the_name() {
        assert_eq!(path("out", "//lib:text", "text").as_str(), "out/lib/text");
        assert_eq!(path("out", "//:z", "bin/z").as_str(), "out/bin/z");
        assert_eq!(
            path("build/out", "//lib/text:fold", "fold.o").as_str(),
            "build/out/lib/text/fold.o"
        );
    }

    #[test]
    fn a_root_directory_contributes_nothing() {
        assert_eq!(path("out", "//:app", "app").as_str(), "out/app");
        assert_eq!(path("", "//lib:text", "text").as_str(), "lib/text");
        assert_eq!(path("", "//:app", "app").as_str(), "app");
    }

    #[test]
    fn displays_as_its_text() {
        assert_eq!(
            path("out", "//lib:text", "text").to_string(),
            "out/lib/text"
        );
    }

    #[test]
    fn orders_as_its_text_does() {
        let mut paths = [
            path("out", "//lib:text", "text"),
            path("out", "//:lib2", "lib2"),
            path("out", "//:app", "app"),
            path("out", "//:lib", "lib"),
        ];
        paths.sort();
        let sorted: Vec<&str> = paths.iter().map(OutputPath::as_str).collect();
        assert_eq!(sorted, ["out/app", "out/lib", "out/lib/text", "out/lib2"]);
    }

    #[test]
    fn two_targets_can_be_given_the_same_path() {
        assert_eq!(
            path("out", "//:lib", "text"),
            path("out", "//:other", "text")
        );
        assert_ne!(
            path("out", "//:lib", "text"),
            path("out", "//lib:text", "text")
        );
    }

    #[test]
    fn serializes_as_its_text() {
        let json = serde_json::to_string(&path("out", "//lib:text", "text")).unwrap();
        assert_eq!(json, r#""out/lib/text""#);
    }
}
