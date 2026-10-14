//! The grammar of a memory size, such as `16MB`.
//!
//! Its own file because the manifest writes its one memory ceiling as text with a unit, and a
//! size read from text has things to get wrong that a number does not: a missing unit, an
//! unknown one, a size of zero, and a size no machine can count.
//!
//! Responsibilities: [`memory_bytes`].
//!
//! Non-responsibilities: what the size limits, and saying where in the manifest a rejected
//! size stands. The reason returned here is joined to its key and its position by the caller.

use core::num::NonZeroU64;

/// The units a size may be written in, each with its length in bytes: every one is 1024 times
/// the one before.
const UNITS: [(&str, u64); 4] = [
    ("B", 1),
    ("KB", 1024),
    ("MB", 1024 * 1024),
    ("GB", 1024 * 1024 * 1024),
];

/// The number of bytes `raw` states, or the reason it states none.
///
/// A size is a whole number in ASCII digits directly followed by one unit: `B`, `KB`, `MB` or
/// `GB`. It is never zero.
pub(crate) fn memory_bytes(raw: &str) -> core::result::Result<NonZeroU64, &'static str> {
    if raw.is_empty() {
        return Err("must not be empty");
    }
    let digits = raw.bytes().take_while(u8::is_ascii_digit).count();
    let (count, unit) = raw.split_at(digits);
    if count.is_empty() {
        return Err("must start with a whole number");
    }
    let Some(unit_bytes) = UNITS
        .into_iter()
        .find_map(|(word, bytes)| (word == unit).then_some(bytes))
    else {
        return Err("must end in one of the units B, KB, MB and GB");
    };
    let bytes = count
        .parse::<u64>()
        .ok()
        .and_then(|count| count.checked_mul(unit_bytes))
        .ok_or("is too large")?;
    NonZeroU64::new(bytes).ok_or("must not be zero")
}

#[cfg(test)]
mod tests {
    use super::memory_bytes;

    #[test]
    fn each_unit_is_1024_times_the_one_before() {
        let cases = [
            ("1B", 1),
            ("1KB", 1024),
            ("1MB", 1024 * 1024),
            ("1GB", 1024 * 1024 * 1024),
            ("16MB", 16 * 1024 * 1024),
            ("016MB", 16 * 1024 * 1024),
            ("17179869183GB", 17_179_869_183 * 1024 * 1024 * 1024),
        ];
        for (raw, expected) in cases {
            assert_eq!(
                memory_bytes(raw).map(core::num::NonZeroU64::get),
                Ok(expected),
                "{raw:?}"
            );
        }
    }

    #[test]
    fn zero_a_missing_unit_and_an_overflow_are_rejected() {
        let number = "must start with a whole number";
        let unit = "must end in one of the units B, KB, MB and GB";
        let cases = [
            ("", "must not be empty"),
            ("MB", number),
            ("-1MB", number),
            (" 16MB", number),
            ("16", unit),
            ("16 MB", unit),
            ("16mb", unit),
            ("16MiB", unit),
            ("16M", unit),
            ("1.5GB", unit),
            ("16MB ", unit),
            ("0MB", "must not be zero"),
            ("000B", "must not be zero"),
            ("17179869184GB", "is too large"),
            ("18446744073709551616B", "is too large"),
        ];
        for (raw, reason) in cases {
            assert_eq!(memory_bytes(raw), Err(reason), "{raw:?}");
        }
    }
}
