//! The "built" rule: whether a program is one the build itself makes.
//!
//! Its own file because Plan and Execute must agree on it. A program one of a target's
//! dependencies makes needs no entry in the workspace ceiling: declaring the dependency in the
//! graph is the permission.
//!
//! Responsibilities: [`built`].
//!
//! Non-responsibilities: expanding a command into its program; and holding a program to the
//! ceiling when it is not built.

use crate::types::{OutputPath, Program, TreePath};

/// Whether `program` is built: it is the path of one of `deps`, or lies below one.
///
/// `deps` holds the output paths of the targets the command's own target depends on. Only the
/// text of the program is compared, so it does not matter how the command spelled it: written
/// out, named through a setting, or taken from `$deps`. A program that is not written as a path,
/// such as `go`, is never built. Below an output means a plain descent: a `.`, `..` or empty
/// segment after the output's path is not below it, since it could leave the output.
pub(crate) fn built(program: &Program, deps: &[OutputPath]) -> bool {
    let text = program.as_str();
    if !text.contains('/') {
        return false;
    }
    deps.iter().any(|dep| {
        text.strip_prefix(dep.as_str())
            .is_some_and(|rest| rest.is_empty() || descends(rest))
    })
}

/// Whether `rest` is a `/` followed by a path that stays inside the directory it starts from.
fn descends(rest: &str) -> bool {
    rest.strip_prefix('/')
        .is_some_and(|below| TreePath::parse(below).is_ok())
}

#[cfg(test)]
mod tests {
    use super::built;
    use crate::grants::fixtures::{output_path, program};

    #[test]
    fn a_dependency_output_is_built() {
        assert!(built(&program("out/gen"), &[output_path("//:gen", "gen")]));
    }

    #[test]
    fn a_path_below_a_directory_output_is_built() {
        assert!(built(
            &program("out/gen/bin/x"),
            &[output_path("//:gen", "gen")]
        ));
    }

    #[test]
    fn a_path_sharing_only_a_prefix_is_not_built() {
        assert!(!built(
            &program("out/gen2"),
            &[output_path("//:gen", "gen")]
        ));
    }

    #[test]
    fn a_path_with_no_dependency_is_not_built() {
        assert!(!built(&program("out/gen"), &[]));
    }

    #[test]
    fn a_host_program_is_not_built() {
        assert!(!built(&program("go"), &[output_path("//:gen", "gen")]));
    }

    #[test]
    fn an_input_path_is_not_built() {
        assert!(!built(
            &program("scripts/gen.sh"),
            &[output_path("//:gen", "gen")]
        ));
    }

    #[test]
    fn a_path_climbing_out_of_a_dependency_is_not_built() {
        assert!(!built(
            &program("out/gen/../x"),
            &[output_path("//:gen", "gen")]
        ));
    }

    #[test]
    fn a_path_with_a_dot_segment_is_not_built() {
        assert!(!built(
            &program("out/gen/./x"),
            &[output_path("//:gen", "gen")]
        ));
    }

    #[test]
    fn a_path_with_an_empty_segment_is_not_built() {
        assert!(!built(
            &program("out/gen//x"),
            &[output_path("//:gen", "gen")]
        ));
    }

    #[test]
    fn the_targets_own_output_is_not_built() {
        assert!(!built(&program("out/x"), &[output_path("//:gen", "gen")]));
    }
}
