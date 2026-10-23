//! A plugin's `extension.toml`, read with airsl and turned into a [`PluginRef`].
//!
//! Its own file because the manifest is the one place a plugin states what it asks for, and
//! buildl reads only part of airsl's grammar: `proc.run` and `env.read`. Everything else airsl
//! accepts (`fs.read`, `fs.write`, module flags, an optional block, `[limits]`) is a request
//! buildl would otherwise ignore, and a request silently ignored reads as one granted, so each
//! is refused instead. airsl's own checks (the shape, the `api`, the entry file) are passed on as
//! refusals too.
//!
//! Responsibilities: [`read_manifest`].
//!
//! Non-responsibilities: running the entry file, and deciding which directory a plugin name maps
//! to. The caller does both and hands the directory in.

use std::fmt::Display;
use std::path::{Path, PathBuf};

use airsl::extension::{MANIFEST_FILE, Manifest, Variables};
use buildl_core::{Diagnostic, EnvName, PluginName, PluginRef, PluginVersion, Program};

/// Reads `<dir>/extension.toml` and checks it describes the plugin `expected`.
///
/// Returns what the manifest asks for, as a [`PluginRef`], and the entry file relative to `dir`.
///
/// # Errors
///
/// Returns a diagnostic naming `<dir>/extension.toml` and the reason when airsl refuses the
/// manifest (unreadable, malformed, an unknown key, an unsupported `api`, an entry that is not a
/// file inside `dir`), when its `name` is not `expected`, when it holds a name or version buildl
/// cannot use, or when it requests anything but `proc.run` and `env.read`: `fs.read`, `fs.write`,
/// a module flag, a `[capabilities.optional]` entry, or a `[limits]` value.
pub(crate) fn read_manifest(
    dir: &Path,
    expected: &PluginName,
) -> Result<(PluginRef, PathBuf), Diagnostic> {
    let named = dir.join(MANIFEST_FILE);
    let refuse = |reason: &dyn Display| Diagnostic::new(format!("{}: {reason}", named.display()));

    let manifest = Manifest::from_dir(dir, &Variables::default()).map_err(|error| match error {
        // These two already name the manifest's path; the diagnostic leads with it, so only the
        // reason is kept.
        airsl::Error::ManifestParse { reason, .. } => refuse(&reason),
        airsl::Error::ManifestRead { source, .. } => refuse(&source),
        other => refuse(&other),
    })?;

    let name = PluginName::parse(manifest.name().as_str()).map_err(|error| refuse(&error))?;
    if &name != expected {
        return Err(refuse(&format_args!(
            "names the plugin `{name}`, but the workspace uses it as `{expected}`"
        )));
    }
    let version = PluginVersion::parse(manifest.version()).map_err(|error| refuse(&error))?;

    let required = manifest.required();
    if !required.fs_read().is_empty() {
        return Err(refuse(&"requests fs.read, which a plugin may not"));
    }
    if !required.fs_write().is_empty() {
        return Err(refuse(&"requests fs.write, which a plugin may not"));
    }
    if let Some(module) = required.modules().next() {
        return Err(refuse(&format_args!(
            "requests the module `{module}`, which a plugin may not"
        )));
    }
    if !manifest.optional().is_empty() {
        return Err(refuse(
            &"holds a [capabilities.optional] block, which a plugin may not",
        ));
    }
    if manifest.limits().memory().is_some() {
        return Err(refuse(&"sets limits.memory, which a plugin may not"));
    }
    if manifest.limits().instructions().is_some() {
        return Err(refuse(&"sets limits.instructions, which a plugin may not"));
    }

    let run = required
        .proc_run()
        .map(|program| Program::parse(program).map_err(|error| refuse(&error)))
        .collect::<Result<_, _>>()?;
    let env = required
        .env_read()
        .map(|variable| EnvName::parse(variable).map_err(|error| refuse(&error)))
        .collect::<Result<_, _>>()?;

    let plugin = PluginRef {
        name,
        version,
        run,
        env,
    };
    Ok((plugin, manifest.entry().to_path_buf()))
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
        let diagnostic = match read_manifest(dir.path(), &go()) {
            Err(diagnostic) => diagnostic.to_string(),
            Ok(read) => panic!("expected a refusal, got {read:?}"),
        };
        let named = dir.path().join("extension.toml");
        assert!(
            diagnostic.starts_with(&format!("{}: ", named.display())),
            "{diagnostic}"
        );
        diagnostic
    }

    #[test]
    fn a_valid_manifest_becomes_a_plugin_ref() {
        let dir = plugin_dir(&manifest(
            "[capabilities]\nproc.run = [\"go\", \"gofmt\"]\nenv.read = [\"GOCACHE\", \"HOME\"]\n",
        ));
        let (plugin, entry) = read_manifest(dir.path(), &go()).unwrap();
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
    fn a_manifest_without_capabilities_asks_for_nothing() {
        let dir = plugin_dir(&manifest(""));
        let (plugin, _) = read_manifest(dir.path(), &go()).unwrap();
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
        let said = read_manifest(dir.path(), &go()).unwrap_err().to_string();
        assert!(said.contains("extension.entry"), "{said}");
    }

    #[test]
    fn a_missing_manifest_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let said = read_manifest(dir.path(), &go()).unwrap_err().to_string();
        assert!(said.contains("extension.toml"), "{said}");
    }
}
