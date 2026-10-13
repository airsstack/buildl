//! How many actions a build runs at once, validated at construction.
//!
//! Its own file because a manifest writes this as a number in which `0` is not a count: it
//! means one worker per core. Reading it into a type with a variant for that keeps the `0` out
//! of everything that reads the setting.
//!
//! Responsibilities: [`Workers`], its constructors, [`Workers::on`], and its rendering.
//!
//! Non-responsibilities: counting the machine's cores, and starting workers. Both belong to the
//! adapter that dispatches actions; the core count reaches [`Workers::on`] as a value.

use core::fmt;
use core::num::NonZeroUsize;
use core::str::FromStr;

use crate::error::{Error, NameKind, Result};

/// How many actions a build runs at once.
///
/// # Examples
///
/// ```
/// use core::num::NonZeroUsize;
///
/// use buildl_core::Workers;
///
/// let cores = NonZeroUsize::MIN.saturating_add(7);
/// assert_eq!(Workers::parse("0")?, Workers::PerCore);
/// assert_eq!(Workers::PerCore.on(cores).get(), 8);
/// assert_eq!(Workers::parse("2")?.on(cores).get(), 2);
/// assert!(Workers::parse("-1").is_err());
/// # Ok::<(), buildl_core::Error>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Workers {
    /// One per core of the machine the build runs on. A manifest writes this as `0`. The
    /// default.
    #[default]
    PerCore,
    /// This many, whatever the machine.
    Fixed(NonZeroUsize),
}

impl Workers {
    /// The setting a manifest writes as the number `count`: one per core for `0`, and otherwise
    /// exactly that many.
    #[must_use]
    pub const fn of(count: usize) -> Self {
        match NonZeroUsize::new(count) {
            Some(count) => Self::Fixed(count),
            None => Self::PerCore,
        }
    }

    /// The setting a manifest writes as the text `raw`: a whole number in decimal digits.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidName`] when `raw` is empty, holds anything but ASCII digits — a
    /// sign and a space included — or is a number too large to count workers with.
    pub fn parse(raw: &str) -> Result<Self> {
        let rejected = |reason| Error::InvalidName {
            kind: NameKind::Workers,
            value: raw.to_owned(),
            reason,
        };
        if raw.is_empty() {
            return Err(rejected("must not be empty"));
        }
        if !raw.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(rejected("must be a whole number, written in digits only"));
        }
        raw.parse::<usize>()
            .map(Self::of)
            .map_err(|_| rejected("is too large"))
    }

    /// How many actions run at once on a machine with `cores` cores.
    #[must_use]
    pub const fn on(self, cores: NonZeroUsize) -> NonZeroUsize {
        match self {
            Self::PerCore => cores,
            Self::Fixed(count) => count,
        }
    }
}

/// Renders the setting as a manifest writes it: `0` for one per core.
impl fmt::Display for Workers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PerCore => f.write_str("0"),
            Self::Fixed(count) => write!(f, "{count}"),
        }
    }
}

impl FromStr for Workers {
    type Err = Error;
    fn from_str(raw: &str) -> Result<Self> {
        Self::parse(raw)
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use core::num::NonZeroUsize;

    use super::Workers;

    fn count(raw: usize) -> NonZeroUsize {
        NonZeroUsize::new(raw).unwrap()
    }

    #[test]
    fn zero_means_one_per_core_and_any_other_number_is_that_many() {
        assert_eq!(Workers::of(0), Workers::PerCore);
        assert_eq!(Workers::of(1), Workers::Fixed(count(1)));
        assert_eq!(Workers::parse("0").unwrap(), Workers::PerCore);
        assert_eq!(Workers::parse("000").unwrap(), Workers::PerCore);
        assert_eq!(Workers::parse("8").unwrap(), Workers::Fixed(count(8)));
        assert_eq!(Workers::parse("016").unwrap(), Workers::Fixed(count(16)));
    }

    #[test]
    fn text_that_is_not_a_whole_number_is_rejected_with_its_reason() {
        let digits = "must be a whole number, written in digits only";
        let cases = [
            ("", "must not be empty"),
            ("-1", digits),
            ("+4", digits),
            (" 4", digits),
            ("4.0", digits),
            ("four", digits),
            ("٤", digits),
            ("99999999999999999999999999", "is too large"),
        ];
        for (raw, reason) in cases {
            let err = Workers::parse(raw).unwrap_err();
            assert_eq!(
                err.to_string(),
                format!("invalid worker count: {raw:?} — {reason}")
            );
        }
    }

    #[test]
    fn one_per_core_follows_the_machine_and_a_fixed_count_does_not() {
        assert_eq!(Workers::PerCore.on(count(1)), count(1));
        assert_eq!(Workers::PerCore.on(count(12)), count(12));
        assert_eq!(Workers::of(4).on(count(1)), count(4));
        assert_eq!(Workers::of(4).on(count(12)), count(4));
    }

    #[test]
    fn the_default_is_one_per_core() {
        assert_eq!(Workers::default(), Workers::PerCore);
    }

    #[test]
    fn workers_render_as_a_manifest_writes_them() {
        assert_eq!(Workers::PerCore.to_string(), "0");
        assert_eq!(Workers::of(8).to_string(), "8");
        for workers in [Workers::PerCore, Workers::of(3)] {
            assert_eq!(workers.to_string().parse::<Workers>().unwrap(), workers);
        }
    }
}
