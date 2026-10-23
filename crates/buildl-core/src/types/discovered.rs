//! What discovery targets last produced, as the declaration port hands it to a build file.
//!
//! Its own file because the map is a value of its own on the build-file port: Load hands it to
//! every evaluation unchanged, and nothing in this crate looks inside it.
//!
//! Responsibilities: [`Discovered`].
//!
//! Non-responsibilities: producing the results, which running a discovery target does, and
//! reading them back from the stores that kept them.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::types::Label;

/// The result of each discovery target that has one: its label, and the bytes of its one output.
///
/// Ordered by label, so two maps holding the same results are equal however they were built.
/// Clones share the results, so handing the map to every evaluation copies no bytes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Discovered {
    results: Arc<BTreeMap<Label, Vec<u8>>>,
}

impl Discovered {
    /// A map holding no result: no discovery target has run.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            results: Arc::new(BTreeMap::new()),
        }
    }

    /// A map holding `results`.
    #[must_use]
    pub fn new(results: BTreeMap<Label, Vec<u8>>) -> Self {
        Self {
            results: Arc::new(results),
        }
    }

    /// The bytes the discovery target `label` produced, or `None` when the map holds none.
    #[must_use]
    pub fn get(&self, label: &Label) -> Option<&[u8]> {
        self.results.get(label).map(Vec::as_slice)
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeMap;
    use std::sync::Arc;

    use super::Discovered;
    use crate::types::Label;

    fn label(raw: &str) -> Label {
        Label::parse(raw).unwrap()
    }

    #[test]
    fn an_empty_map_holds_no_result() {
        assert_eq!(Discovered::empty().get(&label("//svc:pkgs")), None);
        assert_eq!(Discovered::empty(), Discovered::default());
    }

    #[test]
    fn a_clone_shares_the_results_it_was_cloned_from() {
        let discovered = Discovered::new(BTreeMap::from([(label("//svc:pkgs"), b"x".to_vec())]));
        let clone = discovered.clone();
        assert!(Arc::ptr_eq(&discovered.results, &clone.results));
        assert_eq!(clone, discovered);
    }

    #[test]
    fn a_result_is_found_by_its_label_and_only_by_it() {
        let discovered =
            Discovered::new(BTreeMap::from([(label("//svc:pkgs"), b"[\"a\"]".to_vec())]));
        assert_eq!(
            discovered.get(&label("//svc:pkgs")),
            Some(b"[\"a\"]".as_slice())
        );
        assert_eq!(discovered.get(&label("//svc:other")), None);
    }
}
