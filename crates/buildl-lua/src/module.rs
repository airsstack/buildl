//! The `buildl` module table: the declaration API a build file calls.
//!
//! Its own file because it is the one airsl host module buildl writes. airsl installs it as
//! `airsstack.buildl`, and its installation binds the same table to the global `buildl`. It also
//! withholds the three globals that would let a build file's output vary between runs or reach the
//! host's stdout: `math.random`, `math.randomseed` and `print`, and replaces `tostring` and
//! `string.format` with versions that refuse address-derived text.
//!
//! Responsibilities: [`BuildlModule`]; building a primitives table whose declarations carry a
//! given origin; the snapshot of the language globals taken at install; turning a primitive call
//! into a staged record or a sticky refusal that names the line, in the build file or a plugin
//! file, that made the call.
//!
//! Non-responsibilities: the argument shapes, which `primitives` owns, the walk behind
//! `buildl.sources`, which `sources` owns, the replacement functions, which `stable_text` owns,
//! and loading the plugins `buildl.use` names, which `plugin` owns.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, TryLockError};

use airsl::mlua::{self, Lua, Table, Value};
use airsl::{HostModule, InstallContext, ModuleName};
use buildl_core::{EvaluationFailure, EvaluationLimit, Origin, PluginName, StagedItem, TargetRole};

use crate::plugin::Plugins;
use crate::primitives;
use crate::refusal::{CallSite, Refusal};
use crate::snapshot::copy_tables;
use crate::sources::Sources;
use crate::stable_text;
use crate::staging::{Exceeded, Staging};

/// The `buildl` host module for one build file's evaluation.
#[derive(Debug)]
pub(crate) struct BuildlModule {
    name: ModuleName,
    staging: Arc<Mutex<Staging>>,
    sources: Sources,
    plugins: Plugins,
}

impl BuildlModule {
    /// A module staging into `staging`, whose `buildl.sources` walks `sources` and whose
    /// `buildl.use` loads the plugins in `plugins`, each from the directory it maps to.
    ///
    /// `root` is the workspace root, which names the files a plugin's errors point at.
    ///
    /// # Errors
    ///
    /// Never in practice: the only failure is `buildl` not being a valid module name.
    pub(crate) fn new(
        staging: Arc<Mutex<Staging>>,
        sources: Sources,
        root: &Path,
        plugins: BTreeMap<PluginName, PathBuf>,
    ) -> airsl::Result<Self> {
        let plugins = Plugins::new(root, plugins, &staging, &sources);
        Ok(Self {
            name: ModuleName::new("buildl")?,
            staging,
            sources,
            plugins,
        })
    }
}

impl HostModule for BuildlModule {
    fn name(&self) -> &ModuleName {
        &self.name
    }

    fn install(
        &self,
        lua: &Lua,
        table: &Table,
        _context: &InstallContext<'_>,
    ) -> airsl::Result<()> {
        let fail = |error| airsl::Error::lua("buildl", error);

        let primitives = primitives_table(lua, &self.staging, &self.sources, &Origin::BuildFile)
            .map_err(fail)?;
        // `table` is the module table airsl binds as `airsstack.buildl`, so the primitives are
        // copied into it rather than replacing it.
        for pair in primitives.pairs::<String, Value>() {
            let (name, function) = pair.map_err(fail)?;
            table.set(name, function).map_err(fail)?;
        }
        self.plugins.bind_use(lua, table, &[]).map_err(fail)?;

        let globals = lua.globals();
        globals.set("buildl", table.clone()).map_err(fail)?;
        let math: Table = globals.get("math").map_err(fail)?;
        math.set("random", Value::Nil).map_err(fail)?;
        math.set("randomseed", Value::Nil).map_err(fail)?;
        globals.set("print", Value::Nil).map_err(fail)?;
        stable_text::install(lua).map_err(fail)?;
        record_language(lua).map_err(fail)?;
        Ok(())
    }
}

/// The Lua registry key holding the snapshot of the language globals [`record_language`] takes.
pub(crate) const LANGUAGE_REGISTRY_KEY: &str = "buildl.language";

