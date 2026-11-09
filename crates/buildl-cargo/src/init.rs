//! `cargo buildl init`: set a Cargo project up for buildl in one command.
//!
//! [`init`] reads the Cargo project, refuses a folder that is set up already or has no
//! `Cargo.lock`, and writes `build.lua`, `buildl.toml` and `.gitignore` at the workspace root.
//! A write that fails takes back the files this run created. The plugin is then fetched by the
//! command the caller hands in, so that command's output and exit status are the run's own.

use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
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

/// Why [`write_files`] stopped: the file, whether it was being read or written, and the I/O error.
#[derive(Debug)]
struct WriteError {
    /// `"read"` or `"write"`.
    action: &'static str,
    /// The file the step was on, by name in the workspace root.
    file: &'static str,
    /// What the operating system said.
    source: io::Error,
}

impl std::fmt::Display for WriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cannot {} {}: {}", self.action, self.file, self.source)
    }
}

/// Writes `build.lua`, then `buildl.toml`, then `.gitignore` in `root`.
///
/// `build.lua` and `buildl.toml` are created new, so one that is already there fails the step and
/// is left alone. The `.gitignore` written is the one there with the missing lines added, or a new
/// one. When a step fails, the files this call created are removed and the error is returned; a
/// file that was there before is never removed.
fn write_files(root: &Path, files: &Files<'_>) -> Result<(), WriteError> {
    let mut created: Vec<PathBuf> = Vec::new();
    let result = write_in_order(root, files, &mut created);
    if result.is_err() {
        for path in &created {
            let _ = fs::remove_file(path);
        }
    }
    result
}

/// Creates `file` in `root` holding `text`; once it exists, it is added to `created`.
fn create_new(
    root: &Path,
    file: &'static str,
    text: &str,
    created: &mut Vec<PathBuf>,
) -> Result<(), WriteError> {
    let path = root.join(file);
    let write_error = |source| WriteError {
        action: "write",
        file,
        source,
    };
    let mut handle = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(write_error)?;
    created.push(path);
    handle.write_all(text.as_bytes()).map_err(write_error)
}

/// The steps of [`write_files`]; `created` receives each file this call has created.
fn write_in_order(
    root: &Path,
    files: &Files<'_>,
    created: &mut Vec<PathBuf>,
) -> Result<(), WriteError> {
    create_new(root, "build.lua", files.build_lua, created)?;
    create_new(root, "buildl.toml", files.buildl_toml, created)?;

    let gitignore = root.join(".gitignore");
    let existing = match fs::read_to_string(&gitignore) {
        Ok(text) => Some(text),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(source) => {
            return Err(WriteError {
                action: "read",
                file: ".gitignore",
                source,
            });
        }
    };
    if existing.is_none() {
        created.push(gitignore.clone());
    }
    fs::write(&gitignore, scaffold::gitignore(existing.as_deref())).map_err(|source| WriteError {
        action: "write",
        file: ".gitignore",
        source,
    })
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
/// | the current directory cannot be read; cargo cannot be run or read; a file cannot be written, or `.gitignore` cannot be read | [`Exit::Infrastructure`] |
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
        say(stderr, &format!("error: {error}"));
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
    fn a_failure_after_two_files_leaves_an_existing_gitignore_as_it_was() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".gitignore"), "target\n").unwrap();
        fs::create_dir(dir.path().join("buildl.toml")).unwrap();

        assert!(write_files(dir.path(), &FILES).is_err());

        assert_eq!(
            fs::read_to_string(dir.path().join(".gitignore")).unwrap(),
            "target\n"
        );
        assert!(!dir.path().join("build.lua").exists());
    }

    #[test]
    fn a_failure_names_the_file_and_whether_it_was_a_read_or_a_write() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join(".gitignore")).unwrap();
        let error = write_files(dir.path(), &FILES).unwrap_err();
        assert!(
            error.to_string().starts_with("cannot read .gitignore: "),
            "{error}"
        );

        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("buildl.toml")).unwrap();
        let error = write_files(dir.path(), &FILES).unwrap_err();
        assert!(
            error.to_string().starts_with("cannot write buildl.toml: "),
            "{error}"
        );
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
