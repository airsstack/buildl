//! How long one action may run, validated at construction.
//!
//! Its own file because a manifest writes this as text with a unit, such as `10m`, and a length
//! of time read from text has things to get wrong that a number does not: a missing unit, an
//! unknown one, and a length of zero, which would end every action before it began.
//!
//! Responsibilities: [`Timeout`], its [`Timeout::parse`] constructor, and its rendering.
//!
//! Non-responsibilities: timing an action, and ending one that ran too long. Both belong to the
//! adapter that runs actions; this crate reads no clock.

use core::fmt;
use core::str::FromStr;
use core::time::Duration;

use crate::error::{Error, NameKind, Result};

/// The shortest unit a timeout may be written in, and the one its length is counted in.
const MILLISECOND: &str = "ms";

/// The other units a timeout may be written in, longest first, each with its length in
/// milliseconds.
const LONGER_UNITS: [(&str, u64); 3] = [("h", 3_600_000), ("m", 60_000), ("s", 1_000)];

/// The length of the unit `word` names, in milliseconds.
fn unit_millis(word: &str) -> Option<u64> {
    if word == MILLISECOND {
        return Some(1);
    }
    LONGER_UNITS
        .into_iter()
        .find_map(|(unit, millis)| (unit == word).then_some(millis))
}

/// The grammar of a timeout, returning its length in milliseconds or the reason a candidate is
/// rejected.
fn timeout_grammar(raw: &str) -> core::result::Result<u64, &'static str> {
    if raw.is_empty() {
        return Err("must not be empty");
    }
    let digits = raw.bytes().take_while(u8::is_ascii_digit).count();
    let (count, unit) = raw.split_at(digits);
    if count.is_empty() {
        return Err("must start with a whole number");
    }
    let Some(unit_millis) = unit_millis(unit) else {
        return Err("must end in one of the units ms, s, m and h");
    };
    let millis = count
        .parse::<u64>()
        .ok()
        .and_then(|count| count.checked_mul(unit_millis))
        .ok_or("is too long")?;
    if millis == 0 {
        return Err("must be longer than zero");
    }
    Ok(millis)
}

/// The longest one action may run before it is ended: a whole number of milliseconds, never
/// zero.
///
/// Written as a whole number directly followed by a unit — `ms`, `s`, `m` or `h` — such as
/// `500ms`, `90s`, `10m` or `2h`. The default is ten minutes.
///
/// # Examples
///
/// ```
/// use core::time::Duration;
///
/// use buildl_core::Timeout;
///
/// let timeout = Timeout::parse("10m")?;
/// assert_eq!(timeout.as_duration(), Duration::from_secs(600));
/// assert_eq!(timeout, Timeout::default());
/// assert!(Timeout::parse("10").is_err());
/// # Ok::<(), buildl_core::Error>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timeout(u64);

impl Timeout {
    /// Validates `raw` and reads the length of time it states.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidName`] when `raw` is empty, does not start with a whole number in
    /// ASCII digits, does not end in exactly one of `ms`, `s`, `m` and `h`, states a length of
    /// zero, or states more milliseconds than a `u64` can count.
    pub fn parse(raw: &str) -> Result<Self> {
        match timeout_grammar(raw) {
            Ok(millis) => Ok(Self(millis)),
            Err(reason) => Err(Error::InvalidName {
                kind: NameKind::Timeout,
                value: raw.to_owned(),
                reason,
            }),
        }
    }

    /// The timeout as a length of time.
    #[must_use]
    pub const fn as_duration(self) -> Duration {
        Duration::from_millis(self.0)
    }
}

impl Default for Timeout {
    fn default() -> Self {
        Self(600_000)
    }
}

/// Renders the timeout in the longest unit that states it exactly, so `90s` stays `90s` and
/// `120s` becomes `2m`.
impl fmt::Display for Timeout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (count, unit) = LONGER_UNITS
            .into_iter()
            .find(|(_, millis)| self.0.is_multiple_of(*millis))
            .map_or((self.0, MILLISECOND), |(unit, millis)| {
                (self.0 / millis, unit)
            });
        write!(f, "{count}{unit}")
    }
}

impl FromStr for Timeout {
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

    use core::time::Duration;

    use super::Timeout;

    #[test]
    fn a_number_and_a_unit_state_a_length_of_time() {
        let cases = [
            ("500ms", Duration::from_millis(500)),
            ("1ms", Duration::from_millis(1)),
            ("90s", Duration::from_secs(90)),
            ("10m", Duration::from_secs(600)),
            ("2h", Duration::from_secs(7_200)),
            ("010m", Duration::from_secs(600)),
        ];
        for (raw, expected) in cases {
            assert_eq!(
                Timeout::parse(raw).unwrap().as_duration(),
                expected,
                "{raw:?}"
            );
        }
    }

    #[test]
    fn text_that_states_no_length_is_rejected_with_its_reason() {
        let number = "must start with a whole number";
        let unit = "must end in one of the units ms, s, m and h";
        let cases = [
            ("", "must not be empty"),
            ("m", number),
            ("-5m", number),
            ("+5m", number),
            (" 10m", number),
            ("ten minutes", number),
            ("10", unit),
            ("10 m", unit),
            ("10min", unit),
            ("10M", unit),
            ("10m ", unit),
            ("1.5h", unit),
            ("1h30m", unit),
            ("10d", unit),
            ("0s", "must be longer than zero"),
            ("000ms", "must be longer than zero"),
            ("99999999999999999999999s", "is too long"),
            ("18446744073709551615s", "is too long"),
        ];
        for (raw, reason) in cases {
            let err = Timeout::parse(raw).unwrap_err();
            assert_eq!(
                err.to_string(),
                format!("invalid timeout: {raw:?} — {reason}")
            );
        }
    }

    #[test]
    fn the_longest_length_a_timeout_can_state_is_accepted() {
        let longest = format!("{}ms", u64::MAX);
        assert_eq!(
            Timeout::parse(&longest).unwrap().as_duration(),
            Duration::from_millis(u64::MAX)
        );
    }

    #[test]
    fn the_default_is_ten_minutes() {
        assert_eq!(Timeout::default(), Timeout::parse("10m").unwrap());
    }

    #[test]
    fn a_timeout_renders_in_the_longest_unit_that_states_it_exactly() {
        let cases = [
            ("10m", "10m"),
            ("600s", "10m"),
            ("90s", "90s"),
            ("7200s", "2h"),
            ("90m", "90m"),
            ("1500ms", "1500ms"),
            ("1000ms", "1s"),
            ("1ms", "1ms"),
        ];
        for (raw, rendered) in cases {
            let timeout = Timeout::parse(raw).unwrap();
            assert_eq!(timeout.to_string(), rendered, "{raw:?}");
            assert_eq!(rendered.parse::<Timeout>().unwrap(), timeout);
        }
    }

    #[test]
    fn timeouts_order_by_length() {
        assert!(Timeout::parse("59s").unwrap() < Timeout::parse("1m").unwrap());
        assert!(Timeout::parse("1h").unwrap() > Timeout::parse("59m").unwrap());
    }
}
