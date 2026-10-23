//! Authority as values: what a workspace allows, and what a build wants.
//!
//! Its own file because nothing in a build is ambient: what may run, which variables may be
//! read and which directories may be touched are written down once for a workspace, what a
//! build needs is derived from its targets, and the two are compared before anything runs.
//!
//! Responsibilities: [`Ceiling`], [`WantedSet`] and [`Want`].
//!
//! Non-responsibilities: the comparison. Deciding whether a want is inside a ceiling needs both
//! values and the graph, so it belongs to the phase that plans; nothing here grants or refuses.
//! A [`WantedSet`] only answers questions about itself and another set of its own kind.

use core::fmt;
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::types::{Directory, EnvName, Label, Program, SourcePath};

/// The most a workspace allows any build to do.
///
/// Four lists, each empty unless the workspace says otherwise: an empty ceiling allows nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ceiling {
    /// The programs a command may run.
    pub run: BTreeSet<Program>,
    /// The environment variables a target may read.
    pub env: BTreeSet<EnvName>,
    /// The directories a target's inputs may lie in.
    pub read: BTreeSet<Directory>,
    /// The directories a build may write into.
    pub write: BTreeSet<Directory>,
}

/// What one build wants, derived from its targets and its workspace.
///
/// The files a build reads are not copied here: they are each target's declared inputs, and stay
/// in the graph.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WantedSet {
    /// Every program a command names.
    pub programs: BTreeSet<Program>,
    /// Every environment variable a target reads.
    pub env: BTreeSet<EnvName>,
    /// Every directory the build writes into.
    pub write: BTreeSet<Directory>,
    /// Every target that declared it reaches the network.
    pub network: BTreeSet<Label>,
}

impl WantedSet {
    /// Whether every member of `other` is also a member of `self`.
    ///
    /// An empty set is covered by any set, itself included.
    #[must_use]
    pub fn covers(&self, other: &Self) -> bool {
        other.programs.is_subset(&self.programs)
            && other.env.is_subset(&self.env)
            && other.write.is_subset(&self.write)
            && other.network.is_subset(&self.network)
    }

    /// The members of `self` that `other` does not hold.
    #[must_use]
    pub fn minus(&self, other: &Self) -> Self {
        Self {
            programs: self.programs.difference(&other.programs).cloned().collect(),
            env: self.env.difference(&other.env).cloned().collect(),
            write: self.write.difference(&other.write).cloned().collect(),
            network: self.network.difference(&other.network).cloned().collect(),
        }
    }

    /// The members of either set.
    #[must_use]
    pub fn union(&self, other: &Self) -> Self {
        Self {
            programs: self.programs.union(&other.programs).cloned().collect(),
            env: self.env.union(&other.env).cloned().collect(),
            write: self.write.union(&other.write).cloned().collect(),
            network: self.network.union(&other.network).cloned().collect(),
        }
    }

    /// Whether the set holds no member of any kind.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.programs.is_empty()
            && self.env.is_empty()
            && self.write.is_empty()
            && self.network.is_empty()
    }

    /// The label and the members of each kind that holds any, in the order they are shown.
    fn lines(&self) -> Vec<(&'static str, String)> {
        fn joined<T: fmt::Display>(members: &BTreeSet<T>) -> String {
            members
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        }
        let mut lines = Vec::new();
        if !self.programs.is_empty() {
            lines.push(("run programs:", joined(&self.programs)));
        }
        if !self.env.is_empty() {
            lines.push(("read variables:", joined(&self.env)));
        }
        if !self.write.is_empty() {
            lines.push(("write into:", joined(&self.write)));
        }
        if !self.network.is_empty() {
            lines.push(("reach the network:", joined(&self.network)));
        }
        lines
    }
}

/// The set as the approval prompt and a refusal show it.
///
/// One line for each kind that holds a member, in the order programs, variables, directories,
/// network. Each line is indented two spaces; its label is padded so the members line up one
/// space past the longest label shown. Members are joined by `, ` in the set's own order. There is
/// no trailing newline, and an empty set renders as nothing.
impl fmt::Display for WantedSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let lines = self.lines();
        let width = lines
            .iter()
            .map(|(label, _)| label.len())
            .max()
            .unwrap_or(0)
            + 1;
        for (index, (label, members)) in lines.iter().enumerate() {
            if index > 0 {
                f.write_str("\n")?;
            }
            write!(f, "  {label:<width$}{members}")?;
        }
        Ok(())
    }
}

