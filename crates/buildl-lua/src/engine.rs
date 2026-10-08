//! The airsl engine one build file is evaluated on.
//!
//! Its own file because the engine's configuration is the declaration phase's sandbox, and it is
//! stated once, here. The language surface is airsl's minimal one (no `os`, no `coroutine`, no
//! `require`), the engine holds no grant of any kind, and the host modules are a curated set:
//! `json`, `path` without `absolute`, `regex`, `hash` and `glob`, then `buildl`. With no grant,
//! `glob.walk` is refused; `buildl.sources` is the only filesystem read.
//!
//! Responsibilities: [`build`].
//!
//! Non-responsibilities: what the `buildl` module installs, which `module` owns.

use airsl::modules::{Glob, Hash, Json, Regex};
use airsl::{Engine, GrantSet, LanguageSurface, ModuleSet, Policy};

use crate::curated::CuratedPath;
use crate::limits::DeclarationLimits;
use crate::module::BuildlModule;

/// A fresh engine under `limits`, with `buildl` installed last.
///
/// # Errors
///
/// Returns airsl's error when the engine cannot be configured or a module cannot be installed.
pub(crate) fn build(limits: &DeclarationLimits, buildl: BuildlModule) -> airsl::Result<Engine> {
    // dyn: `ModuleSet::insert` takes `Box<dyn HostModule>`; the set is airsl's extension seam.
    let mut modules = ModuleSet::new();
    modules.insert(Box::new(Json::new()))?;
    modules.insert(Box::new(CuratedPath::default()))?;
    modules.insert(Box::new(Regex::new()))?;
    modules.insert(Box::new(Hash::new()))?;
    modules.insert(Box::new(Glob::new()))?;
    modules.insert(Box::new(buildl))?;
    let policy = Policy::confined()
        .with_language(LanguageSurface::Minimal)
        .with_grants(GrantSet::declared())
        .with_limits(limits.resource_limits());
    Engine::builder().policy(policy).stdlib(modules).build()
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use core::num::NonZeroU64;
    use std::fs;
    use std::sync::{Arc, Mutex};

    use airsl::{Engine, Script};
    use buildl_core::Directory;

    use super::build;
    use crate::limits::DeclarationLimits;
    use crate::module::BuildlModule;
    use crate::sources::Sources;
    use crate::staging::Staging;

    fn engine() -> (tempfile::TempDir, Engine) {
        engine_with(&DeclarationLimits::default())
    }

    fn engine_with(limits: &DeclarationLimits) -> (tempfile::TempDir, Engine) {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let module = BuildlModule::new(
            Arc::new(Mutex::new(Staging::new(u64::MAX))),
            Sources::new(&root, &Directory::root()),
        )
        .unwrap();
        (dir, build(limits, module).unwrap())
    }

    fn eval(engine: &Engine, source: &str) -> airsl::Result<String> {
        engine.eval_to::<String>(&Script::from_source(source, "build.lua").unwrap())
    }

    #[test]
    fn installs_the_curated_modules_and_nothing_else() {
        let (_dir, engine) = engine();
        let names: Vec<&str> = engine
            .module_names()
            .iter()
            .map(|name| name.as_str())
            .collect();
        assert_eq!(names, ["json", "path", "regex", "hash", "glob", "buildl"]);
    }

    #[test]
    fn withholds_the_ambient_globals() {
        let (_dir, engine) = engine();
        let found = eval(
            &engine,
            "return table.concat({ type(os), type(io), type(coroutine), type(require), \
             type(load), type(dofile), type(debug), type(print), type(math.random), \
             type(airsstack.path.absolute) }, ',')",
        )
        .unwrap();
        assert_eq!(found, "nil,nil,nil,nil,nil,nil,nil,nil,nil,nil");
    }

    #[test]
    fn the_supplied_instruction_ceiling_bounds_evaluation() {
        // The loop costs far less than airsl's confined default ceiling, so only the ceiling
        // passed to `build` can stop it.
        let tight = DeclarationLimits::new(
            DeclarationLimits::default().memory_bytes(),
            NonZeroU64::new(1_000).unwrap(),
        );
        let script = "local n = 0 for i = 1, 100000 do n = n + i end return tostring(n)";

        let (_tight_dir, tight_engine) = engine_with(&tight);
        let error = eval(&tight_engine, script).unwrap_err();
        assert!(
            matches!(error, airsl::Error::InstructionLimit { .. }),
            "{error}"
        );

        let (_default_dir, default_engine) = engine();
        assert_eq!(eval(&default_engine, script).unwrap(), "5000050000");
    }

    #[test]
    fn glob_walk_has_no_grant_to_read_with() {
        let (dir, engine) = engine();
        let root = fs::canonicalize(dir.path()).unwrap();
        let source = format!(
            "return table.concat(airsstack.glob.walk('{}', '*'), ',')",
            root.display()
        );
        let message = eval(&engine, &source).unwrap_err().to_string();
        assert!(message.contains("glob.walk denied"), "{message}");
    }
}
