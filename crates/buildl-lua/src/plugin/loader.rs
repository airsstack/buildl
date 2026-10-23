//! `buildl.use`: loading a plugin's entry file as one instance of that plugin.
//!
//! Its own file because loading is where the rules about who may use whom live. A plugin's name
//! resolves only through the plugins the workspace names, a plugin cannot be used by itself or by
//! one it used, and each instance is keyed by the whole chain of plugins that led to it: `go`
//! used by a build file and `go` used by `ourgo` are two instances with their own modules and
//! their own origin.
//!
//! Responsibilities: [`Plugins`], one evaluation's plugins: the `use` it binds onto a primitives
//! table, the instances it has loaded, and the refusals around both.
//!
//! Non-responsibilities: reading the manifest, which `manifest` owns, the environment an
//! instance runs under, which `environment` owns, and `require`, which `require` owns.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use airsl::mlua::{self, Lua, Table, Value};
use buildl_core::{EvaluationFailure, PluginName, PluginRef};

use crate::module::guarded;
use crate::plugin::environment;
use crate::plugin::manifest::read_manifest;
use crate::refusal::Refusal;
use crate::sources::Sources;
use crate::staging::Staging;
use crate::values;

/// The plugins one evaluation may use, and the instances it has loaded so far.
///
/// Clones share their instances, so every `use` of one evaluation, from the build file or from a
/// plugin, draws on one cache.
#[derive(Debug, Clone)]
pub(crate) struct Plugins {
    shared: Arc<Shared>,
}

#[derive(Debug)]
struct Shared {
    root: PathBuf,
    directories: BTreeMap<PluginName, PathBuf>,
    staging: Arc<Mutex<Staging>>,
    sources: Sources,
    instances: Mutex<BTreeMap<Vec<PluginName>, Value>>,
}

/// What looking a `use` up found.
enum Found {
    /// The instance for this chain is already loaded; this is its module.
    Loaded(Value),
    /// The instance must be loaded.
    Fresh(Box<Fresh>),
}

/// A plugin ready to load: what its manifest says and its entry file's text.
struct Fresh {
    plugin: PluginRef,
    /// The plugin's canonical directory.
    directory: PathBuf,
    /// The plugin's directory as the workspace names it.
    label: String,
    /// The name the entry chunk goes by.
    chunk: String,
    text: String,
}

impl Plugins {
    /// The plugins of an evaluation in the workspace at `root`: each name in `directories` maps to
    /// the directory holding that plugin's `extension.toml`.
    ///
    /// `root` is the workspace root, which names the files a plugin's errors point at. Every
    /// declaration a plugin makes lands in `staging`, and `sources` is the walk its
    /// `buildl.sources` shares with the build file.
    pub(crate) fn new(
        root: &Path,
        directories: BTreeMap<PluginName, PathBuf>,
        staging: &Arc<Mutex<Staging>>,
        sources: &Sources,
    ) -> Self {
        Self {
            shared: Arc::new(Shared {
                root: root.to_path_buf(),
                directories,
                staging: Arc::clone(staging),
                sources: sources.clone(),
                instances: Mutex::new(BTreeMap::new()),
            }),
        }
    }

    /// The buffer every declaration of the evaluation is staged into.
    pub(super) fn staging(&self) -> &Arc<Mutex<Staging>> {
        &self.shared.staging
    }

    /// The source walk the build file and its plugins share.
    pub(super) fn sources(&self) -> &Sources {
        &self.shared.sources
    }

    /// Sets `use` on `table`, a primitives table whose caller is the last plugin of `chain`, or the
    /// build file when `chain` is empty.
    ///
    /// # Errors
    ///
    /// Returns mlua's error when the function cannot be created or set.
    pub(crate) fn bind_use(
        &self,
        lua: &Lua,
        table: &Table,
        chain: &[PluginRef],
    ) -> mlua::Result<()> {
        let plugins = self.clone();
        let chain = chain.to_vec();
        let function =
            lua.create_function(move |lua, name: Value| plugins.load(lua, &chain, &name))?;
        table.set("use", function)
    }

