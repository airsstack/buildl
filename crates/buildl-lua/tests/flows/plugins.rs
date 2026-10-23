//! `buildl.use`: a plugin's module, its environment, its `require`, and the origin of what it
//! declares.

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use buildl_core::{
    BuildFile, DeclarationSource, Diagnostic, Directory, Error, Evaluated, EvaluationFailure,
    PluginName, Provenance, StagedFile, StagedItem, Written,
};
use buildl_lua::{DeclarationLimits, LuaSource};

use crate::common::{root_file, workspace};

/// Writes the plugin `name` at version 1.0.0 under `<root>/plugins/<name>`: its manifest, an entry
/// file `main.lua` holding `main`, and `files`, each relative to the plugin directory.
fn plugin(root: &Path, name: &str, main: &str, files: &[(&str, &str)]) {
    let dir = root.join("plugins").join(name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("extension.toml"),
        format!(
            "[extension]\nname = \"{name}\"\nversion = \"1.0.0\"\nentry = \"main.lua\"\napi = 1\n"
        ),
    )
    .unwrap();
    fs::write(dir.join("main.lua"), main).unwrap();
    for (path, text) in files {
        let file = dir.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, text).unwrap();
    }
}

/// A source whose plugins are the directories `<root>/plugins/<name>` for each of `names`.
fn source(root: &Path, names: &[&str]) -> LuaSource {
    let plugins: BTreeMap<PluginName, PathBuf> = names
        .iter()
        .map(|name| {
            (
                PluginName::parse(*name).unwrap(),
                root.join("plugins").join(name),
            )
        })
        .collect();
    LuaSource::new(root, DeclarationLimits::default(), [], plugins)
}

/// Evaluates `file` of the workspace at `root`, with `names` as its plugins.
fn evaluate_file(
    root: &Path,
    names: &[&str],
    file: &BuildFile,
) -> Result<StagedFile, (EvaluationFailure, Diagnostic)> {
    match source(root, names).evaluate(file) {
        Ok(Evaluated::Staged(staged)) => Ok(staged),
        Ok(Evaluated::Absent) => panic!("the build file exists"),
        Err(Error::Evaluation {
            failure,
            diagnostic,
            ..
        }) => Err((failure, diagnostic)),
        Err(other) => panic!("expected an evaluation failure, got {other:?}"),
    }
}

/// Evaluates the workspace's root build file, with `names` as its plugins.
fn staged(root: &Path, names: &[&str]) -> Result<StagedFile, (EvaluationFailure, Diagnostic)> {
    evaluate_file(root, names, &root_file())
}

/// The origin of each declaration, in call order.
fn origins(staged: &StagedFile) -> Vec<String> {
    staged
        .declarations
        .iter()
        .map(|declaration| declaration.origin.to_string())
        .collect()
}

/// A workspace whose root build file is `build_lua` and whose plugin `p` has the entry `main`.
fn with_p(build_lua: &str, main: &str, files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
    let (dir, root) = workspace(build_lua);
    plugin(&root, "p", main, files);
    (dir, root)
}

/// The failure of using `p`, whose entry is `main` and whose other files are `files`.
fn refused_with(main: &str, files: &[(&str, &str)]) -> (EvaluationFailure, Diagnostic) {
    let (_dir, root) = with_p("buildl.use('p')", main, files);
    staged(&root, &["p"]).unwrap_err()
}

#[test]
fn use_returns_the_plugin_module_and_its_targets_carry_its_origin() {
    let (_dir, root) = with_p(
        "local p = buildl.use('p')\n\
         p.generate('a')\n\
         buildl.target('b', { run = { 'x' } })",
        "local module = {}\n\
         function module.generate(name) buildl.target(name, { run = { 'p' } }) end\n\
         return module",
        &[],
    );
    let staged = staged(&root, &["p"]).unwrap();
    assert_eq!(origins(&staged), ["p@1.0.0", "build file"]);
}

