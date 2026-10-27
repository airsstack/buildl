//! A cache row: what the last successful build of one target recorded.
//!
//! Its own file because a row is what a later plan compares against: it holds the key the
//! target was built under, to say whether anything changed, and the digests of what the build
//! produced, which the targets depending on it key on. It also holds the digests of what the
//! action printed, so a build that finds the target unchanged can report the same output
//! without running it.
//!
//! The digest of an output is the digest of its
//! [`OutputManifest`](crate::types::OutputManifest), not of a file's bytes: an output is a file
//! that may or may not be run, or a whole directory, and the manifest is what says which.
//!
//! Responsibilities: [`CacheRow`], its constructor, and pairing its outputs with their names.
//!
//! Non-responsibilities: storing rows, and deciding what a row means for the next build.

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};

use crate::types::{ActionKey, Digest, Produced};

/// What the last successful build of one target recorded: its key, what it produced, and what
/// its action printed.
///
/// A row holds one digest per output the key declares, in the same order. That is checked once,
/// by [`CacheRow::new`], so every reader may pair names with digests without checking again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CacheRow {
    key: ActionKey,
    outputs: Vec<Digest>,
    stdout: Digest,
    stderr: Digest,
}

impl CacheRow {
    /// A row for a build under `key` that produced `outputs`, one digest per declared output in
    /// declared order, and whose action wrote the content `stdout` to standard output and
    /// `stderr` to standard error; `None` when `outputs` does not hold exactly one digest per
    /// output.
    #[must_use]
    pub fn new(
        key: ActionKey,
        outputs: Vec<Digest>,
        stdout: Digest,
        stderr: Digest,
    ) -> Option<Self> {
        (outputs.len() == key.components().own.outputs.len()).then_some(Self {
            key,
            outputs,
            stdout,
            stderr,
        })
    }

    /// The key the target was built under.
    #[must_use]
    pub const fn key(&self) -> &ActionKey {
        &self.key
    }

    /// The digest of what the action wrote to standard output.
    #[must_use]
    pub const fn stdout(&self) -> Digest {
        self.stdout
    }

    /// The digest of what the action wrote to standard error.
    #[must_use]
    pub const fn stderr(&self) -> Digest {
        self.stderr
    }

    /// The digest of each declared output, in declared order: the digest of the output's
    /// manifest.
    #[must_use]
    pub fn outputs(&self) -> &[Digest] {
        &self.outputs
    }

    /// Each declared output with the digest of what was produced under that name, in declared
    /// order.
    #[must_use]
    pub fn produced(&self) -> Vec<Produced> {
        self.key
            .components()
            .own
            .outputs
            .iter()
            .zip(&self.outputs)
            .map(|(name, digest)| Produced {
                name: name.clone(),
                digest: *digest,
            })
            .collect()
    }
}

/// A row is read back through [`CacheRow::new`], so a written row whose outputs are not one per
/// output its key declares is refused, as is one holding a field a row does not have.
impl<'de> Deserialize<'de> for CacheRow {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        /// A row as it is written, its outputs not yet counted against its key's. It names
        /// every field of [`CacheRow`]: one it lacked would be refused as unknown.
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct StoredRow {
            key: ActionKey,
            outputs: Vec<Digest>,
            stdout: Digest,
            stderr: Digest,
        }

        let stored = StoredRow::deserialize(deserializer)?;
        Self::new(stored.key, stored.outputs, stored.stdout, stored.stderr).ok_or_else(|| {
            D::Error::custom(
                "a cache row must record exactly one output for each output its key declares",
            )
        })
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeMap;

    use super::CacheRow;
    use crate::error::Error;
    use crate::json::canonical;
    use crate::types::{
        ActionKey, Argument, Command, Digest, KeyComponents, NetworkAccess, OutputName,
        OutputSource, OwnComponents, Platform,
    };

    /// A key declaring `outputs`, with nothing else of note.
    fn key(outputs: &[&str]) -> ActionKey {
        ActionKey::new(KeyComponents {
            own: OwnComponents {
                command: Command::new(vec![Argument::parse("cc").unwrap()]).unwrap(),
                inputs: Vec::new(),
                outputs: outputs
                    .iter()
                    .map(|name| OutputName::parse(*name).unwrap())
                    .collect(),
                env: BTreeMap::new(),
                tool: None,
                settings: BTreeMap::new(),
                network: NetworkAccess::Sealed,
                output_source: OutputSource::Command,
                platform: Platform::parse("linux/x86_64").unwrap(),
            },
            deps: Vec::new(),
        })
        .unwrap()
    }

    /// A row under a key declaring `outputs`, holding `digests` for them, for an action that
    /// printed `compiling` and warned `careful`.
    fn row(outputs: &[&str], digests: Vec<Digest>) -> Option<CacheRow> {
        CacheRow::new(
            key(outputs),
            digests,
            Digest::of(b"compiling"),
            Digest::of(b"careful"),
        )
    }

