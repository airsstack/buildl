//! The plan: what a build of a workspace would do, target by target, and why.
//!
//! Its own file because a plan is a different value from the graph it is made from: the graph
//! says what each target is, a plan says whether each one needs to run, and it is the value the
//! `plan` command prints and the executor later walks. It is also where a run's build settings
//! are fixed: the graph holds each setting's default, a plan the value in force.
//!
//! Responsibilities: [`Plan`], the [`Verdict`] it holds for each target, the parts of a verdict
//! — [`DirtyReason`], [`Difference`], [`PlannedKey`], [`Unfinished`] — and a plan's serialized
//! form.
//!
//! Non-responsibilities: reaching a verdict. Comparing a key with the one a previous build
//! recorded belongs to the phase that plans; this file stores what it decided.

use std::collections::BTreeMap;

use serde::{Serialize, Serializer};

use crate::types::{
    ActionKey, EnvName, Label, NodeId, OwnComponents, SettingName, SettingValue, SourcePath,
    WantedSet,
};

/// One way a target's key differs from the key its last build recorded.
///
/// A difference names what changed, never the old or the new value: both are in the two keys.
/// Non-exhaustive: a key that gains a component gains a difference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum Difference {
    /// The command.
    Command,
    /// An input that was added, removed, or whose content changed.
    Input(SourcePath),
    /// The same inputs, written in another order.
    InputOrder,
    /// The declared outputs.
    Outputs,
    /// An environment variable that was added, removed, or whose value changed.
    Env(EnvName),
    /// The program the command names, or its fingerprint.
    Tool,
    /// A build setting that was added, removed, or whose value changed.
    Setting(SettingName),
    /// Whether the action may reach the network.
    Network,
    /// The platform.
    Platform,
    /// A dependency that was added, removed, or that produced something else.
    Dep(Label),
    /// The same dependencies, written in another order.
    DepOrder,
}

/// Why a target will run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum DirtyReason {
    /// The target is declared to run on every build.
    Always,
    /// No previous build recorded a result for it.
    NeverBuilt,
    /// The run asked for its tests to run again, and this target is one.
    Rerun,
    /// Its key differs from the recorded one in these ways, in a fixed order.
    Changed(Vec<Difference>),
}

/// A key that cannot be finished before the build: some dependency has yet to produce its
/// outputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unfinished {
    /// The parts of the key that are already known.
    pub own: OwnComponents,
    /// The dependencies whose outputs are not known yet, in the order they were written.
    pub awaiting: Vec<NodeId>,
}

/// The key of a target that will run: whole, or waiting on its dependencies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlannedKey {
    /// Every dependency is clean, so the key is complete.
    Finished(ActionKey),
    /// Some dependency is not clean, so the key waits for what it produces.
    Unfinished(Unfinished),
}

/// What a build would do with one target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Nothing: its key equals the one its last build recorded.
    Clean {
        /// The key.
        key: ActionKey,
    },
    /// Run it.
    Dirty {
        /// Why.
        reason: DirtyReason,
        /// Its key, as far as it is known before the build.
        key: PlannedKey,
    },
    /// Run it only if a dependency it waits on produces something other than what the last
    /// build recorded; nothing else about it changed.
    Conditional(Unfinished),
}

/// What a build of a workspace would do: one verdict per target, what the build wants, and the
/// value every build setting has for it.
///
/// A plan is obtained from [`plan`](fn@crate::plan). Its verdicts are addressed by the
/// [`NodeId`]s of the graph it was made from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    labels: Vec<Label>,
    verdicts: Vec<Verdict>,
    wanted: WantedSet,
    settings: BTreeMap<SettingName, SettingValue>,
}

impl Plan {
    /// Assembles a plan from one label and one verdict per node, both in id order, with what the
    /// build wants and the value of every declared setting.
    pub(crate) const fn new(
        labels: Vec<Label>,
        verdicts: Vec<Verdict>,
        wanted: WantedSet,
        settings: BTreeMap<SettingName, SettingValue>,
    ) -> Self {
        Self {
            labels,
            verdicts,
            wanted,
            settings,
        }
    }