#[test]
fn a_nested_require_loads_once() {
    let (_dir, root) = with_p(
        "buildl.use('p')",
        "local a = require('lib.a')\n\
         local b = require('lib.b')\n\
         assert(a.b == b and require('lib.b') == b, 'a module loads once')\n\
         return {}",
        &[
            ("lib/a.lua", "return { b = require('lib.b') }"),
            (
                "lib/b.lua",
                "buildl.alias('loaded', 'x')\nreturn { name = 'b' }",
            ),
        ],
    );
    let staged = staged(&root, &["p"]).unwrap();
    assert_eq!(staged.declarations.len(), 1);
    assert_eq!(origins(&staged), ["p@1.0.0"]);
}

#[test]
fn a_module_runs_under_its_plugin_s_environment() {
    let (_dir, root) = with_p(
        "buildl.use('p')",
        "return require('m')",
        &[(
            "m.lua",
            "assert(file_global == nil)\n\
             buildl.alias('from_module', 'x')\n\
             return {}",
        )],
    );
    let staged = staged(&root, &["p"]).unwrap();
    assert_eq!(origins(&staged), ["p@1.0.0"]);
}

#[test]
fn require_with_dotdot_slash_or_empty_part_is_refused() {
    for name in [
        "..x",
        "../x",
        "a/b",
        "a\\\\b",
        "a..b",
        ".a",
        "a.",
        "",
        "/etc/passwd",
    ] {
        let (failure, diagnostic) =
            refused_with(&format!("return require('{name}')"), &[("x.lua", "")]);
        assert_eq!(failure, EvaluationFailure::Refused, "{name:?}");
        assert!(
            diagnostic.as_str().contains("require"),
            "{name:?}: {diagnostic}"
        );
    }
}

#[test]
fn require_of_a_module_that_does_not_exist_is_refused() {
    let (failure, diagnostic) = refused_with("return require('missing')", &[]);
    assert_eq!(failure, EvaluationFailure::Refused);
    assert!(diagnostic.as_str().contains("missing"), "{diagnostic}");
}

#[test]
fn require_through_a_link_out_is_refused() {
    let (_dir, root) = with_p(
        "buildl.use('p')",
        "return require('file_link')",
        &[("local.lua", "return {}")],
    );
    fs::write(root.join("outside.lua"), "return {}").unwrap();
    fs::create_dir_all(root.join("elsewhere")).unwrap();
    fs::write(root.join("elsewhere/x.lua"), "return {}").unwrap();
    let dir = root.join("plugins/p");
    symlink(root.join("outside.lua"), dir.join("file_link.lua")).unwrap();
    symlink(root.join("elsewhere"), dir.join("dir_link")).unwrap();

    for module in ["file_link", "dir_link.x"] {
        fs::write(dir.join("main.lua"), format!("return require('{module}')")).unwrap();
        let (failure, diagnostic) = staged(&root, &["p"]).unwrap_err();
        assert_eq!(failure, EvaluationFailure::Refused, "{module}");
        assert!(
            diagnostic.as_str().contains("outside the plugin"),
            "{module}: {diagnostic}"
        );
    }
}

#[test]
fn a_self_require_is_refused() {
    let (failure, diagnostic) = refused_with(
        "return require('again')",
        &[("again.lua", "return require('again')")],
    );
    assert_eq!(failure, EvaluationFailure::Refused);
    assert!(diagnostic.as_str().contains("again"), "{diagnostic}");
}

#[test]
fn a_require_cycle_is_refused() {
    let (failure, _) = refused_with(
        "return require('a')",
        &[
            ("a.lua", "return require('b')"),
            ("b.lua", "return require('a')"),
        ],
    );
    assert_eq!(failure, EvaluationFailure::Refused);
}

#[test]
fn airsstack_buildl_in_a_plugin_is_the_stamped_table() {
    let (_dir, root) = with_p(
        "buildl.use('p')",
        "assert(airsstack.buildl == buildl, 'one table under two names')\n\
         airsstack.buildl.target('t', { run = { 'x' } })\n\
         return {}",
        &[],
    );
    let staged = staged(&root, &["p"]).unwrap();
    assert_eq!(origins(&staged), ["p@1.0.0"]);
}

