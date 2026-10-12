//! A cache row: what the last successful build of one target recorded.
//!
//! Its own file because a row is what a later plan compares against: it holds the key the
//! target was built under, to say whether anything changed, and the digests of what the build
//! produced, which the targets depending on it key on. It also holds the digests of what the
//! action printed, so a build that finds the target unchanged can report the same output
//! without running it.
//!
//! Responsibilities: [`CacheRow`], its constructor, pairing its outputs with their names, and
//! listing every digest it holds.
//!
//! Non-responsibilities: storing rows, and deciding what a row means for the next build.

use crate::types::{ActionKey, Digest, Produced};

/// What the last successful build of one target recorded: its key, what it produced, and what
/// its action printed.
///
/// A row holds one digest per output the key declares, in the same order. That is checked once,
/// by [`CacheRow::new`], so every reader may pair names with digests without checking again.
#[derive(Debug, Clone, PartialEq, Eq)]
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

    /// Every digest the row holds: the outputs in declared order, then standard output, then
    /// standard error.
    ///
    /// These are the contents a build needs to still have before it may treat the target as
    /// already built.
    pub fn digests(&self) -> impl Iterator<Item = Digest> {
        self.outputs
            .iter()
            .copied()
            .chain([self.stdout, self.stderr])
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
    fn a_row_keeps_the_digests_of_what_the_action_printed() {
        let row = row(&["app"], vec![Digest::of(b"app")]).unwrap();
        assert_eq!(row.stdout(), Digest::of(b"compiling"));
        assert_eq!(row.stderr(), Digest::of(b"careful"));
    }

    #[test]
    fn digests_lists_the_outputs_in_declared_order_then_what_was_printed() {
        let app = Digest::of(b"app");
        let map = Digest::of(b"map");
        let row = row(&["z.out", "a.out"], vec![app, map]).unwrap();
        let digests: Vec<Digest> = row.digests().collect();
        assert_eq!(
            digests,
            [app, map, Digest::of(b"compiling"), Digest::of(b"careful")]
        );
    }

    #[test]
    fn a_row_with_no_output_still_holds_what_was_printed() {
        let row = row(&[], Vec::new()).unwrap();
        let digests: Vec<Digest> = row.digests().collect();
        assert_eq!(digests, [Digest::of(b"compiling"), Digest::of(b"careful")]);
    }

    #[test]
    fn two_rows_differing_only_in_what_was_printed_are_not_equal() {
        let app = Digest::of(b"app");
        let quiet = CacheRow::new(key(&["app"]), vec![app], Digest::of(b""), Digest::of(b""));
        assert_ne!(row(&["app"], vec![app]), quiet);
    }
}
