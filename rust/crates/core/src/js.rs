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

/// `Number.prototype.toString()` (and `String(x)`, template literals): the
/// shortest round-trip digits, in exponent form below 1e-6 and from 1e21.
pub fn number_to_string(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    if x == 0.0 {
        return "0".into();
    }
    // `{:e}` gives the shortest round-trip digits: "1.2345e3".
    let sci = format!("{:e}", x.abs());
    let (mantissa, exp) = sci.split_once('e').expect("`{:e}` has an exponent");
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let k = digits.len() as i32;
    let n = exp.parse::<i32>().expect("integer exponent") + 1;
    let body = if k <= n && n <= 21 {
        format!("{digits}{}", "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
    } else if -6 < n && n <= 0 {
        format!("0.{}{digits}", "0".repeat((-n) as usize))
    } else {
        let e = n - 1;
        let sign = if e < 0 { '-' } else { '+' };
        let rest = if k > 1 { format!(".{}", &digits[1..]) } else { String::new() };
        format!("{}{rest}e{sign}{}", &digits[..1], e.abs())
    };
    if x < 0.0 { format!("-{body}") } else { body }
}

/// `encodeURIComponent`: UTF-8 percent-encoding of everything but
/// `A-Z a-z 0-9 - _ . ! ~ * ' ( )`, with upper-case hex.
pub fn encode_uri_component(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// `x.toFixed(digits)`: rounds the exact binary value, ties away from zero
/// ("pick the larger n"), unlike Rust's ties-to-even formatting.
pub fn to_fixed(x: f64, digits: usize) -> String {
    if !x.is_finite() {
        return number_to_string(x);
    }
    if x.abs() >= 1e21 {
        return number_to_string(x);
    }
    // The exact decimal expansion: an f64 has at most 1074 fractional digits.
    let exact = format!("{:.1100}", x.abs());
    let (int_part, frac) = exact.split_once('.').expect("a fraction was asked for");
    let mut digits_vec: Vec<u8> = int_part.bytes().chain(frac.bytes().take(digits)).map(|b| b - b'0').collect();
    let rest = &frac[digits..];
    let round_up = rest.as_bytes().first().is_some_and(|&b| b >= b'5');
    if round_up {
        let mut i = digits_vec.len();
        loop {
            if i == 0 {
                digits_vec.insert(0, 1);
                break;
            }
            i -= 1;
            if digits_vec[i] == 9 {
                digits_vec[i] = 0;
            } else {
                digits_vec[i] += 1;
                break;
            }
        }
    }
    let text: String = digits_vec.iter().map(|d| char::from(b'0' + d)).collect();
    let (int_digits, frac_digits) = text.split_at(text.len() - digits);
    let body = if digits == 0 { int_digits.to_owned() } else { format!("{int_digits}.{frac_digits}") };
    // `(-0.04).toFixed(1)` is `"-0.0"`: the sign stays.
    if x.is_sign_negative() && x != 0.0 { format!("-{body}") } else { body }
}

/// `text.normalize('NFKD')`
pub fn normalize_nfkd(text: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    text.nfkd().collect()
}

/// `JSON.stringify(text)` for a string: quoted, with JSON escapes.
pub fn json_string(text: &str) -> String {
    serde_json::to_string(text).expect("strings serialise")
}

/// `JSON.stringify(value)`: compact JSON, keys in the order the value built them,
/// numbers as JavaScript writes them (integral doubles without a fraction). This is
/// what the trade site receives from the reference, and the screenshot recordings
/// key on a hash of those exact bytes.
pub fn stringify<T: Serialize + ?Sized>(value: &T) -> String {
    let value = json_value(value);
    serde_json::to_string(&value).expect("JSON values always serialise")
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
    fn to_fixed_rounds_ties_up() {
        assert_eq!(to_fixed(0.25, 1), "0.3");
        assert_eq!(to_fixed(0.35, 1), "0.3"); // 0.34999... in binary
        assert_eq!(to_fixed(9.96, 1), "10.0");
        assert_eq!(to_fixed(-1.25, 1), "-1.3");
        assert_eq!(to_fixed(2.0, 1), "2.0");
        assert_eq!(normalize_nfkd("é"), "e\u{301}");
    }

    #[test]
    fn uri_components_encode_like_javascript() {
        assert_eq!(encode_uri_component("Two-Stone Ring"), "Two-Stone%20Ring");
        assert_eq!(encode_uri_component("+14% (a)\n{x}"), "%2B14%25%20(a)%0A%7Bx%7D");
        assert_eq!(encode_uri_component("—'*~!"), "%E2%80%94'*~!");
        assert_eq!(json_string("\"a\" b"), r#""\"a\" b""#);
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
    fn numbers_print_like_javascript() {
        for (x, expected) in [
            (20.0, "20"),
            (1.5, "1.5"),
            (-0.0, "0"),
            (0.1 + 0.2, "0.30000000000000004"),
            (123456789.0, "123456789"),
            (1e21, "1e+21"),
            (1.5e21, "1.5e+21"),
            (1e20, "100000000000000000000"),
            (0.000001, "0.000001"),
            (1e-7, "1e-7"),
            (-2.5e-8, "-2.5e-8"),
            (f64::NAN, "NaN"),
            (f64::NEG_INFINITY, "-Infinity"),
        ] {
            assert_eq!(number_to_string(x), expected, "{x:?}");
        }
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
