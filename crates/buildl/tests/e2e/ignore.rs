//! What `.buildlignore` does to a target that lists its sources with `buildl.sources`: an ignored
//! file is not an input, so editing it leaves the target clean.

use std::fs;

use buildl::buildl_core::{Label, Verdict};

use crate::common::Fixture;

/// A copy of the `twins` workspace whose root build file has one target, `//:all`, reading every
/// file `buildl.sources("**/*")` lists, with `files` written under it.
fn listing(files: &[(&str, &str)]) -> Fixture {
    let workspace = Fixture::copy_of("twins", &[]);
    let build_file = "buildl.target(\"all\", {\n  inputs = buildl.sources(\"**/*\"),\n  run    = { \"cp\", \"src/a.c\", \"$out\" },\n})\n";
    fs::write(workspace.root().join("build.lua"), build_file).unwrap();
    for (relative, text) in files {
        let path = workspace.root().join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    workspace
}

/// The paths of the inputs `//:all` is declared to read.
fn inputs_of_all(workspace: &Fixture) -> Vec<String> {
    let (graph, _) = workspace.open().graph().unwrap();
    let id = graph.id_of(&Label::parse("//:all").unwrap()).unwrap();
    graph
        .node(id)
        .unwrap()
        .inputs
        .iter()
        .map(ToString::to_string)
        .collect()
}

/// Whether the plan of the workspace has every target clean.
fn all_clean(workspace: &Fixture) -> bool {
    workspace
        .try_plan()
        .unwrap()
        .verdicts()
        .iter()
        .all(|verdict| matches!(verdict, Verdict::Clean { .. }))
}

#[test]
fn editing_an_ignored_file_leaves_the_target_clean() {
    let workspace = listing(&[
        (".buildlignore", "notes/\n"),
        ("notes/a.md", "notes\n"),
        ("src/a.c", "int main(void) { return 0; }\n"),
    ]);
    workspace.build();
    assert!(all_clean(&workspace));

    workspace.append("notes/a.md", b"more notes\n");
    assert!(all_clean(&workspace));

    workspace.append("src/a.c", b"/* edited */\n");
    assert!(!all_clean(&workspace));
}

#[test]
fn a_nested_buildlignore_applies_below_its_folder() {
    let workspace = listing(&[
        ("src/.buildlignore", "gen/\n"),
        ("src/a.c", "int main(void) { return 0; }\n"),
        ("src/gen/g.c", "int g;\n"),
        ("gen/top.c", "int top;\n"),
    ]);

    let inputs = inputs_of_all(&workspace);

    assert!(!inputs.contains(&"src/gen/g.c".to_owned()), "{inputs:?}");
    assert!(inputs.contains(&"gen/top.c".to_owned()), "{inputs:?}");
    assert!(inputs.contains(&"src/a.c".to_owned()), "{inputs:?}");
}
