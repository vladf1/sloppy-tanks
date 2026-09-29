//! JSON text the way the former TypeScript wrote it with `JSON.stringify`.
//!
//! Messages keep the TypeScript key order and number spelling (`1`, not `1.0`), so
//! existing clients and the traffic bots read the same bytes they did from the Node host.

use crate::sim::math::js_round;

/// Wire quantisation (`PRECISION` in `scene-codec.ts`): positions in millimetres, rotations
/// and angles in ten-thousandths, timers and meters in hundredths.
pub const POSITION_SCALE: f64 = 1000.0;
pub const ROTATION_SCALE: f64 = 10000.0;
pub const VALUE_SCALE: f64 = 100.0;

/// `Math.round(value * scale) / scale || 0`: rounds half up and never produces `-0`.
///
/// # Panics
/// On a non-finite value, like the TypeScript `wireNumber`: the simulation state is corrupt.
pub fn wire_round(value: f64, scale: f64) -> f64 {
    assert!(value.is_finite(), "Non-finite wire number");
    let rounded = js_round(value * scale) / scale;
    if rounded == 0.0 { 0.0 } else { rounded }
}

pub fn position(value: f64) -> f64 {
    wire_round(value, POSITION_SCALE)
}

pub fn rotation(value: f64) -> f64 {
    wire_round(value, ROTATION_SCALE)
}

pub fn value(value: f64) -> f64 {
    wire_round(value, VALUE_SCALE)
}

/// Appends a number as JavaScript prints it: integers without a fraction, everything else
/// in the shortest form that reads back to the same double.
pub fn write_number(out: &mut String, value: f64) {
    use std::fmt::Write;
    if !value.is_finite() {
        // JSON.stringify writes null for NaN and infinities.
        out.push_str("null");
    } else if value == value.trunc() && value.abs() < 1e15 {
        let _ = write!(out, "{}", value as i64);
    } else {
        let _ = write!(out, "{value}");
    }
}

pub fn write_int(out: &mut String, value: u64) {
    use std::fmt::Write;
    let _ = write!(out, "{value}");
}

/// Appends a JSON string literal with `JSON.stringify` escaping.
pub fn write_str(out: &mut String, value: &str) {
    use std::fmt::Write;
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Writes one JSON object's fields in order into a borrowed buffer.
pub struct ObjectWriter<'a> {
    out: &'a mut String,
    first: bool,
}

impl<'a> ObjectWriter<'a> {
    pub fn new(out: &'a mut String) -> Self {
        out.push('{');
        Self { out, first: true }
    }

    /// Starts a field and returns the buffer to write its value into.
    pub fn key(&mut self, key: &str) -> &mut String {
        if !self.first {
            self.out.push(',');
        }
        self.first = false;
        write_str(self.out, key);
        self.out.push(':');
        self.out
    }

    pub fn number(&mut self, key: &str, value: f64) -> &mut Self {
        write_number(self.key(key), value);
        self
    }

    pub fn int(&mut self, key: &str, value: u64) -> &mut Self {
        write_int(self.key(key), value);
        self
    }

    pub fn string(&mut self, key: &str, value: &str) -> &mut Self {
        write_str(self.key(key), value);
        self
    }

    pub fn boolean(&mut self, key: &str, value: bool) -> &mut Self {
        self.key(key).push_str(if value { "true" } else { "false" });
        self
    }

    pub fn raw(&mut self, key: &str, json: &str) -> &mut Self {
        self.key(key).push_str(json);
        self
    }

    pub fn null(&mut self, key: &str) -> &mut Self {
        self.key(key).push_str("null");
        self
    }

    pub fn finish(self) {
        self.out.push('}');
    }
}

/// Builds one JSON object as an owned string.
pub fn object(build: impl FnOnce(&mut ObjectWriter<'_>)) -> String {
    let mut out = String::new();
    let mut writer = ObjectWriter::new(&mut out);
    build(&mut writer);
    writer.finish();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_print_like_javascript() {
        let print = |value: f64| {
            let mut out = String::new();
            write_number(&mut out, value);
            out
        };
        assert_eq!(print(1.0), "1");
        assert_eq!(print(-3.0), "-3");
        assert_eq!(print(0.1), "0.1");
        assert_eq!(print(-12.345), "-12.345");
        assert_eq!(print(0.0001), "0.0001");
        assert_eq!(print(1200.0), "1200");
        assert_eq!(print(f64::NAN), "null");
    }

    #[test]
    fn rounding_matches_the_typescript_wire_number() {
        assert_eq!(position(1.23456), 1.235);
        assert_eq!(position(-0.0001), 0.0);
        assert!(position(-0.0001).is_sign_positive());
        assert_eq!(rotation(0.123_456), 0.1235);
        assert_eq!(value(0.125), 0.13);
        assert_eq!(value(-0.125), -0.12, "half rounds toward +infinity");
    }

    #[test]
    fn strings_escape_like_json_stringify() {
        let mut out = String::new();
        write_str(&mut out, "a\"b\\c\n\u{1}é");
        assert_eq!(out, "\"a\\\"b\\\\c\\n\\u0001é\"");
    }
}
