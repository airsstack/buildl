//! Sorted-key, float-free JSON serialization, and reading it back.
//!
//! Its own file because the determinism rule it implements belongs in one place. Sorting is not
//! implemented here: routing a value through `serde_json::to_value` yields byte-order key
//! sorting at every level, because with `preserve_order` off a `serde_json::Map` is a
//! `BTreeMap`. What this file adds is the refusal of anything that would make the bytes
//! disagree with the value, in both directions: a value is written one way only, and only
//! bytes written that way are read.
//!
//! Responsibilities: [`to_vec`] and [`to_string`], which write, and [`from_slice`], which
//! reads.
//!
//! Non-responsibilities: deciding what to serialize, where the bytes go, and what makes a value
//! valid. A value read back is rebuilt by its own type's `Deserialize`, which is where that
//! type's rules are enforced.
//!
//! # What this module does and does not guarantee
//!
//! No float can reach here from a type in this crate: `f32` and `f64` are banned by
//! `crates/buildl-core/clippy.toml`, asserted by `cargo make guard-core-purity`. That ban, not
//! the walk below, is the real guarantee.
//!
//! The walk is the second line, for a float arriving through a dependency's `Serialize`
//! implementation. It catches every finite float, whole-valued ones included. It cannot catch a
//! non-finite one: `serde_json` converts `NaN` and `±Infinity` to `Value::Null` before any walk
//! sees them, so such a value is indistinguishable from a genuine `null` at this point.
//!
//! Two further residues, neither currently reachable:
//!
//! - A non-string map key is stringified silently — a `BTreeMap<u32, _>` renders as `{"7":…}`.
//!   What keeps the maps in this crate honest is that the types keying a serialized map —
//!   [`Label`](crate::Label), [`EnvName`](crate::EnvName) and
//!   [`SettingName`](crate::SettingName) — each serializes as its `Display` string, and is read
//!   back from one by its own constructor. It is not a crate-wide property:
//!   [`NodeId`](crate::NodeId) and [`Timestamp`](crate::Timestamp) are transparent scalars that
//!   serialize as bare numbers, and neither keys a map. Nothing mechanically prevents a future
//!   type keying on a number.
//! - Float *formatting*, were a float ever permitted, would be `serde_json`'s own and therefore
//!   pinned to a dependency version rather than to a specification.

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::{Error, Result};

