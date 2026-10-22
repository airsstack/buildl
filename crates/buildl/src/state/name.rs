//! The name a directory or file gets from the bytes it stands for.
//!
//! Its own file because the scratch directory and the file of remembered approvals are each named
//! for a workspace root, and no adapter may name another: one workspace has to have one name in
//! both places.
//!
//! Responsibilities: [`name_of`].

use buildl_core::Digest;

/// How many hexadecimal digits of a digest name a directory or file.
const NAME_DIGITS: usize = 32;

/// The name `bytes` give a directory or file: the first digits of their digest.
#[must_use]
pub fn name_of(bytes: &[u8]) -> String {
    Digest::of(bytes)
        .to_string()
        .chars()
        .take(NAME_DIGITS)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::name_of;

    #[test]
    fn a_name_is_thirty_two_lowercase_hex_digits() {
        let name = name_of(b"/work/one");
        assert_eq!(name.len(), 32, "{name}");
        assert!(
            name.bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
            "{name}"
        );
    }

    #[test]
    fn the_same_bytes_give_the_same_name_and_other_bytes_another() {
        assert_eq!(name_of(b"/work/one"), name_of(b"/work/one"));
        assert_ne!(name_of(b"/work/one"), name_of(b"/work/two"));
    }
}
