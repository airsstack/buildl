//! `cargo buildl init`: set a Cargo project up for buildl in one command.
//!
//! [`init`] reads the Cargo project, refuses a folder that is set up already or has no
//! `Cargo.lock`, and writes `build.lua`, `buildl.toml` and `.gitignore` at the workspace root.
//! A write that fails takes back the files this run created. The plugin is then fetched by the
//! command the caller hands in, so that command's output and exit status are the run's own.

use std::ffi::OsStr;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use buildl::cli::Exit;

use crate::{generate, metadata, scaffold};

/// What `init` shows when the files are written and the fetch failed.
const FETCH_FAILED: &str = "init: the files are written but the plugin is not fetched; \
                            run cargo buildl plugins fetch";

/// The text of the three files `init` writes.
struct Files<'a> {
    /// The new `build.lua`.
    build_lua: &'a str,
    /// The new `buildl.toml`.
    buildl_toml: &'a str,
}

/// Writes `build.lua`, then `buildl.toml`, then `.gitignore` in `root`.
///
/// The `.gitignore` written is the one there with the missing lines added, or a new one. When a
/// step fails, the files this call created are removed and the error is returned; a `.gitignore`
/// that was there before is never removed.
fn write_files(root: &Path, files: &Files<'_>) -> io::Result<()> {
    let mut created: Vec<PathBuf> = Vec::new();
    let result = write_in_order(root, files, &mut created);
    if result.is_err() {
        for path in &created {
            let _ = fs::remove_file(path);
        }
    }
    result
}

/// The steps of [`write_files`]; `created` receives each file as soon as it may exist.
fn write_in_order(root: &Path, files: &Files<'_>, created: &mut Vec<PathBuf>) -> io::Result<()> {
    let build_lua = root.join("build.lua");
    created.push(build_lua.clone());
    fs::write(&build_lua, files.build_lua)?;

    let buildl_toml = root.join("buildl.toml");
    created.push(buildl_toml.clone());
    fs::write(&buildl_toml, files.buildl_toml)?;

    let gitignore = root.join(".gitignore");
    let existing = match fs::read_to_string(&gitignore) {
        Ok(text) => Some(text),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    if existing.is_none() {
        created.push(gitignore.clone());
    }
    fs::write(&gitignore, scaffold::gitignore(existing.as_deref()))
}

/// Writes `message` as a line of `stderr`. A failed write cannot be reported anywhere else.
fn say<E: Write>(stderr: &mut E, message: &str) {
    let _ = writeln!(stderr, "{message}");
}

/// The file in `root` that makes `init` refuse, if any: `buildl.toml` first, then `build.lua`.
fn existing_file(root: &Path) -> Option<&'static str> {
    ["buildl.toml", "build.lua"]
        .into_iter()
        .find(|name| root.join(name).exists())
}

