//! How a reference to a build setting is written inside other text, and how one is found.
//!
//! Its own file because three readers must agree on it: an argument and a description each list
//! the settings they refer to, and the executor replaces every reference where it stands. They
//! all read text through the functions here, so no two of them can disagree on where a
//! reference starts or ends.
//!
//! Responsibilities: [`MARKER`], [`leading`], [`cut`] and [`names`].
//!
//! Non-responsibilities: whether the name a reference holds is one a setting can have, or one
//! that is declared. A name is returned as written.

use crate::types::grammar;

/// The text that starts a reference to a build setting.
pub(crate) const MARKER: &str = "$opt:";

/// The longest run of name characters `text` starts with — ASCII letters, digits, `-` and `_` —
/// and what follows it. The run may be empty.
fn name_run(text: &str) -> (&str, &str) {
    let end = text
        .bytes()
        .position(|byte| !grammar::is_key_byte(byte))
        .unwrap_or(text.len());
    text.split_at(end)
}

/// The name of the reference `text` starts with, and what follows that name; `None` when `text`
/// does not start with `$opt:` and a name character.
pub(crate) fn leading(text: &str) -> Option<(&str, &str)> {
    let (name, tail) = name_run(text.strip_prefix(MARKER)?);
    (!name.is_empty()).then_some((name, tail))
}

/// `text` cut at its first `$opt:`: what comes before it, the run of name characters after it,
/// and what follows that run. The run is empty when the `$opt:` starts no reference.
pub(crate) fn cut(text: &str) -> Option<(&str, &str, &str)> {
    let (before, after) = text.split_once(MARKER)?;
    let (name, tail) = name_run(after);
    Some((before, name, tail))
}

/// The setting names `text` refers to, in the order they appear.
///
/// A reference is `$opt:` followed by a name: the longest run of ASCII letters, digits, `-` and
/// `_`. An `$opt:` with no such character after it is not a reference.
pub(crate) fn names(text: &str) -> Vec<&str> {
    let mut names = Vec::new();
    let mut rest = text;
    while let Some((_, name, tail)) = cut(rest) {
        if !name.is_empty() {
            names.push(name);
        }
        rest = tail;
    }
    names
}

#[cfg(test)]
mod tests {
    use super::{MARKER, cut, leading, names};

    #[test]
    fn a_reference_at_the_start_yields_its_name_and_what_follows() {
        assert_eq!(leading("$opt:mode"), Some(("mode", "")));
        assert_eq!(
            leading("$opt:opt-level_2/bin"),
            Some(("opt-level_2", "/bin"))
        );
        assert_eq!(leading("$opt:a$opt:b"), Some(("a", "$opt:b")));
    }

    #[test]
    fn text_that_does_not_start_with_a_reference_has_no_leading_one() {
        assert_eq!(leading(""), None);
        assert_eq!(leading("$opt:"), None, "a marker and no name");
        assert_eq!(leading("$opt:.x"), None, "no name character after it");
        assert_eq!(leading("$opt"), None, "half a marker");
        assert_eq!(leading("-$opt:mode"), None, "a reference further on");
        assert_eq!(leading("$in"), None);
    }

    #[test]
    fn text_is_cut_at_its_first_marker() {
        assert_eq!(cut("-O$opt:level -g"), Some(("-O", "level", " -g")));
        assert_eq!(cut("$opt:a$opt:b"), Some(("", "a", "$opt:b")));
        assert_eq!(cut("to $opt: go"), Some(("to ", "", " go")), "an empty run");
        assert_eq!(cut("$in $out"), None);
        assert_eq!(cut(""), None);
    }

    #[test]
    fn names_are_listed_in_order_and_a_marker_with_no_name_is_passed_over() {
        assert_eq!(names("$opt:a/$opt:b $opt:a"), ["a", "b", "a"]);
        assert_eq!(
            names("$opt:a-$opt:b"),
            ["a-", "b"],
            "'-' is a name character"
        );
        assert_eq!(names("$opt: $opt:late"), ["late"]);
        assert_eq!(names("$in $out $deps $option"), Vec::<&str>::new());
        assert_eq!(names(""), Vec::<&str>::new());
    }

    #[test]
    fn a_name_is_returned_as_written_however_long() {
        let long = "n".repeat(300);
        let text = format!("{MARKER}{long}.o");
        assert_eq!(names(&text), [long.as_str()]);
        assert_eq!(leading(&text), Some((long.as_str(), ".o")));
    }

    #[test]
    fn reading_a_text_dollar_by_dollar_finds_the_references_it_lists() {
        for text in [
            "$opt:a",
            "x$opt:a$opt:b.c",
            "$opt:$opt:a",
            "$$opt:a",
            "$opt",
            "$in $opt:a-b $out",
        ] {
            let mut found = Vec::new();
            for (at, _) in text.match_indices('$') {
                let (_, from) = text.split_at(at);
                found.extend(leading(from).map(|(name, _)| name));
            }
            assert_eq!(found, names(text), "{text}");
        }
    }
}