#[test]
fn g_buildl_in_a_plugin_is_the_stamped_table() {
    let (_dir, root) = with_p(
        "buildl.use('p')",
        "assert(_G._G == _G and _G.buildl == buildl and _G.airsstack.buildl == buildl)\n\
         _G.buildl.target('t', { run = { 'x' } })\n\
         _G.airsstack.buildl.alias('a', 't')\n\
         return {}",
        &[],
    );
    let staged = staged(&root, &["p"]).unwrap();
    assert_eq!(origins(&staged), ["p@1.0.0", "p@1.0.0"]);
}

#[test]
fn a_file_global_is_absent_in_a_plugin() {
    let (_dir, root) = with_p(
        "x = buildl\n\
         local p = buildl.use('p')\n\
         assert(y == nil, 'a plugin global leaked into the build file')\n\
         assert(_G.x == x and airsstack.buildl == x)",
        "assert(x == nil and _G.x == nil, 'a build file global reached a plugin')\n\
         assert(arg == nil and package == nil and load == nil and os == nil)\n\
         y = 1\n\
         return {}",
        &[],
    );
    staged(&root, &["p"]).unwrap();
}

#[test]
fn a_plugin_sees_the_curated_modules_and_the_language() {
    let (_dir, root) = with_p(
        "buildl.use('p')",
        "assert(airsstack.json.decode('[1]')[1] == 1)\n\
         assert(airsstack.path.join('a', 'b') == 'a/b')\n\
         assert(airsstack.hash.sha256('') ~= nil and airsstack.glob.match('*.c', 'a.c'))\n\
         assert(airsstack.regex.is_match('a+', 'aa'))\n\
         assert(airsstack.path.absolute == nil and airsstack.fs == nil)\n\
         assert(type(string.format) == 'function' and type(math.floor) == 'function')\n\
         assert(type(pairs) == 'function' and type(table.concat) == 'function')\n\
         return {}",
        &[],
    );
    staged(&root, &["p"]).unwrap();
}

#[test]
fn go_direct_and_through_ourgo_are_separate_instances() {
    let (_dir, root) = workspace(
        "local go = buildl.use('go')\n\
         local ourgo = buildl.use('ourgo')\n\
         assert(buildl.use('go') == go and buildl.use('ourgo') == ourgo, 'cached per chain')\n\
         assert(ourgo.go ~= go, 'a chain of its own is an instance of its own')\n\
         go.declare('a')\n\
         ourgo.declare('b')",
    );
    plugin(
        &root,
        "go",
        "local module = {}\n\
         function module.declare(name) buildl.target(name, { run = { 'go' } }) end\n\
         return module",
        &[],
    );
    plugin(
        &root,
        "ourgo",
        "local go = buildl.use('go')\n\
         return { go = go, declare = function(name) go.declare(name) end }",
        &[],
    );
    let staged = staged(&root, &["go", "ourgo"]).unwrap();
    assert_eq!(origins(&staged), ["go@1.0.0", "ourgo@1.0.0 > go@1.0.0"]);
}

#[test]
fn an_unknown_plugin_is_refused() {
    let (_dir, root) = workspace("\nbuildl.use('nope')");
    let (failure, diagnostic) = staged(&root, &[]).unwrap_err();
    assert_eq!(failure, EvaluationFailure::Refused);
    let said = diagnostic.as_str();
    assert!(said.starts_with("line 2: buildl.use 'nope': "), "{said}");
}

#[test]
fn a_plugin_cannot_bring_a_plugin_the_workspace_does_not_name() {
    let (failure, diagnostic) = refused_with("buildl.use('hidden')", &[]);
    assert_eq!(failure, EvaluationFailure::Refused);
    assert!(diagnostic.as_str().contains("hidden"), "{diagnostic}");
}

