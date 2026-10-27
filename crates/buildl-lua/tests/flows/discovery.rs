//! `buildl.discover` stages a discovery target, and `buildl.discovered` reads the result the
//! declaration port was handed.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use buildl_core::{
    BuildFile, DeclarationSource, Directory, Discovered, Evaluated, EvaluationFailure, Freshness,
    Label, NetworkAccess, OutputSource, PluginName, Provenance, StagedFile, StagedItem,
    StagedTarget, TargetRole, Written,
};
use buildl_lua::{DeclarationLimits, LuaSource};

use crate::common::{failure_of, root_file, workspace};

/// The result of the discovery target `//svc:pkgs`: `{"pkgs":["a","b"]}`.
fn pkgs() -> Discovered {
    Discovered::new(BTreeMap::from([(
        Label::parse("//svc:pkgs").unwrap(),
        br#"{"pkgs":["a","b"]}"#.to_vec(),
    )]))
}

/// What `pkgs` holds, as the text a build file reads.
const PKGS_TEXT: &str = r#"{"pkgs":["a","b"]}"#;

/// Evaluates `file` of the workspace at `root`, handing it `discovered`, with the plugins
/// `<root>/plugins/<name>` for each of `plugins`.
fn staged(root: &Path, plugins: &[&str], file: &BuildFile, discovered: &Discovered) -> StagedFile {
    let plugins: BTreeMap<PluginName, PathBuf> = plugins
        .iter()
        .map(|name| {
            (
                PluginName::parse(*name).unwrap(),
                root.join("plugins").join(name),
            )
        })
        .collect();
    let source = LuaSource::new(root, DeclarationLimits::default(), [], plugins);
    match source.evaluate(file, discovered).unwrap() {
        Evaluated::Staged(staged) => staged,
        Evaluated::Absent => panic!("the build file exists"),
    }
}

/// The build file in `svc/`.
fn svc_file() -> BuildFile {
    BuildFile::new(Provenance::new(
        PathBuf::from("svc/build.lua"),
        Directory::parse("svc").unwrap(),
    ))
}

/// A workspace whose `svc/build.lua` holds `source`.
fn svc_workspace(source: &str) -> (tempfile::TempDir, PathBuf) {
    let (dir, root) = workspace("");
    fs::create_dir(root.join("svc")).unwrap();
    fs::write(root.join("svc/build.lua"), source).unwrap();
    (dir, root)
}

/// Writes the plugin `p` at version 1.0.0 under `<root>/plugins/p`, its entry file holding `main`.
fn plugin_p(root: &Path, main: &str) {
    let dir = root.join("plugins/p");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("extension.toml"),
        "[extension]\nname = \"p\"\nversion = \"1.0.0\"\nentry = \"main.lua\"\napi = 1\n",
    )
    .unwrap();
    fs::write(dir.join("main.lua"), main).unwrap();
}

/// The name of each alias a file declared, in call order.
fn alias_names(staged: &StagedFile) -> Vec<&str> {
    staged
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.item {
            StagedItem::Alias(alias) => Some(alias.name.as_written()),
            _ => None,
        })
        .collect()
}

#[test]
fn discover_stages_a_discovery_target() {
    let (_dir, root) = workspace(
        "buildl.discover('pkgs', { run = { 'go', 'list' }, inputs = { 'go.mod' }, \
         deps = { ':gen' }, env = { 'HOME' }, network = true })",
    );
    let staged = staged(&root, &[], &root_file(), &Discovered::empty());
    let [declaration] = staged.declarations.as_slice() else {
        panic!("one declaration, found {:?}", staged.declarations);
    };
    assert_eq!(
        declaration.item,
        StagedItem::Target(StagedTarget {
            name: Written::new("pkgs"),
            role: TargetRole::Discovery,
            rule: None,
            run: Some(vec![Written::new("go"), Written::new("list")]),
            inputs: vec![Written::new("go.mod")],
            deps: vec![Written::new(":gen")],
            outputs: None,
            env: vec![Written::new("HOME")],
            network: NetworkAccess::Declared,
            output_source: OutputSource::Command,
            freshness: Freshness::Cached,
        })
    );
}

