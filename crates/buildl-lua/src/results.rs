//! The discovery results one build file's evaluation may read through `buildl.discovered`.
//!
//! Its own file because the results belong to the evaluation, not to any one primitives table:
//! the build file and every plugin instance it loads read the same map, and a relative label in
//! any of them resolves against the build file's directory, as a dependency does.
//!
//! Responsibilities: [`Results`], the map and the directory a label resolves against.
//!
//! Non-responsibilities: producing the results, which running a discovery target does, and the
//! shape of the `discovered` call, which `primitives` owns.

use std::sync::Arc;

use buildl_core::{Directory, Discovered, Label};

/// The discovery results handed to one evaluation, and the build file's directory.
///
/// Clones share the results.
#[derive(Debug, Clone)]
pub(crate) struct Results {
    discovered: Arc<Discovered>,
    directory: Directory,
}

impl Results {
    /// The results `discovered`, read by the build file in `directory`.
    pub(crate) fn new(discovered: &Discovered, directory: &Directory) -> Self {
        Self {
            discovered: Arc::new(discovered.clone()),
            directory: directory.clone(),
        }
    }

    /// No result, for a build file in the workspace root.
    #[cfg(test)]
    pub(crate) fn none() -> Self {
        Self::new(&Discovered::empty(), &Directory::root())
    }

    /// The directory a relative label resolves against.
    pub(crate) const fn directory(&self) -> &Directory {
        &self.directory
    }

    /// The bytes the discovery target `label` produced, or `None` when there is no result.
    pub(crate) fn get(&self, label: &Label) -> Option<&[u8]> {
        self.discovered.get(label)
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeMap;

    use buildl_core::{Directory, Discovered, Label};

    use super::Results;

    #[test]
    fn results_answer_by_label_and_keep_the_directory() {
        let label = Label::parse("//svc:pkgs").unwrap();
        let discovered = Discovered::new(BTreeMap::from([(label.clone(), b"x".to_vec())]));
        let results = Results::new(&discovered, &Directory::parse("svc").unwrap());
        assert_eq!(results.get(&label), Some(b"x".as_slice()));
        assert_eq!(results.get(&Label::parse("//svc:other").unwrap()), None);
        assert_eq!(results.directory(), &Directory::parse("svc").unwrap());
        assert_eq!(Results::none().get(&label), None);
    }
}
