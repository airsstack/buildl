//! Text exactly as a build file wrote it, typed by what it is meant to become.
//!
//! Its own file because an as-written value is its own concept: it has a destination type but has
//! not been validated against it yet. Keeping the destination in the type means a value written
//! as a label can only ever be resolved as a label.
//!
//! Responsibilities: [`Written`], and one conversion per destination type.
//!
//! Non-responsibilities: deciding anything between two different concepts. Every conversion here
//! either parses its own destination type or combines it with a [`Directory`] of the same family —
//! a [`Label`] contains a directory, and [`Written::under`] joins two directories.

use core::marker::PhantomData;

use crate::error::{Error, NameKind, Result};
use crate::types::{
    Argument, Description, Directory, EnvName, FieldName, Label, OutputName, SettingName,
    SettingValue, SourcePath, TargetName,
};

/// Text as a build file wrote it, not yet validated as the `T` it is meant to become.
///
/// Construction never fails; validation happens in the one conversion each `T` has. The marker is
/// `PhantomData<fn() -> T>`, so a `Written<T>` is `Send`, `Sync` and covariant whatever `T` is.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Written<T> {
    text: String,
    target: PhantomData<fn() -> T>,
}

impl<T> Written<T> {
    /// Records `text` exactly as written.
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            target: PhantomData,
        }
    }

    /// The text exactly as written.
    #[must_use]
    pub fn as_written(&self) -> &str {
        &self.text
    }
}

impl Written<TargetName> {
    /// Validates the text as a target name.
    ///
    /// # Errors
    ///
    /// Returns whatever [`TargetName::parse`] rejects.
    pub fn parse(&self) -> Result<TargetName> {
        TargetName::parse(self.text.clone())
    }
}

impl Written<Label> {
    /// Resolves the text as a reference written in the build file of `base`.
    ///
    /// # Errors
    ///
    /// Returns whatever [`Label::resolve`] rejects.
    pub fn resolve(&self, base: &Directory) -> Result<Label> {
        Label::resolve(&self.text, base)
    }
}

impl Written<Directory> {
    /// Joins the text under `base` and validates the result as a directory.
    ///
    /// The text names a directory relative to `base`, so a `..` or a leading `/` produces a path
    /// the directory grammar refuses: a written directory can never leave the workspace.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidName`] when the text is empty, and whatever [`Directory::parse`]
    /// rejects about the joined path.
    pub fn under(&self, base: &Directory) -> Result<Directory> {
        if self.text.is_empty() {
            return Err(Error::InvalidName {
                kind: NameKind::Directory,
                value: String::new(),
                reason: "must not be empty",
            });
        }
        if base.is_root() {
            Directory::parse(self.text.clone())
        } else {
            Directory::parse(format!("{base}/{}", self.text))
        }
    }
}

impl Written<SourcePath> {
    /// Validates the text as a source path.
    ///
    /// # Errors
    ///
    /// Returns whatever [`SourcePath::parse`] rejects.
    pub fn parse(&self) -> Result<SourcePath> {
        SourcePath::parse(self.text.clone())
    }
}

impl Written<OutputName> {
    /// Validates the text as an output name.
    ///
    /// # Errors
    ///
    /// Returns whatever [`OutputName::parse`] rejects.
    pub fn parse(&self) -> Result<OutputName> {
        OutputName::parse(self.text.clone())
    }
}

impl Written<EnvName> {
    /// Validates the text as an environment variable name.
    ///
    /// # Errors
    ///
    /// Returns whatever [`EnvName::parse`] rejects.
    pub fn parse(&self) -> Result<EnvName> {
        EnvName::parse(self.text.clone())
    }
}

impl Written<Argument> {
    /// Validates the text as a command argument.
    ///
    /// # Errors
    ///
    /// Returns whatever [`Argument::parse`] rejects.
    pub fn parse(&self) -> Result<Argument> {
        Argument::parse(self.text.clone())
    }
}

impl Written<Description> {
    /// Validates the text as a description.
    ///
    /// # Errors
    ///
    /// Returns whatever [`Description::parse`] rejects.
    pub fn parse(&self) -> Result<Description> {
        Description::parse(self.text.clone())
    }
}

