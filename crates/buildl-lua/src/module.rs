//! The `buildl` module table: the declaration API a build file calls.
//!
//! Its own file because it is the one airsl host module buildl writes. airsl installs it as
//! `airsstack.buildl`, and its installation binds the same table to the global `buildl`. It also
//! withholds the three globals that would let a build file's output vary between runs or reach the
//! host's stdout: `math.random`, `math.randomseed` and `print`, and replaces `tostring` and
//! `string.format` with versions that refuse address-derived text.
//!
//! Responsibilities: [`BuildlModule`]; turning a primitive call into a staged record or a sticky
//! refusal that names the build file's line.
//!
//! Non-responsibilities: the argument shapes, which `primitives` owns, the walk behind
//! `buildl.sources`, which `sources` owns, and the replacement functions, which `stable_text`
//! owns.

use std::sync::{Arc, Mutex, TryLockError};

use airsl::mlua::{self, Lua, Table, Value};
use airsl::{HostModule, InstallContext, ModuleName};
use buildl_core::{EvaluationFailure, EvaluationLimit, StagedItem, TargetRole};

use crate::primitives;
use crate::refusal::Refusal;
use crate::sources::Sources;
use crate::stable_text;
use crate::staging::{Exceeded, Staging};

/// The `buildl` host module for one build file's evaluation.
#[derive(Debug)]
pub(crate) struct BuildlModule {
    name: ModuleName,
    staging: Arc<Mutex<Staging>>,
    sources: Sources,
}

impl BuildlModule {
    /// A module staging into `staging`, whose `buildl.sources` walks `sources`.
    ///
    /// # Errors
    ///
    /// Never in practice: the only failure is `buildl` not being a valid module name.
    pub(crate) fn new(staging: Arc<Mutex<Staging>>, sources: Sources) -> airsl::Result<Self> {
        Ok(Self {
            name: ModuleName::new("buildl")?,
            staging,
            sources,
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

        for (primitive, role) in [("target", TargetRole::Build), ("test", TargetRole::Test)] {
            let staging = Arc::clone(&self.staging);
            let function = lua
                .create_function(move |lua, (name, options): (Value, Value)| {
                    declare(lua, &staging, primitive, || {
                        primitives::target(&name, &options, role).map(StagedItem::Target)
                    })
                })
                .map_err(fail)?;
            table.set(primitive, function).map_err(fail)?;
        }

        let staging = Arc::clone(&self.staging);
        let rule = lua
            .create_function(move |lua, (name, options): (Value, Value)| {
                declare(lua, &staging, "rule", || {
                    primitives::rule(&name, &options).map(StagedItem::Rule)
                })
            })
            .map_err(fail)?;
        table.set("rule", rule).map_err(fail)?;

        let staging = Arc::clone(&self.staging);
        let alias = lua
            .create_function(move |lua, (name, target): (Value, Value)| {
                declare(lua, &staging, "alias", || {
                    primitives::alias(&name, &target).map(StagedItem::Alias)
                })
            })
            .map_err(fail)?;
        table.set("alias", alias).map_err(fail)?;

        let staging = Arc::clone(&self.staging);
        let option = lua
            .create_function(move |lua, (name, options): (Value, Value)| {
                declare(lua, &staging, "option", || {
                    primitives::option(&name, &options).map(StagedItem::Setting)
                })
            })
            .map_err(fail)?;
        table.set("option", option).map_err(fail)?;

        let staging = Arc::clone(&self.staging);
        let subdir = lua
            .create_function(move |lua, path: Value| {
                guarded(lua, &staging, "subdir", |staging| {
                    let path = primitives::subdir(&path)?;
                    staging.stage_subdir(path).map_err(exceeded)
                })
            })
            .map_err(fail)?;
        table.set("subdir", subdir).map_err(fail)?;

        let staging = Arc::clone(&self.staging);
        let walk = self.sources.clone();
        let sources = lua
            .create_function(move |lua, pattern: Value| {
                let found = guarded(lua, &staging, "sources", |_| {
                    walk.find(&primitives::pattern(&pattern)?)
                })?;
                lua.create_sequence_from(found)
            })
            .map_err(fail)?;
        table.set("sources", sources).map_err(fail)?;

        let globals = lua.globals();
        globals.set("buildl", table.clone()).map_err(fail)?;
        let math: Table = globals.get("math").map_err(fail)?;
        math.set("random", Value::Nil).map_err(fail)?;
        math.set("randomseed", Value::Nil).map_err(fail)?;
        globals.set("print", Value::Nil).map_err(fail)?;
        stable_text::install(lua).map_err(fail)?;
        Ok(())
    }
}

/// Reads a declaration with `read` and stages it.
fn declare(
    lua: &Lua,
    staging: &Mutex<Staging>,
    primitive: &'static str,
    read: impl FnOnce() -> Result<StagedItem, Refusal>,
) -> mlua::Result<()> {
    guarded(lua, staging, primitive, |staging| {
        staging.stage(read()?).map_err(exceeded)
    })
}

/// Runs one primitive call against the staging buffer.
///
/// A file that already failed fails every later call with the same message. A new refusal is
/// recorded — so a `pcall` that swallows it does not un-fail the file — and raised as a Lua error
/// naming the line that made the call.
///
/// The buffer stays locked while `work` runs, and `work` calls into Lua, which can run a
/// finalizer that calls a primitive again. That re-entrant call finds the lock taken and fails with
/// a Lua error instead of waiting on itself; it records nothing, because the buffer is unavailable.
fn guarded<T>(
    lua: &Lua,
    staging: &Mutex<Staging>,
    primitive: &'static str,
    work: impl FnOnce(&mut Staging) -> Result<T, Refusal>,
) -> mlua::Result<T> {
    let line = call_site(lua);
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
                let diagnostic = refusal.diagnostic(primitive, line);
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

/// The line of the nearest Lua frame on the stack: the build-file line that made the call, even
/// when it went through `pcall`.
fn call_site(lua: &Lua) -> Option<usize> {
    #[expect(
        clippy::redundant_closure_for_method_calls,
        reason = "`Debug::current_line` as a path is not general over `Debug`'s lifetime"
    )]
    (1..)
        .map_while(|level| lua.inspect_stack(level, |debug| debug.current_line()))
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

    use std::fs;
    use std::sync::{Arc, Mutex};

    use airsl::{Engine, LanguageSurface, ModuleSet, Policy, Script};
    use buildl_core::{
        DeclarationOrder, Diagnostic, Directory, EvaluationFailure, EvaluationLimit, Freshness,
        NetworkAccess, StagedAlias, StagedDeclaration, StagedFile, StagedItem, StagedRule,
        StagedSetting, StagedSubdir, StagedTarget, TargetRole, Written,
    };

    use super::BuildlModule;
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
                    ),
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
}
