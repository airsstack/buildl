//! Settling a target that may start: must it run, or does what its last build recorded still
//! stand?
//!
//! Its own file because the question can only be answered once a target's dependencies have
//! finished: a key holds what each dependency produced, and that is known only then. So the
//! key is finished here, during the build, and compared with the recorded one.
//!
//! Responsibilities: [`finished_key`], [`stands`], [`restorable`], and [`settle`] with
//! [`Settled`], which asks the ports what the decisions need.
//!
//! Non-responsibilities: keeping rows and content, which are the ports'; and what happens to a
//! target once it is settled.

use crate::error::{Error, PortName, Result};
use crate::ports::{ActionCache, ContentStore};
use crate::types::{
    ActionKey, CacheRow, DepProducts, Diagnostic, Digest, Forcing, KeyComponents, Label,
    OutputManifest, OwnComponents,
};

/// The key of a target whose dependencies have all succeeded: `own`, and what each dependency
/// produced.
///
/// `deps` holds each dependency's label with the row its build recorded, in the order the
/// dependencies were written.
///
/// # Errors
///
/// Returns [`Error::Adapter`], naming [`PortName::ActionCache`], for the first dependency with
/// no row: a dependency that succeeded was recorded or found recorded, so its row must exist.
/// Returns [`Error::CanonicalJson`] when the key cannot be serialized.
pub(crate) fn finished_key(
    own: &OwnComponents,
    deps: &[(&Label, Option<&CacheRow>)],
) -> Result<ActionKey> {
    let deps = deps
        .iter()
        .map(|(label, row)| {
            row.map(|row| DepProducts {
                label: (*label).clone(),
                outputs: row.produced(),
            })
            .ok_or_else(|| Error::Adapter {
                port: PortName::ActionCache,
                diagnostic: Diagnostic::new(format!("{label} succeeded and has no row")),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    ActionKey::new(KeyComponents {
        own: own.clone(),
        deps,
    })
}

/// Whether `row` still stands for a target whose key is now `key`: nothing forces the target to
/// run, neither a declaration to run on every build nor a rerun of its tests, and `key` has the
/// digest of the row's key.
pub(crate) fn stands(forcing: Forcing, key: &ActionKey, row: &CacheRow) -> bool {
    forcing == Forcing::None && row.key().digest() == key.digest()
}

/// What each output of `row` is, in declared order, when everything needed to put the outputs
/// back is still held; `None` when any of it is gone.
///
/// `held` says whether content is held, and `read` returns held content. Needed are what the
/// action printed, the manifest of each output, and every file a manifest names: the one file
/// of a file output, and each file of a tree. A manifest that is gone, or whose bytes no
/// longer read as one, makes the row no reason to skip the action, as does one file of a tree:
/// a tree is put back whole or not at all.
///
/// # Errors
///
/// Returns whatever `read` returns.
pub(crate) fn restorable(
    row: &CacheRow,
    held: impl Fn(&Digest) -> bool,
    read: impl Fn(&Digest) -> Result<Option<Vec<u8>>>,
) -> Result<Option<Vec<OutputManifest>>> {
    if !(held(&row.stdout()) && held(&row.stderr())) {
        return Ok(None);
    }
    row.outputs()
        .iter()
        .map(|digest| {
            let bytes = read(digest)?;
            Ok(bytes
                .as_deref()
                .and_then(OutputManifest::from_bytes)
                .filter(|manifest| files_held(manifest, &held)))
        })
        .collect()
}

/// Whether every file `manifest` names is held.
fn files_held(manifest: &OutputManifest, held: impl Fn(&Digest) -> bool) -> bool {
    match manifest {
        OutputManifest::File(file) => held(&file.digest),
        OutputManifest::Tree(tree) => tree.entries().iter().all(|entry| held(&entry.digest)),
    }
}

/// What settling a target that may start found.
#[derive(Debug)]
pub(crate) enum Settled {
    /// It need not run: what its last build recorded still stands.
    Cached {
        /// What that build recorded.
        row: CacheRow,
        /// What each output it recorded is, in declared order.
        manifests: Vec<OutputManifest>,
    },
    /// It must run, under this key.
    Runs(ActionKey),
}

/// Settles the target `label`, whose dependencies `deps` have all succeeded.
///
/// Its key is finished from `own` and the row `cache` holds for each dependency, in the order
/// the dependencies were written. It is cached when the row `cache` holds for the target itself
/// [`stands`] under that key and is [`restorable`] from what `cas` still holds.
///
/// # Errors
///
/// Returns whatever [`finished_key`] returns, and whatever `cas` returns when the manifest of
/// an output cannot be read.
pub(crate) fn settle<A: ActionCache, C: ContentStore>(
    label: &Label,
    forcing: Forcing,
    own: &OwnComponents,
    deps: &[&Label],
    cache: &A,
    cas: &C,
) -> Result<Settled> {
    let rows: Vec<(&Label, Option<&CacheRow>)> =
        deps.iter().map(|dep| (*dep, cache.row(dep))).collect();
    let key = finished_key(own, &rows)?;
    let Some(row) = cache.row(label).filter(|row| stands(forcing, &key, row)) else {
        return Ok(Settled::Runs(key));
    };
    let manifests = restorable(row, |digest| cas.contains(digest), |digest| cas.get(digest))?;
    Ok(manifests.map_or_else(
        || Settled::Runs(key),
        |manifests| Settled::Cached {
            row: row.clone(),
            manifests,
        },
    ))
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeMap;

    use super::{finished_key, restorable, stands};
    use crate::error::{Error, PortName, Result};
    use crate::execute::fixtures::{command, digest_of, file, key, label, own, row, tree};
    use crate::types::{
        ActionKey, CacheRow, DepProducts, Diagnostic, Digest, Forcing, Freshness, KeyComponents,
        OutputManifest, Rerun, TargetRole,
    };

    #[test]
    fn a_key_is_finished_from_what_each_dependency_produced() {
        let lib = label("//:lib");
        let main = label("//:main.o");
        let lib_row = row(&["lib.a", "lib.map"]);
        let main_row = row(&["main.o"]);

        let finished = finished_key(
            &own(&["app"]),
            &[(&main, Some(&main_row)), (&lib, Some(&lib_row))],
        )
        .unwrap();

        let expected = ActionKey::new(KeyComponents {
            own: own(&["app"]),
            deps: vec![
                DepProducts {
                    label: main.clone(),
                    outputs: main_row.produced(),
                },
                DepProducts {
                    label: lib.clone(),
                    outputs: lib_row.produced(),
                },
            ],
        })
        .unwrap();
        assert_eq!(finished, expected);

        let swapped = finished_key(
            &own(&["app"]),
            &[(&lib, Some(&lib_row)), (&main, Some(&main_row))],
        )
        .unwrap();
        assert_ne!(
            swapped.digest(),
            finished.digest(),
            "the dependencies are kept in the order given"
        );
    }

    #[test]
    fn a_target_that_depends_on_nothing_is_keyed_by_its_own_components() {
        assert_eq!(finished_key(&own(&["app"]), &[]).unwrap(), key(&["app"]));
    }

    #[test]
    fn a_dependency_with_no_row_is_the_action_caches_failure() {
        let lib = label("//:lib");
        let main = label("//:main.o");
        let main_row = row(&["main.o"]);
        let error = finished_key(
            &own(&["app"]),
            &[(&main, Some(&main_row)), (&lib, None), (&main, None)],
        )
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            "action cache: //:lib succeeded and has no row"
        );
        match error {
            Error::Adapter { port, .. } => assert_eq!(port, PortName::ActionCache),
            other => unreachable!("expected Adapter, got {other:?}"),
        }
    }

    #[test]
    fn a_row_stands_when_it_was_recorded_under_the_targets_key() {
        let row = row(&["app", "app.map"]);
        assert!(stands(Forcing::None, &key(&["app", "app.map"]), &row));
    }

    #[test]
    fn no_row_stands_for_a_target_declared_to_always_run() {
        let row = row(&["app"]);
        assert!(!stands(Forcing::Always, &key(&["app"]), &row));
    }

    #[test]
    fn a_rerun_test_row_does_not_stand() {
        let row = row(&["app"]);
        assert!(!stands(Forcing::Rerun, &key(&["app"]), &row));
    }

    #[test]
    fn a_rerun_build_target_row_stands() {
        let row = row(&["app"]);
        let forcing = Forcing::of(Freshness::Cached, TargetRole::Build, Rerun::Tests);
        assert!(stands(forcing, &key(&["app"]), &row));
    }

    #[test]
    fn a_row_recorded_under_another_key_does_not_stand() {
        let row = row(&["app"]);
        let mut changed = own(&["app"]);
        changed.command = command(&["cc", "-O2"]);
        let now = ActionKey::new(KeyComponents {
            own: changed,
            deps: Vec::new(),
        })
        .unwrap();
        assert!(!stands(Forcing::None, &now, &row));
    }

    /// A store holding the bytes given, and the manifest of each output given.
    struct Store {
        blobs: BTreeMap<Digest, Vec<u8>>,
    }

    impl Store {
        fn holding(contents: &[&str], manifests: &[&OutputManifest]) -> Self {
            let contents = contents.iter().map(|content| content.as_bytes().to_vec());
            let manifests = manifests
                .iter()
                .map(|manifest| manifest.to_bytes().unwrap());
            Self {
                blobs: contents
                    .chain(manifests)
                    .map(|bytes| (Digest::of(&bytes), bytes))
                    .collect(),
            }
        }

        fn without(mut self, digest: &Digest) -> Self {
            self.blobs.remove(digest);
            self
        }

        fn restorable(&self, row: &CacheRow) -> Option<Vec<OutputManifest>> {
            restorable(
                row,
                |digest| self.blobs.contains_key(digest),
                |digest| Ok(self.blobs.get(digest).cloned()),
            )
            .unwrap()
        }
    }

    /// The row of a build under the key of `names` that left `manifests`, one per name, and
    /// printed `out` and `err`.
    fn recorded(names: &[&str], manifests: &[&OutputManifest]) -> CacheRow {
        CacheRow::new(
            key(names),
            manifests
                .iter()
                .map(|manifest| digest_of(manifest))
                .collect(),
            Digest::of(b"out"),
            Digest::of(b"err"),
        )
        .unwrap()
    }

    #[test]
    fn a_row_is_restorable_when_everything_its_outputs_are_made_of_is_held() {
        let app = file("app");
        let site = tree(&[("index.html", "page"), ("assets/app.js", "script")]);
        let row = recorded(&["app", "site"], &[&app, &site]);
        let store = Store::holding(&["out", "err", "app", "page", "script"], &[&app, &site]);

        assert_eq!(store.restorable(&row), Some(vec![app, site]));
    }

    #[test]
    fn a_row_with_no_output_is_restorable_when_what_was_printed_is_held() {
        let row = recorded(&[], &[]);
        assert_eq!(
            Store::holding(&["out", "err"], &[]).restorable(&row),
            Some(Vec::new())
        );
        assert_eq!(Store::holding(&["out"], &[]).restorable(&row), None);
    }

    #[test]
    fn a_row_is_not_restorable_when_anything_it_needs_is_gone() {
        let app = file("app");
        let site = tree(&[("index.html", "page"), ("assets/app.js", "script")]);
        let row = recorded(&["app", "site"], &[&app, &site]);
        let whole = || Store::holding(&["out", "err", "app", "page", "script"], &[&app, &site]);

        for (gone, what) in [
            (Digest::of(b"out"), "what the action printed"),
            (Digest::of(b"err"), "what the action printed as errors"),
            (digest_of(&app), "the manifest of a file output"),
            (Digest::of(b"app"), "the file of a file output"),
            (digest_of(&site), "the manifest of a tree"),
            (Digest::of(b"page"), "one file of a tree"),
            (Digest::of(b"script"), "another file of a tree"),
        ] {
            assert_eq!(whole().without(&gone).restorable(&row), None, "{what}");
        }
        assert!(
            whole()
                .without(&Digest::of(b"something else"))
                .restorable(&row)
                .is_some(),
            "content the row does not need"
        );
    }

    #[test]
    fn a_manifest_whose_bytes_no_longer_read_as_one_is_as_good_as_gone() {
        let app = file("app");
        let row = recorded(&["app"], &[&app]);
        let mut store = Store::holding(&["out", "err", "app"], &[]);
        store
            .blobs
            .insert(digest_of(&app), b"not a manifest".to_vec());

        assert_eq!(store.restorable(&row), None);
    }

    #[test]
    fn a_store_that_cannot_read_a_manifest_fails_the_question() {
        let row = recorded(&["app"], &[&file("app")]);
        let unreadable = |_: &Digest| -> Result<Option<Vec<u8>>> {
            Err(Error::Adapter {
                port: PortName::ContentStore,
                diagnostic: Diagnostic::new("the store cannot be read"),
            })
        };

        let error = restorable(&row, |_| true, unreadable).unwrap_err();

        assert_eq!(error.to_string(), "content store: the store cannot be read");
    }
}
