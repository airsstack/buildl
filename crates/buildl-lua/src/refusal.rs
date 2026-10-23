//! Why a `buildl` primitive turned a call down.
//!
//! Its own file because three different readers produce a refusal — the value readers, the
//! primitives and `buildl.sources` — and none of them knows where the call was made. The refusal
//! carries what they do know; the module that installed the primitive adds the call site when it
//! renders the diagnostic.
//!
//! Responsibilities: [`Refusal`], the [`CallSite`] it is rendered with, and its rendering into a
//! [`Diagnostic`].
//!
//! Non-responsibilities: recording a refusal so it sticks, which the staging buffer does.

use buildl_core::{Diagnostic, EvaluationFailure};

/// Where in Lua code a primitive was called.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CallSite {
    /// A line of the build file.
    BuildFile { line: usize },
    /// A line of a plugin's file, which the workspace names `chunk`.
    Plugin { chunk: String, line: usize },
}

/// A primitive call that was refused: the failure's kind, what was wrong, and — once known — the
/// name the call declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Refusal {
    failure: EvaluationFailure,
    detail: String,
    subject: Option<String>,
}

impl Refusal {
    /// A refusal of kind `failure`, explained by `detail`.
    pub(crate) fn new(failure: EvaluationFailure, detail: impl Into<String>) -> Self {
        Self {
            failure,
            detail: detail.into(),
            subject: None,
        }
    }

    /// The same refusal, attributed to the declaration named `subject`.
    #[must_use]
    pub(crate) fn about(mut self, subject: &str) -> Self {
        self.subject = Some(subject.to_owned());
        self
    }

    /// The failure's kind.
    pub(crate) const fn failure(&self) -> &EvaluationFailure {
        &self.failure
    }

    /// Renders the refusal for `buildl.<primitive>`, prefixed with where the call was made when
    /// that is known: `line N: ` for the build file, `<file>:N: ` for a plugin's file.
    pub(crate) fn diagnostic(&self, primitive: &str, site: Option<&CallSite>) -> Diagnostic {
        let location = match site {
            None => String::new(),
            Some(CallSite::BuildFile { line }) => format!("line {line}: "),
            Some(CallSite::Plugin { chunk, line }) => format!("{chunk}:{line}: "),
        };
        let subject = self
            .subject
            .as_ref()
            .map_or_else(String::new, |subject| format!(" '{subject}'"));
        Diagnostic::new(format!(
            "{location}buildl.{primitive}{subject}: {}",
            self.detail
        ))
    }
}

#[cfg(test)]
mod tests {
    use buildl_core::{EvaluationFailure, Written};

    use super::{CallSite, Refusal};

    fn wrong_run() -> Refusal {
        Refusal::new(
            EvaluationFailure::WrongFieldType {
                field: Written::new("run"),
            },
            "field 'run' must be a list of strings, found number",
        )
    }

    #[test]
    fn renders_line_primitive_subject_and_detail() {
        let site = CallSite::BuildFile { line: 12 };
        let diagnostic = wrong_run().about("app").diagnostic("target", Some(&site));
        assert_eq!(
            diagnostic.as_str(),
            "line 12: buildl.target 'app': field 'run' must be a list of strings, found number"
        );
    }

    #[test]
    fn names_the_plugin_file_for_a_call_made_there() {
        let site = CallSite::Plugin {
            chunk: "plugins/go/main.lua".to_owned(),
            line: 6,
        };
        let diagnostic = wrong_run().about("app").diagnostic("target", Some(&site));
        assert_eq!(
            diagnostic.as_str(),
            "plugins/go/main.lua:6: buildl.target 'app': field 'run' must be a list of strings, found number"
        );
    }

    #[test]
    fn omits_what_is_not_known() {
        let diagnostic = wrong_run().diagnostic("rule", None);
        assert_eq!(
            diagnostic.as_str(),
            "buildl.rule: field 'run' must be a list of strings, found number"
        );
    }

    #[test]
    fn keeps_the_failure_kind() {
        assert_eq!(
            wrong_run().about("app").failure(),
            &EvaluationFailure::WrongFieldType {
                field: Written::new("run"),
            }
        );
    }
}
