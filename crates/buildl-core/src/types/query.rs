//! A question about the target graph, read from the text of a query.
//!
//! Its own file because the two forms a query takes, and the text they are written in, are a
//! grammar of their own: the target inside the parentheses is read by whoever calls
//! [`Query::parse`], so this file does not know what a relative target is relative to.
//!
//! Responsibilities: [`Query`], [`Direction`], and reading a query from its text.
//!
//! Non-responsibilities: answering a query, which the phase that holds the graph does; and
//! deciding what a relative target is relative to, which the caller of [`Query::parse`] does.

use crate::error::{Error, Result};
use crate::types::Label;

/// Which way a query walks the graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Toward what a target needs.
    Deps,
    /// Toward what needs a target.
    Rdeps,
}

impl Direction {
    /// The function name a query is written with.
    const fn name(self) -> &'static str {
        match self {
            Self::Deps => "deps",
            Self::Rdeps => "rdeps",
        }
    }
}

/// A question about one target: what it needs, or what needs it.
///
/// Written `deps(<target>)` or `rdeps(<target>)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    direction: Direction,
    target: Label,
}

impl Query {
    /// Joins a direction and the target it starts from.
    #[must_use]
    pub const fn new(direction: Direction, target: Label) -> Self {
        Self { direction, target }
    }

    /// Reads `raw` as `deps(<target>)` or `rdeps(<target>)`.
    ///
    /// Function names are lowercase and exact. Whitespace around the whole text and inside the
    /// parentheses is ignored. The target text goes to `label`, so the caller decides what a
    /// relative target is relative to.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidQuery`] holding `raw` as given when it is neither form, or when its
    /// target is empty or holds whitespace, a parenthesis or a comma. Returns whatever `label`
    /// returns when the target fails its own grammar.
    pub fn parse(raw: &str, label: impl FnOnce(&str) -> Result<Label>) -> Result<Self> {
        let invalid = || Error::InvalidQuery {
            expression: raw.to_owned(),
        };

        let text = raw.trim();
        let (direction, argument) = [Direction::Deps, Direction::Rdeps]
            .into_iter()
            .find_map(|direction| {
                let argument = text
                    .strip_prefix(direction.name())?
                    .strip_prefix('(')?
                    .strip_suffix(')')?;
                Some((direction, argument.trim()))
            })
            .ok_or_else(invalid)?;

        if argument.is_empty()
            || argument.contains(|c: char| c.is_whitespace() || matches!(c, '(' | ')' | ','))
        {
            return Err(invalid());
        }

        Ok(Self::new(direction, label(argument)?))
    }

    /// Which way the query walks.
    #[must_use]
    pub const fn direction(&self) -> Direction {
        self.direction
    }

    /// The target the query starts from.
    #[must_use]
    pub const fn target(&self) -> &Label {
        &self.target
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use super::*;
    use crate::types::Directory;

    fn label(raw: &str) -> Label {
        Label::parse(raw).unwrap()
    }

    #[test]
    fn each_function_reads_its_direction_and_target() {
        let deps = Query::parse("deps(//:app)", Label::parse).unwrap();
        assert_eq!(deps, Query::new(Direction::Deps, label("//:app")));
        let rdeps = Query::parse("rdeps(//lib:text)", Label::parse).unwrap();
        assert_eq!(rdeps, Query::new(Direction::Rdeps, label("//lib:text")));
    }

    #[test]
    fn whitespace_around_the_query_and_inside_the_parentheses_is_ignored() {
        let q = Query::parse("  rdeps(  //lib:text \t)\n", Label::parse).unwrap();
        assert_eq!(q, Query::new(Direction::Rdeps, label("//lib:text")));
    }

    #[test]
    fn the_target_is_read_by_the_callers_reader() {
        let base = Directory::parse("lib").unwrap();
        let q = Query::parse("deps(:text)", |raw| Label::resolve(raw, &base)).unwrap();
        assert_eq!(q.direction(), Direction::Deps);
        assert_eq!(q.target(), &label("//lib:text"));
    }

    #[test]
    fn a_query_in_neither_form_is_refused_as_written() {
        let refused = [
            "",
            "deps",
            "deps()",
            "deps(  )",
            "deps(//:a",
            "deps//:a)",
            "deps(//:a))",
            "deps((//:a))",
            "deps(//:a)x",
            "deps (//:a)",
            "DEPS(//:a)",
            "dep(//:a)",
            "reverse(//:a)",
            "deps(//:a,//:b)",
            "deps(//:a //:b)",
        ];
        for raw in refused {
            let err = Query::parse(raw, Label::parse).unwrap_err();
            assert!(
                matches!(&err, Error::InvalidQuery { expression } if expression == raw),
                "{raw:?}: {err:?}"
            );
        }
    }

    #[test]
    fn a_target_that_fails_its_grammar_is_the_readers_error() {
        let err = Query::parse("deps(//lib)", Label::parse).unwrap_err();
        assert!(matches!(err, Error::InvalidName { .. }), "{err:?}");
    }
}
