//! [`LocalHost`], the [`Host`] that is the machine buildl itself runs on.
//!
//! Its own file because the platform and the environment are the two ambient facts an action
//! key folds, and this is the one place either is read. The same goes for the machine's own
//! folders, which a key must not hold. A variable is read when it is asked for and never
//! before: a snapshot of the environment would read names the workspace's ceiling does not
//! allow.
//!
//! Responsibilities: [`LocalHost`] and its [`Host`] implementation, including the written and
//! canonical forms of the home, workspace and sandbox folders, and [`written_root`], which
//! finds the form of the workspace the shell names from the `PWD` it was handed.
//!
//! Non-responsibilities: deciding which variables may be read, which is the ceiling's — a name
//! reaches this adapter only after it allowed it — and what a folder's presence in a text
//! means.

use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use buildl_core::{
    Diagnostic, EnvName, EnvValue, Error, Host, MachineFolders, Platform, PortName, Result,
};

/// The machine buildl runs on: the platform it was compiled for, its own environment, and the
/// folders of the workspace, its sandbox and the user's home.
///
/// # Examples
///
/// ```
/// use std::path::Path;
///
/// use buildl::host::LocalHost;
/// use buildl_core::{EnvName, Host};
///
/// let host = LocalHost::new(Path::new("/work/shop"), None, Path::new("/tmp/buildl-1"))?;
/// assert_eq!(host.platform().as_str().split('/').count(), 2);
///
/// let unset = EnvName::parse("BUILDL_DOC_A_NAME_NOTHING_SETS")?;
/// assert_eq!(host.env(&unset)?, None);
/// # Ok::<(), buildl_core::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalHost {
    platform: Platform,
    folders: MachineFolders,
}

impl LocalHost {
    /// The host this binary was compiled for, for the workspace at `root` whose actions run in
    /// the `sandbox` folder. `written` is the root as the user's shell names it, when that
    /// differs from `root` by a symbolic link; see [`written_root`]. The home folder is the
    /// `HOME` this process holds now, read once here.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidName`] if the compiler's text for the operating system or the
    /// architecture is not a platform part.
    pub fn new(root: &Path, written: Option<&Path>, sandbox: &Path) -> Result<Self> {
        Self::with_home(root, written, sandbox, env::var_os("HOME"))
    }

    /// As [`LocalHost::new`], with the user's home given as `home` instead of read from the
    /// process; `None` leaves the home folder out, as does a value that is empty, `/` or not
    /// Unicode.
    ///
    /// # Errors
    ///
    /// Returns what [`LocalHost::new`] returns.
    pub fn with_home(
        root: &Path,
        written: Option<&Path>,
        sandbox: &Path,
        home: Option<OsString>,
    ) -> Result<Self> {
        let home = home.map(PathBuf::from).map(|home| forms(&home));
        let home: Vec<&str> = home.iter().flatten().map(String::as_str).collect();
        let workspace: Vec<String> = [Some(root), written]
            .into_iter()
            .flatten()
            .flat_map(forms)
            .collect();
        let workspace: Vec<&str> = workspace.iter().map(String::as_str).collect();
        let sandbox = forms(sandbox);
        let sandbox: Vec<&str> = sandbox.iter().map(String::as_str).collect();
        Ok(Self {
            platform: Platform::parse(format!("{}/{}", env::consts::OS, env::consts::ARCH))?,
            folders: MachineFolders::new(&home, &workspace, &sandbox),
        })
    }
}

/// The workspace root as the shell names it, derived from `pwd`, the `PWD` the process holds.
///
/// `current` is the directory the process runs in and `root` the canonical workspace root at or
/// above it. `pwd` counts only when it names `current` itself; the root is then `pwd` less the
/// trailing names `current` has below `root`, provided that names `root`. Returns `None` for an
/// unset, relative or mismatched `PWD`, and when `current` is not below `root`.
#[must_use]
pub fn written_root(pwd: Option<OsString>, current: &Path, root: &Path) -> Option<PathBuf> {
    let pwd = PathBuf::from(pwd?);
    let current = current.canonicalize().ok()?;
    if !pwd.is_absolute() || pwd.canonicalize().ok()? != current {
        return None;
    }
    let depth = current.strip_prefix(root).ok()?.components().count();
    let names: Vec<_> = pwd.components().collect();
    let kept = names.len().checked_sub(depth)?;
    let written: PathBuf = names.into_iter().take(kept).collect();
    (written.canonicalize().ok()? == root).then_some(written)
}

