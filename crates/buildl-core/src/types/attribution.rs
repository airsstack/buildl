//! Attribution: which plugins' targets are behind each want of a build.
//!
//! Its own file because it is a different value from the wanted set it explains: the set says
//! what a build wants and is what an approval remembers, while attribution says who asked, is
//! recomputed on every plan, and is only ever shown.
//!
//! Responsibilities: [`Attribution`], its rendering and its serialized form.
//!
//! Non-responsibilities: deciding which targets want what, which needs the graph and belongs to
//! `grants`; and the wanted set's own rendering, which is [`WantedSet`]'s.

use core::fmt;
use std::collections::BTreeMap;

use serde::{Serialize, Serializer};

use crate::types::{EnvName, Label, PluginChain, Program, Want, WantedSet};

/// What a want is called in a line of attribution.
fn name_of(want: &Want) -> String {
    match want {
        Want::Program(program) => program.to_string(),
        Want::Env(name) => name.to_string(),
        Want::Read(path) => path.to_string(),
        Want::Write(directory) => directory.to_string(),
        Want::Network(label) => label.to_string(),
    }
}

/// For each want, the plugin chains whose targets want it, each with the lowest-labelled such
/// target.
///
/// Only programs, environment variables and the network are ever attributed; a want that only
/// targets written in a build file want has no entry. Held in the order of [`Want`] and then of
/// the chain, so that nothing about it depends on the order targets were declared in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Attribution(BTreeMap<Want, BTreeMap<PluginChain, Label>>);

impl Attribution {
    /// Records that the target `label`, declared through `chain`, wants `want`.
    ///
    /// A chain already recorded for `want` keeps the label it has: the first one recorded stays,
    /// so a caller that records in label order keeps the lowest.
    pub fn record(&mut self, want: Want, chain: &PluginChain, label: &Label) {
        self.0
            .entry(want)
            .or_default()
            .entry(chain.clone())
            .or_insert_with(|| label.clone());
    }

    /// What is attributed among the members of `wanted`: the entries of the wants it holds.
    ///
    /// A want the set does not hold is left out, so a caller that has narrowed the set to what
    /// is new gets the attribution of exactly that.
    #[must_use]
    pub fn within(&self, wanted: &WantedSet) -> Self {
        Self(
            self.0
                .iter()
                .filter(|(want, _)| match want {
                    Want::Program(program) => wanted.programs.contains(program),
                    Want::Env(name) => wanted.env.contains(name),
                    Want::Write(directory) => wanted.write.contains(directory),
                    Want::Network(label) => wanted.network.contains(label),
                    Want::Read(_) => false,
                })
                .map(|(want, chains)| (want.clone(), chains.clone()))
                .collect(),
        )
    }

    /// The chains whose targets want `want`, each with its lowest-labelled target; `None` when
    /// no plugin's target wants it.
    #[must_use]
    pub fn get(&self, want: &Want) -> Option<&BTreeMap<PluginChain, Label>> {
        self.0.get(want)
    }

    /// Every attributed want with its chains, in the order they are shown.
    pub fn iter(&self) -> impl Iterator<Item = (&Want, &BTreeMap<PluginChain, Label>)> {
        self.0.iter()
    }

    /// Whether no want is attributed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// One line for each chain of each attributed want, such as
/// `  go: declared by plugin go@1.2.0 for //services/api:server`.
///
/// Wants come in the order the wanted set shows them — programs, variables, network — and the
/// chains of one want in chain order. A chain of several plugins reads outermost first, as in
/// `ourgo@0.1.0 > go@1.2.0`. Each line is indented two spaces; there is no trailing newline, and
/// an empty attribution renders as nothing.
impl fmt::Display for Attribution {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;
        for (want, chains) in &self.0 {
            for (chain, label) in chains {
                if !first {
                    f.write_str("\n")?;
                }
                first = false;
                write!(f, "  {}: declared by plugin ", name_of(want))?;
                for (index, plugin) in chain.as_slice().iter().enumerate() {
                    if index > 0 {
                        f.write_str(" > ")?;
                    }
                    write!(f, "{plugin}")?;
                }
                write!(f, " for {label}")?;
            }
        }
        Ok(())
    }
}

/// One chain behind a want, and the target of it that wants it.
#[derive(Serialize)]
struct Site<'a> {
    chain: &'a PluginChain,
    target: &'a Label,
}

/// Attribution as it is written to JSON: the chains behind each program, variable and network
/// target, each under its name, in the field order of the wanted set.
#[derive(Serialize)]
struct AttributionView<'a> {
    env: BTreeMap<&'a EnvName, Vec<Site<'a>>>,
    network: BTreeMap<&'a Label, Vec<Site<'a>>>,
    programs: BTreeMap<&'a Program, Vec<Site<'a>>>,
}

/// Written under the three kinds of want it holds, each want listing its chains in chain order.
/// Read and write wants are never attributed, so they have no place in it.
impl Serialize for Attribution {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut view = AttributionView {
            env: BTreeMap::new(),
            network: BTreeMap::new(),
            programs: BTreeMap::new(),
        };
        for (want, chains) in &self.0 {
            let sites = chains
                .iter()
                .map(|(chain, target)| Site { chain, target })
                .collect();
            match want {
                Want::Program(program) => {
                    view.programs.insert(program, sites);
                }
                Want::Env(name) => {
                    view.env.insert(name, sites);
                }
                Want::Network(label) => {
                    view.network.insert(label, sites);
                }
                Want::Read(_) | Want::Write(_) => {}
            }
        }
        view.serialize(serializer)
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeSet;