#[test]
fn a_use_cycle_is_refused() {
    let (_dir, root) = workspace("buildl.use('a')");
    plugin(&root, "a", "return { b = buildl.use('b') }", &[]);
    plugin(&root, "b", "return { a = buildl.use('a') }", &[]);
    let (failure, diagnostic) = staged(&root, &["a", "b"]).unwrap_err();
    assert_eq!(failure, EvaluationFailure::Refused);
    assert!(diagnostic.as_str().contains("cycle"), "{diagnostic}");
}

#[test]
fn a_plugin_using_itself_is_a_cycle() {
    let (failure, diagnostic) = refused_with("buildl.use('p')", &[]);
    assert_eq!(failure, EvaluationFailure::Refused);
    assert!(diagnostic.as_str().contains("cycle"), "{diagnostic}");
}

#[test]
fn a_bad_manifest_is_refused_naming_it() {
    let (_dir, root) = with_p("buildl.use('p')", "return {}", &[]);
    fs::write(
        root.join("plugins/p/extension.toml"),
        "[extension]\nname = \"p\"\nversion = \"1\"\nentry = \"main.lua\"\napi = 2\n",
    )
    .unwrap();
    let (failure, diagnostic) = staged(&root, &["p"]).unwrap_err();
    assert_eq!(failure, EvaluationFailure::Refused);
    assert!(
        diagnostic.as_str().contains("extension.toml"),
        "{diagnostic}"
    );
}

#[test]
fn plugin_lua_error_names_the_plugin_file_and_line() {
    let (_dir, root) = with_p("buildl.use('p')", "\n\nerror('boom')", &[]);
    let (failure, diagnostic) = staged(&root, &["p"]).unwrap_err();
    assert_eq!(failure, EvaluationFailure::Runtime);
    assert!(
        diagnostic.as_str().contains("plugins/p/main.lua:3: boom"),
        "{diagnostic}"
    );
}

#[test]
fn a_plugin_module_error_names_the_module_file_and_line() {
    let (failure, diagnostic) = refused_with(
        "return require('lib.m')",
        &[("lib/m.lua", "local x = 1\nerror('bad module')")],
    );
    assert_eq!(failure, EvaluationFailure::Runtime);
    assert!(
        diagnostic
            .as_str()
            .contains("plugins/p/lib/m.lua:2: bad module"),
        "{diagnostic}"
    );
}

#[test]
fn print_and_math_random_are_absent_in_a_plugin() {
    let (_dir, root) = with_p(
        "buildl.use('p')",
        "assert(print == nil and math.random == nil and math.randomseed == nil)\n\
         assert(not pcall(tostring, {}), 'tostring still refuses an address')\n\
         assert(not pcall(string.format, '%p', 1), 'format still refuses an address')\n\
         return {}",
        &[],
    );
    staged(&root, &["p"]).unwrap();
}

#[test]
fn sources_in_a_plugin_walks_the_calling_file_s_directory() {
    let (_dir, root) = workspace("");
    plugin(
        &root,
        "p",
        "return { list = function()\n\
         buildl.target('t', { run = { 'x' }, inputs = buildl.sources('*.c') })\n\
         end }",
        &[],
    );
    fs::create_dir_all(root.join("lib")).unwrap();
    fs::write(root.join("lib/build.lua"), "buildl.use('p').list()").unwrap();
    for file in ["lib/a.c", "lib/b.c", "main.c"] {
        fs::write(root.join(file), "").unwrap();
    }
    let lib = BuildFile::new(Provenance::new(
        PathBuf::from("lib/build.lua"),
        Directory::parse("lib").unwrap(),
    ));
    let staged = evaluate_file(&root, &["p"], &lib).unwrap();
    let StagedItem::Target(target) = &staged.declarations[0].item else {
        panic!("the plugin declares a target");
    };
    let inputs: Vec<&str> = target.inputs.iter().map(Written::as_written).collect();
    assert_eq!(inputs, ["a.c", "b.c"]);
    assert_eq!(origins(&staged), ["p@1.0.0"]);
}
