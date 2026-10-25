//! Where a table of `buildl.toml` is first written.
//!
//! Its own file because `toml`'s spans cover the values a table holds and nothing else: a table
//! that no header opens, as `remote` is by `[remote.token]` or by a dotted `remote.url`, has no
//! span, and a refusal of the table as a whole — a missing key it must hold — still has to say
//! where to look. Reading the document again with an editor-grade parser, which keeps the places
//! of keys and headers, answers that.
//!
//! Responsibilities: [`table_written_at`].
//!
//! Non-responsibilities: reading any value, and deciding what a table must hold.

use core::ops::Range;

use toml_edit::{ImDocument, Item};

/// The span where the table at `path` is first written: its header when a header opens it, and
/// otherwise the key that names it, as in `remote.url = …` or `remote = { … }`, or the first
/// segment of a longer header such as `[remote.token]`.
///
/// `None` when the text is not a document, no table is at `path`, or `path` is empty.
pub(crate) fn table_written_at(text: &str, path: &[&str]) -> Option<Range<usize>> {
    let document = ImDocument::parse(text).ok()?;
    let (last, parents) = path.split_last()?;
    let mut table = document.as_table() as &dyn toml_edit::TableLike;
    for name in parents {
        table = table.get(name)?.as_table_like()?;
    }
    let (key, item) = table.get_key_value(last)?;
    match item {
        Item::Table(header) => header.span().or_else(|| key.span()),
        _ => key.span(),
    }
}

#[cfg(test)]
mod tests {
    use super::table_written_at;

    fn start(text: &str, path: &[&str]) -> Option<usize> {
        table_written_at(text, path).map(|span| span.start)
    }

    #[test]
    fn a_table_with_a_header_is_written_at_the_bracket() {
        let text = "[workspace]\n\n[remote]\nurl = \"u\"\n";
        assert_eq!(start(text, &["remote"]), Some(13));
    }

    #[test]
    fn an_implied_table_is_written_at_the_name_it_is_first_given() {
        assert_eq!(start("[remote.token]\n", &["remote"]), Some(1));
        assert_eq!(start("remote.url = \"u\"\n", &["remote"]), Some(0));
        assert_eq!(start("a = 1\nremote = {}\n", &["remote"]), Some(6));
    }

    #[test]
    fn a_nested_table_is_found_through_its_parents() {
        let text = "[remote]\nurl = \"u\"\n\n[remote.token]\n";
        assert_eq!(start(text, &["remote", "token"]), Some(20));
        let inline = "[remote]\ntoken = {}\n";
        assert_eq!(start(inline, &["remote", "token"]), Some(9));
        let dotted = "[remote]\ntoken.env = \"T\"\n";
        assert_eq!(start(dotted, &["remote", "token"]), Some(9));
    }

    #[test]
    fn no_table_at_the_path_gives_no_place() {
        assert_eq!(start("[workspace]\n", &["remote"]), None);
        assert_eq!(start("remote = 1\n", &["remote", "token"]), None);
        assert_eq!(start("[remote\n", &["remote"]), None);
        assert_eq!(start("[remote]\n", &[]), None);
    }
}
