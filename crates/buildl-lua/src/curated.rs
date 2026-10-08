//! airsl's `path` module without its one ambient read.
//!
//! Its own file because it changes an airsl module rather than adding one. `airsstack.path` is
//! pure path arithmetic except `absolute`, which resolves against the process working directory:
//! a build file using it would declare different text depending on where `buildl` was run from,
//! and that difference cannot be seen by evaluating the file twice in one process. The
//! `airsstack` table does not exist yet while modules install, so the function is removed from
//! the module's own table, here, rather than from `airsstack.path` afterwards.
//!
//! Responsibilities: [`CuratedPath`].
//!
//! Non-responsibilities: everything else `path` does, which is airsl's.

use airsl::mlua::{Lua, Table, Value};
use airsl::modules::Path;
use airsl::{HostModule, InstallContext, ModuleName};

/// `airsstack.path`, minus `absolute`.
#[derive(Debug, Default)]
pub(crate) struct CuratedPath(Path);

impl HostModule for CuratedPath {
    fn name(&self) -> &ModuleName {
        self.0.name()
    }

    fn install(&self, lua: &Lua, table: &Table, context: &InstallContext<'_>) -> airsl::Result<()> {
        self.0.install(lua, table, context)?;
        table
            .set("absolute", Value::Nil)
            .map_err(|error| airsl::Error::lua("path", error))
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use airsl::{Engine, ModuleSet, Policy, Script};

    use super::CuratedPath;

    #[test]
    fn keeps_path_arithmetic_and_drops_absolute() {
        // dyn: `ModuleSet::insert` takes `Box<dyn HostModule>`; the set is airsl's extension seam.
        let mut modules = ModuleSet::new();
        modules.insert(Box::new(CuratedPath::default())).unwrap();
        let engine = Engine::builder()
            .policy(Policy::pure())
            .stdlib(modules)
            .build()
            .unwrap();
        let result = engine
            .eval_to::<String>(
                &Script::from_source(
                    "return type(airsstack.path.absolute) .. ' ' .. airsstack.path.stem('src/a.c')",
                    "build.lua",
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(result, "nil a");
    }
}