/// `folder` as written and as the filesystem names it, each as text; a form that is not Unicode
/// is left out.
fn forms(folder: &Path) -> Vec<String> {
    [folder.to_path_buf(), canonical(folder)]
        .into_iter()
        .filter_map(|form| form.into_os_string().into_string().ok())
        .collect()
}

/// The name the filesystem gives `folder`. A folder not made yet takes the canonical name of
/// its nearest existing parent, with the names below it as written.
fn canonical(folder: &Path) -> PathBuf {
    let mut missing = Vec::new();
    let mut parent = folder;
    loop {
        if let Ok(found) = parent.canonicalize() {
            return missing
                .iter()
                .rev()
                .fold(found, |path, name| path.join(name));
        }
        match (parent.parent(), parent.file_name()) {
            (Some(above), Some(name)) => {
                missing.push(name);
                parent = above;
            }
            _ => return folder.to_path_buf(),
        }
    }
}

impl Host for LocalHost {
    /// The operating system and the architecture, such as `linux/x86_64` or `macos/aarch64`.
    fn platform(&self) -> Platform {
        self.platform.clone()
    }

    /// The value the variable `name` has in this process now. A variable set to nothing is
    /// `Some("")`, not `None`.
    fn env(&self, name: &EnvName) -> Result<Option<EnvValue>> {
        value(name, env::var_os(name.as_str()))
    }

    /// The user's home, the workspace and the sandbox folder, each as written and as the
    /// filesystem names it.
    fn machine_folders(&self) -> MachineFolders {
        self.folders.clone()
    }
}

/// What a variable `held` in the environment is as a value: `None` when it is unset.
fn value(name: &EnvName, held: Option<OsString>) -> Result<Option<EnvValue>> {
    held.map(|held| {
        let text = held
            .into_string()
            .map_err(|_| failed(name, "does not hold valid Unicode"))?;
        EnvValue::parse(text).map_err(|_| failed(name, "holds a NUL byte"))
    })
    .transpose()
}

