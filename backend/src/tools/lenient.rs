//! Deserializers for integer tool arguments. A model sometimes writes a number as a string
//! (`"9593"` where the schema says integer); the number is unambiguous, and failing the call with a
//! type error only costs a round trip, so these accept both. Anything that isn't a number — or a
//! string that doesn't parse as one — still fails with the usual error.

use std::fmt::Display;
use std::str::FromStr;

use serde::{de, Deserialize, Deserializer};
use serde_json::Value;

/// A number, or a string holding one, as `T`; anything else is an error that says what was expected
fn to_int<T, E>(value: Value) -> Result<T, E>
where
    T: FromStr,
    T::Err: Display,
    E: de::Error,
{
    match value {
        Value::Number(number) => number.to_string().parse().map_err(E::custom),
        Value::String(text) => text.trim().parse().map_err(E::custom),
        other => Err(E::custom(format!("expected an integer or a numeric string, got {other}"))),
    }
}

/// An optional integer given as a number or as a numeric string. Needs `#[serde(default)]` next
/// to it so that leaving the argument out stays `None`.
pub fn opt_int<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr,
    T::Err: Display,
{
    match Option::<Value>::deserialize(deserializer)? {
        None | Some(Value::Null) => Ok(None),
        Some(value) => to_int(value).map(Some),
    }
}

/// A list of integers, each given as a number or as a numeric string
pub fn int_vec<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr,
    T::Err: Display,
{
    Vec::<Value>::deserialize(deserializer)?.into_iter().map(to_int).collect()
}
