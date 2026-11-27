//! Filter helpers both references share: `web/price-check/filters/util.ts`, and
//! `floorToBracket`/`ceilToBracket` from `create-item-filters.ts`.

/// `Math.floor` or `Math.ceil`, as `percentRoll` takes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rounding {
    Floor,
    Ceil,
}

impl Rounding {
    const fn apply(self, x: f64) -> f64 {
        match self {
            Self::Floor => x.floor(),
            Self::Ceil => x.ceil(),
        }
    }
}

/// `decimalPlaces` with a boolean `dp`: none for integers and values of 10 or
/// more, two below 2.3, otherwise one.
fn decimal_places(value: f64, dp: bool) -> i32 {
    if !dp || value.abs() >= 10.0 {
        0
    } else if value.abs() < 2.3 {
        2
    } else {
        1
    }
}

/// `roundRoll`: truncates to the roll's decimal places.
pub fn round_roll(value: f64, dp: bool) -> f64 {
    let rounding = 10f64.powi(decimal_places(value, dp));
    (value * rounding).trunc() / rounding
}

/// `percentRoll`: `value` moved by `p` percent of itself, rounded with `method`.
pub fn percent_roll(value: f64, p: f64, method: Rounding, dp: bool) -> f64 {
    let res = value + value.abs() * p / 100.0;
    let rounding = 10f64.powi(decimal_places(value, dp));
    method.apply((res + f64::EPSILON) * rounding) / rounding
}

/// `percentRollDelta`: `value` moved by `p` percent of `delta`.
pub fn percent_roll_delta(value: f64, delta: f64, p: f64, method: Rounding, dp: bool) -> f64 {
    let res = value + delta * p / 100.0;
    let rounding = 10f64.powi(decimal_places(value, dp));
    method.apply((res + f64::EPSILON) * rounding) / rounding
}

/// `floorToBracket`: the largest bracket not above `value`, or the first.
pub fn floor_to_bracket(value: f64, brackets: &[f64]) -> f64 {
    // Callers pass a fixed, non-empty ladder; an empty one returns `value`.
    let Some((&first, rest)) = brackets.split_first() else {
        return value;
    };
    let mut prev = first;
    for &num in rest {
        if num > value {
            return prev;
        }
        prev = num;
    }
    prev
}

/// `ceilToBracket`: brackets descend; the smallest not below `value`.
pub fn ceil_to_bracket(value: f64, brackets: &[f64]) -> f64 {
    // Callers pass a fixed, non-empty ladder; an empty one returns `value`.
    let Some((&first, rest)) = brackets.split_first() else {
        return value;
    };
    let mut prev = first;
    for &num in rest {
        if num < value {
            return prev;
        }
        prev = num;
    }
    prev
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rolls_round_by_magnitude() {
        assert_eq!(round_roll(1.239, true), 1.23);
        assert_eq!(round_roll(5.67, true), 5.6);
        assert_eq!(round_roll(12.9, true), 12.0);
        assert_eq!(round_roll(5.67, false), 5.0);
        assert_eq!(percent_roll(50.0, -10.0, Rounding::Floor, false), 45.0);
        assert_eq!(percent_roll(50.0, 10.0, Rounding::Ceil, false), 55.0);
        assert_eq!(percent_roll_delta(20.0, 10.0, -10.0, Rounding::Floor, false), 19.0);
    }

    #[test]
    fn brackets() {
        let floor = [1.0, 50.0, 68.0, 75.0, 84.0];
        assert_eq!(floor_to_bracket(70.0, &floor), 68.0);
        assert_eq!(floor_to_bracket(90.0, &floor), 84.0);
        let ceil = [100.0, 74.0, 67.0, 49.0];
        assert_eq!(ceil_to_bracket(70.0, &ceil), 74.0);
        assert_eq!(ceil_to_bracket(30.0, &ceil), 49.0);
        assert_eq!(ceil_to_bracket(84.0, &ceil), 100.0);
    }
}