/// A variable whose value cannot be handed on, as the host adapter's error.
fn failed(name: &EnvName, reason: &str) -> Error {
    Error::Adapter {
        port: PortName::Host,
        diagnostic: Diagnostic::new(format!("environment variable {name} {reason}")),
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::env;
    use std::ffi::OsString;
    use std::fs;
    use std::os::unix::ffi::OsStringExt;
    use std::os::unix::fs::symlink;
    use std::path::{Path, PathBuf};

    use buildl_core::{EnvName, Error, Host, PortName};

    use super::{LocalHost, value, written_root};

    #[cfg(target_os = "linux")]
    const OS: &str = "linux";
    #[cfg(target_os = "macos")]
    const OS: &str = "macos";

    #[cfg(target_arch = "x86_64")]
    const ARCH: &str = "x86_64";
    #[cfg(target_arch = "aarch64")]
    const ARCH: &str = "aarch64";

    fn name(text: &str) -> EnvName {
        EnvName::parse(text).unwrap()
    }

    fn host(root: &Path, sandbox: &Path) -> LocalHost {
        LocalHost::new(root, None, sandbox).unwrap()
    }

    /// A workspace `shop` with a folder `lib` below it, reached through the link `shop-link`,
    /// all inside a temporary folder whose canonical form is returned first.
    fn linked_workspace() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().canonicalize().unwrap();
        fs::create_dir_all(real.join("shop").join("lib")).unwrap();
        symlink(real.join("shop"), real.join("shop-link")).unwrap();
        (dir, real)
    }

    #[test]
    fn the_written_root_is_pwd_less_the_names_below_the_root() {
        let (_dir, real) = linked_workspace();
        let root = real.join("shop");
        let link = real.join("shop-link");

        assert_eq!(
            written_root(Some(link.clone().into_os_string()), &root, &root),
            Some(link.clone())
        );
        assert_eq!(
            written_root(
                Some(link.join("lib").into_os_string()),
                &root.join("lib"),
                &root
            ),
            Some(link)
        );
    }

    #[test]
    fn there_is_no_written_root_without_a_matching_pwd() {
        let (_dir, real) = linked_workspace();
        let root = real.join("shop");
        let link = real.join("shop-link");

        assert_eq!(written_root(None, &root, &root), None);
        assert_eq!(
            written_root(Some(OsString::from("shop-link")), &root, &root),
            None,
            "relative"
        );
        assert_eq!(
            written_root(Some(real.join("elsewhere").into_os_string()), &root, &root),
            None,
            "not there"
        );
        assert_eq!(
            written_root(Some(link.join("lib").into_os_string()), &root, &root),
            None,
            "names another folder than the current one"
        );
    }

    #[test]
    fn local_host_reports_the_workspace_as_written() {
        let (_dir, real) = linked_workspace();
        let link = real.join("shop-link");

        let folders =
            LocalHost::with_home(&real.join("shop"), Some(&link), Path::new("/s/box"), None)
                .unwrap()
                .machine_folders();

        let link = link.to_str().unwrap();
        assert_eq!(folders.find_in(&format!("cat {link}/a")), Some(link));
        assert_eq!(
            folders.find_in(&format!("cat {}/a", real.join("shop").to_str().unwrap())),
            Some(real.join("shop").to_str().unwrap())
        );
    }

    #[test]
    fn the_platform_is_the_compiled_os_and_architecture() {
        let host = host(Path::new("/w"), Path::new("/s"));
        assert_eq!(host.platform().as_str(), format!("{OS}/{ARCH}"));
        assert_eq!(host.platform(), host.platform());
    }

    #[test]
    fn a_variable_reads_as_the_process_holds_it() {
        let host = host(Path::new("/w"), Path::new("/s"));
        for variable in ["PATH", "HOME", "BUILDL_TEST_A_NAME_NOTHING_SETS"] {
            let answer = host.env(&name(variable));
            match env::var_os(variable).map(OsString::into_string) {
                None => assert_eq!(answer.unwrap(), None, "{variable}"),
                Some(Ok(text)) => {
                    assert_eq!(answer.unwrap().unwrap().as_str(), text, "{variable}");
                }
                Some(Err(_)) => assert!(answer.is_err(), "{variable}"),
            }
        }
        assert_eq!(
            host.env(&name("BUILDL_TEST_A_NAME_NOTHING_SETS")).unwrap(),
            None
        );
    }

    #[test]
    fn local_host_reports_home_workspace_and_sandbox() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().canonicalize().unwrap();
        fs::create_dir(real.join("shop")).unwrap();
        let link = real.join("shop-link");
        symlink(real.join("shop"), &link).unwrap();
        fs::create_dir(real.join("home")).unwrap();
        let home_link = real.join("home-link");
        symlink(real.join("home"), &home_link).unwrap();
        // The sandbox folder is not made yet, as before a first build.
        let sandbox = link.join("sandbox");
        let home = home_link.to_str().unwrap();

        let folders = LocalHost::with_home(&link, None, &sandbox, Some(OsString::from(home)))
            .unwrap()
            .machine_folders();

        for folder in [
            link.to_str().unwrap(),
            real.join("shop").to_str().unwrap(),
            sandbox.to_str().unwrap(),
            real.join("shop").join("sandbox").to_str().unwrap(),
            home,
            real.join("home").to_str().unwrap(),
        ] {
            assert_eq!(
                folders.find_in(&format!("cd {folder}/x")),
                Some(folder),
                "{folder}"
            );
        }
        assert_eq!(folders.find_in("cd /elsewhere/x"), None);
    }

    #[test]
    fn local_host_leaves_out_an_unset_home() {
        let folders = LocalHost::with_home(Path::new("/w/shop"), None, Path::new("/s/box"), None)
            .unwrap()
            .machine_folders();

        assert_eq!(folders.find_in("/w/shop/a"), Some("/w/shop"));
        assert_eq!(folders.find_in("/s/box/a"), Some("/s/box"));
        assert_eq!(folders.find_in("/home/al/a"), None);
        assert_eq!(folders.find_in("/"), None);
    }

    #[test]
    fn an_unset_an_empty_and_a_non_unicode_value_are_told_apart() {
        let variable = name("P_VALUE");
        assert_eq!(value(&variable, None).unwrap(), None);
        assert_eq!(
            value(&variable, Some(OsString::new()))
                .unwrap()
                .unwrap()
                .as_str(),
            ""
        );
        assert_eq!(
            value(&variable, Some(OsString::from("a b=c")))
                .unwrap()
                .unwrap()
                .as_str(),
            "a b=c"
        );

        let not_unicode = OsString::from_vec(b"caf\xE9\xFF".to_vec());
        let error = value(&variable, Some(not_unicode)).unwrap_err();
        assert!(matches!(
            error,
            Error::Adapter {
                port: PortName::Host,
                ..
            }
        ));
        assert_eq!(
            error.to_string(),
            "host: environment variable P_VALUE does not hold valid Unicode"
        );

        let with_nul = OsString::from_vec(b"a\0b".to_vec());
        assert_eq!(
            value(&variable, Some(with_nul)).unwrap_err().to_string(),
            "host: environment variable P_VALUE holds a NUL byte"
        );
    }
}