    /// The number of targets.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.verdicts.len()
    }

    /// Whether the plan holds no target.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.verdicts.is_empty()
    }

    /// The label of the target `id` names; `None` when the plan's graph did not issue `id`.
    #[must_use]
    pub fn label(&self, id: NodeId) -> Option<&Label> {
        self.labels.get(id.index())
    }

    /// The verdict for the target `id` names; `None` when the plan's graph did not issue `id`.
    #[must_use]
    pub fn verdict(&self, id: NodeId) -> Option<&Verdict> {
        self.verdicts.get(id.index())
    }

    /// Every verdict, in id order.
    #[must_use]
    pub fn verdicts(&self) -> &[Verdict] {
        &self.verdicts
    }

    /// What the build wants.
    #[must_use]
    pub const fn wanted(&self) -> &WantedSet {
        &self.wanted
    }

    /// The settings in force: every setting the plan's graph declares, with the value it has for
    /// this plan — the one it was overridden with, or else its default.
    ///
    /// A key holds only the settings its target's command names; this holds them all, so it is
    /// also where the value of a setting no command names is found.
    #[must_use]
    pub const fn settings(&self) -> &BTreeMap<SettingName, SettingValue> {
        &self.settings
    }
}

/// An unfinished key as it is written to JSON: the dependencies it waits on as labels.
#[derive(Serialize)]
struct UnfinishedView<'a> {
    awaiting: Vec<&'a Label>,
    own: &'a OwnComponents,
}

/// One target's verdict as it is written to JSON.
#[derive(Serialize)]
struct TargetView<'a> {
    key: Option<&'a ActionKey>,
    reason: Option<&'a DirtyReason>,
    unfinished: Option<UnfinishedView<'a>>,
    verdict: &'static str,
}

/// A plan as it is written to JSON: every setting under its name, every target under its label,
/// no id anywhere.
#[derive(Serialize)]
struct PlanView<'a> {
    settings: &'a BTreeMap<SettingName, SettingValue>,
    targets: BTreeMap<&'a Label, TargetView<'a>>,
    wanted: &'a WantedSet,
}

impl Plan {
    /// `unfinished` with the ids it waits on written as labels.
    fn unfinished_view<'a>(&'a self, unfinished: &'a Unfinished) -> UnfinishedView<'a> {
        UnfinishedView {
            awaiting: unfinished
                .awaiting
                .iter()
                .filter_map(|id| self.label(*id))
                .collect(),
            own: &unfinished.own,
        }
    }

    /// `verdict` as it is written to JSON.
    fn target_view<'a>(&'a self, verdict: &'a Verdict) -> TargetView<'a> {
        match verdict {
            Verdict::Clean { key } => TargetView {
                key: Some(key),
                reason: None,
                unfinished: None,
                verdict: "Clean",
            },
            Verdict::Dirty { reason, key } => {
                let (key, unfinished) = match key {
                    PlannedKey::Finished(key) => (Some(key), None),
                    PlannedKey::Unfinished(unfinished) => {
                        (None, Some(self.unfinished_view(unfinished)))
                    }
                };
                TargetView {
                    key,
                    reason: Some(reason),
                    unfinished,
                    verdict: "Dirty",
                }
            }
            Verdict::Conditional(unfinished) => TargetView {
                key: None,
                reason: None,
                unfinished: Some(self.unfinished_view(unfinished)),
                verdict: "Conditional",
            },
        }
    }
}

/// Serializes keyed by label, so that a plan can be read without the graph it was made from.
///
/// Ids are positions in the graph's arena and mean nothing outside it; labels do. There is no
/// `Deserialize`: a plan is made again on every run, never read back.
impl Serialize for Plan {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let targets = self
            .labels
            .iter()
            .zip(&self.verdicts)
            .map(|(label, verdict)| (label, self.target_view(verdict)))
            .collect();
        PlanView {
            settings: &self.settings,
            targets,
            wanted: &self.wanted,
        }
        .serialize(serializer)
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::{BTreeMap, BTreeSet};