#[test]
fn discover_refuses_outputs_and_always() {
    let (failure, diagnostic) =
        failure_of("buildl.discover('pkgs', { run = { 'x' }, outputs = { 'p' } })");
    assert_eq!(
        failure,
        EvaluationFailure::UnknownField {
            field: Written::new("outputs"),
        }
    );
    assert_eq!(
        diagnostic.as_str(),
        "line 1: buildl.discover 'pkgs': unknown field 'outputs'"
    );

    let (failure, diagnostic) =
        failure_of("\nbuildl.discover('pkgs', { run = { 'x' }, always = true })");
    assert_eq!(
        failure,
        EvaluationFailure::UnknownField {
            field: Written::new("always"),
        }
    );
    assert_eq!(
        diagnostic.as_str(),
        "line 2: buildl.discover 'pkgs': unknown field 'always'"
    );
}

#[test]
fn discovered_returns_nil_without_a_result() {
    let (_dir, root) =
        workspace("if buildl.discovered('//svc:pkgs') == nil then buildl.alias('none', 'x') end");
    let staged = staged(&root, &[], &root_file(), &Discovered::empty());
    assert_eq!(alias_names(&staged), ["none"]);
}

#[test]
fn discovered_returns_the_bytes_for_a_relative_label() {
    let (_dir, root) = svc_workspace("buildl.alias(buildl.discovered(':pkgs'), 'x')");
    let staged = staged(&root, &[], &svc_file(), &pkgs());
    assert_eq!(alias_names(&staged), [PKGS_TEXT]);
}

#[test]
fn discovered_resolves_bare_and_absolute_labels_like_deps() {
    let (_dir, root) = svc_workspace(
        "buildl.alias(buildl.discovered('pkgs'), 'bare')\n\
         buildl.alias(buildl.discovered('//svc:pkgs'), 'absolute')\n\
         buildl.alias(tostring(buildl.discovered('//other:pkgs')), 'other')",
    );
    let staged = staged(&root, &[], &svc_file(), &pkgs());
    assert_eq!(alias_names(&staged), [PKGS_TEXT, PKGS_TEXT, "nil"]);
}

#[test]
fn an_unparseable_label_is_refused_naming_the_line() {
    let (failure, diagnostic) = failure_of("\n\nbuildl.discovered('//svc')");
    assert!(matches!(failure, EvaluationFailure::Refused), "{failure:?}");
    assert!(
        diagnostic
            .as_str()
            .starts_with("line 3: buildl.discovered: "),
        "{diagnostic}"
    );
}

#[test]
fn a_plugin_s_discover_carries_its_origin() {
    let (_dir, root) = workspace(
        "local p = buildl.use('p')\n\
         p.generate('pkgs')\n\
         buildl.discover('own', { run = { 'x' } })",
    );
    plugin_p(
        &root,
        "local module = {}\n\
         function module.generate(name) buildl.discover(name, { run = { 'p' } }) end\n\
         return module",
    );
    let staged = staged(&root, &["p"], &root_file(), &Discovered::empty());
    let seen: Vec<_> = staged
        .declarations
        .iter()
        .map(|declaration| match &declaration.item {
            StagedItem::Target(target) => (declaration.origin.to_string(), target.role),
            other => panic!("a target, found {other:?}"),
        })
        .collect();
    assert_eq!(
        seen,
        [
            ("p@1.0.0".to_owned(), TargetRole::Discovery),
            ("build file".to_owned(), TargetRole::Discovery),
        ]
    );
}

#[test]
fn a_plugin_reads_discovered_relative_to_the_build_file() {
    let (_dir, root) = svc_workspace(
        "local p = buildl.use('p')\n\
         buildl.alias(p.read(':pkgs'), 'x')",
    );
    plugin_p(
        &root,
        "local module = {}\n\
         function module.read(label) return buildl.discovered(label) end\n\
         return module",
    );
    let staged = staged(&root, &["p"], &svc_file(), &pkgs());
    assert_eq!(alias_names(&staged), [PKGS_TEXT]);
}
