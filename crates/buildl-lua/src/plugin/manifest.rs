//! A plugin's `extension.toml`, read with airsl and turned into a [`PluginRef`].
//!
//! Its own file because the manifest is the one place a plugin states what it asks for, and
//! buildl reads only part of airsl's grammar: `proc.run` and `env.read`. It adds one key of its
//! own, `env.unkeyed`, which airsl's parser refuses as unknown, so it is read from the text and
//! left out of what airsl is given. Everything else airsl accepts (`fs.read`, `fs.write`, module
//! flags, an optional block, `[limits]`) is a request buildl would otherwise ignore, and a
//! request silently ignored reads as one granted, so each is refused instead. airsl's own checks
//! (the shape, the `api`, the entry file) are passed on as refusals too.
//!
//! Responsibilities: [`read_manifest`].
//!
//! Non-responsibilities: running the entry file, and deciding which directory a plugin name maps
//! to. The caller does both and hands the directory in.

use std::collections::BTreeSet;
use std::fmt::Display;
use std::fs;
use std::path::{Path, PathBuf};

use airsl::extension::{MANIFEST_FILE, Manifest, RawManifest, Variables};
use buildl_core::{Diagnostic, EnvName, Error, PluginName, PluginRef, PluginVersion, Program};

/// Reads `<dir>/extension.toml` and checks it describes the plugin `expected`.
///
/// `label` is `dir` as the workspace names it; the diagnostics name the manifest as
/// `<label>/extension.toml`, so they read the same on every machine.
///
/// Returns what the manifest asks for, as a [`PluginRef`], and the entry file relative to `dir`.
/// `[capabilities] env.unkeyed` is the variables its targets may leave out of their keys.
///
/// # Errors
///
/// Returns a diagnostic naming `<label>/extension.toml` and the reason when airsl refuses the
/// manifest (unreadable, malformed, an unknown key, an unsupported `api`, an entry that is not a
/// file inside `dir`), when its `name` is not `expected`, when it holds a name or version buildl
/// cannot use, when `env.unkeyed` is not a list of variable names or lists one `env.read` does
/// not, or when it requests anything but `proc.run`, `env.read` and `env.unkeyed`: `fs.read`,
/// `fs.write`, a module flag, a `[capabilities.optional]` entry, or a `[limits]` value.
pub(crate) fn read_manifest(
    dir: &Path,
    label: &str,
    expected: &PluginName,
) -> Result<(PluginRef, PathBuf), Diagnostic> {
    let named = if label.is_empty() {
        MANIFEST_FILE.to_owned()
    } else {
        format!("{label}/{MANIFEST_FILE}")
    };

    let (manifest, unkeyed) = load(dir, &named)?;

    let name =
        PluginName::parse(manifest.name().as_str()).map_err(|error| refuse(&named, error))?;
    if &name != expected {
        return Err(refuse(
            &named,
            format_args!("names the plugin `{name}`, but the workspace uses it as `{expected}`"),
        ));
    }
    let version =
        PluginVersion::parse(manifest.version()).map_err(|error| refuse(&named, error))?;

    let required = manifest.required();
    if !required.fs_read().is_empty() {
        return Err(refuse(&named, "requests fs.read, which a plugin may not"));
    }
    if !required.fs_write().is_empty() {
        return Err(refuse(&named, "requests fs.write, which a plugin may not"));
    }
    if let Some(module) = required.modules().next() {
        return Err(refuse(
            &named,
            format_args!("requests the module `{module}`, which a plugin may not"),
        ));
    }
    if !manifest.optional().is_empty() {
        return Err(refuse(
            &named,
            "holds a [capabilities.optional] block, which a plugin may not",
        ));
    }
    if manifest.limits().memory().is_some() {
        return Err(refuse(&named, "sets limits.memory, which a plugin may not"));
    }
    if manifest.limits().instructions().is_some() {
        return Err(refuse(
            &named,
            "sets limits.instructions, which a plugin may not",
        ));
    }

    let run = required
        .proc_run()
        .map(|program| Program::parse(program).map_err(|error| refuse(&named, error)))
        .collect::<Result<_, _>>()?;
    let env = required
        .env_read()
        .map(|variable| EnvName::parse(variable).map_err(|error| refuse(&named, error)))
        .collect::<Result<_, _>>()?;

    let unkeyed = unkeyed
        .iter()
        .map(|variable| EnvName::parse(variable).map_err(|error| refuse(&named, error)))
        .collect::<Result<BTreeSet<_>, _>>()?;
    if let Some(name) = unkeyed.difference(&env).next() {
        return Err(Diagnostic::new(
            Error::UnkeyedNotRead {
                file: named,
                table: "[capabilities]".to_owned(),
                name: name.clone(),
            }
            .to_string(),
        ));
    }

    let plugin = PluginRef {
        name,
        version,
        run,
        env,
        unkeyed,
    };
    Ok((plugin, manifest.entry().to_path_buf()))
}

