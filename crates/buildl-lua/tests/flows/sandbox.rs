//! What a build file can reach: the curated surface, and nothing outside the workspace.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::PathBuf;

use buildl_core::{
    BuildFile, DeclarationSource, Directory, Error, Evaluated, EvaluationFailure, Provenance,
};
use buildl_lua::{DeclarationLimits, LuaSource};

use crate::common::{evaluate, workspace};

/// Each `assert` names what leaked; the file stages one alias only if every check passed.
const SURFACE: &str = r#"
assert(os == nil and io == nil and coroutine == nil and debug == nil, "standard library leak")
assert(require == nil and load == nil and dofile == nil and loadfile == nil, "loader leak")
assert(print == nil, "stdout leak")
assert(math.random == nil and math.randomseed == nil, "entropy leak")
assert(type(math.floor) == "function" and type(string.format) == "function", "pure library missing")
assert(airsstack.fs == nil and airsstack.proc == nil and airsstack.env == nil
       and airsstack.time == nil and airsstack.stdio == nil, "host module leak")
assert(type(airsstack.json) == "table" and type(airsstack.path) == "table"
       and type(airsstack.regex) == "table" and type(airsstack.hash) == "table"
       and type(airsstack.glob) == "table", "curated module missing")
assert(airsstack.path.absolute == nil, "ambient path leak")
assert(buildl == airsstack.buildl and buildl.json == nil, "buildl binding")
buildl.alias("ok", "ok")
"#;

#[test]
fn a_build_file_sees_the_curated_surface_only() {
    let (_dir, root) = workspace(SURFACE);
    let Evaluated::Staged(staged) = evaluate(&root).unwrap() else {
        panic!("the build file exists");
    };
    assert_eq!(staged.declarations.len(), 1);
}

#[test]
fn sources_refuses_a_declaring_directory_that_resolves_outside_the_workspace() {
    let (_dir, root) = workspace("buildl.subdir('lib')");
    let elsewhere = tempfile::tempdir().unwrap();
    fs::write(elsewhere.path().join("build.lua"), "buildl.sources('*.c')").unwrap();
    symlink(elsewhere.path(), root.join("lib")).unwrap();
    let lib = BuildFile::new(Provenance::new(
        PathBuf::from("lib/build.lua"),
        Directory::parse("lib").unwrap(),
    ));
    match LuaSource::new(&root, DeclarationLimits::default()).evaluate(&lib) {
        Err(Error::Evaluation {
            failure,
            diagnostic,
            ..
        }) => {
            assert_eq!(failure, EvaluationFailure::Refused);
            assert!(
                diagnostic
                    .as_str()
                    .contains("resolves outside the workspace"),
                "{diagnostic}"
            );
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
}