/// Globals left out of the language snapshot: the two names a build file reaches buildl through,
/// the globals table itself, and `require`, which plugin code gets its own version of.
const NOT_LANGUAGE: [&str; 4] = ["buildl", "airsstack", "_G", "require"];

/// Builds the table of declaration primitives whose declarations carry `origin`.
///
/// Every table built for one evaluation shares its `staging` and `sources`, so the declarations
/// of a build file and of the plugins it uses land in one numbered buffer under one byte budget,
/// and `sources` walks the build file's directory whichever table asks.
pub(crate) fn primitives_table(
    lua: &Lua,
    staging: &Arc<Mutex<Staging>>,
    sources: &Sources,
    origin: &Origin,
) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let origin = Arc::new(origin.clone());

    for (primitive, role) in [("target", TargetRole::Build), ("test", TargetRole::Test)] {
        let staging = Arc::clone(staging);
        let origin = Arc::clone(&origin);
        let function = lua.create_function(move |lua, (name, options): (Value, Value)| {
            declare(lua, &staging, &origin, primitive, || {
                primitives::target(&name, &options, role).map(StagedItem::Target)
            })
        })?;
        table.set(primitive, function)?;
    }

    let staging_ref = Arc::clone(staging);
    let origin_ref = Arc::clone(&origin);
    let rule = lua.create_function(move |lua, (name, options): (Value, Value)| {
        declare(lua, &staging_ref, &origin_ref, "rule", || {
            primitives::rule(&name, &options).map(StagedItem::Rule)
        })
    })?;
    table.set("rule", rule)?;

    let staging_ref = Arc::clone(staging);
    let origin_ref = Arc::clone(&origin);
    let alias = lua.create_function(move |lua, (name, target): (Value, Value)| {
        declare(lua, &staging_ref, &origin_ref, "alias", || {
            primitives::alias(&name, &target).map(StagedItem::Alias)
        })
    })?;
    table.set("alias", alias)?;

    let staging_ref = Arc::clone(staging);
    let origin_ref = Arc::clone(&origin);
    let option = lua.create_function(move |lua, (name, options): (Value, Value)| {
        declare(lua, &staging_ref, &origin_ref, "option", || {
            primitives::option(&name, &options).map(StagedItem::Setting)
        })
    })?;
    table.set("option", option)?;

    let staging_ref = Arc::clone(staging);
    let subdir = lua.create_function(move |lua, path: Value| {
        guarded(lua, &staging_ref, "subdir", |staging| {
            let path = primitives::subdir(&path)?;
            staging.stage_subdir(path).map_err(exceeded)
        })
    })?;
    table.set("subdir", subdir)?;

    let staging_ref = Arc::clone(staging);
    let walk = sources.clone();
    let found = lua.create_function(move |lua, pattern: Value| {
        let found = guarded(lua, &staging_ref, "sources", |_| {
            walk.find(&primitives::pattern(&pattern)?)
        })?;
        lua.create_sequence_from(found)
    })?;
    table.set("sources", found)?;

    Ok(table)
}

/// Snapshots the language globals (every global name and value except [`NOT_LANGUAGE`]) into the
/// registry, where a build file cannot reach it.
///
/// A table among them, such as `string` or `table`, is snapshotted as a copy. A build file that
/// reassigns `string`, or assigns into it, after this call changes its own globals and nothing the
/// snapshot holds.
pub(crate) fn record_language(lua: &Lua) -> mlua::Result<()> {
    let snapshot = lua.create_table()?;
    copy_tables(lua, &lua.globals(), &snapshot)?;
    for name in NOT_LANGUAGE {
        snapshot.set(name, Value::Nil)?;
    }
    lua.set_named_registry_value(LANGUAGE_REGISTRY_KEY, snapshot)
}