    #[test]
    fn a_row_needs_exactly_one_digest_per_declared_output() {
        let app = Digest::of(b"app");
        let map = Digest::of(b"map");
        assert!(row(&["app", "app.map"], vec![app]).is_none());
        assert!(row(&["app", "app.map"], vec![app, map, app]).is_none());
        assert!(row(&["app", "app.map"], vec![app, map]).is_some());
        assert!(row(&[], Vec::new()).is_some());
        assert!(row(&[], vec![app]).is_none());
    }

    #[test]
    fn a_row_keeps_the_key_it_was_built_under() {
        let row = row(&["app"], vec![Digest::of(b"app")]).unwrap();
        assert_eq!(row.key(), &key(&["app"]));
    }

    #[test]
    fn produced_pairs_each_output_name_with_its_digest_in_declared_order() {
        let app = Digest::of(b"app");
        let map = Digest::of(b"map");
        let row = row(&["z.out", "a.out"], vec![app, map]).unwrap();
        let produced: Vec<(String, Digest)> = row
            .produced()
            .into_iter()
            .map(|output| (output.name.to_string(), output.digest))
            .collect();
        assert_eq!(
            produced,
            [("z.out".to_owned(), app), ("a.out".to_owned(), map)]
        );
    }

    #[test]
    fn outputs_lists_the_digest_of_each_output_in_declared_order() {
        let app = Digest::of(b"app");
        let map = Digest::of(b"map");
        let both = row(&["z.out", "a.out"], vec![app, map]).unwrap();
        assert_eq!(both.outputs(), [app, map]);
        assert_eq!(row(&[], Vec::new()).unwrap().outputs(), []);
    }

    #[test]
    fn a_row_keeps_the_digests_of_what_the_action_printed() {
        let row = row(&["app"], vec![Digest::of(b"app")]).unwrap();
        assert_eq!(row.stdout(), Digest::of(b"compiling"));
        assert_eq!(row.stderr(), Digest::of(b"careful"));
    }

    #[test]
    fn a_row_with_no_output_still_holds_what_was_printed() {
        let row = row(&[], Vec::new()).unwrap();
        assert_eq!(row.outputs(), []);
        assert_eq!(row.stdout(), Digest::of(b"compiling"));
        assert_eq!(row.stderr(), Digest::of(b"careful"));
    }

    #[test]
    fn two_rows_differing_only_in_what_was_printed_are_not_equal() {
        let app = Digest::of(b"app");
        let quiet = CacheRow::new(key(&["app"]), vec![app], Digest::of(b""), Digest::of(b""));
        assert_ne!(row(&["app"], vec![app]), quiet);
    }

    /// A row whose key has more than one of everything a key can hold several of, and no field
    /// left at the value it would have if nothing had been said.
    fn full_row() -> CacheRow {
        use crate::types::{
            DepProducts, EnvName, EnvValue, InputDigest, Label, Produced, Program, SettingName,
            SettingValue, SourcePath, ToolIdentity,
        };

        let produced = |name: &str, content: &str| Produced {
            name: OutputName::parse(name).unwrap(),
            digest: Digest::of(content.as_bytes()),
        };
        let key = ActionKey::new(KeyComponents {
            own: OwnComponents {
                command: Command::new(vec![
                    Argument::parse("cc").unwrap(),
                    Argument::parse("$in").unwrap(),
                    Argument::parse("-O$opt:opt-level").unwrap(),
                ])
                .unwrap(),
                inputs: vec![
                    InputDigest {
                        path: SourcePath::parse("src/z.c").unwrap(),
                        digest: Digest::of(b"z"),
                    },
                    InputDigest {
                        path: SourcePath::parse("src/a.c").unwrap(),
                        digest: Digest::of(b"a"),
                    },
                ],
                outputs: vec![
                    OutputName::parse("app").unwrap(),
                    OutputName::parse("app.map").unwrap(),
                ],
                env: BTreeMap::from([
                    (
                        EnvName::parse("PATH").unwrap(),
                        Some(EnvValue::parse("/bin").unwrap()),
                    ),
                    (EnvName::parse("HOME").unwrap(), None),
                ]),
                tool: Some(ToolIdentity {
                    program: Program::parse("cc").unwrap(),
                    fingerprint: Digest::of(b"cc 14.2"),
                }),
                settings: BTreeMap::from([
                    (
                        SettingName::parse("opt-level").unwrap(),
                        SettingValue::parse("3").unwrap(),
                    ),
                    (
                        SettingName::parse("debug").unwrap(),
                        SettingValue::parse("full").unwrap(),
                    ),
                ]),
                network: NetworkAccess::Declared,
                output_source: OutputSource::Command,
                platform: Platform::parse("macos/aarch64").unwrap(),
            },
            deps: vec![
                DepProducts {
                    label: Label::parse("//lib:text").unwrap(),
                    outputs: vec![produced("text", "text v1")],
                },
                DepProducts {
                    label: Label::parse("//:gen").unwrap(),
                    outputs: vec![produced("gen.h", "header"), produced("gen.c", "source")],
                },
            ],
        })
        .unwrap();
        CacheRow::new(
            key,
            vec![Digest::of(b"app"), Digest::of(b"map")],
            Digest::of(b"compiling"),
            Digest::of(b"careful"),
        )
        .unwrap()
    }

