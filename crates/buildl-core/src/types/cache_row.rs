//! A cache row: what the last successful build of one target recorded.
//!
//! Its own file because a row is what a later plan compares against: it holds the key the
//! target was built under, to say whether anything changed, and the digests of what the build
//! produced, which the targets depending on it key on.
//!
//! Responsibilities: [`CacheRow`], its constructor, and pairing its outputs with their names.
//!
//! Non-responsibilities: storing rows, and deciding what a row means for the next build.

use crate::types::{ActionKey, Digest, Produced};

/// What the last successful build of one target recorded: its key, and what it produced.
///
/// A row holds one digest per output the key declares, in the same order. That is checked once,
/// by [`CacheRow::new`], so every reader may pair names with digests without checking again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheRow {
    key: ActionKey,
    outputs: Vec<Digest>,
}

impl CacheRow {
    /// A row for a build under `key` that produced `outputs`, one digest per declared output in
    /// declared order; `None` when `outputs` does not hold exactly one digest per output.
    #[must_use]
    pub fn new(key: ActionKey, outputs: Vec<Digest>) -> Option<Self> {
        (outputs.len() == key.components().own.outputs.len()).then_some(Self { key, outputs })
    }

    /// The key the target was built under.
    #[must_use]
    pub const fn key(&self) -> &ActionKey {
        &self.key
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

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeMap;

    use super::CacheRow;
    use crate::types::{
        ActionKey, Argument, Command, Digest, KeyComponents, NetworkAccess, OutputName,
        OwnComponents, Platform,
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
                platform: Platform::parse("linux/x86_64").unwrap(),
            },
            deps: Vec::new(),
        })
        .unwrap()
    }

    #[test]
    fn a_row_needs_exactly_one_digest_per_declared_output() {
        let app = Digest::of(b"app");
        let map = Digest::of(b"map");
        assert!(CacheRow::new(key(&["app", "app.map"]), vec![app]).is_none());
        assert!(CacheRow::new(key(&["app", "app.map"]), vec![app, map, app]).is_none());
        assert!(CacheRow::new(key(&["app", "app.map"]), vec![app, map]).is_some());
        assert!(CacheRow::new(key(&[]), Vec::new()).is_some());
        assert!(CacheRow::new(key(&[]), vec![app]).is_none());
    }

    #[test]
    fn a_row_keeps_the_key_it_was_built_under() {
        let row = CacheRow::new(key(&["app"]), vec![Digest::of(b"app")]).unwrap();
        assert_eq!(row.key(), &key(&["app"]));
    }

    #[test]
    fn produced_pairs_each_output_name_with_its_digest_in_declared_order() {
        let app = Digest::of(b"app");
        let map = Digest::of(b"map");
        let row = CacheRow::new(key(&["z.out", "a.out"]), vec![app, map]).unwrap();
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
}
