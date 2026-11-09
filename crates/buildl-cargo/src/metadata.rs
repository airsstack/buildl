//! The Cargo project as `cargo metadata` describes it: the workspace root and, for each member
//! package, the targets it builds.
//!
//! [`read`] runs cargo with `--no-deps`, so only the workspace members are listed and no
//! `Cargo.lock` is written; [`parse`] reads the fields the rest of the crate needs and ignores
//! the others.

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

use buildl::cli::Exit;
use serde::Deserialize;

/// The Cargo project in a folder: what `cargo metadata --no-deps` reports about it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct CargoProject {
    /// The folder holding the `Cargo.toml` that names the workspace, or the package when there
    /// is no workspace.
    pub(crate) workspace_root: PathBuf,
    /// The member packages, in the order cargo lists them.
    pub(crate) packages: Vec<Package>,
}

/// A package of the workspace.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct Package {
    /// The name in `[package]`, as `cargo build -p` takes it.
    pub(crate) name: String,
    /// What the package builds, in the order cargo lists them.
    pub(crate) targets: Vec<Target>,
}

/// A target of a package: a library, a binary, an example, a test, a bench or a build script.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct Target {
    /// The target's name; a library's has `-` turned into `_`.
    pub(crate) name: String,
    /// What cargo calls the target: `bin`, `example`, `test`, `bench`, `custom-build`, `lib`,
    /// `proc-macro`, or a library's crate types (`cdylib`, `staticlib`, `rlib`, `dylib`).
    pub(crate) kind: Vec<String>,
    /// The kinds of file the target produces, in cargo's order.
    pub(crate) crate_types: Vec<String>,
    /// The features that must be on for cargo to build the target; empty when it has no such
    /// condition.
    #[serde(rename = "required-features", default)]
    pub(crate) required_features: Vec<String>,
}

/// Why the Cargo project could not be read, and the exit status that tells it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MetadataError {
    exit: Exit,
    message: String,
}

impl MetadataError {
    /// The exit status of the command that failed.
    pub(crate) const fn exit(&self) -> Exit {
        self.exit
    }
}

impl fmt::Display for MetadataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for MetadataError {}

/// The program that runs cargo: the one `CARGO` names, or `cargo` when it is unset.
///
/// Cargo sets `CARGO` for a subcommand it starts; run on its own, the binary finds `cargo` on
/// the `PATH`.
pub(crate) fn program(cargo_var: Option<OsString>) -> OsString {
    cargo_var.unwrap_or_else(|| OsString::from("cargo"))
}

/// Runs `cargo metadata --format-version 1 --no-deps` with `cargo` in `dir` and reads the result.
///
/// A cargo that cannot be started, and output that is not metadata, end with
/// [`Exit::Infrastructure`]. A cargo that exits non-zero, such as one that refuses the manifest,
/// ends with [`Exit::Definition`] and its own stderr as the message.
pub(crate) fn read(cargo: &OsStr, dir: &Path) -> Result<CargoProject, MetadataError> {
    let output = Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(dir)
        .output()
        .map_err(|error| MetadataError {
            exit: Exit::Infrastructure,
            message: format!("cannot run {}: {error}", cargo.to_string_lossy()),
        })?;
    if !output.status.success() {
        return Err(MetadataError {
            exit: Exit::Definition,
            message: String::from_utf8_lossy(&output.stderr)
                .trim_end()
                .to_owned(),
        });
    }
    parse(&output.stdout)
}

