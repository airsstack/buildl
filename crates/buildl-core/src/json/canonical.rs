//! Sorted-key, float-free JSON serialization.
//!
//! Its own file because the determinism rule it implements belongs in one place. Sorting is not
//! implemented here: routing a value through `serde_json::to_value` yields byte-order key
//! sorting at every level, because with `preserve_order` off a `serde_json::Map` is a
//! `BTreeMap`. What this file adds is the refusal of anything that would make the bytes
//! disagree with the value.
//!
//! Responsibilities: [`to_vec`] and [`to_string`].
//!
//! Non-responsibilities: deciding what to serialize, and where the bytes go.
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
//!   What keeps the maps in this crate honest is that the four types able to appear as a map key
//!   — [`Directory`](crate::Directory), [`TargetName`](crate::TargetName),
//!   [`Label`](crate::Label) and [`Digest`](crate::Digest) — each serializes as its `Display`
//!   string. It is not a crate-wide property: [`NodeId`](crate::NodeId) and
//!   [`Timestamp`](crate::Timestamp) are transparent scalars that serialize as bare numbers, and
//!   neither keys a map. Nothing mechanically prevents a future type keying on a number.
//! - Float *formatting*, were a float ever permitted, would be `serde_json`'s own and therefore
//!   pinned to a dependency version rather than to a specification.

use serde::Serialize;

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

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap known-valid fixtures; a panic is the intended failure signal"
    )]

    use super::{to_string, to_vec};
    use crate::error::Error;
    use std::collections::BTreeMap;

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
}
