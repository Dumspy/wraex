//! Numeric conversions with no lossless `From`/`TryFrom` in std.
//!
//! The workspace denies `as` (`clippy::as_conversions`), so the few lossy
//! conversions the ports need live here, each one a plain `as` with the
//! semantics spelled out. Integer-to-integer conversions do not belong here:
//! use `From`/`TryFrom` where they happen.

/// `x as f32`: rounds to the nearest `f32`, as the UI's `f32` geometry expects.
#[expect(clippy::as_conversions, clippy::cast_possible_truncation, reason = "nearest f32 is the intended rounding")]
pub const fn f64_to_f32(x: f64) -> f32 {
    x as f32
}

/// `n as f64`: exact up to 2^53, which every length and count here is below.
#[expect(clippy::as_conversions, clippy::cast_precision_loss, reason = "lengths and counts stay below 2^53")]
pub const fn usize_to_f64(n: usize) -> f64 {
    n as f64
}

/// `n as f32`: exact up to 2^24, which every UI count and index is below.
#[expect(clippy::as_conversions, clippy::cast_precision_loss, reason = "UI counts and indexes stay below 2^24")]
pub const fn usize_to_f32(n: usize) -> f32 {
    n as f32
}

/// `n as f64`: exact up to 2^53, which covers epoch milliseconds and durations.
#[expect(clippy::as_conversions, clippy::cast_precision_loss, reason = "timestamps and durations stay below 2^53")]
pub const fn i64_to_f64(n: i64) -> f64 {
    n as f64
}

/// `n as f64`: exact up to 2^53, which covers epoch milliseconds and durations.
#[expect(clippy::as_conversions, clippy::cast_precision_loss, reason = "timestamps and durations stay below 2^53")]
pub const fn u64_to_f64(n: u64) -> f64 {
    n as f64
}

/// `x as i64`: truncates toward zero, saturates at the bounds, NaN becomes 0.
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    reason = "Rust's saturating float cast is the intended semantics"
)]
pub const fn f64_to_i64(x: f64) -> i64 {
    x as i64
}

/// `x as u64`: truncates toward zero, saturates at the bounds, NaN and negatives become 0.
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "Rust's saturating float cast is the intended semantics"
)]
pub const fn f64_to_u64(x: f64) -> u64 {
    x as u64
}

/// `x as i32`: truncates toward zero, saturates at the bounds, NaN becomes 0.
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    reason = "Rust's saturating float cast is the intended semantics"
)]
pub const fn f64_to_i32(x: f64) -> i32 {
    x as i32
}

/// `x as u32`: truncates toward zero, saturates at the bounds, NaN and negatives become 0.
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "Rust's saturating float cast is the intended semantics"
)]
pub const fn f64_to_u32(x: f64) -> u32 {
    x as u32
}

/// `x as usize`: truncates toward zero, saturates at the bounds, NaN and negatives become 0.
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "Rust's saturating float cast is the intended semantics"
)]
pub const fn f64_to_usize(x: f64) -> usize {
    x as usize
}

/// `x as u8`: truncates toward zero, saturates at 0 and 255, NaN becomes 0
/// (colour channels).
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "Rust's saturating float cast is the intended semantics"
)]
pub const fn f32_to_u8(x: f32) -> u8 {
    x as u8
}

/// `x as i32`: truncates toward zero, saturates at the bounds, NaN becomes 0.
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    reason = "Rust's saturating float cast is the intended semantics"
)]
pub const fn f32_to_i32(x: f32) -> i32 {
    x as i32
}

/// `x as u32`: truncates toward zero, saturates at the bounds, NaN and negatives become 0.
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "Rust's saturating float cast is the intended semantics"
)]
pub const fn f32_to_u32(x: f32) -> u32 {
    x as u32
}

/// `x as usize`: truncates toward zero, saturates at the bounds, NaN and negatives become 0.
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "Rust's saturating float cast is the intended semantics"
)]
pub const fn f32_to_usize(x: f32) -> usize {
    x as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float_to_int_keeps_as_semantics() {
        assert_eq!(f64_to_i64(-2.9), -2);
        assert_eq!(f64_to_i64(f64::NAN), 0);
        assert_eq!(f64_to_i64(1e300), i64::MAX);
        assert_eq!(f64_to_usize(-1.0), 0);
        assert_eq!(f64_to_u32(4.2e9), 4_200_000_000);
        assert_eq!(f32_to_u8(300.0), 255);
        assert_eq!(f32_to_u8(-1.0), 0);
    }

    #[test]
    fn int_to_float_is_exact_in_range() {
        assert_eq!(usize_to_f64(1 << 53), 9_007_199_254_740_992.0);
        assert_eq!(i64_to_f64(-1_700_000_000_000), -1_700_000_000_000.0);
        assert_eq!(f64_to_f32(0.1), 0.1_f32);
    }
}