/// Refuses any floating-point number in `tree`, naming its location.
fn reject_floats(tree: &serde_json::Value, path: &str) -> Result<()> {
    match tree {
        serde_json::Value::Number(number) if number.is_f64() => Err(Error::FloatRejected {
            path: path.to_owned(),
        }),
        serde_json::Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                reject_floats(item, &format!("{path}[{index}]"))?;
            }
            Ok(())
        }
        serde_json::Value::Object(entries) => {
            for (key, entry) in entries {
                reject_floats(entry, &format!("{path}.{key}"))?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Serializes `value` as canonical JSON bytes: object keys sorted at every level.
///
/// # Errors
///
/// Returns [`Error::CanonicalJson`] when the value cannot become JSON at all, and
/// [`Error::FloatRejected`] when it holds a floating-point number.
pub fn to_vec<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>> {
    let tree = serde_json::to_value(value).map_err(|source| Error::CanonicalJson { source })?;
    reject_floats(&tree, "$")?;
    serde_json::to_vec(&tree).map_err(|source| Error::CanonicalJson { source })
}

/// Serializes `value` as a canonical JSON string.
///
/// # Errors
///
/// Returns [`Error::CanonicalJson`] when the value cannot become JSON at all, and
/// [`Error::FloatRejected`] when it holds a floating-point number.
pub fn to_string<T: Serialize + ?Sized>(value: &T) -> Result<String> {
    let tree = serde_json::to_value(value).map_err(|source| Error::CanonicalJson { source })?;
    reject_floats(&tree, "$")?;
    serde_json::to_string(&tree).map_err(|source| Error::CanonicalJson { source })
}

/// Reads a value back from canonical JSON bytes.
///
/// Accepts exactly the bytes [`to_vec`] writes for some value of `T`, and nothing else: the
/// value is rebuilt by `T`'s own `Deserialize`, written again, and the two byte strings are
/// compared. So a field left out, a key out of order, a map key written twice, whitespace and
/// an escape [`to_vec`] would not use are all refused, and writing a value that was read gives
/// back the bytes it was read from.
///
/// # Errors
///
/// Returns [`Error::UnreadableJson`] when `bytes` are not JSON, or are JSON that `T` refuses,
/// and [`Error::NonCanonicalJson`] when they hold a valid `T` written any other way than
/// [`to_vec`] writes it. Either one means the bytes cannot be trusted.
///
/// # Examples
///
/// ```
/// use buildl_core::json::canonical;
/// use buildl_core::{Digest, Error};
///
/// let digest = Digest::of(b"app");
/// let bytes = canonical::to_vec(&digest)?;
/// assert_eq!(canonical::from_slice::<Digest>(&bytes)?, digest);
///
/// let uppercase = bytes.to_ascii_uppercase();
/// assert!(matches!(
///     canonical::from_slice::<Digest>(&uppercase),
///     Err(Error::UnreadableJson { .. })
/// ));
///
/// let padded = [b" ".as_slice(), &bytes].concat();
/// assert!(matches!(
///     canonical::from_slice::<Digest>(&padded),
///     Err(Error::NonCanonicalJson { offset: 0 })
/// ));
/// # Ok::<(), buildl_core::Error>(())
/// ```
pub fn from_slice<T: DeserializeOwned + Serialize>(bytes: &[u8]) -> Result<T> {
    let value: T =
        serde_json::from_slice(bytes).map_err(|source| Error::UnreadableJson { source })?;
    let canonical = to_vec(&value)?;
    if canonical == bytes {
        return Ok(value);
    }
    let offset = canonical
        .iter()
        .zip(bytes)
        .position(|(written, read)| written != read)
        .unwrap_or_else(|| canonical.len().min(bytes.len()));
    Err(Error::NonCanonicalJson { offset })
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use super::{from_slice, to_string, to_vec};
    use crate::error::Error;
    use serde::{Deserialize, Serialize};
    use std::collections::BTreeMap;

    /// A value with a required field, an optional one and a map, to read back.
    #[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Sample {
        name: String,
        note: Option<String>,
        counts: BTreeMap<String, u32>,
    }

    fn sample() -> Sample {
        Sample {
            name: "app".to_owned(),
            note: Some("first".to_owned()),
            counts: BTreeMap::from([("b".to_owned(), 2), ("a".to_owned(), 1)]),
        }
    }

    #[test]
    fn sorts_object_keys() {
        let mut map = BTreeMap::new();
        map.insert("zebra", 1_u32);
        map.insert("alpha", 2_u32);
        map.insert("middle", 3_u32);
        assert_eq!(
            to_string(&map).unwrap(),
            r#"{"alpha":2,"middle":3,"zebra":1}"#
        );
    }

    #[test]
    fn sorts_at_every_level() {
        let mut inner = BTreeMap::new();
        inner.insert("zebra", 1_u32);
        inner.insert("alpha", 2_u32);
        let mut outer = BTreeMap::new();
        outer.insert("outer_z", inner);
        assert_eq!(
            to_string(&outer).unwrap(),
            r#"{"outer_z":{"alpha":2,"zebra":1}}"#
        );
    }

    #[test]
    fn is_independent_of_insertion_order() {
        let mut first = serde_json::Map::new();
        first.insert("b".to_owned(), 1.into());
        first.insert("a".to_owned(), 2.into());
        let mut second = serde_json::Map::new();
        second.insert("a".to_owned(), 2.into());
        second.insert("b".to_owned(), 1.into());
        assert_eq!(
            to_vec(&serde_json::Value::Object(first)).unwrap(),
            to_vec(&serde_json::Value::Object(second)).unwrap()
        );
    }

    #[test]
    fn preserves_integers_at_the_extremes() {
        assert_eq!(to_string(&u64::MAX).unwrap(), "18446744073709551615");
        assert_eq!(to_string(&i64::MIN).unwrap(), "-9223372036854775808");
    }

    #[test]
    fn emits_no_whitespace() {
        assert_eq!(to_string(&vec![1, 2, 3]).unwrap(), "[1,2,3]");
    }

    #[test]
    fn rejects_a_finite_float_and_names_where_it_was() {
        let err = to_vec(&1.5).unwrap_err();
        assert!(matches!(err, Error::FloatRejected { ref path } if path == "$"));
    }

    #[test]
    fn rejects_a_whole_valued_float_too() {
        assert!(to_vec(&3.0).is_err());
    }

    #[test]
    fn names_a_float_inside_an_object_and_an_array() {
        let nested: serde_json::Value = serde_json::json!({ "timings": { "elapsed": 1.5 } });
        let err = to_vec(&nested).unwrap_err();
        assert!(matches!(err, Error::FloatRejected { ref path } if path == "$.timings.elapsed"));

        let in_array: serde_json::Value = serde_json::json!([1, 2.5]);
        let err = to_vec(&in_array).unwrap_err();
        assert!(matches!(err, Error::FloatRejected { ref path } if path == "$[1]"));
    }

    #[test]
    fn leaves_every_other_value_alone() {
        assert!(to_vec(&vec![1, 2, 3]).is_ok());
        assert!(to_string(&"text").is_ok());
        assert!(to_vec(&serde_json::Value::Null).is_ok());
        assert!(to_vec(&true).is_ok());
    }

    #[test]
    fn reports_a_value_that_cannot_become_json_at_all() {
        // u128::MAX has no JSON representation serde_json will produce: `to_value` itself
        // returns `Err("number out of range")`, driving the `CanonicalJson` arm rather than
        // `FloatRejected`.
        assert!(matches!(
            to_vec(&u128::MAX),
            Err(Error::CanonicalJson { .. })
        ));
        assert!(matches!(
            to_string(&u128::MAX),
            Err(Error::CanonicalJson { .. })
        ));
    }

    #[test]
    fn reads_back_what_it_wrote() {
        let bytes = to_vec(&sample()).unwrap();
        assert_eq!(from_slice::<Sample>(&bytes).unwrap(), sample());
        assert_eq!(from_slice::<Vec<u32>>(b"[1,2,3]").unwrap(), [1, 2, 3]);
        assert_eq!(
            from_slice::<u64>(b"18446744073709551615").unwrap(),
            u64::MAX
        );
        assert_eq!(from_slice::<Option<String>>(b"null").unwrap(), None);
    }

    #[test]
    fn writing_what_was_read_gives_the_bytes_that_were_read() {
        let bytes = to_vec(&sample()).unwrap();
        let read: Sample = from_slice(&bytes).unwrap();
        assert_eq!(to_vec(&read).unwrap(), bytes);
    }

    #[test]
    fn refuses_bytes_that_are_not_json() {
        use serde_json::error::Category;

        let cases: [(&[u8], Category); 7] = [
            (b"", Category::Eof),
            (b"[1,2", Category::Eof),
            (b"not json", Category::Syntax),
            (b"[1,2,]", Category::Syntax),
            (b"[01]", Category::Syntax),
            (b"[1,2] 3", Category::Syntax),
            (b"\xff\xfe", Category::Syntax),
        ];
        for (bytes, category) in cases {
            match from_slice::<Vec<u32>>(bytes) {
                Err(Error::UnreadableJson { source }) => {
                    assert_eq!(source.classify(), category, "{bytes:?}");
                }
                other => unreachable!("{bytes:?} should be unreadable, got {other:?}"),
            }
        }
    }

    #[test]
    fn refuses_json_that_is_not_the_value_asked_for() {
        let cases = [
            ("[]", "invalid length 0, expected struct Sample"),
            (r#""app""#, "invalid type: string"),
            (r#"{"counts":{},"note":null}"#, "missing field `name`"),
            (
                r#"{"counts":{},"extra":1,"name":"app","note":null}"#,
                "unknown field `extra`",
            ),
            (
                r#"{"counts":{},"name":"app","name":"app","note":null}"#,
                "duplicate field `name`",
            ),
            (
                r#"{"counts":{"a":-1},"name":"app","note":null}"#,
                "invalid value: integer `-1`, expected u32",
            ),
            (
                r#"{"counts":{"a":1.0},"name":"app","note":null}"#,
                "invalid type: floating point `1.0`, expected u32",
            ),
            (
                r#"{"counts":{"a":4294967296},"name":"app","note":null}"#,
                "invalid value: integer `4294967296`, expected u32",
            ),
        ];
        for (text, reason) in cases {
            match from_slice::<Sample>(text.as_bytes()) {
                Err(Error::UnreadableJson { source }) => {
                    assert!(source.is_data(), "{text}: {source}");
                    assert!(source.to_string().contains(reason), "{text}: {source}");
                }
                other => unreachable!("{text} should be unreadable, got {other:?}"),
            }
        }
    }

    #[test]
    fn refuses_a_valid_value_written_any_other_way_and_says_where() {
        let canonical = r#"{"counts":{"a":1,"b":2},"name":"app","note":null}"#;
        assert!(from_slice::<Sample>(canonical.as_bytes()).is_ok());

        let cases = [
            // A field that may be absent in other JSON is not absent here.
            (r#"{"counts":{"a":1,"b":2},"name":"app"}"#, 36),
            // Keys out of order, in a struct and in a map.
            (r#"{"name":"app","counts":{"a":1,"b":2},"note":null}"#, 2),
            (r#"{"counts":{"b":2,"a":1},"name":"app","note":null}"#, 12),
            // A map key written twice: the later one would win.
            (
                r#"{"counts":{"a":9,"a":1,"b":2},"name":"app","note":null}"#,
                15,
            ),
            // Whitespace, inside and after.
            (r#"{"counts": {"a":1,"b":2},"name":"app","note":null}"#, 10),
            (
                "{\"counts\":{\"a\":1,\"b\":2},\"name\":\"app\",\"note\":null}\n",
                49,
            ),
            // An escape the writer would not use.
            (
                r#"{"counts":{"a":1,"b":2},"name":"\u0061pp","note":null}"#,
                32,
            ),
        ];
        for (text, offset) in cases {
            match from_slice::<Sample>(text.as_bytes()) {
                Err(Error::NonCanonicalJson { offset: found }) => {
                    assert_eq!(found, offset, "{text}");
                }
                other => unreachable!("{text} should not be canonical, got {other:?}"),
            }
        }
    }
}