/// Runs `cargo buildl init`, with `cargo` as the program that describes the project and `start`
/// as the folder it describes it from; a `start` that could not be read is
/// [`Exit::Infrastructure`].
///
/// The files are written in the workspace root, wherever `start` lies in the workspace. Once
/// they are written, `fetch` is called with the workspace root and `stderr` and its status
/// ends the run. Every message goes to `stderr`.
///
/// | Outcome | Exit |
/// |---|---|
/// | files written and `fetch` succeeded | [`Exit::Success`] |
/// | `buildl.toml` or `build.lua` there already; no `Cargo.lock`; cargo refused the project; a name refused or claimed twice | [`Exit::Definition`] |
/// | the current directory cannot be read; cargo cannot be run or read; a file cannot be written | [`Exit::Infrastructure`] |
/// | `fetch` failed | the status of `fetch`; the files stay |
pub(crate) fn init<E, F>(
    cargo: &OsStr,
    start: io::Result<PathBuf>,
    stderr: &mut E,
    fetch: F,
) -> Exit
where
    E: Write,
    F: FnOnce(&Path, &mut E) -> Exit,
{
    let start = match start {
        Ok(start) => start,
        Err(error) => {
            say(
                stderr,
                &format!("error: the current directory cannot be read: {error}"),
            );
            return Exit::Infrastructure;
        }
    };
    let project = match metadata::read(cargo, &start) {
        Ok(project) => project,
        Err(error) => {
            say(stderr, &error.to_string());
            return error.exit();
        }
    };
    let root = &project.workspace_root;
    if let Some(name) = existing_file(root) {
        say(
            stderr,
            &format!(
                "init: {name} already exists; this folder is set up already, \
                 or remove {name} to start over"
            ),
        );
        return Exit::Definition;
    }
    if !root.join("Cargo.lock").exists() {
        say(
            stderr,
            "init: no Cargo.lock beside Cargo.toml; run cargo generate-lockfile",
        );
        return Exit::Definition;
    }
    let generated = match generate::section(&project) {
        Ok(generated) => generated,
        Err(refusal) => {
            say(stderr, &refusal.to_string());
            return generate::Refusal::EXIT;
        }
    };
    let name = root
        .file_name()
        .map_or_else(|| "workspace".into(), |name| name.to_string_lossy());
    let build_lua = scaffold::build_lua(&generated);
    let buildl_toml = scaffold::buildl_toml(&name);
    let files = Files {
        build_lua: &build_lua,
        buildl_toml: &buildl_toml,
    };
    if let Err(error) = write_files(root, &files) {
        say(stderr, &format!("error: cannot write the files: {error}"));
        return Exit::Infrastructure;
    }
    say(stderr, "init: wrote build.lua, buildl.toml, .gitignore");
    let exit = fetch(root, stderr);
    if exit != Exit::Success {
        say(stderr, FETCH_FAILED);
    }
    exit
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use super::*;

    const FILES: Files<'static> = Files {
        build_lua: "lua\n",
        buildl_toml: "toml\n",
    };

    #[test]
    fn the_three_files_are_written_and_an_existing_gitignore_is_extended() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".gitignore"), "target\n").unwrap();

        write_files(dir.path(), &FILES).unwrap();

        let read = |name: &str| fs::read_to_string(dir.path().join(name)).unwrap();
        assert_eq!(read("build.lua"), "lua\n");
        assert_eq!(read("buildl.toml"), "toml\n");
        assert_eq!(read(".gitignore"), "target\n/out/\n/.buildl/\n");
    }

    #[test]
    fn a_failure_takes_back_the_files_this_call_created() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join(".gitignore")).unwrap();

        assert!(write_files(dir.path(), &FILES).is_err());

        assert!(!dir.path().join("build.lua").exists());
        assert!(!dir.path().join("buildl.toml").exists());
        assert!(dir.path().join(".gitignore").is_dir());
    }

    #[test]
    fn a_failure_on_the_first_file_leaves_the_later_ones_alone() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("build.lua")).unwrap();
        fs::write(dir.path().join("buildl.toml"), "mine\n").unwrap();

        assert!(write_files(dir.path(), &FILES).is_err());

        assert!(dir.path().join("build.lua").is_dir());
        assert_eq!(
            fs::read_to_string(dir.path().join("buildl.toml")).unwrap(),
            "mine\n"
        );
    }

    #[test]
    fn the_first_file_that_exists_is_named_with_buildl_toml_first() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(existing_file(dir.path()), None);
        fs::write(dir.path().join("build.lua"), "").unwrap();
        assert_eq!(existing_file(dir.path()), Some("build.lua"));
        fs::write(dir.path().join("buildl.toml"), "").unwrap();
        assert_eq!(existing_file(dir.path()), Some("buildl.toml"));
    }

    #[test]
    fn an_unreadable_current_directory_is_3() {
        let mut stderr = Vec::new();
        let exit = init(
            OsStr::new("cargo"),
            Err(io::Error::from(io::ErrorKind::NotFound)),
            &mut stderr,
            |_, _| Exit::Success,
        );
        assert_eq!(exit, Exit::Infrastructure);
        let stderr = String::from_utf8_lossy(&stderr);
        assert!(
            stderr.starts_with("error: the current directory cannot be read: "),
            "{stderr}"
        );
    }
}