    use super::Attribution;
    use crate::json::canonical;
    use crate::types::{
        EnvName, Label, PluginChain, PluginName, PluginRef, PluginVersion, Program, Want, WantedSet,
    };

    fn chain(plugins: &[(&str, &str)]) -> PluginChain {
        PluginChain::new(
            plugins
                .iter()
                .map(|(name, version)| PluginRef {
                    name: PluginName::parse(*name).unwrap(),
                    version: PluginVersion::parse(*version).unwrap(),
                    run: BTreeSet::new(),
                    env: BTreeSet::new(),
                })
                .collect(),
        )
        .unwrap()
    }

    fn label(raw: &str) -> Label {
        Label::parse(raw).unwrap()
    }

    fn program(raw: &str) -> Want {
        Want::Program(Program::parse(raw).unwrap())
    }

    fn env(raw: &str) -> Want {
        Want::Env(EnvName::parse(raw).unwrap())
    }

    #[test]
    fn display_follows_the_wanted_set_order() {
        let go = chain(&[("go", "1.2.0")]);
        let mut attribution = Attribution::default();
        attribution.record(Want::Network(label("//:fetch")), &go, &label("//:fetch"));
        attribution.record(env("GOPATH"), &go, &label("//:b"));
        attribution.record(program("protoc"), &go, &label("//:a"));
        attribution.record(program("go"), &go, &label("//services/api:server"));
        assert_eq!(
            attribution.to_string(),
            "  go: declared by plugin go@1.2.0 for //services/api:server\n  \
             protoc: declared by plugin go@1.2.0 for //:a\n  \
             GOPATH: declared by plugin go@1.2.0 for //:b\n  \
             //:fetch: declared by plugin go@1.2.0 for //:fetch"
        );
    }

    #[test]
    fn display_names_each_chain_of_a_want_outermost_first() {
        let ourgo = chain(&[("ourgo", "0.1.0"), ("go", "1.2.0")]);
        let go = chain(&[("go", "1.2.0")]);
        let mut attribution = Attribution::default();
        attribution.record(program("go"), &ourgo, &label("//:b"));
        attribution.record(program("go"), &go, &label("//:a"));
        assert_eq!(
            attribution.to_string(),
            "  go: declared by plugin go@1.2.0 for //:a\n  \
             go: declared by plugin ourgo@0.1.0 > go@1.2.0 for //:b"
        );
        assert_eq!(Attribution::default().to_string(), "");
    }

    #[test]
    fn the_first_target_recorded_for_a_chain_stays() {
        let go = chain(&[("go", "1.2.0")]);
        let mut attribution = Attribution::default();
        attribution.record(program("go"), &go, &label("//:a"));
        attribution.record(program("go"), &go, &label("//:b"));
        let chains = attribution.get(&program("go")).unwrap();
        assert_eq!(chains.get(&go), Some(&label("//:a")));
        assert!(attribution.get(&program("cc")).is_none());
        assert!(!attribution.is_empty());
    }

    #[test]
    fn within_keeps_only_the_wants_the_set_holds() {
        let go = chain(&[("go", "1.2.0")]);
        let mut attribution = Attribution::default();
        attribution.record(program("go"), &go, &label("//:a"));
        attribution.record(program("protoc"), &go, &label("//:a"));
        attribution.record(env("GOPATH"), &go, &label("//:a"));
        attribution.record(Want::Network(label("//:fetch")), &go, &label("//:fetch"));
        let wanted = WantedSet {
            programs: BTreeSet::from([Program::parse("protoc").unwrap()]),
            network: BTreeSet::from([label("//:fetch")]),
            ..WantedSet::default()
        };

        let kept = attribution.within(&wanted);

        assert_eq!(
            kept.to_string(),
            "  protoc: declared by plugin go@1.2.0 for //:a\n  \
             //:fetch: declared by plugin go@1.2.0 for //:fetch"
        );
        assert!(attribution.within(&WantedSet::default()).is_empty());
    }

    #[test]
    fn it_is_written_under_the_kind_of_want() {
        let go = chain(&[("go", "1.2.0")]);
        let mut attribution = Attribution::default();
        attribution.record(program("go"), &go, &label("//:a"));
        attribution.record(env("GOPATH"), &go, &label("//:a"));
        attribution.record(Want::Network(label("//:a")), &go, &label("//:a"));
        let one =
            r#"[{"chain":[{"env":[],"name":"go","run":[],"version":"1.2.0"}],"target":"//:a"}]"#;
        assert_eq!(
            canonical::to_string(&attribution).unwrap(),
            format!(
                r#"{{"env":{{"GOPATH":{one}}},"network":{{"//:a":{one}}},"programs":{{"go":{one}}}}}"#
            )
        );
        assert_eq!(
            canonical::to_string(&Attribution::default()).unwrap(),
            r#"{"env":{},"network":{},"programs":{}}"#
        );
    }
}
