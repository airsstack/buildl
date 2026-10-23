//! The curated airsl modules as the engine installs them.
//!
//! Its own file because it changes airsl modules rather than adding one. `airsstack.path` is
//! pure path arithmetic except `absolute`, which resolves against the process working directory:
//! a build file using it would declare different text depending on where `buildl` was run from,
//! and that difference cannot be seen by evaluating the file twice in one process. The
//! `airsstack` table does not exist yet while modules install, so the function is removed from
//! the module's own table, here, rather than from `airsstack.path` afterwards.
//!
//! The same moment is the only one at which a module's table is known to be untouched by the
//! build file, so [`Recorded`] copies each module's table into the registry as it installs, for
//! the plugins a build file uses to be given their own copies of.
//!
//! Responsibilities: [`CuratedPath`] and [`Recorded`].
//!
//! Non-responsibilities: everything else the modules do, which is airsl's, and what a plugin's
//! environment does with the copies, which `plugin` owns.

use airsl::mlua::{self, Lua, Table, Value};
use airsl::modules::Path;
use airsl::{HostModule, InstallContext, ModuleName};

use crate::snapshot::copy_table;

/// The Lua registry key holding the table of curated modules [`Recorded`] fills, by module name.
pub(crate) const CURATED_REGISTRY_KEY: &str = "buildl.curated";

/// A curated module that records a copy of its table in the registry once it has installed, so the
/// copy holds the module as it was before any build file touched it.
#[derive(Debug)]
pub(crate) struct Recorded<M>(M);

impl<M: HostModule> Recorded<M> {
    /// `module`, recorded as it installs.
    pub(crate) const fn new(module: M) -> Self {
        Self(module)
    }
}

impl<M: HostModule> HostModule for Recorded<M> {
    fn name(&self) -> &ModuleName {
        self.0.name()
    }

    fn install(&self, lua: &Lua, table: &Table, context: &InstallContext<'_>) -> airsl::Result<()> {
        self.0.install(lua, table, context)?;
        let name = self.0.name().as_str();
        record(lua, name, table).map_err(|error| airsl::Error::lua(name, error))
    }
}

/// Adds a copy of `table` to the registry's table of curated modules, under `name`.
fn record(lua: &Lua, name: &str, table: &Table) -> mlua::Result<()> {
    let recorded =
        if let Value::Table(recorded) = lua.named_registry_value::<Value>(CURATED_REGISTRY_KEY)? {
            recorded
        } else {
            let recorded = lua.create_table()?;
            lua.set_named_registry_value(CURATED_REGISTRY_KEY, recorded.clone())?;
            recorded
        };
    recorded.set(name, copy_table(lua, table)?)
}

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

    use airsl::mlua::{Lua, Table};
    use airsl::{Engine, HostModule, InstallContext, ModuleName, ModuleSet, Policy, Script};

    use super::{CURATED_REGISTRY_KEY, CuratedPath, Recorded};

    /// A module that, as it installs, reads back the curated modules recorded before it and
    /// leaves one of them in its own table.
    struct Reader(ModuleName);

    impl HostModule for Reader {
        fn name(&self) -> &ModuleName {
            &self.0
        }

        fn install(
            &self,
            lua: &Lua,
            table: &Table,
            _context: &InstallContext<'_>,
        ) -> airsl::Result<()> {
            let recorded: Table = lua.named_registry_value(CURATED_REGISTRY_KEY).unwrap();
            table
                .set("path", recorded.get::<Table>("path").unwrap())
                .unwrap();
            Ok(())
        }
    }

    #[test]
    fn records_a_copy_of_the_module_table_as_it_installed() {
        // dyn: `ModuleSet::insert` takes `Box<dyn HostModule>`; the set is airsl's extension seam.
        let mut modules = ModuleSet::new();
        modules
            .insert(Box::new(Recorded::new(CuratedPath::default())))
            .unwrap();
        modules
            .insert(Box::new(Reader(ModuleName::new("reader").unwrap())))
            .unwrap();
        let engine = Engine::builder()
            .policy(Policy::pure())
            .stdlib(modules)
            .build()
            .unwrap();
        let result = engine
            .eval_to::<String>(
                &Script::from_source(
                    "local copy = airsstack.reader.path\n\
                     local same = copy == airsstack.path\n\
                     airsstack.path.stem = nil\n\
                     return tostring(same) .. ' ' .. type(copy.stem) .. ' ' .. type(copy.absolute)",
                    "build.lua",
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(result, "false function nil");
    }

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