/// The manifest in `dir`, validated by airsl, and the names its `[capabilities] env.unkeyed`
/// lists, as written.
///
/// airsl refuses `env.unkeyed` as an unknown key, so the key is taken out of the parsed text
/// before airsl reads what is left. A file that does not parse as TOML, or has no such key, goes
/// to airsl as it is and is refused, or read, in airsl's words.
fn load(dir: &Path, named: &str) -> Result<(Manifest, Vec<String>), Diagnostic> {
    let variables = Variables::default();
    let from_dir =
        || Manifest::from_dir(dir, &variables).map_err(|error| airsl_refusal(named, error));

    let Ok(text) = fs::read_to_string(dir.join(MANIFEST_FILE)) else {
        return Ok((from_dir()?, Vec::new()));
    };
    let Ok(mut table) = text.parse::<toml::Table>() else {
        return Ok((from_dir()?, Vec::new()));
    };
    let Some(listed) = take_unkeyed(&mut table) else {
        return Ok((from_dir()?, Vec::new()));
    };
    let unkeyed = listed.map_err(|reason| refuse(named, reason))?;
    let raw = toml::Value::Table(table)
        .try_into::<RawManifest>()
        .map_err(|error| refuse(named, error))?;
    let manifest =
        Manifest::validate(raw, dir, &variables).map_err(|error| airsl_refusal(named, error))?;
    Ok((manifest, unkeyed))
}

/// Takes `capabilities.env.unkeyed` out of `table`: `None` when it is not there, the names it
/// lists, or why it is not a list of names.
fn take_unkeyed(table: &mut toml::Table) -> Option<Result<Vec<String>, &'static str>> {
    let listed = table
        .get_mut("capabilities")?
        .as_table_mut()?
        .get_mut("env")?
        .as_table_mut()?
        .remove("unkeyed")?;
    let names = listed.as_array().and_then(|entries| {
        entries
            .iter()
            .map(|entry| entry.as_str().map(str::to_owned))
            .collect::<Option<Vec<_>>>()
    });
    Some(names.ok_or("env.unkeyed must be a list of variable names"))
}

/// A diagnostic for airsl's refusal `error` of the manifest `named`.
fn airsl_refusal(named: &str, error: airsl::Error) -> Diagnostic {
    match error {
        // These two already name the manifest's path; the diagnostic leads with it, so only the
        // reason is kept.
        airsl::Error::ManifestParse { reason, .. } => refuse(named, reason),
        airsl::Error::ManifestRead { source, .. } => refuse(named, source),
        other => refuse(named, other),
    }
}