    /// The module of the plugin `name` as `chain` reaches it, loading it if this chain has not.
    fn load(&self, lua: &Lua, chain: &[PluginRef], name: &Value) -> mlua::Result<Value> {
        let found = guarded(lua, &self.shared.staging, "use", |_| self.find(chain, name))?;
        let fresh = match found {
            Found::Loaded(module) => return Ok(module),
            Found::Fresh(fresh) => fresh,
        };

        let mut chain = chain.to_vec();
        chain.push(fresh.plugin);
        let environment = environment::build(lua, self, &chain, &fresh.directory, &fresh.label)?;
        let module = lua
            .load(fresh.text)
            .set_name(format!("={}", fresh.chunk))
            .set_environment(environment)
            .eval::<Value>()?;

        self.shared
            .instances
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(
                chain.into_iter().map(|plugin| plugin.name).collect(),
                module.clone(),
            );
        Ok(module)
    }

    /// Resolves a `use` of `name` from `chain` to a loaded instance or a plugin to load, or says
    /// why it cannot be used.
    fn find(&self, chain: &[PluginRef], name: &Value) -> Result<Found, Refusal> {
        let text = values::string(name, "name")?;
        let refuse = |detail: String| Refusal::new(EvaluationFailure::Refused, detail).about(&text);

        let Some((name, directory)) = PluginName::parse(&text)
            .ok()
            .and_then(|name| self.shared.directories.get_key_value(&name))
        else {
            return Err(refuse(
                "the workspace's [plugins] names no such plugin".to_owned(),
            ));
        };
        if chain.iter().any(|plugin| &plugin.name == name) {
            let path: Vec<String> = chain
                .iter()
                .map(|plugin| plugin.name.to_string())
                .chain([name.to_string()])
                .collect();
            return Err(refuse(format!(
                "use cycle: {} would use a plugin it is already inside",
                path.join(" > ")
            )));
        }

        let key: Vec<PluginName> = chain
            .iter()
            .map(|plugin| plugin.name.clone())
            .chain([name.clone()])
            .collect();
        if let Some(module) = self
            .shared
            .instances
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&key)
        {
            return Ok(Found::Loaded(module.clone()));
        }

        let (plugin, entry) =
            read_manifest(directory, name).map_err(|diagnostic| refuse(diagnostic.to_string()))?;
        let canonical = fs::canonicalize(directory).map_err(|error| {
            Refusal::new(
                EvaluationFailure::Runtime,
                format!("{}: {error}", directory.display()),
            )
            .about(&text)
        })?;
        let entry_file = fs::canonicalize(canonical.join(&entry))
            .map_err(|error| refuse(format!("{}: {error}", directory.join(&entry).display())))?;
        if !entry_file.starts_with(&canonical) {
            return Err(refuse(format!(
                "the entry file {} resolves outside the plugin directory",
                entry.display()
            )));
        }
        let source = fs::read_to_string(&entry_file).map_err(|error| {
            Refusal::new(
                EvaluationFailure::Runtime,
                format!("{}: {error}", entry_file.display()),
            )
            .about(&text)
        })?;

