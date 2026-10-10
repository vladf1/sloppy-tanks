//! Small strict readers over parsed JSON (`src/net/schema.ts`), shared by the host and
//! the client mirror.
//!
//! They keep the TypeScript limits and error texts: a failing field is reported as
//! `"name: Invalid number"`, nested fields as `"create: mapMode: Invalid choice"`, and
//! a missing field reads as `undefined` would (only `optional` accepts it). String
//! lengths count UTF-16 code units like JavaScript's `length`.

use serde_json::{Map, Value};

pub type ReadResult<T> = Result<T, String>;
pub type Record = Map<String, Value>;

/// `Number.MAX_SAFE_INTEGER`.
pub const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;
/// Largest magnitude of a replicated number (the TypeScript schema's `number()`).
pub const NUMBER_BOUND: f64 = 1e9;

pub fn record(value: &Value) -> ReadResult<&Record> {
    value
        .as_object()
        .ok_or_else(|| "Expected object".to_string())
}

/// Parses message text into an object, as `record(JSON.parse(text))` would.
pub fn parse_record(text: &str) -> ReadResult<Record> {
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(map)) => Ok(map),
        Ok(_) => Err("Expected object".into()),
        Err(_) => Err("Invalid JSON".into()),
    }
}

pub fn number_in(value: Option<&Value>, min: f64, max: f64, integer: bool) -> ReadResult<f64> {
    match value.and_then(Value::as_f64) {
        Some(number)
            if number.is_finite()
                && number >= min
                && number <= max
                && (!integer || (number.trunc() == number && number.abs() <= MAX_SAFE_INTEGER)) =>
        {
            Ok(number)
        }
        _ => Err("Invalid number".into()),
    }
}

/// `id`: a safe non-negative integer.
pub fn id(value: Option<&Value>) -> ReadResult<u64> {
    number_in(value, 0.0, MAX_SAFE_INTEGER, true).map(|number| number as u64)
}

/// An `id` that must also fit the simulation's 32-bit ids and counters.
pub fn id32(value: Option<&Value>) -> ReadResult<u32> {
    let number = id(value)?;
    u32::try_from(number).map_err(|_| "Invalid number".to_string())
}

pub fn boolean(value: Option<&Value>) -> ReadResult<bool> {
    value
        .and_then(Value::as_bool)
        .ok_or_else(|| "Invalid boolean".into())
}

/// UTF-16 length, as JavaScript counts it.
pub fn text_length(text: &str) -> usize {
    text.encode_utf16().count()
}

pub fn string(value: Option<&Value>, max: usize, min: usize) -> ReadResult<String> {
    match value.and_then(Value::as_str) {
        Some(text) if (min..=max).contains(&text_length(text)) => Ok(text.to_string()),
        _ => Err("Invalid text".into()),
    }
}

/// `enumeration(...)` over strings: the matching option's value.
pub fn choice<T: Copy>(value: Option<&Value>, options: &[(&str, T)]) -> ReadResult<T> {
    value
        .and_then(Value::as_str)
        .and_then(|text| options.iter().find(|(name, _)| *name == text))
        .map(|(_, option)| *option)
        .ok_or_else(|| "Invalid choice".into())
}

/// `optional(reader)`: `None` when the field is absent. `null` is not absent.
pub fn optional<T>(
    value: Option<&Value>,
    read: impl FnOnce(Option<&Value>) -> ReadResult<T>,
) -> ReadResult<Option<T>> {
    match value {
        None => Ok(None),
        some => read(some).map(Some),
    }
}

/// `array(reader, max)`.
pub fn array<T>(
    value: Option<&Value>,
    max: usize,
    mut read: impl FnMut(&Value) -> ReadResult<T>,
) -> ReadResult<Vec<T>> {
    match value {
        Some(Value::Array(items)) if items.len() <= max => items.iter().map(&mut read).collect(),
        _ => Err("Invalid list".into()),
    }
}

/// Reads one field of an object, prefixing a failure with the field name like `object()`.
pub fn field<T>(
    source: &Record,
    key: &str,
    read: impl FnOnce(Option<&Value>) -> ReadResult<T>,
) -> ReadResult<T> {
    read(source.get(key)).map_err(|error| format!("{key}: {error}"))
}

/// A nested `object(...)` field: the value must be an object, read by `read`.
pub fn nested<T>(
    value: Option<&Value>,
    read: impl FnOnce(&Record) -> ReadResult<T>,
) -> ReadResult<T> {
    match value {
        Some(Value::Object(map)) => read(map),
        _ => Err("Expected object".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn readers_keep_the_typescript_limits_and_messages() {
        let number = |value: Option<&Value>| number_in(value, -NUMBER_BOUND, NUMBER_BOUND, false);
        let message = json!({ "a": 1.5, "b": "héllo", "c": null, "d": [1, 2, 3] });
        let map = record(&message).unwrap();
        assert_eq!(field(map, "a", number), Ok(1.5));
        assert_eq!(field(map, "a", id), Err("a: Invalid number".into()));
        assert_eq!(field(map, "b", |v| string(v, 5, 1)), Ok("héllo".into()));
        assert_eq!(
            field(map, "b", |v| string(v, 4, 1)),
            Err("b: Invalid text".into())
        );
        assert_eq!(
            field(map, "c", |v| optional(v, number)),
            Err("c: Invalid number".into())
        );
        assert_eq!(field(map, "missing", |v| optional(v, number)), Ok(None));
        assert_eq!(
            field(map, "d", |v| array(v, 2, |item| id(Some(item)))),
            Err("d: Invalid list".into())
        );
        assert_eq!(
            id(Some(&json!(9_007_199_254_740_992u64))),
            Err("Invalid number".into())
        );
        assert_eq!(number(Some(&json!(1e10))), Err("Invalid number".into()));
        assert_eq!(boolean(None), Err("Invalid boolean".into()));
        assert_eq!(
            choice(Some(&json!("x")), &[("y", 1)]),
            Err("Invalid choice".into())
        );
        assert_eq!(text_length("😀"), 2, "UTF-16 code units");
    }
}
