//! Settling a target that may start: must it run, or does what its last build recorded still
//! stand?
//!
//! Its own file because the question can only be answered once a target's dependencies have
//! finished: a key holds what each dependency produced, and that is known only then. So the
//! key is finished here, during the build, and compared with the recorded one.
//!
//! Responsibilities: [`finished_key`], [`is_cached`], and [`settle`] with [`Settled`], which
//! asks the ports what the two decisions need.
//!
//! Non-responsibilities: keeping rows and content, which are the ports'; and what happens to a
//! target once it is settled.

use crate::error::{Error, PortName, Result};
use crate::ports::{ActionCache, ContentStore};
use crate::types::{
    ActionKey, CacheRow, DepProducts, Diagnostic, Digest, Freshness, KeyComponents, Label,
    OwnComponents,
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

/// Whether a target need not run: what its last build recorded still stands.
///
/// It does when the target is not declared to run on every build, `row` exists, `key` has the
/// digest of the row's key, and `held` answers true for every digest the row holds: each
/// output, and what the action printed. A row whose content is gone is no reason to skip the
/// action, since there would be nothing to put in place.
pub(crate) fn is_cached(
    freshness: Freshness,
    key: &ActionKey,
    row: Option<&CacheRow>,
    held: impl Fn(&Digest) -> bool,
) -> bool {
    freshness != Freshness::Always
        && row.is_some_and(|row| {
            row.key().digest() == key.digest() && row.digests().all(|digest| held(&digest))
        })
}

/// What settling a target that may start found.
#[derive(Debug)]
pub(crate) enum Settled {
    /// It need not run: what its last build recorded, which is this row, still stands.
    Cached(CacheRow),
    /// It must run, under this key.
    Runs(ActionKey),
}

/// Settles the target `label`, whose dependencies `deps` have all succeeded.
///
/// Its key is finished from `own` and the row `cache` holds for each dependency, in the order
/// the dependencies were written. It is cached when [`is_cached`] says so of that key, the
/// row `cache` holds for the target itself, and what `cas` still holds.
///
/// # Errors
///
/// Returns whatever [`finished_key`] returns.
pub(crate) fn settle<A: ActionCache, C: ContentStore>(
    label: &Label,
    freshness: Freshness,
    own: &OwnComponents,
    deps: &[&Label],
    cache: &A,
    cas: &C,
) -> Result<Settled> {
    let rows: Vec<(&Label, Option<&CacheRow>)> =
        deps.iter().map(|dep| (*dep, cache.row(dep))).collect();
    let key = finished_key(own, &rows)?;
    let standing = cache
        .row(label)
        .filter(|row| is_cached(freshness, &key, Some(row), |digest| cas.contains(digest)))
        .cloned();
    Ok(standing.map_or_else(|| Settled::Runs(key), Settled::Cached))
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use super::{finished_key, is_cached};
    use crate::error::{Error, PortName};
    use crate::execute::fixtures::{command, key, label, own, row};
    use crate::types::{ActionKey, DepProducts, Digest, Freshness, KeyComponents};

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
    fn a_target_is_cached_when_its_row_has_its_key_and_everything_the_row_names_is_held() {
        let row = row(&["app", "app.map"]);
        assert!(is_cached(
            Freshness::Cached,
            &key(&["app", "app.map"]),
            Some(&row),
            |_| true
        ));
    }

    #[test]
    fn a_target_declared_to_always_run_is_never_cached() {
        let row = row(&["app"]);
        assert!(!is_cached(
            Freshness::Always,
            &key(&["app"]),
            Some(&row),
            |_| true
        ));
    }

    #[test]
    fn a_target_with_no_row_is_not_cached() {
        assert!(!is_cached(Freshness::Cached, &key(&["app"]), None, |_| {
            true
        }));
    }

    #[test]
    fn a_target_whose_key_is_not_its_rows_is_not_cached() {
        let row = row(&["app"]);
        let mut changed = own(&["app"]);
        changed.command = command(&["cc", "-O2"]);
        let now = ActionKey::new(KeyComponents {
            own: changed,
            deps: Vec::new(),
        })
        .unwrap();
        assert!(!is_cached(Freshness::Cached, &now, Some(&row), |_| true));
    }

    #[test]
    fn a_target_is_not_cached_when_any_digest_of_its_row_is_gone() {
        let row = row(&["app", "app.map"]);
        let key = key(&["app", "app.map"]);
        for (gone, what) in [
            (Digest::of(b"app"), "the first output"),
            (Digest::of(b"app.map"), "the second output"),
            (Digest::of(b"out"), "what the action printed"),
            (Digest::of(b"err"), "what the action printed as errors"),
        ] {
            assert!(
                !is_cached(Freshness::Cached, &key, Some(&row), |digest| *digest
                    != gone),
                "{what}"
            );
        }
        assert!(
            is_cached(Freshness::Cached, &key, Some(&row), |digest| *digest
                != Digest::of(b"something else")),
            "content the row does not name"
        );
    }
}
