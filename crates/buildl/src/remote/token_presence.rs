//! Whether the token the remote settings name can be found, without reading it.
//!
//! `doctor` says whether a build would find the token, and must not hold its content to say so:
//! a variable counts when it is set, a file when it can be opened.
//!
//! Responsibilities: [`token_is_present`].
//!
//! Non-responsibilities: reading the token, which the login every call carries does, and judging
//! whether what it holds can be sent.

use std::ffi::OsString;
use std::fs::File;
use std::path::Path;

use buildl_core::{EnvName, RemoteSettings, TokenSource};

/// Whether the token `settings` name is there to read: the variable is set, or the file opens,
/// with a relative file resolved against `root`. Settings without a token have none to find.
///
/// Neither the variable's value nor the file's content is read into the answer.
#[must_use]
pub fn token_is_present(settings: &RemoteSettings, root: &Path) -> bool {
    token_is_present_with(settings, root, |name| std::env::var_os(name.as_str()))
}

/// [`token_is_present`], taking the value of a variable from `lookup`.
fn token_is_present_with(
    settings: &RemoteSettings,
    root: &Path,
    lookup: impl FnOnce(&EnvName) -> Option<OsString>,
) -> bool {
    match settings.token() {
        None => false,
        Some(TokenSource::Env(name)) => lookup(name).is_some(),
        Some(TokenSource::File(path)) => {
            let file = if path.is_absolute() {
                Path::new(path.as_str()).to_path_buf()
            } else {
                root.join(path.as_str())
            };
            File::open(file).is_ok()
        }
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::fs;
    use std::path::Path;

    use buildl_core::{EnvName, HostPath, RemoteSettings, RemoteUrl, TokenSource};

    use super::token_is_present_with;

    fn settings(token: Option<TokenSource>) -> RemoteSettings {
        let url = RemoteUrl::parse("grpcs://cache.example.com").unwrap();
        let builder = RemoteSettings::builder(url);
        match token {
            Some(token) => builder.token(token),
            None => builder,
        }
        .build()
        .unwrap()
    }

    fn variable(name: &str) -> TokenSource {
        TokenSource::Env(EnvName::parse(name).unwrap())
    }

    #[test]
    fn settings_without_a_token_have_none_to_find() {
        assert!(!token_is_present_with(
            &settings(None),
            Path::new("/"),
            |_| Some("x".into())
        ));
    }

    #[test]
    fn a_set_variable_is_present_and_an_unset_one_is_not() {
        let named = settings(Some(variable("CACHE_TOKEN")));

        assert!(token_is_present_with(&named, Path::new("/"), |name| {
            (name.as_str() == "CACHE_TOKEN").then(|| "s3cret".into())
        }));
        assert!(!token_is_present_with(&named, Path::new("/"), |_| None));
    }

    #[test]
    fn a_readable_file_is_present_relative_to_the_root_and_a_missing_one_is_not() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("token"), "s3cret").unwrap();
        let relative = settings(Some(TokenSource::File(HostPath::parse("token").unwrap())));
        let absent = settings(Some(TokenSource::File(HostPath::parse("nope").unwrap())));

        assert!(token_is_present_with(&relative, dir.path(), |_| None));
        assert!(!token_is_present_with(&absent, dir.path(), |_| None));
    }

    #[test]
    fn an_absolute_file_ignores_the_root() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("token");
        fs::write(&file, "s3cret").unwrap();
        let absolute = settings(Some(TokenSource::File(
            HostPath::parse(file.to_str().unwrap()).unwrap(),
        )));

        assert!(token_is_present_with(
            &absolute,
            Path::new("/nowhere"),
            |_| None
        ));
    }
}