impl Written<SettingName> {
    /// Validates the text as a setting name.
    ///
    /// # Errors
    ///
    /// Returns whatever [`SettingName::parse`] rejects.
    pub fn parse(&self) -> Result<SettingName> {
        SettingName::parse(self.text.clone())
    }
}

impl Written<SettingValue> {
    /// Validates the text as a setting value.
    ///
    /// # Errors
    ///
    /// Returns whatever [`SettingValue::parse`] rejects.
    pub fn parse(&self) -> Result<SettingValue> {
        SettingValue::parse(self.text.clone())
    }
}

impl Written<FieldName> {
    /// Validates the text as a field name.
    ///
    /// # Errors
    ///
    /// Returns whatever [`FieldName::parse`] rejects.
    pub fn parse(&self) -> Result<FieldName> {
        FieldName::parse(self.text.clone())
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use super::Written;
    use crate::types::{
        Argument, Description, Directory, EnvName, FieldName, Label, OutputName, SettingName,
        SettingValue, SourcePath, TargetName,
    };

    #[test]
    fn keeps_the_text_exactly_as_written() {
        let written = Written::<Label>::new("../not a label");
        assert_eq!(written.as_written(), "../not a label");
    }

    #[test]
    fn a_target_name_parses_or_refuses() {
        assert_eq!(
            Written::<TargetName>::new("app").parse().unwrap().as_str(),
            "app"
        );
        assert!(Written::<TargetName>::new("//other:x").parse().is_err());
    }

    #[test]
    fn a_label_resolves_every_reference_form_against_its_base() {
        let base = Directory::parse("app").unwrap();
        for (raw, resolved) in [
            ("main.o", "//app:main.o"),
            (":util.o", "//app:util.o"),
            ("//lib:text", "//lib:text"),
        ] {
            assert_eq!(
                Written::<Label>::new(raw)
                    .resolve(&base)
                    .unwrap()
                    .to_string(),
                resolved
            );
        }
        assert!(Written::<Label>::new("//lib").resolve(&base).is_err());
    }

    #[test]
    fn a_directory_joins_under_its_base() {
        let root = Directory::root();
        let lib = Directory::parse("lib").unwrap();
        assert_eq!(Written::<Directory>::new("lib").under(&root).unwrap(), lib);
        assert_eq!(
            Written::<Directory>::new("text")
                .under(&lib)
                .unwrap()
                .as_str(),
            "lib/text"
        );
    }

    #[test]
    fn a_directory_never_escapes_or_stays_empty() {
        let lib = Directory::parse("lib").unwrap();
        for raw in ["", "..", "../x", "/x", "a//b", "a b"] {
            assert!(
                Written::<Directory>::new(raw).under(&lib).is_err(),
                "{raw:?} should be rejected under lib"
            );
            assert!(
                Written::<Directory>::new(raw)
                    .under(&Directory::root())
                    .is_err(),
                "{raw:?} should be rejected under the root"
            );
        }
    }

    #[test]
    fn an_empty_directory_reports_the_directory_grammar() {
        let err = Written::<Directory>::new("")
            .under(&Directory::root())
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            r#"invalid directory: "" — must not be empty"#
        );
    }

    #[test]
    fn every_plain_conversion_parses_its_own_type() {
        assert!(Written::<SourcePath>::new("src/a.c").parse().is_ok());
        assert!(Written::<SourcePath>::new("../a.c").parse().is_err());
        assert!(Written::<OutputName>::new("bin/app").parse().is_ok());
        assert!(Written::<OutputName>::new("/app").parse().is_err());
        assert!(Written::<EnvName>::new("PATH").parse().is_ok());
        assert!(Written::<EnvName>::new("1X").parse().is_err());
        assert!(Written::<Argument>::new("$in").parse().is_ok());
        assert!(Written::<Argument>::new("a\0b").parse().is_err());
        assert!(Written::<Description>::new("compile $in").parse().is_ok());
        assert!(Written::<Description>::new("a\nb").parse().is_err());
        assert!(Written::<SettingName>::new("test_filter").parse().is_ok());
        assert!(Written::<SettingName>::new("a.b").parse().is_err());
        assert!(Written::<SettingValue>::new("").parse().is_ok());
        assert!(Written::<SettingValue>::new("a\0b").parse().is_err());
        assert!(Written::<FieldName>::new("deps").parse().is_ok());
        assert!(Written::<FieldName>::new("a b").parse().is_err());
    }
}
