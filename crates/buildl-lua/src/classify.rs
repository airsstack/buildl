//! Naming the kind of an airsl evaluation failure.
//!
//! Its own file because the mapping is the one place airsl's error vocabulary meets buildl's, and
//! an airsl upgrade that reshapes its errors should have to change only this. A refusal raised by
//! a `buildl` primitive never reaches here: it was recorded when it was raised, and that record
//! is reported instead.
//!
//! Responsibilities: [`classify`].
//!
//! Non-responsibilities: the diagnostic's text, which is airsl's own rendering of the error.

use airsl::mlua;
use buildl_core::{EvaluationFailure, EvaluationLimit};

/// The kind of failure `error` is.
pub(crate) fn classify(error: &airsl::Error) -> EvaluationFailure {
    match error {
        airsl::Error::InstructionLimit { .. } => EvaluationFailure::LimitReached {
            limit: EvaluationLimit::Instructions,
        },
        airsl::Error::MemoryLimit { .. } => EvaluationFailure::LimitReached {
            limit: EvaluationLimit::Memory,
        },
        airsl::Error::Lua { source, .. } => {
            if matches!(
                source.downcast_ref::<airsl::Error>(),
                Some(airsl::Error::Denied { .. })
            ) {
                EvaluationFailure::Refused
            } else if matches!(**source, mlua::Error::SyntaxError { .. }) {
                EvaluationFailure::Syntax
            } else {
                EvaluationFailure::Runtime
            }
        }
        _ => EvaluationFailure::Runtime,
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use airsl::modules::Glob;
    use airsl::{
        Engine, InstructionLimit, LanguageSurface, MemoryLimit, ModuleSet, Policy, ResourceLimits,
        Script,
    };
    use buildl_core::{EvaluationFailure, EvaluationLimit};

    use super::classify;

    fn failure_of(source: &str) -> EvaluationFailure {
        // dyn: `ModuleSet::insert` takes `Box<dyn HostModule>`; the set is airsl's extension seam.
        let mut modules = ModuleSet::new();
        modules.insert(Box::new(Glob::new())).unwrap();
        let engine = Engine::builder()
            .policy(
                Policy::confined()
                    .with_language(LanguageSurface::Minimal)
                    .with_limits(ResourceLimits::new(
                        Some(MemoryLimit::mebibytes(16)),
                        Some(InstructionLimit::count(1_000_000)),
                    )),
            )
            .stdlib(modules)
            .build()
            .unwrap();
        let error = engine
            .eval(&Script::from_source(source, "build.lua").unwrap())
            .unwrap_err();
        classify(&error)
    }

    #[test]
    fn any_other_airsl_failure_is_a_runtime_failure() {
        // dyn: `ModuleSet::insert` takes `Box<dyn HostModule>`; the set is airsl's extension seam.
        let mut modules = ModuleSet::new();
        modules.insert(Box::new(Glob::new())).unwrap();
        let duplicate = modules.insert(Box::new(Glob::new())).unwrap_err();
        assert!(
            matches!(duplicate, airsl::Error::DuplicateModule { .. }),
            "{duplicate}"
        );
        assert_eq!(classify(&duplicate), EvaluationFailure::Runtime);
    }

    #[test]
    fn a_chunk_that_does_not_compile_is_a_syntax_failure() {
        assert_eq!(failure_of("return ("), EvaluationFailure::Syntax);
    }

    #[test]
    fn a_raised_error_is_a_runtime_failure() {
        assert_eq!(failure_of("error('boom')"), EvaluationFailure::Runtime);
        assert_eq!(failure_of("local x = nil; x()"), EvaluationFailure::Runtime);
    }

    #[test]
    fn a_denied_operation_is_a_refusal() {
        assert_eq!(
            failure_of("airsstack.glob.walk('/', '*')"),
            EvaluationFailure::Refused
        );
    }

    #[test]
    fn an_endless_loop_reaches_the_instruction_ceiling() {
        assert_eq!(
            failure_of("while true do end"),
            EvaluationFailure::LimitReached {
                limit: EvaluationLimit::Instructions,
            }
        );
    }

    #[test]
    fn an_unbounded_allocation_reaches_the_memory_ceiling() {
        assert_eq!(
            failure_of("local s = string.rep('x', 64 * 1024 * 1024)"),
            EvaluationFailure::LimitReached {
                limit: EvaluationLimit::Memory,
            }
        );
    }
}
