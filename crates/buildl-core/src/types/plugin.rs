//! Plugins as values: who a plugin is, what its manifest asks for, what the workspace lets it
//! have, and whose code declared a target.
//!
//! Its own file because the names and the chain are parsed once, where a plugin is read, and
//! every later phase reads them as given: the graph carries an [`Origin`] on each target, and the
//! plan holds a plugin's targets to the caps recorded here.
//!
//! Responsibilities: [`PluginName`], [`PluginVersion`], [`PluginRef`], [`PluginCap`],
//! [`PluginNetwork`], [`PluginChain`], [`Origin`] and [`PluginViolation`].
//!
//! Non-responsibilities: reading a plugin or a manifest, which an adapter does; and the check
//! that a want is inside a plugin's caps, which needs the graph and belongs to the phase that
//! plans.

use core::fmt;
use core::str::FromStr;
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::error::{DeclarationSite, Error, NameKind, Result, allowed_list, directory_text};
use crate::types::grammar;
use crate::types::{EnvName, Program, Want};

/// A plugin's name, as a workspace's `[plugins.<name>]` table writes it, such as `go`.
///
/// Valid names follow the same grammar as a [`TargetName`](crate::TargetName).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct PluginName(String);

impl PluginName {
    /// Validates `raw` and wraps it.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidName`] when `raw` is empty, exceeds 256 bytes, is `.` or `..`, or
    /// holds a character outside ASCII alphanumerics, `.`, `-` and `_`.
    pub fn parse(raw: impl Into<String>) -> Result<Self> {
        let raw = raw.into();
        match grammar::name(&raw) {
            Ok(()) => Ok(Self(raw)),
            Err(reason) => Err(Error::InvalidName {
                kind: NameKind::PluginName,
                value: raw,
                reason,
            }),
        }
    }

    /// The name as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PluginName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for PluginName {
    type Err = Error;
    fn from_str(raw: &str) -> Result<Self> {
        Self::parse(raw)
    }
}

impl From<PluginName> for String {
    fn from(value: PluginName) -> Self {
        value.0
    }
}

impl TryFrom<String> for PluginName {
    type Error = Error;
    fn try_from(raw: String) -> Result<Self> {
        Self::parse(raw)
    }
}

/// The version text a plugin's manifest states, such as `1.2.0`.
///
/// Any non-empty text without a control character. It is shown, never compared or ordered as a
/// version: the manifest states it as free text.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct PluginVersion(String);

impl PluginVersion {
    /// Validates `raw` and wraps it.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidName`] when `raw` is empty or holds a control character.
    pub fn parse(raw: impl Into<String>) -> Result<Self> {
        let raw = raw.into();
        let reason = if raw.is_empty() {
            Some("must not be empty")
        } else if raw.chars().any(char::is_control) {
            Some("must not hold a control character")
        } else {
            None
        };
        match reason {
            None => Ok(Self(raw)),
            Some(reason) => Err(Error::InvalidName {
                kind: NameKind::PluginVersion,
                value: raw,
                reason,
            }),
        }
    }

    /// The version text as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PluginVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<PluginVersion> for String {
    fn from(value: PluginVersion) -> Self {
        value.0
    }
}

impl TryFrom<String> for PluginVersion {
    type Error = Error;
    fn try_from(raw: String) -> Result<Self> {
        Self::parse(raw)
    }
}

/// Whether a plugin's targets may reach the network.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub enum PluginNetwork {
    /// The default: no target of the plugin may declare the network.
    #[default]
    Sealed,
    /// The workspace lets the plugin's targets declare the network.
    Allowed,
}

/// What a workspace lets one plugin have: the `[plugins.<name>]` table.
///
/// Empty unless the workspace says otherwise: the default cap allows nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PluginCap {
    /// The programs the plugin's targets may run.
    pub run: BTreeSet<Program>,
    /// The environment variables the plugin's targets may read.
    pub env: BTreeSet<EnvName>,
    /// Whether the plugin's targets may reach the network.
    pub network: PluginNetwork,
}

/// One plugin in a [`PluginChain`]: who it is and what its own manifest asks for.
///
/// Displays as `<name>@<version>`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PluginRef {
    /// The plugin's name.
    pub name: PluginName,
    /// The version its manifest states.
    pub version: PluginVersion,
    /// The programs its manifest says its targets may run.
    pub run: BTreeSet<Program>,
    /// The environment variables its manifest says its targets may read.
    pub env: BTreeSet<EnvName>,
}

