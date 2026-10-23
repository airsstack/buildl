//! The environment one plugin instance runs under.
//!
//! Its own file because the environment is the plugin's whole reach. It is a new table, never a
//! read-through to the build file's globals: a plugin that could reach the file's `buildl`, as
//! `buildl` or as `airsstack.buildl`, would declare without its origin and slip past its caps.
//!
//! Responsibilities: [`build`], which assembles the table.
//!
//! Non-responsibilities: the `require` it holds, which `require` owns, and `buildl.use`, which
//! `loader` owns.

use std::path::Path;

use airsl::mlua::{self, Lua, Table, Value};
use buildl_core::{Origin, PluginRef};

use crate::module::{LANGUAGE_REGISTRY_KEY, primitives_table};
use crate::plugin::loader::Plugins;
use crate::plugin::require;

/// The curated host modules a plugin shares with the build file that used it.
const CURATED: [&str; 5] = ["json", "path", "regex", "hash", "glob"];

/// The environment of the plugin instance reached through `chain`, whose files live under the
/// canonical directory `dir`, which the workspace calls `label`.
///
/// It holds, and holds only:
///
/// - `buildl`, a primitives table whose declarations carry the origin `chain`, with its own `use`;
/// - `airsstack`, a new table holding the build file's curated modules and, as `buildl`, that same
///   table;
/// - `require`, confined to `dir`;
/// - `_G`, the environment itself;
/// - the language globals as the build file saw them when the engine was built.
///
/// # Errors
///
/// Returns mlua's error when a table or function cannot be created, or `chain` is empty.
pub(super) fn build(
    lua: &Lua,
    plugins: &Plugins,
    chain: &[PluginRef],
    dir: &Path,
    label: &str,
) -> mlua::Result<Table> {
    let origin = Origin::plugins(chain.to_vec())
        .map_err(|error| mlua::Error::RuntimeError(error.to_string()))?;
    let environment = lua.create_table()?;

    let language: Table = lua.named_registry_value(LANGUAGE_REGISTRY_KEY)?;
    for pair in language.pairs::<Value, Value>() {
        let (name, value) = pair?;
        environment.set(name, value)?;
    }

    let buildl = primitives_table(lua, plugins.staging(), plugins.sources(), &origin)?;
    plugins.bind_use(lua, &buildl, chain)?;

    let airsstack = lua.create_table()?;
    if let Value::Table(shared) = lua.globals().get::<Value>("airsstack")? {
        for module in CURATED {
            let value: Value = shared.get(module)?;
            if !value.is_nil() {
                airsstack.set(module, value)?;
            }
        }
    }
    airsstack.set("buildl", buildl.clone())?;

    environment.set("buildl", buildl)?;
    environment.set("airsstack", airsstack)?;
    environment.set("_G", environment.clone())?;
    let require = require::function(lua, plugins.staging(), &environment, dir, label)?;
    environment.set("require", require)?;
    Ok(environment)
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeMap;
    use std::collections::BTreeSet;
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    use airsl::mlua::{Lua, Table, Value};
    use buildl_core::{Directory, PluginName, PluginRef, PluginVersion};

    use super::build;
    use crate::limits::DeclarationLimits;
    use crate::module::{LANGUAGE_REGISTRY_KEY, record_language};
    use crate::plugin::loader::Plugins;
    use crate::sources::Sources;
    use crate::staging::Staging;

    fn plugin(name: &str) -> PluginRef {
        PluginRef {
            name: PluginName::parse(name).unwrap(),
            version: PluginVersion::parse("1.0.0").unwrap(),
            run: BTreeSet::new(),
            env: BTreeSet::new(),
        }
    }

    /// A state with the build file's globals: `json` and `fs` under `airsstack`, and a `file_global`.
    fn lua_with_globals() -> Lua {
        let lua = Lua::new();
        let airsstack = lua.create_table().unwrap();
        for module in ["json", "fs"] {
            airsstack.set(module, lua.create_table().unwrap()).unwrap();
        }
        lua.globals().set("airsstack", airsstack).unwrap();
        lua.globals().set("file_global", 1).unwrap();
        record_language(&lua).unwrap();
        lua
    }

    fn environment(lua: &Lua) -> Table {
        let staging = Arc::new(Mutex::new(Staging::new(u64::MAX)));
        let sources = Sources::new(
            Path::new("."),
            &Directory::root(),
            DeclarationLimits::default().walk_entries(),
            &[],
        );
        let plugins = Plugins::new(Path::new("."), BTreeMap::new(), &staging, &sources);
        build(lua, &plugins, &[plugin("p")], Path::new("."), "plugins/p").unwrap()
    }

    fn names(table: &Table) -> Vec<String> {
        let mut names: Vec<String> = table
            .pairs::<String, Value>()
            .map(|pair| pair.unwrap().0)
            .collect();
        names.sort();
        names
    }

    #[test]
    fn the_environment_holds_the_language_and_the_four_names_and_nothing_else() {
        let lua = lua_with_globals();
        let recorded: Table = lua.named_registry_value(LANGUAGE_REGISTRY_KEY).unwrap();
        let mut expected = names(&recorded);
        assert!(expected.contains(&"file_global".to_owned()));
        assert!(expected.contains(&"string".to_owned()));
        expected.extend(["buildl", "airsstack", "_G", "require"].map(str::to_owned));
        expected.sort();
        assert_eq!(names(&environment(&lua)), expected);
    }

    #[test]
    fn airsstack_holds_the_curated_modules_and_the_stamped_buildl_only() {
        let lua = lua_with_globals();
        let environment = environment(&lua);
        let airsstack: Table = environment.get("airsstack").unwrap();
        // `fs` is not curated, so a plugin does not get it even when the state holds it.
        assert_eq!(names(&airsstack), ["buildl", "json"]);
        let buildl: Table = environment.get("buildl").unwrap();
        let inner: Table = airsstack.get("buildl").unwrap();
        assert!(buildl.equals(&inner).unwrap());
    }

    #[test]
    fn the_environment_is_its_own_globals_and_not_the_states() {
        let lua = lua_with_globals();
        let environment = environment(&lua);
        let own: Table = environment.get("_G").unwrap();
        assert!(own.equals(&environment).unwrap());
        assert!(!own.equals(&lua.globals()).unwrap());
        let theirs: Table = environment.get("airsstack").unwrap();
        let shared: Table = lua.globals().get("airsstack").unwrap();
        assert!(!theirs.equals(&shared).unwrap());
    }
}
