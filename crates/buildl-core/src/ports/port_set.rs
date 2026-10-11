//! One instance of every port, held together.
//!
//! Its own file because it is the value beside the [`Ports`] trait: the trait names one
//! implementation of each port, and this struct holds one instance of each, so a pipeline is
//! built from one argument and a phase that needs several ports borrows one value.
//!
//! Responsibilities: [`PortSet`], and its `Debug` rendering, which asks only that the seven port
//! types be `Debug`.
//!
//! Non-responsibilities: building the instances. A composition root does that.

use std::fmt;

use crate::ports::Ports;

/// One instance of each port a bundle names.
///
/// The fields are public: a set has no invariant of its own, and whoever built the adapters may
/// reach them again between two commands.
pub struct PortSet<P: Ports> {
    /// Evaluates build files.
    pub source: P::Source,
    /// Reads the workspace's manifest.
    pub manifest: P::Manifest,
    /// Answers for the machine actions run on.
    pub host: P::Host,
    /// Reads a file's metadata and content digest.
    pub digester: P::Digester,
    /// Keeps the digests of files already hashed.
    pub stat_cache: P::StatCache,
    /// Finds the programs commands name.
    pub tools: P::Tools,
    /// Holds the previous build's rows.
    pub cache: P::Cache,
}

/// Written by hand because a derive would require the bundle type itself to be `Debug`, and a
/// bundle is only a choice of port types. This asks only that the seven port types be `Debug`.
impl<P: Ports> fmt::Debug for PortSet<P>
where
    P::Source: fmt::Debug,
    P::Manifest: fmt::Debug,
    P::Host: fmt::Debug,
    P::Digester: fmt::Debug,
    P::StatCache: fmt::Debug,
    P::Tools: fmt::Debug,
    P::Cache: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PortSet")
            .field("source", &self.source)
            .field("manifest", &self.manifest)
            .field("host", &self.host)
            .field("digester", &self.digester)
            .field("stat_cache", &self.stat_cache)
            .field("tools", &self.tools)
            .field("cache", &self.cache)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeSet;

    use super::PortSet;
    use crate::error::Result;
    use crate::ports::{
        ActionCache, DeclarationSource, Digester, Host, Manifest, Ports, StatCache, ToolResolver,
    };
    use crate::types::{
        BuildFile, CacheRow, Ceiling, Digest, Directory, EnvName, EnvValue, Evaluated, FileStat,
        Label, Platform, Program, SourcePath, StatEntry, Workspace,
    };

    /// One port type answering that there is nothing, standing in for all seven ports.
    #[derive(Debug)]
    struct Stub;

    impl DeclarationSource for Stub {
        fn evaluate(&self, _file: &BuildFile) -> Result<Evaluated> {
            Ok(Evaluated::Absent)
        }
    }

    impl Manifest for Stub {
        fn workspace(&self) -> Result<Workspace> {
            Ok(Workspace {
                out: Directory::parse("out")?,
                state: Directory::parse(".buildl")?,
                ceiling: Ceiling {
                    run: BTreeSet::new(),
                    env: BTreeSet::new(),
                    read: BTreeSet::new(),
                    write: BTreeSet::new(),
                },
            })
        }
    }

    impl Host for Stub {
        fn platform(&self) -> Platform {
            Platform::parse("linux/x86_64").unwrap()
        }

        fn env(&self, _name: &EnvName) -> Result<Option<EnvValue>> {
            Ok(None)
        }
    }

    impl Digester for Stub {
        fn stat(&self, _path: &SourcePath) -> Result<Option<FileStat>> {
            Ok(None)
        }

        fn digest(&self, _paths: &[SourcePath]) -> Result<Vec<Digest>> {
            Ok(Vec::new())
        }
    }

    impl StatCache for Stub {
        fn entry(&self, _path: &SourcePath) -> Option<StatEntry> {
            None
        }

        fn record(&mut self, _path: SourcePath, _entry: StatEntry) {}

        fn commit(&mut self) -> Result<()> {
            Ok(())
        }
    }

    impl ToolResolver for Stub {
        fn fingerprint(&self, _program: &Program) -> Result<Option<Digest>> {
            Ok(None)
        }
    }

    impl ActionCache for Stub {
        fn row(&self, _label: &Label) -> Option<&CacheRow> {
            None
        }
    }

    /// A bundle choosing [`Stub`] for every port. It deliberately does not implement `Debug`:
    /// a bundle is only a choice of port types, so rendering a set must not require it.
    struct UndebuggableBundle;

    impl Ports for UndebuggableBundle {
        type Source = Stub;
        type Manifest = Stub;
        type Host = Stub;
        type Digester = Stub;
        type StatCache = Stub;
        type Tools = Stub;
        type Cache = Stub;
    }

    #[test]
    fn debug_renders_every_port_without_the_bundle_being_debug() {
        let set = PortSet::<UndebuggableBundle> {
            source: Stub,
            manifest: Stub,
            host: Stub,
            digester: Stub,
            stat_cache: Stub,
            tools: Stub,
            cache: Stub,
        };

        let text = format!("{set:?}");

        assert!(text.starts_with("PortSet"), "unexpected rendering: {text}");
        for field in [
            "source",
            "manifest",
            "host",
            "digester",
            "stat_cache",
            "tools",
            "cache",
        ] {
            assert!(
                text.contains(&format!("{field}: Stub")),
                "field `{field}` missing from: {text}"
            );
        }
    }
}