        let label = self.label(&canonical);
        Ok(Found::Fresh(Box::new(Fresh {
            plugin,
            chunk: format!("{label}/{}", entry.display()),
            label,
            directory: canonical,
            text: source,
        })))
    }

    /// `directory` as the workspace names it: relative to the root when it lies under it.
    fn label(&self, directory: &Path) -> String {
        directory
            .strip_prefix(&self.shared.root)
            .unwrap_or(directory)
            .to_string_lossy()
            .into_owned()
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        clippy::panic,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::{BTreeMap, BTreeSet};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    use airsl::mlua::{Lua, Value};
    use buildl_core::{Directory, EvaluationFailure, PluginName, PluginRef, PluginVersion};

    use super::{Found, Plugins};
    use crate::limits::DeclarationLimits;
    use crate::refusal::Refusal;
    use crate::sources::Sources;
    use crate::staging::Staging;

    /// A workspace root holding the plugin `p` at `plugins/p`, and the `Plugins` that know it.
    fn workspace() -> (tempfile::TempDir, PathBuf, Plugins) {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let plugin = root.join("plugins/p");
        fs::create_dir_all(&plugin).unwrap();
        fs::write(
            plugin.join("extension.toml"),
            "[extension]\nname = \"p\"\nversion = \"2.0.0\"\nentry = \"main.lua\"\napi = 1\n",
        )
        .unwrap();
        fs::write(plugin.join("main.lua"), "return {}").unwrap();
        let staging = Arc::new(Mutex::new(Staging::new(u64::MAX)));
        let sources = Sources::new(
            &root,
            &Directory::root(),
            DeclarationLimits::default().walk_entries(),
            &[],
        );
        let directories = BTreeMap::from([(PluginName::parse("p").unwrap(), plugin.clone())]);
        let plugins = Plugins::new(&root, directories, &staging, &sources);
        (dir, plugin, plugins)
    }

    fn name(lua: &Lua, text: &str) -> Value {
        Value::String(lua.create_string(text).unwrap())
    }

    fn reference(name: &str) -> PluginRef {
        PluginRef {
            name: PluginName::parse(name).unwrap(),
            version: PluginVersion::parse("1.0.0").unwrap(),
            run: BTreeSet::new(),
            env: BTreeSet::new(),
        }
    }

    /// The diagnostic of a refusal, which must be a plain refusal.
    fn refused(found: Result<Found, Refusal>) -> String {
        match found {
            Err(refusal) => {
                assert_eq!(refusal.failure(), &EvaluationFailure::Refused);
                refusal.diagnostic("use", None).to_string()
            }
            Ok(_) => panic!("expected a refusal"),
        }
    }

    #[test]
    fn a_name_the_workspace_does_not_list_is_refused() {
        let (_dir, _plugin, plugins) = workspace();
        let lua = Lua::new();
        let said = refused(plugins.find(&[], &name(&lua, "nope")));
        assert_eq!(
            said,
            "buildl.use 'nope': the workspace's [plugins] names no such plugin"
        );
    }

    #[test]
    fn text_that_is_not_a_plugin_name_is_refused_as_unknown() {
        let (_dir, _plugin, plugins) = workspace();
        let lua = Lua::new();
        let said = refused(plugins.find(&[], &name(&lua, "not a name")));
        assert!(said.contains("names no such plugin"), "{said}");
    }

    #[test]
    fn a_value_that_is_not_text_is_refused() {
        let (_dir, _plugin, plugins) = workspace();
        let Err(refusal) = plugins.find(&[], &Value::Integer(1)) else {
            panic!("expected a refusal");
        };
        assert!(matches!(
            refusal.failure(),
            EvaluationFailure::WrongFieldType { .. }
        ));
    }

    #[test]
    fn a_plugin_already_in_the_chain_is_a_cycle() {
        let (_dir, _plugin, plugins) = workspace();
        let lua = Lua::new();
        let chain = [reference("a"), reference("p")];
        let said = refused(plugins.find(&chain, &name(&lua, "p")));
        assert!(said.contains("use cycle: a > p > p"), "{said}");
    }

    #[test]
    fn a_plugin_to_load_carries_its_manifest_and_a_workspace_relative_chunk() {
        let (_dir, plugin, plugins) = workspace();
        let lua = Lua::new();
        let Ok(Found::Fresh(fresh)) = plugins.find(&[], &name(&lua, "p")) else {
            panic!("the plugin is there to load");
        };
        assert_eq!(fresh.plugin.to_string(), "p@2.0.0");
        assert_eq!(fresh.label, "plugins/p");
        assert_eq!(fresh.chunk, "plugins/p/main.lua");
        assert_eq!(fresh.text, "return {}");
        assert_eq!(fresh.directory, plugin);
    }

    #[test]
    fn a_manifest_that_names_another_plugin_is_refused() {
        let (_dir, plugin, plugins) = workspace();
        let manifest = plugin.join("extension.toml");
        let text = fs::read_to_string(&manifest).unwrap();
        fs::write(&manifest, text.replace("\"p\"", "\"q\"")).unwrap();
        let lua = Lua::new();
        let said = refused(plugins.find(&[], &name(&lua, "p")));
        assert!(said.contains("extension.toml"), "{said}");
    }
}
