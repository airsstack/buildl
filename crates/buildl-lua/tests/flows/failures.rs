//! Every way a build file can fail maps onto one `EvaluationFailure`, with a usable diagnostic.

use buildl_core::{EvaluationFailure, EvaluationLimit, Written};

use crate::common::failure_of;

#[test]
fn a_file_that_does_not_parse_is_a_syntax_failure() {
    let (failure, diagnostic) = failure_of("buildl.target(");
    assert_eq!(failure, EvaluationFailure::Syntax);
    assert!(diagnostic.as_str().contains("build.lua:1:"), "{diagnostic}");
}

#[test]
fn a_raised_error_is_a_runtime_failure() {
    let (failure, diagnostic) = failure_of("\nerror('boom')");
    assert_eq!(failure, EvaluationFailure::Runtime);
    assert!(
        diagnostic.as_str().contains("build.lua:2: boom"),
        "{diagnostic}"
    );
}

#[test]
fn reading_through_airsl_s_glob_is_refused() {
    let (failure, diagnostic) = failure_of("airsstack.glob.walk('/', '*')");
    assert_eq!(failure, EvaluationFailure::Refused);
    assert!(
        diagnostic.as_str().contains("glob.walk denied"),
        "{diagnostic}"
    );
}

#[test]
fn an_endless_loop_reaches_the_instruction_ceiling() {
    let (failure, _) = failure_of("while true do end");
    assert_eq!(
        failure,
        EvaluationFailure::LimitReached {
            limit: EvaluationLimit::Instructions,
        }
    );
}

#[test]
fn a_huge_allocation_reaches_the_memory_ceiling() {
    let (failure, _) = failure_of("local s = string.rep('x', 64 * 1024 * 1024)");
    assert_eq!(
        failure,
        EvaluationFailure::LimitReached {
            limit: EvaluationLimit::Memory,
        }
    );
}

#[test]
fn staging_past_the_staging_budget_reaches_the_staging_limit() {
    let (failure, diagnostic) = failure_of("for i = 1, 200000 do buildl.alias('a' .. i, 'b') end");
    assert_eq!(
        failure,
        EvaluationFailure::LimitReached {
            limit: EvaluationLimit::Staging,
        }
    );
    assert!(
        diagnostic
            .as_str()
            .ends_with("buildl.alias: staging budget of 16777216 bytes exceeded"),
        "{diagnostic}"
    );
}

#[test]
fn an_unknown_option_field_is_named() {
    let (failure, diagnostic) = failure_of("buildl.target('app', { run = { 'cc' }, srcs = {} })");
    assert_eq!(
        failure,
        EvaluationFailure::UnknownField {
            field: Written::new("srcs"),
        }
    );
    assert_eq!(
        diagnostic.as_str(),
        "line 1: buildl.target 'app': unknown field 'srcs'"
    );
}

#[test]
fn a_wrongly_typed_field_is_named() {
    let (failure, diagnostic) = failure_of("\n\nbuildl.rule('cc', { run = 'cc -c' })");
    assert_eq!(
        failure,
        EvaluationFailure::WrongFieldType {
            field: Written::new("run"),
        }
    );
    assert_eq!(
        diagnostic.as_str(),
        "line 3: buildl.rule 'cc': field 'run' must be a list of strings, found string"
    );
}

#[test]
fn a_refusal_caught_with_pcall_still_fails_the_file() {
    let (failure, diagnostic) = failure_of(
        "local ok = pcall(buildl.option, 'mode', { default = 1 })\n\
         buildl.alias('default', 'app')",
    );
    assert_eq!(
        failure,
        EvaluationFailure::WrongFieldType {
            field: Written::new("default"),
        }
    );
    assert_eq!(
        diagnostic.as_str(),
        "line 1: buildl.option 'mode': field 'default' must be a string, found integer"
    );
}
