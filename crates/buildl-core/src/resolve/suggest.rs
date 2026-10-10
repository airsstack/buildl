//! The nearest declared name to one that names nothing.
//!
//! Its own file because it is the one part of this phase that knows nothing about build files:
//! it compares rendered names, and the caller decides which names are candidates.
//!
//! Responsibilities: [`nearest`], and the edit distance it ranks by.
//!
//! Non-responsibilities: choosing the candidates. A dependency may be a target or an alias and a
//! rule reference only a rule; the caller filters before asking.

use core::fmt;

/// The candidate nearest to `wanted`, if one is near enough to be a likely misspelling.
///
/// Distance is the edit distance between the rendered texts. A candidate is near enough when
/// its distance is at most a third of `wanted`'s length in bytes, and never less than one edit.
/// Among equally near candidates the lowest by `T`'s own order is returned, so the answer does
/// not depend on the order `candidates` arrive in.
pub(crate) fn nearest<'a, T, I>(wanted: &str, candidates: I) -> Option<&'a T>
where
    T: fmt::Display + Ord + 'a,
    I: IntoIterator<Item = &'a T>,
{
    let limit = (wanted.len() / 3).max(1);
    candidates
        .into_iter()
        .map(|candidate| (distance(wanted, &candidate.to_string()), candidate))
        .filter(|(distance, _)| *distance <= limit)
        .min()
        .map(|(_, candidate)| candidate)
}

/// The Levenshtein distance between `a` and `b`, counted in bytes.
///
/// Bytes rather than characters because every name compared here is ASCII by its grammar.
fn distance(a: &str, b: &str) -> usize {
    let b = b.as_bytes();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (row, x) in a.bytes().enumerate() {
        let mut current = Vec::with_capacity(b.len() + 1);
        current.push(row + 1);
        for (column, y) in b.iter().enumerate() {
            let substitute = previous[column] + usize::from(x != *y);
            let delete = previous[column + 1] + 1;
            let insert = current[column] + 1;
            current.push(substitute.min(delete).min(insert));
        }
        previous = current;
    }
    previous[b.len()]
}

#[cfg(test)]
mod tests {
    use super::{distance, nearest};

    #[test]
    fn distance_counts_single_byte_edits() {
        assert_eq!(distance("", ""), 0);
        assert_eq!(distance("abc", "abc"), 0);
        assert_eq!(distance("", "abc"), 3);
        assert_eq!(distance("abc", ""), 3);
        assert_eq!(distance("//:mian.o", "//:main.o"), 2);
        assert_eq!(distance("kitten", "sitting"), 3);
        assert_eq!(distance("test_fliter", "test_filter"), 2);
    }

    #[test]
    fn the_nearest_candidate_within_the_limit_is_returned() {
        let candidates = ["//:main.o", "//:util.o", "//lib:text"].map(str::to_owned);
        assert_eq!(
            nearest("//:mian.o", &candidates),
            Some(&"//:main.o".to_owned())
        );
    }

    #[test]
    fn a_candidate_at_the_limit_is_offered_and_one_past_it_is_not() {
        // Nine bytes allow three edits.
        let at_limit = ["abcdefxyz".to_owned()];
        assert_eq!(nearest("abcdefghi", &at_limit), Some(&at_limit[0]));
        let past_limit = ["abcdewxyz".to_owned()];
        assert_eq!(nearest("abcdefghi", &past_limit), None);
    }

    #[test]
    fn a_short_name_still_allows_one_edit() {
        let candidates = ["b".to_owned()];
        assert_eq!(nearest("a", &candidates), Some(&candidates[0]));
        let far = ["bc".to_owned()];
        assert_eq!(nearest("a", &far), None);
    }

    #[test]
    fn equally_near_candidates_resolve_to_the_lowest_whatever_their_order() {
        let forward = ["//:ab".to_owned(), "//:ac".to_owned()];
        let backward = ["//:ac".to_owned(), "//:ab".to_owned()];
        assert_eq!(nearest("//:aa", &forward), Some(&"//:ab".to_owned()));
        assert_eq!(nearest("//:aa", &backward), Some(&"//:ab".to_owned()));
    }

    #[test]
    fn no_candidates_offer_nothing() {
        let none: [String; 0] = [];
        assert_eq!(nearest("//:app", &none), None);
    }
}
