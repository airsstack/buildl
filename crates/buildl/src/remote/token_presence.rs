//! Whether the token the remote settings name can be read, as a build would read it.
//!
//! `doctor` says whether a build would find the token. It asks the login every call carries, so
//! the two cannot disagree: the variable is set and holds text, or the file reads as text, and
//! the token holds nothing a header cannot carry. The token is dropped as soon as it has been
//! read and is never part of the answer.
//!
//! Responsibilities: [`token_is_present`].
//!
//! Non-responsibilities: reading and sending the token, which [`Login`] does.

use std::path::Path;

use buildl_core::{EnvName, RemoteSettings};

use super::login::Login;

/// Whether the settings name a token that a build would read.
///
/// The variable is set to text, or the file reads as text, with a relative file resolved against
/// `root`, and what it holds can be sent in the header the settings choose. Settings without a
/// token have none to find.
///
/// The token is read and dropped; it is never returned or shown.
#[must_use]
pub fn token_is_present(settings: &RemoteSettings, root: &Path) -> bool {
    token_is_present_with(settings, root, |name| std::env::var(name.as_str()).ok())
}

/// [`token_is_present`], taking the value of a variable from `lookup`.
fn token_is_present_with(
    settings: &RemoteSettings,
    root: &Path,
    lookup: impl FnOnce(&EnvName) -> Option<String>,
) -> bool {
    settings.token().is_some() && Login::read_with(settings, root, lookup).is_ok()
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
            |_| Some("x".to_owned())
        ));
    }

    #[test]
    fn a_set_variable_is_present_and_an_unset_one_is_not() {
        let named = settings(Some(variable("CACHE_TOKEN")));

        assert!(token_is_present_with(&named, Path::new("/"), |name| {
            (name.as_str() == "CACHE_TOKEN").then(|| "s3cret".to_owned())
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

    #[test]
    fn a_token_a_build_would_refuse_is_not_present() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("a-directory")).unwrap();
        fs::write(dir.path().join("two-lines"), "one\ntwo").unwrap();
        let directory = settings(Some(TokenSource::File(
            HostPath::parse("a-directory").unwrap(),
        )));
        let two_lines = settings(Some(TokenSource::File(
            HostPath::parse("two-lines").unwrap(),
        )));
        let named = settings(Some(variable("CACHE_TOKEN")));

        assert!(!token_is_present_with(&directory, dir.path(), |_| None));
        assert!(!token_is_present_with(&two_lines, dir.path(), |_| None));
        assert!(!token_is_present_with(&named, dir.path(), |_| Some(
            "one\ntwo".to_owned()
        )));
    }
}
