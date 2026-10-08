//! The values that cross the build-file evaluation port: the file asked for, and what came back.
//!
//! Its own file because these are the port's vocabulary rather than Load's output. What comes back
//! is staged — every textual value still exactly as the build file wrote it — so that every
//! validation and every decision about it happens on this side of the port.
//!
//! Responsibilities: [`BuildFile`], [`Evaluated`], [`StagedFile`], [`StagedSubdir`],
//! [`StagedDeclaration`], [`StagedItem`] and its four kinds, and [`DeclarationOrder`].
//!
//! Non-responsibilities: validation. A staged value is recorded, not checked; Load turns it into a
//! [`Declaration`](crate::types::Declaration).

use core::fmt;
use std::path::Path;

use crate::types::{
    Argument, Description, Directory, EnvName, Freshness, Label, NetworkAccess, OutputName,
    Provenance, SettingName, SettingValue, SourcePath, TargetName, TargetRole, Written,
};

/// The position of a call among the calls one build file made, counting from zero.
///
/// It locates an error inside its file. It is not part of any declaration: a `pairs` loop may
/// issue the same calls in a different order from one evaluation to the next.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeclarationOrder(u32);

impl DeclarationOrder {
    /// Wraps a call's index.
    #[must_use]
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    /// The index.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl fmt::Display for DeclarationOrder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}

/// One build file to evaluate: its workspace-relative path and the directory it is evaluated in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildFile(Provenance);

impl BuildFile {
    /// Names the build file `provenance` describes.
    #[must_use]
    pub const fn new(provenance: Provenance) -> Self {
        Self(provenance)
    }

    /// The file and directory, as every declaration it makes will carry them.
    #[must_use]
    pub const fn provenance(&self) -> &Provenance {
        &self.0
    }

    /// The directory the file is evaluated in.
    #[must_use]
    pub const fn directory(&self) -> &Directory {
        self.0.directory()
    }

    /// The file's workspace-relative path.
    #[must_use]
    pub fn file(&self) -> &Path {
        self.0.file()
    }
}

/// The outcome of evaluating one build file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Evaluated {
    /// The file exists and was evaluated.
    Staged(StagedFile),
    /// No build file exists at that path.
    Absent,
}

/// Everything one build file declared and requested, exactly as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedFile {
    /// Its declarations, in call order.
    pub declarations: Vec<StagedDeclaration>,
    /// Its `subdir` requests, in call order.
    pub subdirs: Vec<StagedSubdir>,
}

/// One `subdir` request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedSubdir {
    /// The directory, relative to the requesting file's directory.
    pub path: Written<Directory>,
    /// The request's position among the file's calls.
    pub order: DeclarationOrder,
}

/// One declaration call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedDeclaration {
    /// The call's position among the file's calls.
    pub order: DeclarationOrder,
    /// What the call declared.
    pub item: StagedItem,
}

/// The four kinds of declaration call, each exactly as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StagedItem {
    /// `b.target` or `b.test`.
    Target(StagedTarget),
    /// `b.rule`.
    Rule(StagedRule),
    /// `b.alias`.
    Alias(StagedAlias),
    /// `b.option`.
    Setting(StagedSetting),
}

/// A `b.target` or `b.test` call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedTarget {
    /// The target's name, never a label: a declaration stays in its own directory.
    pub name: Written<TargetName>,
    /// Whether it was declared with `b.target` or `b.test`.
    pub role: TargetRole,
    /// The `rule` field, if given.
    pub rule: Option<Written<Label>>,
    /// The `run` field, if given.
    pub run: Option<Vec<Written<Argument>>>,
    /// The `inputs` field, relative to the declaring directory.
    pub inputs: Vec<Written<SourcePath>>,
    /// The `deps` field.
    pub deps: Vec<Written<Label>>,
    /// The `outputs` field, if given.
    pub outputs: Option<Vec<Written<OutputName>>>,
    /// The `env` field.
    pub env: Vec<Written<EnvName>>,
    /// The `network` field.
    pub network: NetworkAccess,
    /// The `always` field.
    pub freshness: Freshness,
}

/// A `b.rule` call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedRule {
    /// The rule's name.
    pub name: Written<TargetName>,
    /// The `run` field.
    pub run: Vec<Written<Argument>>,
    /// The `desc` field, if given.
    pub description: Option<Written<Description>>,
}

/// A `b.alias` call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedAlias {
    /// The alias's name.
    pub name: Written<TargetName>,
    /// The target it names.
    pub target: Written<Label>,
}

/// A `b.option` call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedSetting {
    /// The setting's name.
    pub name: Written<SettingName>,
    /// The `default` field.
    pub default: Written<SettingValue>,
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::path::{Path, PathBuf};

    use super::{BuildFile, DeclarationOrder};
    use crate::types::{Directory, Provenance};

    #[test]
    fn declaration_order_renders_and_orders_by_index() {
        assert_eq!(DeclarationOrder::new(3).to_string(), "#3");
        assert_eq!(DeclarationOrder::new(3).get(), 3);
        assert!(DeclarationOrder::new(2) < DeclarationOrder::new(10));
    }

    #[test]
    fn build_file_exposes_its_provenance() {
        let lib = Directory::parse("lib").unwrap();
        let provenance = Provenance::new(PathBuf::from("lib/build.lua"), lib.clone());
        let file = BuildFile::new(provenance.clone());
        assert_eq!(file.provenance(), &provenance);
        assert_eq!(file.directory(), &lib);
        assert_eq!(file.file(), Path::new("lib/build.lua"));
    }
}
