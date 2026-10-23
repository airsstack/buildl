//! `require` for one plugin instance, confined to the plugin's directory.
//!
//! Its own file because airsl's `require` only comes with a language surface that also exposes
//! `os.time` and `os.clock`, which differ from run to run. A plugin gets this one instead: a
//! dotted module name is turned into `<plugin dir>/a/b.lua`, and nothing outside the plugin
//! directory can be named, by a path, a `..` or a link.
//!
//! Responsibilities: [`function`], the `require` a plugin instance is given; the module-name and
//! containment rules it applies.
//!
//! Non-responsibilities: the environment a module runs under, which the caller hands in, and
//! deciding which directory is the plugin's.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use airsl::mlua::{Function, Lua, Table, Value};
use buildl_core::EvaluationFailure;

use crate::module::guarded;
use crate::refusal::Refusal;
use crate::staging::Staging;
use crate::values;

/// One plugin instance's `require`: where its modules live, what runs them, and which have run.
#[derive(Debug)]
struct Loader {
    dir: PathBuf,
    label: String,
    environment: Table,
    staging: Arc<Mutex<Staging>>,
    loaded: Mutex<BTreeMap<PathBuf, Value>>,
    loading: Mutex<BTreeSet<PathBuf>>,
}

/// What looking a module up found.
enum Found {
    /// The module already ran for this instance; this is its value.
    Loaded(Value),
    /// The module has not run: its canonical file, the name its chunk goes by, and its text.
    Fresh {
        file: PathBuf,
        chunk: String,
        text: String,
    },
}

/// The `require` of the plugin instance whose modules live under `dir` and run under
/// `environment`.
///
/// `dir` is the plugin's canonical directory. `label` is that directory as the workspace names
/// it, which prefixes the chunk name of every module so an error points at a workspace file.
/// A refusal is recorded in `staging`, so it fails the build file even when the plugin catches
/// the error.
///
/// # Errors
///
/// Returns mlua's error when the function cannot be created.
pub(super) fn function(
    lua: &Lua,
    staging: &Arc<Mutex<Staging>>,
    environment: &Table,
    dir: &Path,
    label: &str,
) -> airsl::mlua::Result<Function> {
    let loader = Loader {
        dir: dir.to_path_buf(),
        label: label.to_owned(),
        environment: environment.clone(),
        staging: Arc::clone(staging),
        loaded: Mutex::new(BTreeMap::new()),
        loading: Mutex::new(BTreeSet::new()),
    };
    lua.create_function(move |lua, name: Value| loader.require(lua, &name))
}

impl Loader {
    /// Returns the module `name`, running it first if it has not run for this instance.
    fn require(&self, lua: &Lua, name: &Value) -> airsl::mlua::Result<Value> {
        let found = guarded(lua, &self.staging, "require", |_| self.find(name))?;
        let (file, chunk, text) = match found {
            Found::Loaded(value) => return Ok(value),
            Found::Fresh { file, chunk, text } => (file, chunk, text),
        };

        self.loading
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(file.clone());
        let ran = lua
            .load(text)
            .set_name(format!("={chunk}"))
            .set_environment(self.environment.clone())
            .eval::<Value>();
        self.loading
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&file);

        // As in Lua's own `require`, a module that returns nothing is recorded as `true`.
        let value = match ran? {
            Value::Nil => Value::Boolean(true),
            value => value,
        };
        self.loaded
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(file, value.clone());
        Ok(value)
    }

    /// Resolves `name` to a module of this instance and reads it, or says why it cannot be.
    fn find(&self, name: &Value) -> Result<Found, Refusal> {
        let name = values::string(name, "module")?;
        let (file, parts) =
            module_file(&self.dir, &name).map_err(|refusal| refusal.about(&name))?;

        if let Some(value) = self
            .loaded
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&file)
        {
            return Ok(Found::Loaded(value.clone()));
        }
        if self
            .loading
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(&file)
        {
            return Err(Refusal::new(
                EvaluationFailure::Refused,
                "the module requires itself, directly or through the modules it requires",
            )
            .about(&name));
        }
        let text = fs::read_to_string(&file).map_err(|error| {
            Refusal::new(
                EvaluationFailure::Runtime,
                format!("{}: {error}", file.display()),
            )
            .about(&name)
        })?;
        Ok(Found::Fresh {
            file,
            chunk: format!("{}/{}.lua", self.label, parts.join("/")),
            text,
        })
    }
}