/// Reads the metadata JSON `cargo metadata` printed.
///
/// Output that is not metadata ends with [`Exit::Infrastructure`].
pub(crate) fn parse(json: &[u8]) -> Result<CargoProject, MetadataError> {
    serde_json::from_slice(json).map_err(|error| MetadataError {
        exit: Exit::Infrastructure,
        message: format!("cannot read the output of cargo metadata: {error}"),
    })
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::fs;

    use super::*;

    fn kinds(package: &Package) -> Vec<(&str, Vec<&str>)> {
        package
            .targets
            .iter()
            .map(|target| {
                (
                    target.name.as_str(),
                    target.kind.iter().map(String::as_str).collect(),
                )
            })
            .collect()
    }

    #[test]
    fn parses_the_workspace_shape() {
        let project = parse(include_bytes!(
            "../tests/fixtures/metadata/rust-workspace.json"
        ))
        .unwrap();
        assert_eq!(project.workspace_root, Path::new("/work/rust-workspace"));
        let names: Vec<_> = project.packages.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["greeting", "answer", "cli"]);
        assert_eq!(kinds(&project.packages[0]), [("greeting", vec!["lib"])]);
        assert_eq!(
            kinds(&project.packages[1]),
            [("answer", vec!["proc-macro"])]
        );
        assert_eq!(kinds(&project.packages[2]), [("cli", vec!["bin"])]);
        assert_eq!(project.packages[1].targets[0].crate_types, ["proc-macro"]);
    }

    #[test]
    fn parses_a_library_with_several_crate_types() {
        let project = parse(include_bytes!("../tests/fixtures/metadata/my-lib.json")).unwrap();
        let package = &project.packages[0];
        assert_eq!(package.name, "my-lib");
        assert_eq!(
            kinds(package)[0],
            ("my_lib", vec!["cdylib", "staticlib", "rlib"])
        );
        assert_eq!(
            package.targets[0].crate_types,
            ["cdylib", "staticlib", "rlib"]
        );
        let rest: Vec<_> = kinds(package).into_iter().skip(1).collect();
        assert_eq!(
            rest,
            [
                ("extra", vec!["bin"]),
                ("tool-a", vec!["bin"]),
                ("ex", vec!["example"]),
                ("it", vec!["test"]),
                ("build-script-build", vec!["custom-build"]),
            ]
        );
    }

    #[test]
    fn parses_required_features() {
        let project = parse(include_bytes!(
            "../tests/fixtures/metadata/required-features.json"
        ))
        .unwrap();
        let targets = &project.packages[0].targets;
        assert_eq!(targets[0].name, "feat-tool");
        assert_eq!(targets[0].required_features, ["extra"]);
        assert_eq!(targets[1].name, "plain");
        assert_eq!(targets[1].required_features, Vec::<String>::new());
    }

    #[test]
    fn parses_an_example_library_and_a_non_ascii_name() {
        let project = parse(include_bytes!(
            "../tests/fixtures/metadata/example-library.json"
        ))
        .unwrap();
        let example = &project.packages[0].targets[1];
        assert_eq!(example.kind, ["example"]);
        assert_eq!(example.crate_types, ["cdylib"]);

        let project = parse(include_bytes!(
            "../tests/fixtures/metadata/non-ascii-name.json"
        ))
        .unwrap();
        assert_eq!(project.packages[0].name, "café");
    }

    #[test]
    fn parses_two_packages_with_one_bin_name() {
        let project = parse(include_bytes!(
            "../tests/fixtures/metadata/same-bin-name.json"
        ))
        .unwrap();
        let bins: Vec<_> = project
            .packages
            .iter()
            .map(|p| (p.name.as_str(), p.targets[0].name.as_str()))
            .collect();
        assert_eq!(bins, [("one", "tool"), ("two", "tool")]);
    }

    #[test]
    fn output_that_is_not_metadata_is_3() {
        let error = parse(b"not json").unwrap_err();
        assert_eq!(error.exit(), Exit::Infrastructure);
        assert!(
            error
                .to_string()
                .starts_with("cannot read the output of cargo metadata"),
            "{error}"
        );
    }

    #[test]
    fn the_program_is_cargo_unless_cargo_names_one() {
        assert_eq!(program(None), OsString::from("cargo"));
        assert_eq!(
            program(Some(OsString::from("/opt/cargo"))),
            OsString::from("/opt/cargo")
        );
    }

    #[test]
    fn a_cargo_that_cannot_start_is_3() {
        let dir = tempfile::tempdir().unwrap();
        let error = read(OsStr::new("/nonexistent/cargo"), dir.path()).unwrap_err();
        assert_eq!(error.exit(), Exit::Infrastructure);
        assert!(error.to_string().contains("/nonexistent/cargo"), "{error}");
    }

    #[test]
    fn a_refused_manifest_is_5() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package\nname = ").unwrap();
        let error = read(OsStr::new("cargo"), dir.path()).unwrap_err();
        assert_eq!(error.exit(), Exit::Definition);
        assert!(error.to_string().contains("Cargo.toml"), "{error}");
    }

    #[test]
    fn reads_a_real_project() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src/main.rs"), "fn main() {}\n").unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        let project = read(OsStr::new("cargo"), dir.path()).unwrap();
        assert_eq!(project.packages[0].name, "demo");
        assert!(!dir.path().join("Cargo.lock").exists());
    }
}