    use super::{Difference, DirtyReason, Plan, PlannedKey, Unfinished, Verdict};
    use crate::json::canonical;
    use crate::types::{
        ActionKey, Argument, Command, Directory, EnvName, KeyComponents, Label, NetworkAccess,
        NodeId, OutputName, OwnComponents, Platform, Program, SettingName, SettingValue,
        SourcePath, WantedSet,
    };

    fn label(raw: &str) -> Label {
        Label::parse(raw).unwrap()
    }

    /// The known components of a target producing one output, `output`.
    fn own(output: &str) -> OwnComponents {
        OwnComponents {
            command: Command::new(vec![Argument::parse("cc").unwrap()]).unwrap(),
            inputs: Vec::new(),
            outputs: vec![OutputName::parse(output).unwrap()],
            env: BTreeMap::new(),
            tool: None,
            settings: BTreeMap::new(),
            network: NetworkAccess::Sealed,
            platform: Platform::parse("linux/x86_64").unwrap(),
        }
    }

    fn key(output: &str) -> ActionKey {
        ActionKey::new(KeyComponents {
            own: own(output),
            deps: Vec::new(),
        })
        .unwrap()
    }

    fn wanted() -> WantedSet {
        WantedSet {
            programs: BTreeSet::from([Program::parse("cc").unwrap()]),
            env: BTreeSet::from([EnvName::parse("HOME").unwrap()]),
            write: BTreeSet::from([
                Directory::parse("out").unwrap(),
                Directory::parse(".buildl").unwrap(),
            ]),
            network: BTreeSet::from([label("//:e")]),
        }
    }

    /// Two settings, `mode` and `test_filter`, the second with an empty value.
    fn settings() -> BTreeMap<SettingName, SettingValue> {
        [("test_filter", ""), ("mode", "release")]
            .into_iter()
            .map(|(name, value)| {
                (
                    SettingName::parse(name).unwrap(),
                    SettingValue::parse(value).unwrap(),
                )
            })
            .collect()
    }

    /// Five targets, `//:a` to `//:e`: clean, dirty for each of the three reasons — the last
    /// with an unfinished key — and conditional.
    fn plan() -> Plan {
        let labels = ["//:a", "//:b", "//:c", "//:d", "//:e"].map(label).to_vec();
        let verdicts = vec![
            Verdict::Clean { key: key("a") },
            Verdict::Dirty {
                reason: DirtyReason::Always,
                key: PlannedKey::Finished(key("b")),
            },
            Verdict::Dirty {
                reason: DirtyReason::NeverBuilt,
                key: PlannedKey::Finished(key("c")),
            },
            Verdict::Dirty {
                reason: DirtyReason::Changed(vec![
                    Difference::Command,
                    Difference::Input(SourcePath::parse("src/main.c").unwrap()),
                ]),
                key: PlannedKey::Unfinished(Unfinished {
                    own: own("d"),
                    awaiting: vec![NodeId::new(2), NodeId::new(1)],
                }),
            },
            Verdict::Conditional(Unfinished {
                own: own("e"),
                awaiting: vec![NodeId::new(3)],
            }),
        ];
        Plan::new(labels, verdicts, wanted(), settings())
    }

    #[test]
    fn a_plan_answers_by_id_and_by_position() {
        let plan = plan();
        assert_eq!(plan.len(), 5);
        assert!(!plan.is_empty());
        assert_eq!(plan.label(NodeId::new(3)), Some(&label("//:d")));
        assert_eq!(
            plan.verdict(NodeId::new(0)),
            Some(&Verdict::Clean { key: key("a") })
        );
        assert_eq!(plan.verdicts().len(), 5);
        assert_eq!(plan.wanted(), &wanted());
        assert_eq!(plan.settings(), &settings());
    }

    #[test]
    fn an_id_the_plans_graph_did_not_issue_answers_none() {
        let plan = plan();
        assert!(plan.label(NodeId::new(5)).is_none());
        assert!(plan.verdict(NodeId::new(5)).is_none());
    }

