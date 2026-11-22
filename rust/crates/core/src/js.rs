//! JavaScript built-ins the ported code relies on, with their exact semantics:
//! `Number()`, `parseInt`, `parseFloat`, `String.prototype.trim`, truthiness, and
//! how `JSON.stringify` writes numbers.
//!
//! The references keep numbers as doubles, so the ports do too (`f64`), including
//! `NaN` where JavaScript produces it; `JSON.stringify` writes `NaN` as `null`.

use serde::Serialize;
use serde_json::Value;

/// `WhiteSpace` and `LineTerminator` from the ECMAScript spec: what `trim()`
/// strips and `Number()` ignores around the number.
pub fn is_whitespace(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'
    )
}

/// `String.prototype.trim`
pub fn trim(s: &str) -> &str {
    s.trim_matches(is_whitespace)
}

/// `String.prototype.trimStart`
pub fn trim_start(s: &str) -> &str {
    s.trim_start_matches(is_whitespace)
}

/// `String.prototype.trimEnd`
pub fn trim_end(s: &str) -> &str {
    s.trim_end_matches(is_whitespace)
}

/// `str.length`: UTF-16 code units.
pub fn length(s: &str) -> usize {
    s.encode_utf16().count()
}

/// A number's truthiness: `x || fallback` keeps `x` unless it is `0` or `NaN`.
pub fn truthy(x: f64) -> bool {
    x != 0.0 && !x.is_nan()
}

/// The longest prefix of `s` that is a `StrUnsignedDecimalLiteral`
/// (`1`, `1.5`, `.5`, `1.`, `1e5`), as a byte length.
fn decimal_prefix(s: &str) -> usize {
    let b = s.as_bytes();
    let digits = |mut i: usize| {
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        i
    };
    let int_end = digits(0);
    let mut end = int_end;
    let mut mantissa = int_end > 0;
    if end < b.len() && b[end] == b'.' {
        let frac_end = digits(end + 1);
        if frac_end > end + 1 || mantissa {
            mantissa = true;
            end = frac_end;
        }
    }
    if !mantissa {
        return 0;
    }
    if end < b.len() && (b[end] == b'e' || b[end] == b'E') {
        let mut i = end + 1;
        if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
            i += 1;
        }
        let exp_end = digits(i);
        if exp_end > i {
            end = exp_end;
        }
    }
    end
}

fn signed(s: &str) -> (f64, &str) {
    match s.as_bytes().first() {
        Some(b'-') => (-1.0, &s[1..]),
        Some(b'+') => (1.0, &s[1..]),
        _ => (1.0, s),
    }
}

/// `Number(s)` for a string.
pub fn number(s: &str) -> f64 {
    let s = trim(s);
    if s.is_empty() {
        return 0.0;
    }
    for (prefix, radix) in [("0x", 16), ("0X", 16), ("0o", 8), ("0O", 8), ("0b", 2), ("0B", 2)] {
        if let Some(rest) = s.strip_prefix(prefix) {
            if rest.is_empty() || !rest.chars().all(|c| c.is_digit(radix)) {
                return f64::NAN;
            }
            return rest.chars().fold(0.0, |acc, c| acc * f64::from(radix) + f64::from(c.to_digit(radix).unwrap()));
        }
    }
    let (sign, rest) = signed(s);
    if rest == "Infinity" {
        return sign * f64::INFINITY;
    }
    if decimal_prefix(rest) != rest.len() {
        return f64::NAN;
    }
    sign * rest.parse::<f64>().expect("validated decimal literal")
}

/// `parseFloat(s)`
pub fn parse_float(s: &str) -> f64 {
    let (sign, rest) = signed(trim_start(s));
    if rest.starts_with("Infinity") {
        return sign * f64::INFINITY;
    }
    match decimal_prefix(rest) {
        0 => f64::NAN,
        end => sign * rest[..end].parse::<f64>().expect("validated decimal literal"),
    }
}

/// `parseInt(s, 10)`
pub fn parse_int(s: &str) -> f64 {
    let (sign, rest) = signed(trim_start(s));
    let end = rest.bytes().position(|b| !b.is_ascii_digit()).unwrap_or(rest.len());
    if end == 0 {
        return f64::NAN;
    }
    // Same rounding as JavaScript for long digit strings: parse as a decimal.
    sign * rest[..end].parse::<f64>().expect("ASCII digits")
}

/// Serialises `value` and rewrites its numbers the way `JSON.stringify` writes
/// them: integral doubles without a fraction (`9`, not `9.0`), `-0` as `0`,
/// non-finite as `null`.
pub fn json_value<T: Serialize + ?Sized>(value: &T) -> Value {
    let mut value = serde_json::to_value(value).expect("ported values serialise");
    normalize_numbers(&mut value);
    value
}

fn normalize_numbers(value: &mut Value) {
    match value {
        Value::Number(n) if n.is_f64() => {
            let f = n.as_f64().unwrap();
            // Integral doubles below 1e21 print without exponent or fraction.
            if f.fract() == 0.0 && f.abs() < 9.007_199_254_740_992e15 {
                *value = Value::from(f as i64);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(normalize_numbers),
        Value::Object(map) => map.values_mut().for_each(normalize_numbers),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn same(a: f64, b: f64) -> bool {
        (a.is_nan() && b.is_nan()) || a == b
    }

    #[test]
    fn number_follows_the_spec() {
        for (input, expected) in [
            ("", 0.0),
            ("  ", 0.0),
            ("12", 12.0),
            (" 12\n", 12.0),
            ("+1.5", 1.5),
            ("-.5", -0.5),
            ("1.", 1.0),
            ("1e3", 1000.0),
            ("0x1F", 31.0),
            ("-Infinity", f64::NEG_INFINITY),
            ("12px", f64::NAN),
            ("1,5", f64::NAN),
            ("inf", f64::NAN),
            ("NaN", f64::NAN),
            (".", f64::NAN),
            ("-0x1", f64::NAN),
        ] {
            assert!(same(number(input), expected), "Number({input:?}) = {}", number(input));
        }
    }

    #[test]
    fn parse_int_and_parse_float_read_a_prefix() {
        assert_eq!(parse_int("+20% (augmented)"), 20.0);
        assert_eq!(parse_int(" -7 to 9"), -7.0);
        assert_eq!(parse_int("20 (Max)"), 20.0);
        assert!(parse_int("abc").is_nan());
        assert!(parse_int("+").is_nan());
        assert_eq!(parse_int("1.9"), 1.0);
        assert_eq!(parse_float("1.50 (augmented)"), 1.5);
        assert_eq!(parse_float("7.00%"), 7.0);
        assert_eq!(parse_float(".5e1x"), 5.0);
        assert_eq!(parse_float("1e"), 1.0);
        assert!(parse_float("e5").is_nan());
    }

    #[test]
    fn trims_javascript_whitespace() {
        assert_eq!(trim("\u{feff} a \u{a0}"), "a");
        // U+0085 is Unicode whitespace but not JavaScript's.
        assert_eq!(trim("\u{85}a"), "\u{85}a");
        assert_eq!(length("😀a"), 3);
    }

    #[test]
    fn json_numbers_print_like_json_stringify() {
        let v = json_value(&serde_json::json!({ "a": 9.0, "b": 1.5, "c": [-0.0, 3] }));
        assert_eq!(v.to_string(), r#"{"a":9,"b":1.5,"c":[0,3]}"#);
        assert_eq!(json_value(&f64::NAN), Value::Null);
    }
}
