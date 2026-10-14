//! The action key: everything that decides what a target's action produces, and its digest.
//!
//! Its own file because the key is one value built from many: a key that differs from the one a
//! target's last build recorded is what marks that target for rebuilding, so what a key holds,
//! in what order, and how it is hashed are decided in one place.
//!
//! Responsibilities: the components of a key — [`InputDigest`], [`Produced`], [`ToolIdentity`],
//! [`DepProducts`], [`OwnComponents`], [`KeyComponents`] — and [`ActionKey`], which pairs them
//! with their digest. Each reads back from what it serializes to, refusing a field it does not
//! declare; a key read back is rebuilt from its components, so its digest is theirs.
//!
//! Non-responsibilities: gathering the components, and comparing two keys. Both belong to the
//! phase that plans; this file only says what a key is.

use std::collections::BTreeMap;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};

use crate::error::Result;
use crate::json::canonical;
use crate::types::{
    Command, Digest, EnvName, EnvValue, Label, NetworkAccess, OutputName, Platform, Program,
    SettingName, SettingValue, SourcePath,
};

/// One workspace file a target reads, with the digest of its content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputDigest {
    /// The file.
    pub path: SourcePath,
    /// The digest of its content.
    pub digest: Digest,
}

/// One output a target produced — a file, or a directory of files — with the digest that
/// stands for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Produced {
    /// The output's name.
    pub name: OutputName,
    /// The digest of the output's [`OutputManifest`](crate::types::OutputManifest), not of a
    /// file's bytes: the manifest names every file of the output, with its bytes' digest, its
    /// path and whether it may be run, so two outputs share this digest exactly when they are
    /// the same in all of that.
    pub digest: Digest,
}

/// The program a command runs, with what identifies the program itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolIdentity {
    /// The program, as the command names it.
    pub program: Program,
    /// What identifies the program found under that name: replacing the tool changes it.
    pub fingerprint: Digest,
}

/// What one dependency produced.
///
/// A key holds a dependency's outputs, not the dependency's own key: a dependency rebuilt to
/// the same outputs leaves the keys of the targets that depend on it unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DepProducts {
    /// The dependency.
    pub label: Label,
    /// Its outputs, in the order it declares them.
    pub outputs: Vec<Produced>,
}

/// Every part of a key that does not wait on another target.
///
/// Its own type because a target downstream of one that is about to run has exactly this much
/// of its key before the build: its dependency's outputs do not exist yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnComponents {
    /// The command, placeholders unexpanded.
    pub command: Command,
    /// The files the target reads, in written order.
    pub inputs: Vec<InputDigest>,
    /// The names of the outputs it produces, each a file or a directory, in written order.
    pub outputs: Vec<OutputName>,
    /// Each environment variable it reads, with its value; `None` when the variable is unset.
    pub env: BTreeMap<EnvName, Option<EnvValue>>,
    /// The program the command names, and its fingerprint; `None` when it names none.
    pub tool: Option<ToolIdentity>,
    /// Each build setting the command refers to, with the value in effect.
    pub settings: BTreeMap<SettingName, SettingValue>,
    /// Whether the action may reach the network.
    pub network: NetworkAccess,
    /// The platform the action runs on.
    pub platform: Platform,
}

/// Everything an action key is the digest of.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyComponents {
    /// The parts that do not wait on another target.
    pub own: OwnComponents,
    /// What each dependency produced, in the order the dependencies were written.
    pub deps: Vec<DepProducts>,
}

/// An action key: the components of a target's action, and the digest that stands for them.
///
/// The components are kept beside the digest so that two keys can be compared field by field,
/// which is how a rebuild is explained. A key is built only by [`ActionKey::new`], so its digest
/// always matches its components. That holds for a key read back from its serialized form too:
/// the written digest is compared with the one [`ActionKey::new`] takes of the written
/// components, and a key whose two halves disagree is refused.
///
/// # Examples
///
/// ```
/// use std::collections::BTreeMap;
///
/// use buildl_core::{
///     ActionKey, Argument, Command, KeyComponents, NetworkAccess, OutputName, OwnComponents,
///     Platform,
/// };
///
/// let components = KeyComponents {
///     own: OwnComponents {
///         command: Command::new(vec![Argument::parse("cc")?])?,
///         inputs: Vec::new(),
///         outputs: vec![OutputName::parse("app")?],
///         env: BTreeMap::new(),
///         tool: None,
///         settings: BTreeMap::new(),
///         network: NetworkAccess::Sealed,
///         platform: Platform::parse("linux/x86_64")?,
///     },
///     deps: Vec::new(),
/// };
/// let key = ActionKey::new(components.clone())?;
/// assert_eq!(key.components(), &components);
/// assert_eq!(key.digest(), ActionKey::new(components)?.digest());
/// # Ok::<(), buildl_core::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActionKey {
    digest: Digest,
    components: KeyComponents,
}