impl fmt::Display for PluginRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.name, self.version)
    }
}

/// The plugins that made a declaration, outermost first: the one a build file used, then each
/// plugin that one used, down to the one that made the declaration.
///
/// Never empty: [`PluginChain::new`] and reading one from JSON both refuse an empty list. Written
/// as the plain list of its plugins.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "Vec<PluginRef>", into = "Vec<PluginRef>")]
pub struct PluginChain(Vec<PluginRef>);

impl PluginChain {
    /// Wraps `plugins`, outermost first.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidName`] when `plugins` is empty.
    pub fn new(plugins: Vec<PluginRef>) -> Result<Self> {
        if plugins.is_empty() {
            return Err(Error::InvalidName {
                kind: NameKind::PluginChain,
                value: String::new(),
                reason: "must name at least one plugin",
            });
        }
        Ok(Self(plugins))
    }

    /// The plugins, outermost first. Never empty.
    #[must_use]
    pub fn as_slice(&self) -> &[PluginRef] {
        &self.0
    }
}

impl From<PluginChain> for Vec<PluginRef> {
    fn from(value: PluginChain) -> Self {
        value.0
    }
}

impl TryFrom<Vec<PluginRef>> for PluginChain {
    type Error = Error;
    fn try_from(plugins: Vec<PluginRef>) -> Result<Self> {
        Self::new(plugins)
    }
}

/// Whose code declared a target.
///
/// Shown and checked, never part of an action key: the same declaration built the same way has
/// the same key whoever wrote it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Origin {
    /// A build file the workspace itself wrote.
    BuildFile,
    /// Plugin code: the chain of plugins from the one a build file used to the one that made the
    /// declaration.
    Plugins(PluginChain),
}

impl Origin {
    /// The origin of a declaration made by the last plugin of `chain`, reached through the ones
    /// before it.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidName`] when `chain` is empty.
    pub fn plugins(chain: Vec<PluginRef>) -> Result<Self> {
        PluginChain::new(chain).map(Self::Plugins)
    }
}

/// `build file`, or a plugin chain outermost first, such as `ourgo@0.1.0 > go@1.2.0`.
impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BuildFile => f.write_str("build file"),
            Self::Plugins(chain) => {
                for (index, plugin) in chain.as_slice().iter().enumerate() {
                    if index > 0 {
                        f.write_str(" > ")?;
                    }
                    write!(f, "{plugin}")?;
                }
                Ok(())
            }
        }
    }
}

/// A want of a plugin's target that one plugin of its chain does not allow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginViolation {
    /// What the target wanted.
    pub want: Want,
    /// The target that wanted it.
    pub site: DeclarationSite,
    /// The plugin that does not allow it.
    pub plugin: PluginRef,
    /// What that plugin allows of the same kind: its manifest's list and its cap's list, both.
    pub allowed: BTreeSet<String>,
}