/// Reads a declaration with `read` and stages it as made by `origin`.
fn declare(
    lua: &Lua,
    staging: &Mutex<Staging>,
    origin: &Origin,
    primitive: &'static str,
    read: impl FnOnce() -> Result<StagedItem, Refusal>,
) -> mlua::Result<()> {
    guarded(lua, staging, primitive, |staging| {
        staging.stage(read()?, origin).map_err(exceeded)
    })
}

/// Runs one primitive call against the staging buffer.
///
/// A file that already failed fails every later call with the same message. A new refusal is
/// recorded — so a `pcall` that swallows it does not un-fail the file — and raised as a Lua error
/// naming the line that made the call: a line of the build file, or of the plugin file the call
/// came from.
///
/// The buffer stays locked while `work` runs, and `work` calls into Lua, which can run a
/// finalizer that calls a primitive again. That re-entrant call finds the lock taken and fails with
/// a Lua error instead of waiting on itself; it records nothing, because the buffer is unavailable.
pub(crate) fn guarded<T>(
    lua: &Lua,
    staging: &Mutex<Staging>,
    primitive: &'static str,
    work: impl FnOnce(&mut Staging) -> Result<T, Refusal>,
) -> mlua::Result<T> {
    let site = call_site(lua);
    let mut staging = match staging.try_lock() {
        Ok(staging) => staging,
        Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
        Err(TryLockError::WouldBlock) => {
            return Err(mlua::Error::RuntimeError(format!(
                "buildl.{primitive}: buildl primitives cannot be called while another buildl call is in progress"
            )));
        }
    };
    let recorded = staging.recorded().map(ToString::to_string);
    let outcome = recorded.map_or_else(
        || {
            work(&mut staging).map_err(|refusal| {
                let diagnostic = refusal.diagnostic(primitive, site.as_ref());
                let message = diagnostic.to_string();
                staging.record(refusal.failure().clone(), diagnostic);
                message
            })
        },
        Err,
    );
    drop(staging);
    outcome.map_err(mlua::Error::RuntimeError)
}

/// Marks a chunk name as a plugin's. Lua takes a name that starts with `=` as given, and airsl
/// names the build file's chunk with a leading `@`, so the mark tells the two apart.
const PLUGIN_CHUNK_MARK: char = '=';

/// The name the plugin file `chunk`, as the workspace names it, is compiled under.
pub(crate) fn plugin_chunk_name(chunk: &str) -> String {
    format!("{PLUGIN_CHUNK_MARK}{chunk}")
}

/// Where the nearest Lua frame on the stack is: the line that made the call, even when it went
/// through `pcall`, in the build file or in the plugin file the frame's chunk is named after.
fn call_site(lua: &Lua) -> Option<CallSite> {
    (1..)
        .map_while(|level| {
            lua.inspect_stack(level, |debug| {
                let line = debug.current_line()?;
                let source = debug.source().source;
                let plugin_chunk = source
                    .as_deref()
                    .and_then(|name| name.strip_prefix(PLUGIN_CHUNK_MARK));
                Some(
                    plugin_chunk.map_or(CallSite::BuildFile { line }, |chunk| CallSite::Plugin {
                        chunk: chunk.to_owned(),
                        line,
                    }),
                )
            })
        })
        .flatten()
        .next()
}