    /// `row` as the tree its canonical bytes hold.
    fn tree_of(row: &CacheRow) -> serde_json::Value {
        serde_json::from_slice(&canonical::to_vec(row).unwrap()).unwrap()
    }

    /// Why reading `tree` back as a row is refused, as `serde_json` words it.
    fn refusal(tree: &serde_json::Value) -> String {
        match canonical::from_slice::<CacheRow>(&canonical::to_vec(tree).unwrap()) {
            Err(Error::UnreadableJson { source }) => source.to_string(),
            other => unreachable!("{tree} should be unreadable, got {other:?}"),
        }
    }

    #[test]
    fn a_row_reads_back_equal_to_the_row_that_was_written() {
        let row = full_row();
        let bytes = canonical::to_vec(&row).unwrap();
        let read: CacheRow = canonical::from_slice(&bytes).unwrap();
        assert_eq!(read, row);
        assert_eq!(read.produced(), row.produced());
        assert_eq!(canonical::to_vec(&read).unwrap(), bytes);
    }

    #[test]
    fn a_row_with_no_output_reads_back_too() {
        let row = row(&[], Vec::new()).unwrap();
        let bytes = canonical::to_vec(&row).unwrap();
        assert_eq!(canonical::from_slice::<CacheRow>(&bytes).unwrap(), row);
    }

    #[test]
    fn a_row_is_written_with_its_key_as_the_key_is_written() {
        let row = full_row();
        let tree = tree_of(&row);
        assert_eq!(
            canonical::to_vec(&tree["key"]).unwrap(),
            canonical::to_vec(row.key()).unwrap()
        );
        assert_eq!(tree["stdout"], Digest::of(b"compiling").to_string());
        assert_eq!(tree["stderr"], Digest::of(b"careful").to_string());
        assert_eq!(tree["outputs"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn a_row_read_back_needs_exactly_one_output_per_declared_output() {
        let reason = "exactly one output for each output its key declares";

        let mut fewer = tree_of(&full_row());
        fewer["outputs"].as_array_mut().unwrap().pop();
        assert!(refusal(&fewer).contains(reason), "{}", refusal(&fewer));

        let mut more = tree_of(&full_row());
        let again = more["outputs"][0].clone();
        more["outputs"].as_array_mut().unwrap().push(again);
        assert!(refusal(&more).contains(reason), "{}", refusal(&more));

        // A key that is sound in itself, but is not the one these outputs were recorded under.
        let mut other_key = tree_of(&full_row());
        other_key["key"] =
            serde_json::from_slice(&canonical::to_vec(&key(&["app"])).unwrap()).unwrap();
        assert!(
            refusal(&other_key).contains(reason),
            "{}",
            refusal(&other_key)
        );
    }

    #[test]
    fn a_row_that_cannot_be_trusted_is_refused_when_read() {
        let cases = [
            (
                "/key/components/own/platform",
                "\"linux/x86_64\"",
                "is not the digest of its components",
            ),
            ("/key/digest", "\"abc\"", "invalid digest"),
            ("/key/components/own/command", "[]", "invalid command"),
            ("/key", "null", "invalid type: null"),
            ("/outputs", "{}", "invalid type: map"),
            ("/stdout", "\"ABC\"", "invalid digest"),
            ("/stderr", "7", "invalid type: integer"),
            ("", "[]", "invalid length 0"),
        ];
        for (pointer, text, expected) in cases {
            let mut tree = tree_of(&full_row());
            *tree.pointer_mut(pointer).unwrap() = serde_json::from_str(text).unwrap();
            let reason = refusal(&tree);
            assert!(reason.contains(expected), "{pointer} = {text}: {reason}");
        }
    }

    #[test]
    fn a_row_with_a_field_nothing_declares_or_one_left_out_is_refused() {
        let written = tree_of(&full_row());

        let mut extra = written.clone();
        extra["extra"] = serde_json::Value::Null;
        assert!(
            refusal(&extra).contains("unknown field `extra`"),
            "{}",
            refusal(&extra)
        );

        let fields: Vec<String> = written.as_object().unwrap().keys().cloned().collect();
        for field in fields {
            let mut tree = written.clone();
            tree.as_object_mut().unwrap().remove(&field);
            let reason = refusal(&tree);
            assert!(
                reason.contains(&format!("missing field `{field}`")),
                "{field}: {reason}"
            );
        }
    }

    #[test]
    fn a_row_read_without_the_canonical_reader_is_still_checked() {
        let mut tree = tree_of(&full_row());
        tree["outputs"].as_array_mut().unwrap().pop();
        assert!(serde_json::from_value::<CacheRow>(tree).is_err());
    }
}
