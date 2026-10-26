//! The lines a watch says about itself.
//!
//! Its own file because none of these lines belongs to a build: each says what the watching loop
//! around builds is doing, for a person, and every one starts with `watch: ` so that it can be
//! told from the blocks and summaries of the builds between them.
//!
//! Responsibilities: [`building_line`], [`waiting_line`] and [`watch_error_line`].
//!
//! Non-responsibilities: writing the text anywhere, and the words of an error, which are
//! [`error_text`]'s.

use buildl_core::{Error, Trigger};

use crate::report::message::error_text;
use crate::report::paint::Paint;

/// The line that opens a cycle: `watch: building` when the watch has just begun, or the first
/// path of the changes that started it, with how many more there are.
///
/// Ends with a newline.
#[must_use]
pub fn building_line(trigger: &Trigger) -> String {
    match trigger {
        Trigger::Start => "watch: building\n".to_owned(),
        Trigger::Changed(batch) => match batch.len() - 1 {
            0 => format!("watch: {} changed\n", batch.first()),
            more => format!("watch: {} and {more} more changed\n", batch.first()),
        },
    }
}

/// The line that says the loop has nothing to do: `watch: waiting for changes`, ending with a
/// newline.
#[must_use]
pub fn waiting_line() -> String {
    "watch: waiting for changes\n".to_owned()
}

/// The line for an error that ends the watch: `watch: ` followed by the error as a command shows
/// it, see [`error_text`].
#[must_use]
pub fn watch_error_line(error: &Error, paint: Paint) -> String {
    format!("watch: {}", error_text(error, paint))
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use std::collections::BTreeSet;

    use buildl_core::{ChangeBatch, ChangedPath, Error, SettingName, Trigger};

    use super::{building_line, waiting_line, watch_error_line};
    use crate::report::message::error_text;
    use crate::report::paint::Paint;

    fn changed(paths: &[&str]) -> Trigger {
        let set: BTreeSet<ChangedPath> = paths
            .iter()
            .map(|path| ChangedPath::parse(*path).unwrap())
            .collect();
        Trigger::Changed(ChangeBatch::new(set).unwrap())
    }

    fn an_error() -> Error {
        Error::UnknownOverride {
            name: SettingName::parse("optimise").unwrap(),
        }
    }

    #[test]
    fn start_is_building() {
        assert_eq!(building_line(&Trigger::Start), "watch: building\n");
    }

    #[test]
    fn one_path_is_named() {
        assert_eq!(
            building_line(&changed(&["lib/text.c"])),
            "watch: lib/text.c changed\n"
        );
    }

    #[test]
    fn more_paths_are_counted() {
        assert_eq!(
            building_line(&changed(&["b.h", "a.c", "c.h"])),
            "watch: a.c and 2 more changed\n"
        );
    }

    #[test]
    fn waiting_is_said() {
        assert_eq!(waiting_line(), "watch: waiting for changes\n");
    }

    #[test]
    fn a_watcher_error_is_prefixed() {
        let error = an_error();
        assert_eq!(
            watch_error_line(&error, Paint::Plain),
            format!("watch: {}", error_text(&error, Paint::Plain))
        );
        assert_eq!(
            watch_error_line(&error, Paint::Plain),
            "watch: error: setting optimise is overridden but not declared\n"
        );
    }
}
