//! The character grammars several domain types share.
//!
//! Its own file because a grammar shared by two types belongs to neither: a source path and a
//! directory are different concepts written in the same characters, and a setting name and a
//! field name likewise. Each type keeps its own [`NameKind`](crate::NameKind) and wraps the
//! reason returned here into its own error.
//!
//! Responsibilities: the four shared grammars — [`path`], [`name`], [`key`] and [`text`].
//!
//! Non-responsibilities: construction. Nothing here builds a value; each function only says why
//! a candidate is rejected.

/// Longest accepted path, in bytes.
const PATH_MAX_LEN: usize = 1024;

/// Longest accepted name or key, in bytes.
const NAME_MAX_LEN: usize = 256;

/// Whether `byte` may appear in a path segment or a name.
const fn is_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'-' || byte == b'_'
}

/// A non-empty workspace-relative path: `/`-separated segments of ASCII letters, digits, `.`,
/// `-` and `_`, none of them empty, `.` or `..`, at most 1024 bytes in all.
pub(super) fn path(raw: &str) -> Result<(), &'static str> {
    if raw.is_empty() {
        return Err("must not be empty");
    }
    if raw.len() > PATH_MAX_LEN {
        return Err("must be at most 1024 bytes");
    }
    if raw.starts_with('/') || raw.ends_with('/') {
        return Err("must not start or end with '/'");
    }
    for segment in raw.split('/') {
        if segment.is_empty() {
            return Err("must not contain an empty path segment");
        }
        if segment == "." || segment == ".." {
            return Err("must not contain a '.' or '..' segment");
        }
        if !segment.bytes().all(is_name_byte) {
            return Err("segments may hold only ASCII letters, digits, '.', '-' and '_'");
        }
    }
    Ok(())
}

/// A single name: ASCII letters, digits, `.`, `-` and `_`, neither `.` nor `..`, 1 to 256 bytes.
pub(super) fn name(raw: &str) -> Result<(), &'static str> {
    if raw.is_empty() {
        return Err("must not be empty");
    }
    if raw.len() > NAME_MAX_LEN {
        return Err("must be at most 256 bytes");
    }
    if raw == "." || raw == ".." {
        return Err("must not be '.' or '..'");
    }
    if !raw.bytes().all(is_name_byte) {
        return Err("may hold only ASCII letters, digits, '.', '-' and '_'");
    }
    Ok(())
}

/// A key: ASCII letters, digits, `-` and `_`, 1 to 256 bytes.
pub(super) fn key(raw: &str) -> Result<(), &'static str> {
    if raw.is_empty() {
        return Err("must not be empty");
    }
    if raw.len() > NAME_MAX_LEN {
        return Err("must be at most 256 bytes");
    }
    if !raw
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("may hold only ASCII letters, digits, '-' and '_'");
    }
    Ok(())
}

/// Free text that can travel in an argument vector: anything but a `NUL` byte.
pub(super) fn text(raw: &str) -> Result<(), &'static str> {
    if raw.contains('\0') {
        return Err("must not contain a NUL byte");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{key, name, path, text};

    #[test]
    fn path_accepts_nested_workspace_relative_paths() {
        for raw in ["lib", "lib/text", "src/main.c", "a-b_c.d/e"] {
            assert_eq!(path(raw), Ok(()), "{raw} should pass");
        }
    }

    #[test]
    fn path_rejects_each_documented_form_with_its_reason() {
        assert_eq!(path(""), Err("must not be empty"));
        assert_eq!(path(&"a".repeat(1025)), Err("must be at most 1024 bytes"));
        assert_eq!(path("/lib"), Err("must not start or end with '/'"));
        assert_eq!(path("lib/"), Err("must not start or end with '/'"));
        assert_eq!(
            path("lib//text"),
            Err("must not contain an empty path segment")
        );
        assert_eq!(
            path("lib/../x"),
            Err("must not contain a '.' or '..' segment")
        );
        assert_eq!(path("."), Err("must not contain a '.' or '..' segment"));
        assert_eq!(
            path("a b"),
            Err("segments may hold only ASCII letters, digits, '.', '-' and '_'")
        );
        assert_eq!(path(&"a".repeat(1024)), Ok(()));
    }

    #[test]
    fn name_rejects_each_documented_form_with_its_reason() {
        assert_eq!(name("main.o"), Ok(()));
        assert_eq!(name(""), Err("must not be empty"));
        assert_eq!(name(&"a".repeat(257)), Err("must be at most 256 bytes"));
        assert_eq!(name(&"a".repeat(256)), Ok(()));
        assert_eq!(name(".."), Err("must not be '.' or '..'"));
        assert_eq!(
            name("a/b"),
            Err("may hold only ASCII letters, digits, '.', '-' and '_'")
        );
    }

    #[test]
    fn key_rejects_each_documented_form_with_its_reason() {
        assert_eq!(key("test_filter"), Ok(()));
        assert_eq!(key("a-b"), Ok(()));
        assert_eq!(key(""), Err("must not be empty"));
        assert_eq!(key(&"a".repeat(257)), Err("must be at most 256 bytes"));
        assert_eq!(key(&"a".repeat(256)), Ok(()));
        assert_eq!(
            key("a.b"),
            Err("may hold only ASCII letters, digits, '-' and '_'")
        );
    }

    #[test]
    fn text_rejects_only_a_nul_byte() {
        assert_eq!(text(""), Ok(()));
        assert_eq!(text("$opt:name with spaces"), Ok(()));
        assert_eq!(text("a\0b"), Err("must not contain a NUL byte"));
    }
}