    #[test]
    fn an_empty_plan_holds_nothing() {
        let empty = WantedSet {
            programs: BTreeSet::new(),
            env: BTreeSet::new(),
            write: BTreeSet::new(),
            network: BTreeSet::new(),
        };
        let plan = Plan::new(Vec::new(), Vec::new(), empty, BTreeMap::new());
        assert!(plan.is_empty());
        assert_eq!(plan.len(), 0);
        assert!(plan.settings().is_empty());
        assert_eq!(
            canonical::to_string(&plan).unwrap(),
            concat!(
                r#"{"settings":{},"targets":{},"#,
                r#""wanted":{"env":[],"network":[],"programs":[],"write":[]}}"#
            )
        );
    }

    #[test]
    fn every_difference_serializes_as_its_name_or_as_one_key() {
        let cases = [
            (Difference::Command, r#""Command""#),
            (
                Difference::Input(SourcePath::parse("src/main.c").unwrap()),
                r#"{"Input":"src/main.c"}"#,
            ),
            (Difference::InputOrder, r#""InputOrder""#),
            (Difference::Outputs, r#""Outputs""#),
            (
                Difference::Env(EnvName::parse("HOME").unwrap()),
                r#"{"Env":"HOME"}"#,
            ),
            (Difference::Tool, r#""Tool""#),
            (
                Difference::Setting(SettingName::parse("test_filter").unwrap()),
                r#"{"Setting":"test_filter"}"#,
            ),
            (Difference::Network, r#""Network""#),
            (Difference::Platform, r#""Platform""#),
            (
                Difference::Dep(label("//lib:text")),
                r#"{"Dep":"//lib:text"}"#,
            ),
            (Difference::DepOrder, r#""DepOrder""#),
        ];
        for (difference, json) in cases {
            assert_eq!(canonical::to_string(&difference).unwrap(), json);
        }
    }

    #[test]
    fn every_reason_serializes_as_its_name_or_as_its_differences() {
        let changed = DirtyReason::Changed(vec![Difference::Tool, Difference::DepOrder]);
        assert_eq!(
            canonical::to_string(&DirtyReason::Always).unwrap(),
            r#""Always""#
        );
        assert_eq!(
            canonical::to_string(&DirtyReason::NeverBuilt).unwrap(),
            r#""NeverBuilt""#
        );
        assert_eq!(
            canonical::to_string(&changed).unwrap(),
            r#"{"Changed":["Tool","DepOrder"]}"#
        );
    }

    #[test]
    fn a_rerun_reason_serializes_as_rerun() {
        assert_eq!(
            canonical::to_string(&DirtyReason::Rerun).unwrap(),
            r#""Rerun""#
        );
    }

    #[test]
    fn a_plan_serializes_its_settings_by_name_and_its_targets_by_label_awaited_ones_as_labels() {
        let json = canonical::to_string(&plan()).unwrap();
        let key_of = |output: &str| canonical::to_string(&key(output)).unwrap();
        let own_of = |output: &str| canonical::to_string(&own(output)).unwrap();
        let expected = [
            r#"{"settings":{"mode":"release","test_filter":""},"targets":{"#,
            r#""//:a":{"key":"#,
            &key_of("a"),
            r#","reason":null,"unfinished":null,"verdict":"Clean"},"#,
            r#""//:b":{"key":"#,
            &key_of("b"),
            r#","reason":"Always","unfinished":null,"verdict":"Dirty"},"#,
            r#""//:c":{"key":"#,
            &key_of("c"),
            r#","reason":"NeverBuilt","unfinished":null,"verdict":"Dirty"},"#,
            r#""//:d":{"key":null,"reason":{"Changed":["Command",{"Input":"src/main.c"}]},"#,
            r#""unfinished":{"awaiting":["//:c","//:b"],"own":"#,
            &own_of("d"),
            r#"},"verdict":"Dirty"},"#,
            r#""//:e":{"key":null,"reason":null,"unfinished":{"awaiting":["//:d"],"own":"#,
            &own_of("e"),
            r#"},"verdict":"Conditional"}},"#,
            r#""wanted":{"env":["HOME"],"network":["//:e"],"programs":["cc"],"#,
            r#""write":[".buildl","out"]}}"#,
        ]
        .concat();
        assert_eq!(json, expected);
    }
}
