//! The folders of the machine buildl runs on, and the search for one of them in a text.
//!
//! Its own file because the match rule is a grammar to get wrong: a folder inside a longer name or
//! inside another path is not this machine's folder, and a false match would block a workspace.
//!
//! Responsibilities: [`MachineFolders`], its [`MachineFolders::new`] constructor and
//! [`MachineFolders::find_in`].
//!
//! Non-responsibilities: learning the folders, which is the host's, and deciding what a match
//! means for a command, a variable or a discovery output, which is the phase that reads the text.

/// This machine's home, workspace and sandbox folders, each in the forms it is known by.
///
/// A folder matches in a text where it starts the text, follows a character that cannot be part
/// of a path (anything but a letter, a digit, `.`, `_`, `-`, `/`), or follows a flag (a `-` and one
/// or more ASCII letters that start the text or follow such a character, as in `-I/home/al`); and
/// where it ends the text or is followed by a character that cannot continue its last name
/// (anything but a letter, a digit, `.`, `_`, `-`).
///
/// # Examples
///
/// ```
/// use buildl_core::MachineFolders;
///
/// let folders = MachineFolders::new(&["/home/al/"], &["/home/al/shop"], &[]);
/// assert_eq!(folders.find_in("cd /home/al/shop && make"), Some("/home/al/shop"));
/// assert_eq!(folders.find_in("cd /home/alice/shop"), None);
/// assert_eq!(folders.find_in("cc -I/home/al/include"), Some("/home/al"));
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MachineFolders {
    /// Every folder form, trailing `/` taken off, none empty.
    forms: Vec<String>,
}

impl MachineFolders {
    /// The folders given as `home`, `workspace` and `sandbox`, each a list of the forms the folder
    /// is known by (as given and canonical).
    ///
    /// A form loses a trailing `/`; one that is then empty, so a home of `/`, is left out, and so
    /// is a form given twice. An empty list leaves its folder out.
    #[must_use]
    pub fn new(home: &[&str], workspace: &[&str], sandbox: &[&str]) -> Self {
        let mut forms: Vec<String> = Vec::new();
        for raw in home.iter().chain(workspace).chain(sandbox) {
            let form = raw.trim_end_matches('/');
            if !form.is_empty() && !forms.iter().any(|known| known == form) {
                forms.push(form.to_owned());
            }
        }
        Self { forms }
    }

    /// The longest folder `text` holds, by the match rule on the type.
    #[must_use]
    pub fn find_in(&self, text: &str) -> Option<&str> {
        self.forms
            .iter()
            .filter(|form| holds(text, form))
            .max_by_key(|form| form.len())
            .map(String::as_str)
    }
}

/// Whether `text` holds `folder` as a whole path prefix, not inside a longer name or path.
fn holds(text: &str, folder: &str) -> bool {
    text.match_indices(folder).any(|(start, found)| {
        let after = text[start + found.len()..].chars().next();
        starts_a_path(&text[..start]) && after.is_none_or(|c| !continues_name(c))
    })
}

/// Whether a path may start right after `before`: at the text's start, after a character that
/// cannot be part of a path, or after a flag.
fn starts_a_path(before: &str) -> bool {
    before.chars().next_back().is_none_or(outside_a_path) || ends_with_flag(before)
}

/// Whether `before` ends in a flag: a `-` and one or more ASCII letters, the `-` starting the text
/// or following a character that cannot be part of a path.
fn ends_with_flag(before: &str) -> bool {
    let without_letters = before.trim_end_matches(|c: char| c.is_ascii_alphabetic());
    without_letters.len() < before.len()
        && without_letters
            .strip_suffix('-')
            .is_some_and(|rest| rest.chars().next_back().is_none_or(outside_a_path))
}

/// Whether `c` cannot be part of a path.
fn outside_a_path(c: char) -> bool {
    !(continues_name(c) || c == '/')
}

/// Whether `c` can be part of a name in a path.
fn continues_name(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '.' | '_' | '-')
}

#[cfg(test)]
mod tests {
    use super::MachineFolders;

    fn home() -> MachineFolders {
        MachineFolders::new(&["/home/al"], &[], &[])
    }

    #[test]
    fn matches_followed_by_slash() {
        assert_eq!(home().find_in("cd /home/al/shop && make"), Some("/home/al"));
    }

    #[test]
    fn matches_the_whole_text() {
        assert_eq!(home().find_in("/home/al"), Some("/home/al"));
    }

    #[test]
    fn matches_followed_by_space() {
        assert_eq!(home().find_in("cd /home/al && make"), Some("/home/al"));
    }

    #[test]
    fn matches_followed_by_colon() {
        assert_eq!(home().find_in("/home/al:/usr/lib"), Some("/home/al"));
    }

    #[test]
    fn matches_after_a_quote() {
        assert_eq!(
            home().find_in(r#""workspace_root":"/home/al/tcapp""#),
            Some("/home/al")
        );
    }

    #[test]
    fn does_not_match_a_longer_name() {
        assert_eq!(home().find_in("cd /home/alice/shop"), None);
    }

    #[test]
    fn does_not_match_inside_another_path() {
        assert_eq!(home().find_in("/data/home/al/x"), None);
    }

    #[test]
    fn does_not_match_absent() {
        assert_eq!(home().find_in("cd shop && make"), None);
    }

    #[test]
    fn skips_a_root_home() {
        let folders = MachineFolders::new(&["/"], &[], &[]);
        assert_eq!(folders.find_in("/usr/bin/env"), None);
    }

    #[test]
    fn trailing_slash_is_taken_off() {
        let folders = MachineFolders::new(&["/home/al/"], &[], &[]);
        assert_eq!(folders.find_in("/home/al/x"), Some("/home/al"));
    }

    #[test]
    fn names_the_longest_match() {
        let folders = MachineFolders::new(&["/home/al"], &["/home/al/shop"], &[]);
        assert_eq!(
            folders.find_in("cd /home/al/shop/src"),
            Some("/home/al/shop")
        );
    }

    #[test]
    fn matches_after_a_joined_flag() {
        assert_eq!(home().find_in("-I/home/al/include"), Some("/home/al"));
    }

    #[test]
    fn matches_after_a_long_joined_flag() {
        assert_eq!(home().find_in("-isystem/home/al/x"), Some("/home/al"));
        assert_eq!(home().find_in("cc -L/home/al/lib"), Some("/home/al"));
    }

    #[test]
    fn matches_after_an_equals() {
        assert_eq!(home().find_in("--prefix=/home/al"), Some("/home/al"));
    }

    #[test]
    fn does_not_match_a_longer_name_after_a_flag() {
        assert_eq!(home().find_in("-I/home/alice/x"), None);
    }

    #[test]
    fn does_not_take_a_dash_inside_a_name_for_a_flag() {
        assert_eq!(home().find_in("/opt/x-y/home/al"), None);
    }

    #[test]
    fn does_not_take_a_bare_dash_for_a_flag() {
        assert_eq!(home().find_in("-/home/al"), None);
    }

    #[test]
    fn does_not_take_a_dash_and_digits_for_a_flag() {
        assert_eq!(home().find_in("-1/home/al"), None);
    }

    #[test]
    fn does_not_match_after_a_dot() {
        assert_eq!(home().find_in("-I./home/al"), None);
    }
}
