//! The generated section of `build.lua`, written from Cargo metadata.
//!
//! [`section`] turns a [`CargoProject`] into the lines that sit between the two marker lines of
//! `build.lua`. The text follows from the metadata alone, so the same `Cargo.toml` always gives
//! the same bytes.

use std::collections::BTreeMap;
use std::fmt;

use buildl::buildl_core::TargetName;
use buildl::cli::Exit;

use crate::metadata::{CargoProject, Package, Target};

/// A name the generated section cannot carry, worded as `error: <what>: <detail>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Refusal {
    message: String,
}

impl Refusal {
    /// The exit status of a command that was refused: the project's names are at fault.
    pub(crate) const EXIT: Exit = Exit::Definition;
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Refusal {}

/// Kinds of target that are not a library: cargo builds them with their package, or a plain
/// `cargo build` skips them.
const NOT_A_LIBRARY: [&str; 5] = ["bin", "example", "test", "bench", "custom-build"];

/// A generated target: the name it is declared under and the line that declares it.
struct Call {
    name: String,
    text: String,
}

/// The lines between the markers of `build.lua`, each ending in `\n`.
///
/// Packages come sorted by name; for each, its library when it builds a `cdylib` or `staticlib`,
/// then its binaries sorted by name, then its test. A binary with `required-features` gets no
/// call, because a plain `cargo build` skips it. No `profile` is written.
///
/// # Errors
///
/// A target name that buildl's target-name rule refuses, or two generated targets with one name,
/// is a [`Refusal`] naming the target and the package or packages.
pub(crate) fn section(project: &CargoProject) -> Result<String, Refusal> {
    let mut packages: Vec<&Package> = project.packages.iter().collect();
    packages.sort_by(|a, b| a.name.cmp(&b.name));

    let mut claimed: BTreeMap<String, &str> = BTreeMap::new();
    let mut text = String::from("local rust = buildl.use(\"rust\")\n\n");
    for package in packages {
        for call in package_calls(package) {
            claim(&mut claimed, &call.name, &package.name)?;
            text.push_str(&call.text);
            text.push('\n');
        }
    }
    Ok(text)
}

/// The calls of one package, in the order they are written.
fn package_calls(package: &Package) -> Vec<Call> {
    let name = package.name.as_str();
    let mut calls: Vec<Call> = package
        .targets
        .iter()
        .filter_map(|target| library_call(name, target))
        .collect();

    let mut bins: Vec<&Target> = package
        .targets
        .iter()
        .filter(|target| {
            target.kind.iter().any(|kind| kind == "bin") && target.required_features.is_empty()
        })
        .collect();
    bins.sort_by(|a, b| a.name.cmp(&b.name));
    calls.extend(bins.into_iter().map(|bin| Call {
        name: bin.name.clone(),
        text: format!(
            "rust.binary(\"{}\", {{ args = rust.package(\"{name}\") }})",
            bin.name
        ),
    }));

    let test = format!("{name}-test");
    calls.push(Call {
        text: format!("rust.test(\"{test}\", {{ args = rust.package(\"{name}\") }})"),
        name: test,
    });
    calls
}

/// The `rust.library` call for a library target that builds a `cdylib` or `staticlib`.
fn library_call(package: &str, target: &Target) -> Option<Call> {
    if target
        .kind
        .iter()
        .any(|kind| NOT_A_LIBRARY.contains(&kind.as_str()))
    {
        return None;
    }
    let kinds: Vec<String> = target
        .crate_types
        .iter()
        .filter(|crate_type| matches!(crate_type.as_str(), "cdylib" | "staticlib"))
        .map(|crate_type| format!("\"{crate_type}\""))
        .collect();
    if kinds.is_empty() {
        return None;
    }
    Some(Call {
        name: target.name.clone(),
        text: format!(
            "rust.library(\"{}\", {{ kind = {{ {} }}, args = rust.package(\"{package}\") }})",
            target.name,
            kinds.join(", ")
        ),
    })
}

/// Records that `package` declares `name`, refusing a name the grammar rejects or one already
/// declared.
fn claim<'a>(
    claimed: &mut BTreeMap<String, &'a str>,
    name: &str,
    package: &'a str,
) -> Result<(), Refusal> {
    if let Err(error) = TargetName::parse(name) {
        return Err(Refusal {
            message: format!(
                "error: target \"{name}\" of package \"{package}\" cannot be declared: {error}"
            ),
        });
    }
    if let Some(first) = claimed.insert(name.to_owned(), package) {
        let packages = if first == package {
            format!("package \"{package}\" twice")
        } else {
            format!("packages \"{first}\" and \"{package}\"")
        };
        return Err(Refusal {
            message: format!(
                "error: target \"{name}\" cannot be declared: it would be declared by {packages}"
            ),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use super::*;
    use crate::metadata::parse;

    fn project(json: &[u8]) -> CargoProject {
        parse(json).unwrap()
    }

    fn generated(json: &[u8]) -> String {
        section(&project(json)).unwrap()
    }

    #[test]
    fn rust_workspace_gives_three_tests_and_one_binary() {
        let text = generated(include_bytes!(
            "../tests/fixtures/metadata/rust-workspace.json"
        ));
        assert_eq!(
            text,
            "local rust = buildl.use(\"rust\")\n\
             \n\
             rust.test(\"answer-test\", { args = rust.package(\"answer\") })\n\
             rust.binary(\"cli\", { args = rust.package(\"cli\") })\n\
             rust.test(\"cli-test\", { args = rust.package(\"cli\") })\n\
             rust.test(\"greeting-test\", { args = rust.package(\"greeting\") })\n"
        );
    }

    #[test]
    fn a_library_lists_its_crate_types_in_cargo_order() {
        let text = generated(include_bytes!("../tests/fixtures/metadata/my-lib.json"));
        assert_eq!(
            text,
            "local rust = buildl.use(\"rust\")\n\
             \n\
             rust.library(\"my_lib\", { kind = { \"cdylib\", \"staticlib\" }, args = rust.package(\"my-lib\") })\n\
             rust.binary(\"extra\", { args = rust.package(\"my-lib\") })\n\
             rust.binary(\"tool-a\", { args = rust.package(\"my-lib\") })\n\
             rust.test(\"my-lib-test\", { args = rust.package(\"my-lib\") })\n"
        );
    }

    #[test]
    fn rlib_proc_macro_example_and_build_script_get_no_call() {
        // rust-workspace holds an rlib and a proc-macro library; my-lib an example, a test
        // target and a build script.
        let workspace = generated(include_bytes!(
            "../tests/fixtures/metadata/rust-workspace.json"
        ));
        assert!(!workspace.contains("rust.library"), "{workspace}");
        let my_lib = generated(include_bytes!("../tests/fixtures/metadata/my-lib.json"));
        for skipped in ["\"ex\"", "\"it\"", "build-script-build"] {
            assert!(!my_lib.contains(skipped), "{my_lib}");
        }
    }

    #[test]
    fn an_example_library_gets_no_call() {
        let text = generated(include_bytes!(
            "../tests/fixtures/metadata/example-library.json"
        ));
        assert!(!text.contains("rust.library"), "{text}");
        assert!(!text.contains("cdylib"), "{text}");
    }

    #[test]
    fn a_bin_with_required_features_gets_no_call() {
        let text = generated(include_bytes!(
            "../tests/fixtures/metadata/required-features.json"
        ));
        assert!(!text.contains("rust.binary(\"feat-tool\""), "{text}");
        assert!(text.contains("rust.binary(\"plain\""), "{text}");
    }

    #[test]
    fn a_name_outside_the_grammar_is_refused() {
        let refusal = section(&project(include_bytes!(
            "../tests/fixtures/metadata/non-ascii-name.json"
        )))
        .unwrap_err();
        assert_eq!(Refusal::EXIT, Exit::Definition);
        let message = refusal.to_string();
        assert!(message.starts_with("error: target \"café\""), "{message}");
        assert!(message.contains("package \"café\""), "{message}");
    }

    #[test]
    fn two_targets_with_one_name_are_refused() {
        let refusal = section(&project(include_bytes!(
            "../tests/fixtures/metadata/same-bin-name.json"
        )))
        .unwrap_err();
        let message = refusal.to_string();
        assert!(
            message.starts_with("error: target \"tool\" cannot be declared: "),
            "{message}"
        );
        assert!(message.contains("\"one\""), "{message}");
        assert!(message.contains("\"two\""), "{message}");
    }

    #[test]
    fn the_same_metadata_gives_the_same_bytes() {
        let json = include_bytes!("../tests/fixtures/metadata/rust-workspace.json");
        assert_eq!(generated(json), generated(json));
    }
}
