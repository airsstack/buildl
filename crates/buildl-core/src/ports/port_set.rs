//! One instance of every port, held together.
//!
//! Its own file because it is the value beside the [`Ports`] trait: the trait names one
//! implementation of each port, and this struct holds one instance of each, so a pipeline is
//! built from one argument and a phase that needs several ports borrows one value.
//!
//! Responsibilities: [`PortSet`], and its `Debug` rendering, which asks only that the twelve
//! port types be `Debug`.
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
    /// Holds each target's row: the previous build's, then this one's.
    pub cache: P::Cache,
    /// Stores what actions produce, by content.
    pub cas: P::Cas,
    /// Receives the record of how each target ended.
    pub log: P::Log,
    /// Runs one action.
    pub strategy: P::Strategy,
    /// Has several actions run at once.
    pub dispatcher: P::Dispatcher,
    /// Reads the wall clock.
    pub clock: P::Clock,
}

/// Written by hand because a derive would require the bundle type itself to be `Debug`, and a
/// bundle is only a choice of port types. This asks only that the twelve port types be `Debug`.
impl<P: Ports> fmt::Debug for PortSet<P>
where
    P::Source: fmt::Debug,
    P::Manifest: fmt::Debug,
    P::Host: fmt::Debug,
    P::Digester: fmt::Debug,
    P::StatCache: fmt::Debug,
    P::Tools: fmt::Debug,
    P::Cache: fmt::Debug,
    P::Cas: fmt::Debug,
    P::Log: fmt::Debug,
    P::Strategy: fmt::Debug,
    P::Dispatcher: fmt::Debug,
    P::Clock: fmt::Debug,
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
            .field("cas", &self.cas)
            .field("log", &self.log)
            .field("strategy", &self.strategy)
            .field("dispatcher", &self.dispatcher)
            .field("clock", &self.clock)
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
        ActionCache, Clock, ContentStore, DeclarationSource, Digester, Dispatcher, EventLog,
        ExecStrategy, Host, Manifest, Ports, StatCache, ToolResolver,
    };
    use crate::types::{
        BuildFile, CacheRow, Ceiling, Digest, Directory, EnvName, EnvValue, Evaluated, Exit,
        FileMode, FileStat, Found, Label, OutcomeEvent, OutputPath, Platform, Program, RawOutcome,
        ReadyAction, SourcePath, StatEntry, Timestamp, Workspace,
    };

    /// One port type answering that there is nothing, standing in for every port.
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
                execution: crate::types::Execution::default(),
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

        fn record(&mut self, _label: Label, _row: CacheRow) -> Result<()> {
            Ok(())
        }
    }

    impl ContentStore for Stub {
        fn put(&self, bytes: &[u8]) -> Result<Digest> {
            Ok(Digest::of(bytes))
        }

        fn contains(&self, _digest: &Digest) -> bool {
            false
        }

        fn get(&self, _digest: &Digest) -> Result<Option<Vec<u8>>> {
            Ok(None)
        }

        fn clear(&self, _at: &OutputPath) -> Result<()> {
            Ok(())
        }

        fn place(&self, _digest: &Digest, _mode: FileMode, _at: &OutputPath) -> Result<()> {
            Ok(())
        }
    }

    impl EventLog for Stub {
        fn append(&mut self, _event: &OutcomeEvent) -> Result<()> {
            Ok(())
        }
    }

    impl ExecStrategy for Stub {
        type Dir = ();

        fn prepare(&self, _action: &ReadyAction) -> Result<()> {
            Ok(())
        }

        fn run(&self, _dir: &(), _action: &ReadyAction) -> Result<RawOutcome> {
            Ok(RawOutcome {
                exit: Exit::Exited(0),
                stdout: Vec::new(),
                stderr: Vec::new(),
            })
        }

        fn collect<C: ContentStore>(
            &self,
            _dir: &(),
            _action: &ReadyAction,
            _cas: &C,
        ) -> Result<Vec<Found>> {
            Ok(Vec::new())
        }
    }

    impl Dispatcher for Stub {
        fn dispatch<T, R, W, S>(&mut self, _work: W, _step: S) -> Result<()>
        where
            T: Send,
            R: Send,
            W: Fn(T) -> R + Sync,
            S: FnMut(Option<R>) -> Vec<T>,
        {
            Ok(())
        }
    }

    impl Clock for Stub {
        fn now(&self) -> Timestamp {
            Timestamp::UNIX_EPOCH
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
        type Cas = Stub;
        type Log = Stub;
        type Strategy = Stub;
        type Dispatcher = Stub;
        type Clock = Stub;
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
            cas: Stub,
            log: Stub,
            strategy: Stub,
            dispatcher: Stub,
            clock: Stub,
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
            "cas",
            "log",
            "strategy",
            "dispatcher",
            "clock",
        ] {
            assert!(
                text.contains(&format!("{field}: Stub")),
                "field `{field}` missing from: {text}"
            );
        }
    }
}