/// A diagnostic leading with the manifest `named` and then the `reason`.
fn refuse(named: &str, reason: impl Display) -> Diagnostic {
    Diagnostic::new(format!("{named}: {reason}"))
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        clippy::panic,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::fs;
    use std::path::Path;

    use buildl_core::{EnvName, PluginName, Program};

    use super::read_manifest;

    /// The plugin directory as a workspace names it.
    const LABEL: &str = "plugins/go";

    /// A plugin directory holding `extension.toml` with `text` and an entry file `main.lua`.
    fn plugin_dir(text: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("extension.toml"), text).unwrap();
        fs::write(dir.path().join("main.lua"), "return {}").unwrap();
        dir
    }

    /// A manifest for `go` at api 1, followed by `rest`.
    fn manifest(rest: &str) -> String {
        format!(
            "[extension]\nname = \"go\"\nversion = \"1.2.0\"\nentry = \"main.lua\"\napi = 1\n{rest}"
        )
    }

    fn go() -> PluginName {
        PluginName::parse("go").unwrap()
    }

    /// The diagnostic `read_manifest` refuses the manifest `text` with.
    fn refusal(text: &str) -> String {
        let dir = plugin_dir(text);
        let diagnostic = match read_manifest(dir.path(), LABEL, &go()) {
            Err(diagnostic) => diagnostic.to_string(),
            Ok(read) => panic!("expected a refusal, got {read:?}"),
        };
        assert!(
            diagnostic.starts_with("plugins/go/extension.toml: "),
            "{diagnostic}"
        );
        diagnostic
    }

    #[test]
    fn a_plugin_at_the_workspace_root_names_the_bare_file() {
        let dir = plugin_dir(&manifest("").replace("api = 1", "api = 2"));
        let said = read_manifest(dir.path(), "", &go())
            .unwrap_err()
            .to_string();
        assert!(said.starts_with("extension.toml: "), "{said}");
    }

    #[test]
    fn a_valid_manifest_becomes_a_plugin_ref() {
        let dir = plugin_dir(&manifest(
            "[capabilities]\nproc.run = [\"go\", \"gofmt\"]\nenv.read = [\"GOCACHE\", \"HOME\"]\n",
        ));
        let (plugin, entry) = read_manifest(dir.path(), LABEL, &go()).unwrap();
        assert_eq!(plugin.to_string(), "go@1.2.0");
        assert_eq!(
            plugin.run,
            [
                Program::parse("go").unwrap(),
                Program::parse("gofmt").unwrap()
            ]
            .into()
        );
        assert_eq!(
            plugin.env,
            [
                EnvName::parse("GOCACHE").unwrap(),
                EnvName::parse("HOME").unwrap()
            ]
            .into()
        );
        assert_eq!(entry, Path::new("main.lua"));
    }

    #[test]
    fn extension_reads_env_unkeyed() {
        let dir = plugin_dir(&manifest(
            "[capabilities]\nenv.read = [\"HOME\", \"PATH\"]\nenv.unkeyed = [\"PATH\"]\n",
        ));
        let (plugin, _) = read_manifest(dir.path(), LABEL, &go()).unwrap();
        assert_eq!(
            plugin.env,
            [
                EnvName::parse("HOME").unwrap(),
                EnvName::parse("PATH").unwrap()
            ]
            .into()
        );
        assert_eq!(plugin.unkeyed, [EnvName::parse("PATH").unwrap()].into());
    }

    #[test]
    fn extension_env_unkeyed_left_out_is_empty() {
        let dir = plugin_dir(&manifest("[capabilities]\nenv.read = [\"HOME\"]\n"));
        let (plugin, _) = read_manifest(dir.path(), LABEL, &go()).unwrap();
        assert!(plugin.unkeyed.is_empty());
    }

    #[test]
    fn extension_unkeyed_outside_read_is_refused() {
        let said = refusal(&manifest(
            "[capabilities]\nenv.read = [\"PATH\"]\nenv.unkeyed = [\"HOME\"]\n",
        ));
        assert_eq!(
            said,
            "plugins/go/extension.toml: [capabilities] env.unkeyed lists HOME, which env.read does not"
        );
    }

    #[test]
    fn extension_unkeyed_that_is_not_a_list_of_names_is_refused() {
        let said = refusal(&manifest("[capabilities]\nenv.unkeyed = \"PATH\"\n"));
        assert_eq!(
            said,
            "plugins/go/extension.toml: env.unkeyed must be a list of variable names"
        );
    }

    /// A manifest with `env.unkeyed` in it, followed by `rest`.
    fn with_unkeyed(rest: &str) -> String {
        manifest(&format!(
            "[capabilities]\nenv.read = [\"PATH\"]\nenv.unkeyed = [\"PATH\"]\n{rest}"
        ))
    }

    #[test]
    fn with_env_unkeyed_an_unknown_key_is_still_refused() {
        let said = refusal(&with_unkeyed("").replace("api = 1", "api = 1\nbogus = true"));
        assert!(said.contains("unknown field `bogus`"), "{said}");
    }

    #[test]
    fn with_env_unkeyed_an_unknown_table_is_still_refused() {
        let said = refusal(&format!("{}[bogus]\nx = 1\n", with_unkeyed("")));
        assert!(said.contains("unknown field `bogus`"), "{said}");
    }

    #[test]
    fn with_env_unkeyed_fs_read_is_still_refused() {
        let said = refusal(&with_unkeyed("fs.read = [\"/tmp\"]\n"));
        assert!(said.contains("fs.read"), "{said}");
    }

    #[test]
    fn with_env_unkeyed_a_module_flag_is_still_refused() {
        let said = refusal(&with_unkeyed("regex = true\n"));
        assert!(said.contains("regex"), "{said}");
    }

    #[test]
    fn with_env_unkeyed_a_bad_limit_is_still_refused() {
        let said = refusal(&format!(
            "{}[limits]\nmemory = \"64MB\"\n",
            with_unkeyed("")
        ));
        assert!(said.contains("limits.memory"), "{said}");
        let said = refusal(&format!(
            "{}[limits]\nmemory = \"lots\"\n",
            with_unkeyed("")
        ));
        assert!(said.contains("memory"), "{said}");
    }

    #[test]
    fn with_env_unkeyed_the_api_and_the_name_are_still_checked() {
        let said = refusal(&with_unkeyed("").replace("api = 1", "api = 2"));
        assert!(said.contains("api 2"), "{said}");
        let said = refusal(&with_unkeyed("").replace("\"go\"", "\"rust\""));
        assert!(said.contains("rust"), "{said}");
    }

    #[test]
    fn a_manifest_without_capabilities_asks_for_nothing() {
        let dir = plugin_dir(&manifest(""));
        let (plugin, _) = read_manifest(dir.path(), LABEL, &go()).unwrap();
        assert!(plugin.run.is_empty() && plugin.env.is_empty());
    }

    #[test]
    fn a_plugin_table_is_refused() {
        let text = "[plugin]\nname = \"go\"\nversion = \"1.2.0\"\napi = 1\n";
        assert!(refusal(text).contains("unknown field `plugin`"));
    }

    #[test]
    fn api_2_is_refused() {
        let text = manifest("").replace("api = 1", "api = 2");
        assert!(refusal(&text).contains("api 2"));
    }

    #[test]
    fn a_mismatched_name_is_refused() {
        let text = manifest("").replace("\"go\"", "\"rust\"");
        let said = refusal(&text);
        assert!(said.contains("rust") && said.contains("go"), "{said}");
    }

    #[test]
    fn fs_read_is_refused() {
        let said = refusal(&manifest("[capabilities]\nfs.read = [\"/tmp\"]\n"));
        assert!(said.contains("fs.read"), "{said}");
    }

    #[test]
    fn fs_write_is_refused() {
        let said = refusal(&manifest("[capabilities]\nfs.write = [\"/tmp\"]\n"));
        assert!(said.contains("fs.write"), "{said}");
    }

    #[test]
    fn a_module_flag_is_refused() {
        let said = refusal(&manifest("[capabilities]\nregex = true\n"));
        assert!(said.contains("regex"), "{said}");
    }

    #[test]
    fn a_non_empty_optional_block_is_refused() {
        let said = refusal(&manifest("[capabilities.optional]\nproc.run = [\"tar\"]\n"));
        assert!(said.contains("capabilities.optional"), "{said}");
    }

    #[test]
    fn a_memory_limit_is_refused() {
        let said = refusal(&manifest("[limits]\nmemory = \"64MB\"\n"));
        assert!(said.contains("limits.memory"), "{said}");
    }

    #[test]
    fn an_instructions_limit_is_refused() {
        let said = refusal(&manifest("[limits]\ninstructions = 1000\n"));
        assert!(said.contains("limits.instructions"), "{said}");
    }

    #[test]
    fn a_missing_entry_is_refused() {
        let dir = plugin_dir(&manifest(""));
        fs::remove_file(dir.path().join("main.lua")).unwrap();
        let said = read_manifest(dir.path(), LABEL, &go())
            .unwrap_err()
            .to_string();
        assert!(said.contains("extension.entry"), "{said}");
    }

    #[test]
    fn a_missing_manifest_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let said = read_manifest(dir.path(), LABEL, &go())
            .unwrap_err()
            .to_string();
        assert!(said.contains("extension.toml"), "{said}");
    }
}