/// The dot-separated parts of the module name `name`.
///
/// Each part is non-empty and holds no `/` or `\`. A `..` is two empty parts, so it is refused as
/// one.
fn parts(name: &str) -> Result<Vec<&str>, Refusal> {
    let parts: Vec<&str> = name.split('.').collect();
    if parts
        .iter()
        .any(|part| part.is_empty() || part.contains(['/', '\\']))
    {
        return Err(Refusal::new(
            EvaluationFailure::Refused,
            "a module name is names joined by dots, none empty and none holding a slash",
        ));
    }
    Ok(parts)
}

/// The canonical file of the module `name` under the canonical plugin directory `dir`, and the
/// parts of its name.
///
/// A file that resolves outside `dir`, through a link or otherwise, is refused.
fn module_file<'a>(dir: &Path, name: &'a str) -> Result<(PathBuf, Vec<&'a str>), Refusal> {
    let parts = parts(name)?;
    let mut path = dir.to_path_buf();
    path.extend(&parts);
    path.as_mut_os_string().push(".lua");

    let file = fs::canonicalize(&path).map_err(|error| {
        let failure = if error.kind() == ErrorKind::NotFound {
            EvaluationFailure::Refused
        } else {
            EvaluationFailure::Runtime
        };
        Refusal::new(
            failure,
            format!("no module file {}: {error}", path.display()),
        )
    })?;
    if !file.starts_with(dir) {
        return Err(Refusal::new(
            EvaluationFailure::Refused,
            "the module resolves outside the plugin directory",
        ));
    }
    Ok((file, parts))
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::fs;
    use std::os::unix::fs::symlink;

    use buildl_core::EvaluationFailure;

    use super::{module_file, parts};

    #[test]
    fn a_dotted_name_splits_into_its_parts() {
        assert_eq!(parts("a.b.c").unwrap(), ["a", "b", "c"]);
        assert_eq!(parts("single").unwrap(), ["single"]);
    }

    #[test]
    fn an_empty_part_a_slash_or_a_backslash_is_refused() {
        for name in [
            "", ".", "a.", ".a", "a..b", "..", "../a", "a/b", "a\\b", "/a",
        ] {
            let refusal = parts(name).unwrap_err();
            assert_eq!(refusal.failure(), &EvaluationFailure::Refused, "{name:?}");
        }
    }

    #[test]
    fn a_module_resolves_to_its_canonical_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        fs::create_dir_all(root.join("lib")).unwrap();
        fs::write(root.join("lib/util.lua"), "").unwrap();
        let (file, parts) = module_file(&root, "lib.util").unwrap();
        assert_eq!(file, root.join("lib/util.lua"));
        assert_eq!(parts, ["lib", "util"]);
    }

    #[test]
    fn a_missing_module_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let refusal = module_file(&root, "nothing").unwrap_err();
        assert_eq!(refusal.failure(), &EvaluationFailure::Refused);
    }

    #[test]
    fn a_link_to_a_file_outside_the_directory_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let plugin = root.join("plugin");
        fs::create_dir_all(plugin.join("sub")).unwrap();
        fs::write(root.join("outside.lua"), "").unwrap();
        fs::create_dir_all(root.join("elsewhere")).unwrap();
        fs::write(root.join("elsewhere/x.lua"), "").unwrap();
        symlink(root.join("outside.lua"), plugin.join("file.lua")).unwrap();
        symlink(root.join("elsewhere"), plugin.join("dir")).unwrap();

        for name in ["file", "dir.x"] {
            let refusal = module_file(&plugin, name).unwrap_err();
            assert_eq!(refusal.failure(), &EvaluationFailure::Refused, "{name}");
            assert!(
                refusal
                    .diagnostic("require", None)
                    .as_str()
                    .contains("outside the plugin directory"),
                "{name}"
            );
        }
    }

    #[test]
    fn a_link_that_stays_inside_the_directory_is_allowed() {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        fs::write(root.join("real.lua"), "").unwrap();
        symlink(root.join("real.lua"), root.join("alias.lua")).unwrap();
        let (file, _) = module_file(&root, "alias").unwrap();
        assert_eq!(file, root.join("real.lua"));
    }
}