/// One thing a build wants that a ceiling is asked about.
///
/// Ordered by kind — program, variable, input, directory, network — and then by value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Want {
    /// To run a program.
    Program(Program),
    /// To read an environment variable.
    Env(EnvName),
    /// To read a workspace file as an input.
    Read(SourcePath),
    /// To write into a directory.
    Write(Directory),
    /// To reach the network, as the target with this label declared.
    Network(Label),
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use super::WantedSet;
    use crate::json::canonical;
    use crate::types::{Directory, EnvName, Label, Program};

    fn wanted(programs: &[&str], env: &[&str], write: &[&str], network: &[&str]) -> WantedSet {
        WantedSet {
            programs: programs
                .iter()
                .map(|p| Program::parse(*p).unwrap())
                .collect(),
            env: env.iter().map(|e| EnvName::parse(*e).unwrap()).collect(),
            write: write
                .iter()
                .map(|w| Directory::parse(*w).unwrap())
                .collect(),
            network: network.iter().map(|n| Label::parse(n).unwrap()).collect(),
        }
    }

    fn empty() -> WantedSet {
        WantedSet::default()
    }

    #[test]
    fn a_set_covers_its_subset() {
        let whole = wanted(&["go", "protoc"], &["HOME"], &["out"], &["//:fetch"]);
        let part = wanted(&["go"], &["HOME"], &[], &["//:fetch"]);
        assert!(whole.covers(&part));
        assert!(whole.covers(&whole));
    }

    #[test]
    fn a_set_does_not_cover_a_larger_set() {
        let whole = wanted(&["go", "protoc"], &["HOME"], &["out"], &[]);
        let part = wanted(&["go"], &[], &[], &[]);
        assert!(!part.covers(&whole));
        let other_kind = wanted(&[], &["go"], &[], &[]);
        assert!(!part.covers(&other_kind));
    }

    #[test]
    fn an_empty_set_is_covered_by_any() {
        let any = wanted(&["go"], &["HOME"], &["out"], &["//:fetch"]);
        assert!(any.covers(&empty()));
        assert!(empty().covers(&empty()));
        assert!(!empty().covers(&any));
    }

    #[test]
    fn minus_leaves_what_the_other_lacks() {
        let whole = wanted(
            &["go", "protoc"],
            &["HOME", "PATH"],
            &["out"],
            &["//:fetch"],
        );
        let known = wanted(&["go"], &["PATH"], &["out"], &[]);
        let left = whole.minus(&known);
        assert_eq!(left, wanted(&["protoc"], &["HOME"], &[], &["//:fetch"]));
        assert!(whole.minus(&whole).is_empty());
    }

    #[test]
    fn union_holds_both() {
        let a = wanted(&["go"], &["HOME"], &[], &["//:a"]);
        let b = wanted(&["go", "protoc"], &[], &["out"], &["//:b"]);
        let both = a.union(&b);
        assert_eq!(
            both,
            wanted(&["go", "protoc"], &["HOME"], &["out"], &["//:a", "//:b"])
        );
        assert!(both.covers(&a));
        assert!(both.covers(&b));
    }

    #[test]
    fn a_set_round_trips_through_canonical_json() {
        let set = wanted(&["go", "protoc"], &["HOME"], &["out"], &["//:fetch"]);
        let bytes = canonical::to_vec(&set).unwrap();
        let back: WantedSet = canonical::from_slice(&bytes).unwrap();
        assert_eq!(back, set);
    }

    #[test]
    fn display_lists_each_non_empty_kind_in_order() {
        let set = wanted(
            &["protoc", "go"],
            &["PATH", "HOME"],
            &["out", ".buildl"],
            &["//:fetch"],
        );
        assert_eq!(
            set.to_string(),
            "  run programs:      go, protoc\n  read variables:    HOME, PATH\n  \
             write into:        .buildl, out\n  reach the network: //:fetch"
        );
    }

    #[test]
    fn display_leaves_out_an_empty_kind() {
        let set = wanted(&["go"], &[], &["out"], &[]);
        assert_eq!(set.to_string(), "  run programs: go\n  write into:   out");
        assert_eq!(empty().to_string(), "");
    }

    #[test]
    fn is_empty_is_true_only_when_every_kind_is_empty() {
        assert!(empty().is_empty());
        assert!(!wanted(&[], &[], &[], &["//:fetch"]).is_empty());
    }
}