fn exceeded(exceeded: Exceeded) -> Refusal {
    Refusal::new(
        EvaluationFailure::LimitReached {
            limit: EvaluationLimit::Staging,
        },
        exceeded.to_string(),
    )
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::{BTreeMap, BTreeSet};
    use std::fs;
    use std::sync::{Arc, Mutex};

    use airsl::{Engine, LanguageSurface, ModuleSet, Policy, Script};
    use buildl_core::{
        DeclarationOrder, Diagnostic, Directory, EvaluationFailure, EvaluationLimit, Freshness,
        NetworkAccess, Origin, StagedAlias, StagedDeclaration, StagedFile, StagedItem, StagedRule,
        StagedSetting, StagedSubdir, StagedTarget, TargetRole, Written,
    };

    use airsl::mlua::{Lua, Table, Value};
    use buildl_core::{PluginName, PluginRef, PluginVersion};

    use super::{BuildlModule, LANGUAGE_REGISTRY_KEY, primitives_table, record_language};
    use crate::limits::DeclarationLimits;
    use crate::sources::Sources;
    use crate::staging::Staging;

    /// Evaluates `source` with only the `buildl` module installed, staging into `staging`.
    fn evaluate(source: &str, staging: &Arc<Mutex<Staging>>) -> airsl::Result<String> {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        fs::write(root.join("a.c"), "").unwrap();
        fs::write(root.join("b.c"), "").unwrap();
        // dyn: `ModuleSet::insert` takes `Box<dyn HostModule>`; the set is airsl's extension seam.
        let mut modules = ModuleSet::new();
        modules
            .insert(Box::new(
                BuildlModule::new(
                    Arc::clone(staging),
                    Sources::new(
                        &root,
                        &Directory::root(),
                        DeclarationLimits::default().walk_entries(),
                        &[],
                    ),
                    &root,
                    BTreeMap::new(),
                )
                .unwrap(),
            ))
            .unwrap();
        let engine = Engine::builder()
            .policy(Policy::confined().with_language(LanguageSurface::Minimal))
            .stdlib(modules)
            .build()
            .unwrap();
        engine.eval_to::<String>(&Script::from_source(source, "build.lua").unwrap())
    }

    /// Evaluates `source`, returning what `eval_to` gave and what was staged.
    fn run(
        source: &str,
        budget: u64,
    ) -> (
        airsl::Result<String>,
        Result<StagedFile, (EvaluationFailure, Diagnostic)>,
    ) {
        let staging = Arc::new(Mutex::new(Staging::new(budget)));
        let result = evaluate(source, &staging);
        let staged = std::mem::replace(&mut *staging.lock().unwrap(), Staging::new(0)).finish();
        (result, staged)
    }

    fn strings<T>(texts: &[&str]) -> Vec<Written<T>> {
        texts.iter().map(|text| Written::new(*text)).collect()
    }

    #[test]
    fn binds_one_table_under_two_names_without_inheritance() {
        let (result, _) = run(
            "return tostring(buildl == airsstack.buildl) .. ' ' .. tostring(buildl.json)",
            u64::MAX,
        );
        assert_eq!(result.unwrap(), "true nil");
    }

    #[test]
    fn withholds_random_and_print() {
        let (result, _) = run(
            "return type(math.random) .. type(math.randomseed) .. type(print) .. type(math.floor)",
            u64::MAX,
        );
        assert_eq!(result.unwrap(), "nilnilnilfunction");
    }

    #[test]
    fn replaces_tostring_and_format_with_address_free_versions() {
        let (result, _) = run(
            "local a = pcall(tostring, {})\n\
             local b = pcall(string.format, '%p', 1)\n\
             return tostring(a) .. tostring(b) .. string.format('%s', 'x')",
            u64::MAX,
        );
        assert_eq!(result.unwrap(), "falsefalsex");
    }

    #[test]
    fn stages_every_primitive_in_call_order() {
        let (result, staged) = run(
            "buildl.subdir('lib')\n\
             buildl.rule('cc', { run = { 'cc' } })\n\
             buildl.target('app', { rule = 'cc' })\n\
             buildl.test('app_test', { run = { 'app' } })\n\
             buildl.alias('default', 'app')\n\
             buildl.option('mode', { default = 'debug' })\n\
             return 'ok'",
            u64::MAX,
        );
        assert_eq!(result.unwrap(), "ok");
        let target = |name: &str, role, rule: Option<&str>, run: Option<&[&str]>| StagedTarget {
            name: Written::new(name),
            role,
            rule: rule.map(Written::new),
            run: run.map(strings),
            inputs: Vec::new(),
            deps: Vec::new(),
            outputs: None,
            env: Vec::new(),
            network: NetworkAccess::Sealed,
            freshness: Freshness::Cached,
        };
        let declared = |order, item| StagedDeclaration {
            order: DeclarationOrder::new(order),
            origin: Origin::BuildFile,
            item,
        };
        assert_eq!(
            staged.unwrap(),
            StagedFile {
                declarations: vec![
                    declared(
                        1,
                        StagedItem::Rule(StagedRule {
                            name: Written::new("cc"),
                            run: strings(&["cc"]),
                            description: None,
                        })
                    ),
                    declared(
                        2,
                        StagedItem::Target(target("app", TargetRole::Build, Some("cc"), None))
                    ),
                    declared(
                        3,
                        StagedItem::Target(target(
                            "app_test",
                            TargetRole::Test,
                            None,
                            Some(&["app"])
                        ))
                    ),
                    declared(
                        4,
                        StagedItem::Alias(StagedAlias {
                            name: Written::new("default"),
                            target: Written::new("app"),
                        })
                    ),
                    declared(
                        5,
                        StagedItem::Setting(StagedSetting {
                            name: Written::new("mode"),
                            default: Written::new("debug"),
                        })
                    ),
                ],
                subdirs: vec![StagedSubdir {
                    path: Written::new("lib"),
                    order: DeclarationOrder::new(0),
                }],
            }
        );
    }

    #[test]
    fn a_refusal_names_the_line_and_fails_the_file() {
        let (result, staged) = run("\n\nbuildl.target('app', { run = 1 })", u64::MAX);
        let message = result.unwrap_err().to_string();
        assert!(
            message.contains(
                "line 3: buildl.target 'app': field 'run' must be a list of strings, found integer"
            ),
            "{message}"
        );
        assert_eq!(
            staged.unwrap_err(),
            (
                EvaluationFailure::WrongFieldType {
                    field: Written::new("run"),
                },
                Diagnostic::new(
                    "line 3: buildl.target 'app': field 'run' must be a list of strings, found integer"
                ),
            )
        );
    }

    #[test]
    fn a_swallowed_refusal_still_fails_the_file_and_stops_later_calls() {
        let (result, staged) = run(
            "local ok = pcall(buildl.target, 'app', { srcs = {} })\n\
             local again = pcall(buildl.alias, 'default', 'app')\n\
             return tostring(ok) .. ' ' .. tostring(again)",
            u64::MAX,
        );
        assert_eq!(result.unwrap(), "false false");
        let (failure, diagnostic) = staged.unwrap_err();
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
    fn a_swallowed_sources_refusal_still_fails_the_file_and_stops_later_calls() {
        let (result, staged) = run(
            "local ok = pcall(buildl.sources, 'a[')\n\
             local again = pcall(buildl.alias, 'default', 'app')\n\
             return tostring(ok) .. ' ' .. tostring(again)",
            u64::MAX,
        );
        assert_eq!(result.unwrap(), "false false");
        let (failure, diagnostic) = staged.unwrap_err();
        assert_eq!(failure, EvaluationFailure::Runtime);
        assert!(
            diagnostic.as_str().starts_with("line 1: buildl.sources"),
            "{diagnostic:?}"
        );
    }

    #[test]
    fn a_call_while_the_staging_buffer_is_in_use_fails_instead_of_waiting() {
        let staging = Arc::new(Mutex::new(Staging::new(u64::MAX)));
        let held = staging.lock().unwrap();
        let result = evaluate(
            "local ok, message = pcall(buildl.subdir, 'lib')\n\
             return tostring(ok) .. ' ' .. tostring(message)",
            &staging,
        );
        drop(held);
        let output = result.unwrap();
        assert!(
            output.starts_with("false ")
                && output.contains("cannot be called while another buildl call is in progress"),
            "{output}"
        );
        assert!(staging.lock().unwrap().recorded().is_none());
    }

    #[test]
    fn a_call_past_the_budget_is_a_staging_limit() {
        let (result, staged) = run("buildl.subdir('lib')", 1);
        assert!(result.is_err());
        let (failure, diagnostic) = staged.unwrap_err();
        assert_eq!(
            failure,
            EvaluationFailure::LimitReached {
                limit: EvaluationLimit::Staging,
            }
        );
        assert_eq!(
            diagnostic.as_str(),
            "line 1: buildl.subdir: staging budget of 1 bytes exceeded"
        );
    }

    #[test]
    fn sources_returns_a_sorted_lua_list() {
        let (result, staged) = run("return table.concat(buildl.sources('*.c'), ',')", u64::MAX);
        assert_eq!(result.unwrap(), "a.c,b.c");
        assert_eq!(
            staged.unwrap(),
            StagedFile {
                declarations: Vec::new(),
                subdirs: Vec::new(),
            }
        );
    }

    fn plugin(name: &str) -> PluginRef {
        PluginRef {
            name: PluginName::parse(name).unwrap(),
            version: PluginVersion::parse("1.0.0").unwrap(),
            run: BTreeSet::new(),
            env: BTreeSet::new(),
        }
    }

    #[test]
    fn a_table_built_for_an_origin_stamps_its_declarations() {
        let lua = Lua::new();
        let staging = Arc::new(Mutex::new(Staging::new(u64::MAX)));
        let sources = Sources::new(
            std::path::Path::new("."),
            &Directory::root(),
            DeclarationLimits::default().walk_entries(),
            &[],
        );
        let outer = Origin::plugins(vec![plugin("outer")]).unwrap();
        let nested = Origin::plugins(vec![plugin("outer"), plugin("inner")]).unwrap();
        for (name, origin) in [("outer", &outer), ("nested", &nested)] {
            let table = primitives_table(&lua, &staging, &sources, origin).unwrap();
            lua.globals().set(name, table).unwrap();
        }
        lua.load(
            "outer.alias('a', 'x')\n\
             nested.alias('b', 'x')\n\
             outer.option('mode', { default = 'debug' })",
        )
        .exec()
        .unwrap();
        let staged = std::mem::replace(&mut *staging.lock().unwrap(), Staging::new(0))
            .finish()
            .unwrap();
        let origins: Vec<&Origin> = staged.declarations.iter().map(|d| &d.origin).collect();
        assert_eq!(origins, [&outer, &nested, &outer]);
    }

    #[test]
    fn the_file_table_still_stamps_build_file() {
        let (result, staged) = run(
            "buildl.alias('a', 'x')\n\
             airsstack.buildl.target('t', { run = { 'x' } })\n\
             return 'ok'",
            u64::MAX,
        );
        assert_eq!(result.unwrap(), "ok");
        let staged = staged.unwrap();
        assert_eq!(staged.declarations.len(), 2);
        assert!(
            staged
                .declarations
                .iter()
                .all(|declaration| declaration.origin == Origin::BuildFile)
        );
    }

    #[test]
    fn the_language_snapshot_excludes_buildl_airsstack_and_g() {
        let lua = Lua::new();
        let globals = lua.globals();
        for name in ["buildl", "airsstack"] {
            globals.set(name, lua.create_table().unwrap()).unwrap();
        }
        globals.set("answer", 42).unwrap();
        record_language(&lua).unwrap();

        let snapshot: Table = lua.named_registry_value(LANGUAGE_REGISTRY_KEY).unwrap();
        let mut names: Vec<String> = snapshot
            .pairs::<String, Value>()
            .map(|pair| pair.unwrap().0)
            .collect();
        names.sort();
        assert!(names.contains(&"answer".to_owned()), "{names:?}");
        assert!(names.contains(&"math".to_owned()), "{names:?}");
        for excluded in ["buildl", "airsstack", "_G", "require"] {
            assert!(!names.contains(&excluded.to_owned()), "{excluded}");
        }

        globals.set("answer", "changed").unwrap();
        assert_eq!(snapshot.get::<i64>("answer").unwrap(), 42);

        let math: Table = globals.get("math").unwrap();
        math.set("floor", Value::Nil).unwrap();
        let recorded: Table = snapshot.get("math").unwrap();
        assert!(matches!(
            recorded.get::<Value>("floor").unwrap(),
            Value::Function(_)
        ));
    }
}
