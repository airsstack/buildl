//! The bundle that names one implementation of every port a pipeline uses.
//!
//! Its own file because it is a different kind of trait from the ports themselves: it has no
//! methods, only associated types, and it exists so that a pipeline takes one type parameter
//! instead of one per port.
//!
//! Responsibilities: [`Ports`].
//!
//! Non-responsibilities: choosing the implementations. A composition root does that, by
//! implementing this trait.
//!
//! This file holds only a trait definition, so it carries no logic to unit-test.

use crate::ports::{
    ActionCache, DeclarationSource, Digester, Host, Manifest, StatCache, ToolResolver,
};

/// One implementation of each port, chosen together.
///
/// # Examples
///
/// A bundle for a workspace with no build files, on a machine with nothing in it:
///
/// ```
/// use std::collections::BTreeSet;
///
/// use buildl_core::{
///     ActionCache, BuildFile, CacheRow, Ceiling, DeclarationSource, Digest, Digester, Directory,
///     EnvName, EnvValue, Evaluated, FileStat, Host, Label, Manifest, Platform, Ports, Program,
///     Result, SourcePath, StatCache, StatEntry, ToolResolver, Workspace,
/// };
///
/// /// Every port at once, each answering that there is nothing.
/// struct Nothing;
///
/// impl DeclarationSource for Nothing {
///     fn evaluate(&self, _file: &BuildFile) -> Result<Evaluated> {
///         Ok(Evaluated::Absent)
///     }
/// }
///
/// impl Manifest for Nothing {
///     fn workspace(&self) -> Result<Workspace> {
///         Ok(Workspace {
///             out: Directory::parse("out")?,
///             state: Directory::parse(".buildl")?,
///             ceiling: Ceiling {
///                 run: BTreeSet::new(),
///                 env: BTreeSet::new(),
///                 read: BTreeSet::new(),
///                 write: BTreeSet::new(),
///             },
///         })
///     }
/// }
///
/// impl Host for Nothing {
///     fn platform(&self) -> Platform {
///         Platform::parse("linux/x86_64").unwrap_or_else(|_| unreachable!("a valid platform"))
///     }
///
///     fn env(&self, _name: &EnvName) -> Result<Option<EnvValue>> {
///         Ok(None)
///     }
/// }
///
/// impl Digester for Nothing {
///     fn stat(&self, _path: &SourcePath) -> Result<Option<FileStat>> {
///         Ok(None)
///     }
///
///     fn digest(&self, _paths: &[SourcePath]) -> Result<Vec<Digest>> {
///         Ok(Vec::new())
///     }
/// }
///
/// impl StatCache for Nothing {
///     fn entry(&self, _path: &SourcePath) -> Option<StatEntry> {
///         None
///     }
///
///     fn record(&mut self, _path: SourcePath, _entry: StatEntry) {}
///
///     fn commit(&mut self) -> Result<()> {
///         Ok(())
///     }
/// }
///
/// impl ToolResolver for Nothing {
///     fn fingerprint(&self, _program: &Program) -> Result<Option<Digest>> {
///         Ok(None)
///     }
/// }
///
/// impl ActionCache for Nothing {
///     fn row(&self, _label: &Label) -> Option<&CacheRow> {
///         None
///     }
/// }
///
/// /// A bundle choosing it for every port.
/// struct NothingPorts;
///
/// impl Ports for NothingPorts {
///     type Source = Nothing;
///     type Manifest = Nothing;
///     type Host = Nothing;
///     type Digester = Nothing;
///     type StatCache = Nothing;
///     type Tools = Nothing;
///     type Cache = Nothing;
/// }
/// ```
pub trait Ports {
    /// How build files are evaluated.
    type Source: DeclarationSource;
    /// How the workspace's manifest is read.
    type Manifest: Manifest;
    /// How the machine actions run on is asked about itself.
    type Host: Host;
    /// How a file's metadata and content digest are read.
    type Digester: Digester;
    /// Where the digests of files already hashed are kept.
    type StatCache: StatCache;
    /// How the programs commands name are found.
    type Tools: ToolResolver;
    /// Where the previous build's rows are read from.
    type Cache: ActionCache;
}