/// What the violation is, without where it was made: the want, the plugin that does not allow
/// it, and what that plugin allows of that kind.
///
/// Reads as `program curl is outside plugin go@1.3.0 (proc.run allows: go)`; the list is
/// `nothing` when empty. A network want reads as
/// `network is outside plugin go@1.3.0 (network not allowed)`.
impl fmt::Display for PluginViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (wanted, list) = match &self.want {
            Want::Program(program) => (format!("program {program}"), "proc.run"),
            Want::Env(name) => (format!("environment variable {name}"), "env.read"),
            Want::Read(path) => (format!("input {path}"), "fs.read"),
            Want::Write(directory) => (
                format!("directory {}", directory_text(directory)),
                "fs.write",
            ),
            Want::Network(_) => {
                return write!(
                    f,
                    "network is outside plugin {} (network not allowed)",
                    self.plugin
                );
            }
        };
        write!(
            f,
            "{wanted} is outside plugin {} ({list} allows: {})",
            self.plugin,
            allowed_list(&self.allowed)
        )
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeSet;
    use std::path::PathBuf;

    use super::{
        Origin, PluginChain, PluginName, PluginNetwork, PluginRef, PluginVersion, PluginViolation,
    };
    use crate::error::DeclarationSite;
    use crate::types::{Directory, EnvName, Label, Program, Provenance, SourcePath, Want};

    fn plugin(name: &str, version: &str) -> PluginRef {
        PluginRef {
            name: PluginName::parse(name).unwrap(),
            version: PluginVersion::parse(version).unwrap(),
            run: BTreeSet::new(),
            env: BTreeSet::new(),
        }
    }

    #[test]
    fn plugin_name_accepts_a_target_name_and_refuses_a_slash() {
        assert_eq!(PluginName::parse("go").unwrap().as_str(), "go");
        assert!(PluginName::parse("a/b").is_err());
        assert!(PluginName::parse("").is_err());
    }

    #[test]
    fn plugin_version_refuses_empty_and_control_characters() {
        assert_eq!(PluginVersion::parse("1.2.0").unwrap().as_str(), "1.2.0");
        assert!(PluginVersion::parse("2024-rc 1").is_ok());
        assert!(PluginVersion::parse("").is_err());
        assert!(PluginVersion::parse("1.\n2").is_err());
        assert!(PluginVersion::parse("1.2\0").is_err());
    }

    #[test]
    fn plugin_ref_displays_name_at_version() {
        assert_eq!(plugin("go", "1.2.0").to_string(), "go@1.2.0");
    }

    #[test]
    fn origin_refuses_an_empty_chain() {
        assert!(Origin::plugins(Vec::new()).is_err());
        assert!(Origin::plugins(vec![plugin("go", "1.2.0")]).is_ok());
    }

    #[test]
    fn a_chain_refuses_to_be_empty() {
        assert!(PluginChain::new(Vec::new()).is_err());
        let chain = PluginChain::new(vec![plugin("go", "1.2.0")]).unwrap();
        assert_eq!(chain.as_slice(), [plugin("go", "1.2.0")]);
    }

    #[test]
    fn an_empty_chain_is_refused_when_read_from_json() {
        let err = serde_json::from_str::<Origin>(r#"{"Plugins":[]}"#).unwrap_err();
        assert!(err.to_string().contains("plugin chain"), "{err}");
        assert!(serde_json::from_str::<PluginChain>("[]").is_err());
    }

    #[test]
    fn a_chain_is_written_as_the_plain_list_of_its_plugins() {
        let origin = Origin::plugins(vec![plugin("go", "1.2.0")]).unwrap();
        let json = serde_json::to_string(&origin).unwrap();
        assert!(json.starts_with(r#"{"Plugins":[{"name":"go""#), "{json}");
        assert_eq!(serde_json::from_str::<Origin>(&json).unwrap(), origin);
    }

    #[test]
    fn origin_displays_a_chain_outermost_first() {
        let chain = Origin::plugins(vec![plugin("ourgo", "0.1.0"), plugin("go", "1.2.0")]).unwrap();
        assert_eq!(chain.to_string(), "ourgo@0.1.0 > go@1.2.0");
        assert_eq!(Origin::BuildFile.to_string(), "build file");
    }

    /// A violation of `go@1.3.0` by `//:app` of `want`, with `allowed` listed.
    fn violation(want: Want, allowed: &[&str]) -> PluginViolation {
        let go = plugin("go", "1.3.0");
        PluginViolation {
            want,
            site: DeclarationSite {
                label: Label::parse("//:app").unwrap(),
                provenance: Provenance::new(PathBuf::from("build.lua"), Directory::root()),
                origin: Origin::plugins(vec![go.clone()]).unwrap(),
            },
            plugin: go,
            allowed: allowed.iter().map(|text| (*text).to_owned()).collect(),
        }
    }

    #[test]
    fn a_violation_reads_as_the_want_the_plugin_and_what_it_allows() {
        let said = |want, allowed: &[&str]| violation(want, allowed).to_string();
        assert_eq!(
            said(Want::Program(Program::parse("curl").unwrap()), &["go"]),
            "program curl is outside plugin go@1.3.0 (proc.run allows: go)"
        );
        assert_eq!(
            said(Want::Env(EnvName::parse("HOME").unwrap()), &[]),
            "environment variable HOME is outside plugin go@1.3.0 (env.read allows: nothing)"
        );
        assert_eq!(
            said(
                Want::Read(SourcePath::parse("main.c").unwrap()),
                &["src", "inc"]
            ),
            "input main.c is outside plugin go@1.3.0 (fs.read allows: inc, src)"
        );
        assert_eq!(
            said(Want::Write(Directory::root()), &[]),
            "directory . is outside plugin go@1.3.0 (fs.write allows: nothing)"
        );
        assert_eq!(
            said(Want::Network(Label::parse("//:app").unwrap()), &[]),
            "network is outside plugin go@1.3.0 (network not allowed)"
        );
    }

    #[test]
    fn plugin_network_defaults_to_sealed() {
        assert_eq!(PluginNetwork::default(), PluginNetwork::Sealed);
    }
}
