//! The answer to a query as lines for a person: one label a line.
//!
//! Its own file because it renders a set of labels, which no other rendering shows. One label a
//! line suits a person reading the answer and a shell loop consuming it.
//!
//! Responsibilities: [`query_text`].
//!
//! Non-responsibilities: answering the query, which the phase that resolves does; and the
//! canonical JSON of the answer, which is the set's own serialized form.

use std::collections::BTreeSet;

use buildl_core::Label;

/// `labels` as text: each label on its own line, in the set's order, each ending with a
/// newline. An empty set is the empty string.
#[must_use]
pub fn query_text(labels: &BTreeSet<Label>) -> String {
    let mut text = String::new();
    for label in labels {
        text.push_str(&label.to_string());
        text.push('\n');
    }
    text
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeSet;

    use buildl_core::Label;

    use super::query_text;

    fn labels(raw: &[&str]) -> BTreeSet<Label> {
        raw.iter()
            .map(|label| Label::parse(label).unwrap())
            .collect()
    }

    #[test]
    fn an_empty_answer_is_no_text() {
        assert_eq!(query_text(&BTreeSet::new()), "");
    }

    #[test]
    fn each_label_is_one_line_in_label_order() {
        assert_eq!(
            query_text(&labels(&["//lib:a", "//a-b:x", "//:z", "//a:x"])),
            "//:z\n//a:x\n//a-b:x\n//lib:a\n"
        );
    }
}
