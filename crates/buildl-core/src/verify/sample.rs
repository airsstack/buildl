//! Which of a workspace's cached targets a verify re-runs.
//!
//! Its own file because the choice is one pure decision over keys: it needs no port, so it is
//! tested here against hand-built keys.
//!
//! Responsibilities: [`Chosen`] and [`chosen`].
//!
//! Non-responsibilities: knowing what a [`Sample`] is for, which [`types`](crate::types) does;
//! and re-running the targets chosen, which the flow that calls this does.

use crate::types::{ActionKey, Label, Sample};

/// The targets a sample re-runs, and how many it left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Chosen {
    /// The labels to re-run, in label order.
    pub(crate) rerun: Vec<Label>,
    /// How many candidates the sample left out.
    pub(crate) left_out: usize,
}

/// Chooses the candidates `sample` re-runs.
///
/// Each candidate is a label with its finished key. [`Sample::All`] takes every candidate;
/// [`Sample::Lowest`] takes that many with the lowest key digests, or all of them when it asks
/// for more than there are. Candidates with equal digests are taken in label order, so the
/// choice is the same whatever order `candidates` come in.
#[must_use]
pub(crate) fn chosen(mut candidates: Vec<(Label, ActionKey)>, sample: Sample) -> Chosen {
    let total = candidates.len();
    candidates.sort_by(|(left_label, left_key), (right_label, right_key)| {
        left_key
            .digest()
            .cmp(&right_key.digest())
            .then_with(|| left_label.cmp(right_label))
    });
    if let Sample::Lowest(count) = sample {
        candidates.truncate(count.get());
    }
    let mut rerun: Vec<Label> = candidates.into_iter().map(|(label, _)| label).collect();
    rerun.sort();
    Chosen {
        left_out: total - rerun.len(),
        rerun,
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use core::num::NonZeroUsize;

    use super::*;
    use crate::execute::fixtures::{key, label};

    fn lowest(count: usize) -> Sample {
        Sample::Lowest(NonZeroUsize::new(count).unwrap())
    }

    /// Six candidates, labelled `//p:a` to `//p:f`, each with a key of its own. Their key digests
    /// do not sort in label order, so the lowest three by digest are not the first three by label.
    fn candidates() -> Vec<(Label, ActionKey)> {
        ["a", "b", "c", "d", "e", "f"]
            .iter()
            .map(|name| (label(&format!("//p:{name}")), key(&[name])))
            .collect()
    }

    /// The labels of `candidates` ordered by key digest, found without `chosen`.
    fn by_digest(candidates: &[(Label, ActionKey)]) -> Vec<Label> {
        let mut digested: Vec<_> = candidates
            .iter()
            .map(|(label, key)| (key.digest(), label.clone()))
            .collect();
        digested.sort();
        digested.into_iter().map(|(_, label)| label).collect()
    }

    #[test]
    fn all_takes_every_candidate() {
        let chosen = chosen(candidates(), Sample::All);
        assert_eq!(
            chosen.rerun,
            ["a", "b", "c", "d", "e", "f"].map(|name| label(&format!("//p:{name}")))
        );
        assert_eq!(chosen.left_out, 0);
    }

    #[test]
    fn lowest_takes_the_lowest_key_digests() {
        let all = candidates();
        let mut expected: Vec<Label> = by_digest(&all).into_iter().take(3).collect();
        expected.sort();
        // The fixtures must tell digest order from label order, or this test proves nothing.
        let first_by_label: Vec<Label> =
            all.iter().take(3).map(|(label, _)| label.clone()).collect();
        assert_ne!(expected, first_by_label);
        let chosen = chosen(all, lowest(3));
        assert_eq!(chosen.rerun, expected);
    }

    #[test]
    fn lowest_above_the_count_takes_all() {
        let chosen = chosen(candidates(), lowest(9));
        assert_eq!(chosen.rerun.len(), 6);
        assert_eq!(chosen.left_out, 0);
    }

    #[test]
    fn the_choice_does_not_depend_on_the_order_given() {
        let forward = candidates();
        let mut backward = candidates();
        backward.reverse();
        let mut rotated = candidates();
        rotated.rotate_left(1);
        let expected = chosen(forward, lowest(3));
        assert_eq!(chosen(backward, lowest(3)), expected);
        assert_eq!(chosen(rotated, lowest(3)), expected);
    }

    #[test]
    fn the_left_out_count_is_the_rest() {
        assert_eq!(chosen(candidates(), lowest(1)).left_out, 5);
        assert_eq!(chosen(candidates(), lowest(3)).left_out, 3);
        assert_eq!(chosen(Vec::new(), lowest(3)).left_out, 0);
    }

    #[test]
    fn equal_digests_are_taken_in_label_order() {
        let tied = vec![(label("//p:b"), key(&["x"])), (label("//p:a"), key(&["x"]))];
        assert_eq!(chosen(tied, lowest(1)).rerun, vec![label("//p:a")]);
    }
}
