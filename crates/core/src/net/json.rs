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
    } else if let Some(units) = wire_units(value) {
        write_wire_units(out, units);
    } else if value == value.trunc() && value.abs() < 1e15 {
        let _ = write!(out, "{}", value as i64);
    } else {
        let _ = write!(out, "{value}");
    }
}

/// The finest wire quantisation: rotations' ten-thousandths.
const WIRE_UNITS: f64 = ROTATION_SCALE;
/// Below this many units a double is exact to far less than one unit, so the decimal
/// of the units is the double's shortest spelling.
const MAX_WIRE_UNITS: f64 = 1e15;

/// `value` as whole ten-thousandths when it is exactly such a quotient, as every
/// wire-rounded number is.
fn wire_units(value: f64) -> Option<i64> {
    let units = (value * WIRE_UNITS).round();
    (units.abs() < MAX_WIRE_UNITS && units / WIRE_UNITS == value).then_some(units as i64)
}

/// Writes ten-thousandths as a decimal without trailing zeros, the spelling the
/// general shortest-float formatter would choose, without its cost.
fn write_wire_units(out: &mut String, units: i64) {
    if units < 0 {
        out.push('-');
    }
    let magnitude = units.unsigned_abs();
    let scale = WIRE_UNITS as u64;
    write_digits(out, magnitude / scale);
    let mut fraction = magnitude % scale;
    if fraction == 0 {
        return;
    }
    out.push('.');
    let mut place = scale / 10;
    while fraction > 0 {
        out.push(char::from(b'0' + (fraction / place) as u8));
        fraction %= place;
        place /= 10;
    }
}

fn write_digits(out: &mut String, mut value: u64) {
    let mut digits = [0u8; 20];
    let mut start = digits.len();
    loop {
        start -= 1;
        digits[start] = b'0' + (value % 10) as u8;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    out.push_str(std::str::from_utf8(&digits[start..]).expect("ASCII digits"));
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
    fn wire_numbers_print_like_the_general_formatter() {
        let general = |value: f64| {
            if value == value.trunc() {
                format!("{}", value as i64)
            } else {
                format!("{value}")
            }
        };
        let check = |value: f64| {
            let mut out = String::new();
            write_number(&mut out, value);
            assert_eq!(out, general(value), "{value:e}");
        };
        for units in -200_000..=200_000 {
            check(units as f64 / 10_000.0);
        }
        let mut random = crate::sim::math::Random::new(7.0);
        for _ in 0..200_000 {
            let magnitude = 10f64.powf(random.next() * 12.0 - 4.0);
            let raw = (random.next() - 0.5) * magnitude;
            for scale in [POSITION_SCALE, ROTATION_SCALE, VALUE_SCALE] {
                check(wire_round(raw, scale));
            }
            check(raw);
        }
        for value in [
            0.1 + 0.2,
            1e-7,
            123_456_789.123_45,
            99_999_999_999.999_9,
            -0.0,
        ] {
            check(value);
        }
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