impl ActionKey {
    /// The key of `components`: their canonical JSON, hashed.
    ///
    /// # Errors
    ///
    /// Returns [`Error::CanonicalJson`](crate::Error::CanonicalJson) when the components cannot
    /// be serialized.
    pub fn new(components: KeyComponents) -> Result<Self> {
        let bytes = canonical::to_vec(&components)?;
        Ok(Self {
            digest: Digest::of(&bytes),
            components,
        })
    }

    /// The digest of the components.
    #[must_use]
    pub const fn digest(&self) -> Digest {
        self.digest
    }

    /// What the digest was taken of.
    #[must_use]
    pub const fn components(&self) -> &KeyComponents {
        &self.components
    }
}

impl<'de> Deserialize<'de> for ActionKey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> core::result::Result<Self, D::Error> {
        /// A key as it is written, its digest not yet checked against its components.
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct StoredKey {
            digest: Digest,
            components: KeyComponents,
        }

        let stored = StoredKey::deserialize(deserializer)?;
        let key = Self::new(stored.components).map_err(D::Error::custom)?;
        if key.digest == stored.digest {
            Ok(key)
        } else {
            Err(D::Error::custom(format_args!(
                "{} is not the digest of its components, which is {}",
                stored.digest, key.digest
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeMap;

    use serde_json::{Map, Value};

    use super::{
        ActionKey, DepProducts, InputDigest, KeyComponents, OwnComponents, Produced, ToolIdentity,
    };
    use crate::error::Error;
    use crate::json::canonical;
    use crate::types::{
        Argument, Command, Digest, EnvName, EnvValue, Label, NetworkAccess, OutputName, Platform,
        Program, SettingName, SettingValue, SourcePath,
    };

    fn command(arguments: &[&str]) -> Command {
        Command::new(
            arguments
                .iter()
                .map(|argument| Argument::parse(*argument).unwrap())
                .collect(),
        )
        .unwrap()
    }

    fn input(path: &str, content: &str) -> InputDigest {
        InputDigest {
            path: SourcePath::parse(path).unwrap(),
            digest: Digest::of(content.as_bytes()),
        }
    }

    fn produced(name: &str, content: &str) -> Produced {
        Produced {
            name: OutputName::parse(name).unwrap(),
            digest: Digest::of(content.as_bytes()),
        }
    }

    fn env(pairs: &[(&str, Option<&str>)]) -> BTreeMap<EnvName, Option<EnvValue>> {
        pairs
            .iter()
            .map(|(name, value)| {
                (
                    EnvName::parse(*name).unwrap(),
                    value.map(|text| EnvValue::parse(text).unwrap()),
                )
            })
            .collect()
    }

    fn settings(pairs: &[(&str, &str)]) -> BTreeMap<SettingName, SettingValue> {
        pairs
            .iter()
            .map(|(name, value)| {
                (
                    SettingName::parse(*name).unwrap(),
                    SettingValue::parse(*value).unwrap(),
                )
            })
            .collect()
    }

    /// One edit to a key's components.
    type Change = fn(&mut KeyComponents);

    /// A key with something in every field, and inputs written out of path order.
    fn components() -> KeyComponents {
        KeyComponents {
            own: OwnComponents {
                command: command(&["cc", "$in", "--filter=$opt:test_filter"]),
                inputs: vec![input("src/z.c", "z"), input("src/a.c", "a")],
                outputs: vec![
                    OutputName::parse("app").unwrap(),
                    OutputName::parse("app.map").unwrap(),
                ],
                env: env(&[("PATH", Some("/bin")), ("HOME", None)]),
                tool: Some(ToolIdentity {
                    program: Program::parse("cc").unwrap(),
                    fingerprint: Digest::of(b"cc 14.2"),
                }),
                settings: settings(&[("test_filter", "TestLogin")]),
                network: NetworkAccess::Sealed,
                platform: Platform::parse("linux/x86_64").unwrap(),
            },
            deps: vec![DepProducts {
                label: Label::parse("//lib:text").unwrap(),
                outputs: vec![produced("text", "text v1")],
            }],
        }
    }

    #[test]
    fn components_serialize_to_the_exact_canonical_bytes() {
        let json = canonical::to_string(&components()).unwrap();
        let expected = [
            r#"{"deps":[{"label":"//lib:text","outputs":[{"digest":""#,
            &Digest::of(b"text v1").to_string(),
            r#"","name":"text"}]}],"own":{"command":["cc","$in","--filter=$opt:test_filter"],"#,
            r#""env":{"HOME":null,"PATH":"/bin"},"inputs":[{"digest":""#,
            &Digest::of(b"z").to_string(),
            r#"","path":"src/z.c"},{"digest":""#,
            &Digest::of(b"a").to_string(),
            r#"","path":"src/a.c"}],"network":"Sealed","outputs":["app","app.map"],"#,
            r#""platform":"linux/x86_64","settings":{"test_filter":"TestLogin"},"#,
            r#""tool":{"fingerprint":""#,
            &Digest::of(b"cc 14.2").to_string(),
            r#"","program":"cc"}}}"#,
        ]
        .concat();
        assert_eq!(json, expected);
    }

    #[test]
    fn a_command_naming_no_tool_serializes_the_tool_as_null() {
        let mut components = components();
        components.own.tool = None;
        components.deps.clear();
        let json = canonical::to_string(&components).unwrap();
        assert!(json.starts_with(r#"{"deps":[],"own":{"#), "{json}");
        assert!(json.ends_with(r#""tool":null}}"#), "{json}");
    }

    #[test]
    fn a_key_is_the_digest_of_its_components_canonical_bytes() {
        let components = components();
        let bytes = canonical::to_vec(&components).unwrap();
        let key = ActionKey::new(components.clone()).unwrap();
        assert_eq!(key.digest(), Digest::of(&bytes));
        assert_eq!(key.components(), &components);
    }

    #[test]
    fn a_key_serializes_as_its_components_and_its_digest() {
        let key = ActionKey::new(components()).unwrap();
        let json = canonical::to_string(&key).unwrap();
        let expected = format!(
            r#"{{"components":{},"digest":"{}"}}"#,
            canonical::to_string(key.components()).unwrap(),
            key.digest()
        );
        assert_eq!(json, expected);
    }

    #[test]
    fn the_order_maps_were_filled_in_does_not_change_a_key() {
        let mut other = components();
        other.own.env = env(&[("HOME", None), ("PATH", Some("/bin"))]);
        let mut settings_reversed = components();
        settings_reversed.own.settings = settings(&[("b", "2"), ("a", "1")]);
        let mut settings_forward = components();
        settings_forward.own.settings = settings(&[("a", "1"), ("b", "2")]);
        assert_eq!(
            ActionKey::new(other).unwrap().digest(),
            ActionKey::new(components()).unwrap().digest()
        );
        assert_eq!(
            ActionKey::new(settings_reversed).unwrap().digest(),
            ActionKey::new(settings_forward).unwrap().digest()
        );
    }

    #[test]
    fn changing_any_one_component_changes_the_key() {
        let cases: [(&str, Change); 12] = [
            ("command", |c| c.own.command = command(&["cc", "$in"])),
            ("input digest", |c| {
                c.own.inputs = vec![input("src/z.c", "z2"), input("src/a.c", "a")];
            }),
            ("input order", |c| c.own.inputs.reverse()),
            ("outputs", |c| {
                c.own.outputs = vec![OutputName::parse("app").unwrap()];
            }),
            ("env value", |c| {
                c.own.env = env(&[("PATH", Some("/usr/bin")), ("HOME", None)]);
            }),
            ("env unset or empty", |c| {
                c.own.env = env(&[("PATH", Some("/bin")), ("HOME", Some(""))]);
            }),
            ("tool", |c| {
                c.own.tool = Some(ToolIdentity {
                    program: Program::parse("cc").unwrap(),
                    fingerprint: Digest::of(b"cc 15.1"),
                });
            }),
            ("setting", |c| {
                c.own.settings = settings(&[("test_filter", "TestLogout")]);
            }),
            ("network", |c| c.own.network = NetworkAccess::Declared),
            ("platform", |c| {
                c.own.platform = Platform::parse("macos/aarch64").unwrap();
            }),
            ("dep output", |c| {
                c.deps = vec![DepProducts {
                    label: Label::parse("//lib:text").unwrap(),
                    outputs: vec![produced("text", "text v2")],
                }];
            }),
            ("dep label", |c| {
                c.deps = vec![DepProducts {
                    label: Label::parse("//lib:other").unwrap(),
                    outputs: vec![produced("text", "text v1")],
                }];
            }),
        ];
        let original = ActionKey::new(components()).unwrap().digest();
        for (what, change) in cases {
            let mut altered = components();
            change(&mut altered);
            assert_ne!(
                ActionKey::new(altered).unwrap().digest(),
                original,
                "changing the {what} should change the key"
            );
        }
    }

    /// A key with more than one of everything it can hold several of, and no field left at the
    /// value it would have if nothing had been said.
    fn full_key() -> ActionKey {
        let mut components = components();
        components.own.settings = settings(&[("test_filter", "TestLogin"), ("opt-level", "3")]);
        components.own.network = NetworkAccess::Declared;
        components.deps.push(DepProducts {
            label: Label::parse("//:gen").unwrap(),
            outputs: vec![produced("gen.h", "header"), produced("gen.c", "source")],
        });
        ActionKey::new(components).unwrap()
    }

    /// `key` as the tree its canonical bytes hold.
    fn tree_of(key: &ActionKey) -> Value {
        serde_json::from_slice(&canonical::to_vec(key).unwrap()).unwrap()
    }

    /// Why reading `tree` back as a key is refused, as `serde_json` words it.
    fn refusal(tree: &Value) -> String {
        match canonical::from_slice::<ActionKey>(&canonical::to_vec(tree).unwrap()) {
            Err(Error::UnreadableJson { source }) => source.to_string(),
            other => unreachable!("{tree} should be unreadable, got {other:?}"),
        }
    }

    /// The object at `pointer` in `tree`.
    fn object_at<'a>(tree: &'a mut Value, pointer: &str) -> &'a mut Map<String, Value> {
        tree.pointer_mut(pointer).unwrap().as_object_mut().unwrap()
    }

    /// Where each struct a written key is made of sits in its tree. The two maps, `env` and
    /// `settings`, are not structs: their keys are names, not fields.
    const STRUCTS: [&str; 7] = [
        "",
        "/components",
        "/components/own",
        "/components/own/inputs/0",
        "/components/own/tool",
        "/components/deps/1",
        "/components/deps/1/outputs/0",
    ];

    #[test]
    fn a_key_reads_back_equal_to_the_key_that_was_written() {
        let key = full_key();
        let bytes = canonical::to_vec(&key).unwrap();
        let read: ActionKey = canonical::from_slice(&bytes).unwrap();
        assert_eq!(read, key);
        assert_eq!(read.digest(), key.digest());
        assert_eq!(canonical::to_vec(&read).unwrap(), bytes);
    }

    #[test]
    fn a_key_naming_no_tool_and_depending_on_nothing_reads_back_too() {
        let mut components = components();
        components.own.tool = None;
        components.own.inputs.clear();
        components.own.outputs.clear();
        components.own.env.clear();
        components.own.settings.clear();
        components.deps.clear();
        let key = ActionKey::new(components).unwrap();
        let bytes = canonical::to_vec(&key).unwrap();
        assert_eq!(canonical::from_slice::<ActionKey>(&bytes).unwrap(), key);
    }

    #[test]
    fn components_read_back_equal_to_the_components_that_were_written() {
        let key = full_key();
        let bytes = canonical::to_vec(key.components()).unwrap();
        let read: KeyComponents = canonical::from_slice(&bytes).unwrap();
        assert_eq!(&read, key.components());
        assert_eq!(canonical::to_vec(&read).unwrap(), bytes);
    }

    #[test]
    fn a_key_whose_digest_is_not_its_components_is_refused() {
        let other = Value::from(Digest::of(b"another key").to_string());
        let cases = [
            ("/digest", other.clone()),
            ("/components/own/command/0", Value::from("gcc")),
            ("/components/own/inputs/0/digest", other.clone()),
            ("/components/own/outputs/1", Value::from("app.sym")),
            ("/components/own/env/HOME", Value::from("/root")),
            ("/components/own/tool", Value::Null),
            ("/components/own/tool/fingerprint", other.clone()),
            ("/components/own/settings/opt-level", Value::from("0")),
            ("/components/own/network", Value::from("Sealed")),
            ("/components/own/platform", Value::from("macos/aarch64")),
            ("/components/deps/0/label", Value::from("//lib:other")),
            ("/components/deps/1/outputs/0/digest", other),
        ];
        for (pointer, value) in cases {
            let mut tree = tree_of(&full_key());
            *tree.pointer_mut(pointer).unwrap() = value;
            let reason = refusal(&tree);
            assert!(
                reason.contains("is not the digest of its components"),
                "{pointer}: {reason}"
            );
        }
    }

    #[test]
    fn a_value_its_own_type_refuses_is_refused_when_read() {
        let cases = [
            ("/digest", "\"abc\"", "invalid digest"),
            ("/components/own/command", "[]", "invalid command"),
            (
                "/components/own/command/0",
                "\"a\\u0000b\"",
                "invalid argument",
            ),
            (
                "/components/own/inputs/0/path",
                "\"../z.c\"",
                "invalid source path",
            ),
            (
                "/components/own/inputs/0/digest",
                "\"ABC\"",
                "invalid digest",
            ),
            (
                "/components/own/outputs/0",
                "\"/app\"",
                "invalid output name",
            ),
            (
                "/components/own/env/PATH",
                "\"a\\u0000b\"",
                "invalid environment variable value",
            ),
            ("/components/own/tool/program", "\"$in\"", "invalid program"),
            (
                "/components/own/settings/opt-level",
                "\"a\\u0000b\"",
                "invalid setting value",
            ),
            (
                "/components/own/network",
                "\"Open\"",
                "unknown variant `Open`",
            ),
            ("/components/own/platform", "\"linux\"", "invalid platform"),
            ("/components/deps/0/label", "\"//lib\"", "invalid label"),
            (
                "/components/deps/0/outputs/0/name",
                "\"\"",
                "invalid output name",
            ),
            ("/components/own/inputs", "{}", "invalid type: map"),
            ("/components/own/env", "[]", "invalid type: sequence"),
            ("/components/own/env/HOME", "1", "invalid type: integer"),
            ("/components", "null", "invalid type: null"),
        ];
        for (pointer, text, expected) in cases {
            let mut tree = tree_of(&full_key());
            *tree.pointer_mut(pointer).unwrap() = serde_json::from_str(text).unwrap();
            let reason = refusal(&tree);
            assert!(reason.contains(expected), "{pointer} = {text}: {reason}");
        }
    }

    #[test]
    fn a_name_its_own_type_refuses_is_refused_as_a_map_key() {
        let cases = [
            (
                "/components/own/env",
                "1PATH",
                "invalid environment variable name",
            ),
            ("/components/own/settings", "a.b", "invalid setting name"),
        ];
        for (pointer, name, expected) in cases {
            let mut tree = tree_of(&full_key());
            object_at(&mut tree, pointer).insert(name.to_owned(), Value::from("x"));
            let reason = refusal(&tree);
            assert!(reason.contains(expected), "{pointer}/{name}: {reason}");
        }
    }

    #[test]
    fn a_field_nothing_declares_is_refused_wherever_it_is() {
        for pointer in STRUCTS {
            let mut tree = tree_of(&full_key());
            object_at(&mut tree, pointer).insert("extra".to_owned(), Value::Null);
            let reason = refusal(&tree);
            assert!(
                reason.contains("unknown field `extra`"),
                "{pointer}: {reason}"
            );
        }
    }

    #[test]
    fn a_field_left_out_is_refused_wherever_it_is() {
        let written = tree_of(&full_key());
        for pointer in STRUCTS {
            let fields: Vec<String> = written
                .pointer(pointer)
                .unwrap()
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect();
            for field in fields {
                let mut tree = written.clone();
                object_at(&mut tree, pointer).remove(&field);
                let reason = refusal(&tree);
                // A tool that is left out reads as no tool, which is not what the digest was
                // taken of; every other field has no value to fall back to.
                let expected = if field == "tool" {
                    "is not the digest of its components".to_owned()
                } else {
                    format!("missing field `{field}`")
                };
                assert!(reason.contains(&expected), "{pointer}/{field}: {reason}");
            }
        }
    }

    #[test]
    fn no_tool_left_out_rather_than_written_as_null_is_refused_as_not_canonical() {
        let mut components = components();
        components.own.tool = None;
        let key = ActionKey::new(components).unwrap();
        let mut tree = tree_of(&key);
        object_at(&mut tree, "/components/own").remove("tool");
        let bytes = canonical::to_vec(&tree).unwrap();
        assert!(matches!(
            canonical::from_slice::<ActionKey>(&bytes),
            Err(Error::NonCanonicalJson { .. })
        ));
    }

    #[test]
    fn a_key_read_without_the_canonical_reader_still_has_its_digest_checked() {
        let mut tree = tree_of(&full_key());
        *tree.pointer_mut("/components/own/platform").unwrap() = Value::from("macos/aarch64");
        let err = serde_json::from_value::<ActionKey>(tree).unwrap_err();
        assert!(
            err.to_string()
                .contains("is not the digest of its components"),
            "{err}"
        );
    }
}
