//! `buildl.sources` against a real workspace that has build outputs in it.

use core::num::NonZeroU64;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use buildl_core::{BuildFile, DeclarationSource, Directory, Evaluated, Provenance, StagedItem};
use buildl_lua::{DeclarationLimits, LuaSource};

use crate::common::workspace;

const LIST_ALL: &str = "buildl.target('app', { run = { 'cc' }, inputs = buildl.sources('**/*') })";

/// A workspace whose root build file lists every file beneath it, holding sources, the default
/// output and state directories, and an `out` that is not the workspace's own.
fn build_tree() -> (tempfile::TempDir, PathBuf) {
    let (dir, root) = workspace(LIST_ALL);
    for file in [
        "main.c",
        "out/app.o",
        "out/cas/ab.bin",
        ".buildl/state.json",
        "target/gen.c",
        "lib/text.c",
        "lib/out/keep.c",
        "lib/build.lua",
    ] {
        let path = root.join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "").unwrap();
    }
    fs::write(root.join("lib/build.lua"), LIST_ALL).unwrap();
    (dir, root)
}

fn directories(paths: &[&str]) -> Vec<Directory> {
    paths
        .iter()
        .map(|path| Directory::parse(*path).unwrap())
        .collect()
}

/// The inputs the build file at `file`, declared in `directory`, staged for its one target.
fn listed(root: &Path, source: &LuaSource, file: &str, directory: &str) -> Vec<String> {
    let build_file = BuildFile::new(Provenance::new(
        PathBuf::from(file),
        Directory::parse(directory).unwrap(),
    ));
    let Evaluated::Staged(staged) = source.evaluate(&build_file).unwrap() else {
        panic!("{} exists under {}", file, root.display());
    };
    let StagedItem::Target(target) = &staged.declarations[0].item else {
        panic!("the build file declares a target");
    };
    target
        .inputs
        .iter()
        .map(|input| input.as_written().to_owned())
        .collect()
}

#[test]
fn an_empty_skip_list_lists_every_file() {
    let (_dir, root) = build_tree();
    let source = LuaSource::new(&root, DeclarationLimits::default(), [], BTreeMap::new());
    let all = listed(&root, &source, "build.lua", "");
    assert!(all.contains(&"out/app.o".to_owned()));
    assert!(all.contains(&".buildl/state.json".to_owned()));
}

#[test]
fn the_output_and_state_directories_are_not_listed_from_the_workspace_root() {
    let (_dir, root) = build_tree();
    let source = LuaSource::new(
        &root,
        DeclarationLimits::default(),
        directories(&["out", ".buildl"]),
        BTreeMap::new(),
    );
    assert_eq!(
        listed(&root, &source, "build.lua", ""),
        [
            "build.lua",
            "lib/build.lua",
            "lib/out/keep.c",
            "lib/text.c",
            "main.c",
            "target/gen.c",
        ]
    );
}

#[test]
fn differently_named_directories_are_skipped_instead() {
    let (_dir, root) = build_tree();
    let source = LuaSource::new(
        &root,
        DeclarationLimits::default(),
        directories(&["target", "out/cas"]),
        BTreeMap::new(),
    );
    let all = listed(&root, &source, "build.lua", "");
    assert!(!all.iter().any(|path| path.starts_with("target/")));
    assert!(!all.iter().any(|path| path.starts_with("out/cas/")));
    assert!(all.contains(&"out/app.o".to_owned()));
    assert!(all.contains(&".buildl/state.json".to_owned()));
}

#[test]
fn a_nested_directory_with_the_same_name_is_still_listed() {
    let (_dir, root) = build_tree();
    let source = LuaSource::new(
        &root,
        DeclarationLimits::default(),
        directories(&["out", ".buildl"]),
        BTreeMap::new(),
    );
    assert!(listed(&root, &source, "build.lua", "").contains(&"lib/out/keep.c".to_owned()));
}

#[test]
fn a_subdirectory_build_file_never_sees_a_skipped_directory_below_it() {
    let (_dir, root) = build_tree();
    fs::create_dir_all(root.join("lib/gen")).unwrap();
    fs::write(root.join("lib/gen/g.c"), "").unwrap();
    let source = LuaSource::new(
        &root,
        DeclarationLimits::default(),
        directories(&["lib/gen"]),
        BTreeMap::new(),
    );
    assert_eq!(
        listed(&root, &source, "lib/build.lua", "lib"),
        ["build.lua", "out/keep.c", "text.c"]
    );
}

#[test]
fn a_build_file_inside_a_skipped_directory_lists_nothing() {
    let (_dir, root) = build_tree();
    fs::write(root.join("out/build.lua"), LIST_ALL).unwrap();
    let source = LuaSource::new(
        &root,
        DeclarationLimits::default(),
        directories(&["out", ".buildl"]),
        BTreeMap::new(),
    );
    assert_eq!(
        listed(&root, &source, "out/build.lua", "out"),
        Vec::<String>::new()
    );
}

#[test]
fn skipped_directories_do_not_count_against_the_walk_limit() {
    let (_dir, root) = workspace(LIST_ALL);
    for index in 0..50 {
        fs::create_dir_all(root.join("out")).unwrap();
        fs::write(root.join(format!("out/{index}.o")), "").unwrap();
    }
    fs::write(root.join("main.c"), "").unwrap();
    // The root, build.lua and main.c are three entries; out's fifty files must not be visited.
    let limits = DeclarationLimits::default().with_walk_entries(NonZeroU64::new(3).unwrap());
    let source = LuaSource::new(&root, limits, directories(&["out"]), BTreeMap::new());
    assert_eq!(
        listed(&root, &source, "build.lua", ""),
        ["build.lua", "main.c"]
    );
}
