//! `ref/ee2/renderer/src/parser/calc-q20.ts`: item properties at 20% quality, and
//! where a base roll sits in its range.

use super::item::Item;
use super::modifiers::{StatRoll, StatSource, TotalMode, stat_sources_total};

/// The stats that add to a property: flat first, then increased.
#[derive(Debug, Clone, Copy)]
pub struct PropStats {
    pub flat: &'static [&'static str],
    pub incr: &'static [&'static str],
}

/// `QUALITY_STATS`
pub mod quality_stats {
    use super::PropStats;

    pub const ARMOUR: PropStats = PropStats {
        flat: &["# to Armour"],
        incr: &[
            "#% increased Armour",
            "#% increased Armour and Energy Shield",
            "#% increased Armour and Evasion",
            "#% increased Armour, Evasion and Energy Shield",
        ],
    };
    pub const EVASION: PropStats = PropStats {
        flat: &["# to Evasion Rating"],
        incr: &[
            "#% increased Evasion Rating",
            "#% increased Armour and Evasion",
            "#% increased Evasion and Energy Shield",
            "#% increased Armour, Evasion and Energy Shield",
        ],
    };
    pub const ENERGY_SHIELD: PropStats = PropStats {
        flat: &["# to maximum Energy Shield"],
        incr: &[
            "#% increased Energy Shield",
            "#% increased Armour and Energy Shield",
            "#% increased Evasion and Energy Shield",
            "#% increased Armour, Evasion and Energy Shield",
        ],
    };
    pub const PHYSICAL_DAMAGE: PropStats =
        PropStats { flat: &["Adds # to # Physical Damage"], incr: &["#% increased Physical Damage"] };
    pub const WARD: PropStats = PropStats { flat: &["# to maximum Runic Ward"], incr: &["#% increased Runic Ward"] };

    pub const ALL: [PropStats; 5] = [ARMOUR, EVASION, ENERGY_SHIELD, PHYSICAL_DAMAGE, WARD];
}

/// `OTHER_PSEUDO_STATS`
pub mod other_pseudo_stats {
    use super::PropStats;

    pub const ATTACK_SPEED: PropStats = PropStats { flat: &[], incr: &["#% increased Attack Speed"] };
    pub const FIRE_DAMAGE: PropStats = PropStats { flat: &["Adds # to # Fire Damage"], incr: &[] };
    pub const COLD_DAMAGE: PropStats = PropStats { flat: &["Adds # to # Cold Damage"], incr: &[] };
    pub const LIGHTNING_DAMAGE: PropStats = PropStats { flat: &["Adds # to # Lightning Damage"], incr: &[] };
    pub const ELEMENTAL_DAMAGE: PropStats = PropStats {
        flat: &["Adds # to # Fire Damage", "Adds # to # Cold Damage", "Adds # to # Lightning Damage"],
        incr: &[],
    };
    /// The flat ref is not passed to `stat()` upstream, so it is not validated.
    pub const CRIT_CHANCE: PropStats =
        PropStats { flat: &["#% to Critical Hit Chance"], incr: &["#% increased Critical Hit Chance"] };
    pub const RELOAD_SPEED: PropStats = PropStats { flat: &[], incr: &["#% increased Attack Speed"] };
    pub const SPIRIT: PropStats = PropStats { flat: &["# to Spirit"], incr: &["#% increased Spirit"] };
    pub const BLOCK: PropStats = PropStats { flat: &[], incr: &["#% increased Block chance"] };
}

/// Every ref this module passes to `stat()`, for [`crate::data::Data::check_stat_refs`].
pub fn stat_refs() -> impl Iterator<Item = &'static str> {
    use other_pseudo_stats as o;
    let others = [
        o::ATTACK_SPEED,
        o::FIRE_DAMAGE,
        o::COLD_DAMAGE,
        o::LIGHTNING_DAMAGE,
        o::ELEMENTAL_DAMAGE,
        o::RELOAD_SPEED,
        o::SPIRIT,
        o::BLOCK,
    ];
    quality_stats::ALL
        .into_iter()
        .chain(others)
        .flat_map(|p| p.flat.iter().chain(p.incr))
        .chain(o::CRIT_CHANCE.incr)
        .copied()
}

fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() { f64::NAN } else { a.max(b) }
}

fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() { f64::NAN } else { a.min(b) }
}

/// `Math.round`: halves round towards +∞.
fn js_round(x: f64) -> f64 {
    if !x.is_finite() || x.fract() == 0.0 {
        return x;
    }
    let floor = x.floor();
    if x - floor >= 0.5 { floor + 1.0 } else { floor }
}

/// A property roll and the sources behind it.
#[derive(Debug, Clone, PartialEq)]
pub struct PropRoll {
    pub roll: StatRoll,
    pub sources: Vec<StatSource>,
}

/// `propAt20Quality`
pub fn prop_at_20_quality(total: f64, stats: PropStats, item: &Item) -> PropRoll {
    let base = calc_prop_base(stats, item);
    // `calcFlat(total, incr, item.quality)`: an undefined quality takes the
    // parameter's default, 0.
    let flat_base = calc_flat(total, base.incr.value, item.quality.unwrap_or(0.0), false) - base.flat.value;
    let quality =
        if item.is_modifiable() { js_max(20.0, item.quality.unwrap_or(0.0)) } else { item.quality.unwrap_or(0.0) };
    PropRoll {
        roll: StatRoll {
            value: calc_increased(flat_base + base.flat.value, base.incr.value, quality, false),
            min: calc_increased(flat_base + base.flat.min, base.incr.min, quality, false),
            max: calc_increased(flat_base + base.flat.max, base.incr.max, quality, false),
            option: None,
        },
        sources: base.sources.into_iter().map(|s| StatSource { contributes: None, ..s }).collect(),
    }
}

/// `calcPropBounds`
pub fn calc_prop_bounds(total: f64, stats: PropStats, item: &Item, inverted: bool) -> PropRoll {
    let base = calc_prop_base(stats, item);
    let flat_base = calc_flat(total, base.incr.value, 0.0, inverted) - base.flat.value;
    let roll = StatRoll {
        value: calc_increased(flat_base + base.flat.value, base.incr.value, 0.0, inverted),
        min: calc_increased(flat_base + base.flat.min, base.incr.min, 0.0, inverted),
        max: calc_increased(flat_base + base.flat.max, base.incr.max, 0.0, inverted),
        option: None,
    };
    let sources = if !stats.incr.is_empty() && !stats.flat.is_empty() {
        base.sources.into_iter().map(|s| StatSource { contributes: None, ..s }).collect()
    } else if !stats.incr.is_empty() {
        base.sources
            .into_iter()
            .map(|s| {
                let c = s.contributes.expect("increased stats have rolls");
                StatSource {
                    contributes: Some(StatRoll {
                        value: flat_base * c.value / 100.0,
                        min: flat_base * c.min / 100.0,
                        max: flat_base * c.max / 100.0,
                        option: None,
                    }),
                    ..s
                }
            })
            .collect()
    } else {
        base.sources
    };
    PropRoll { roll, sources }
}

/// `calcPropPercentile`: where the base roll sits between `bounds`, 0 to 100.
pub fn calc_prop_percentile(total: f64, bounds: [f64; 2], stats: PropStats, item: &Item) -> f64 {
    let base = calc_prop_base(stats, item);
    let roll = calc_flat(total, base.incr.value, item.quality.unwrap_or(0.0), false) - base.flat.value;
    let [min, max] = bounds;
    let result = js_round(((roll - min) / (max - min)) * 100.0);
    js_min(js_max(result, 0.0), 100.0)
}

/// `calcPropBase`'s result.
#[derive(Debug, Clone)]
pub struct PropBase {
    pub incr: StatRoll,
    pub flat: StatRoll,
    pub sources: Vec<StatSource>,
}

/// `calcPropBase`
pub fn calc_prop_base(stats: PropStats, item: &Item) -> PropBase {
    let zero = StatRoll { value: 0.0, min: 0.0, max: 0.0, option: None };
    let (mut incr, mut flat, mut sources) = (zero, zero, Vec::new());
    for calc in &item.stats_by_type {
        let total = if stats.flat.contains(&calc.stat.stat_ref.as_str()) {
            &mut flat
        } else if stats.incr.contains(&calc.stat.stat_ref.as_str()) {
            &mut incr
        } else {
            continue;
        };
        // The reference asserts a roll (`!`); a stat without one reads as NaN.
        let nan = StatRoll { value: f64::NAN, min: f64::NAN, max: f64::NAN, option: None };
        let roll = stat_sources_total(&calc.sources, TotalMode::Sum).unwrap_or(nan);
        total.value += roll.value;
        total.min += roll.min;
        total.max += roll.max;
        sources.extend(calc.sources.iter().cloned());
    }
    PropBase { incr, flat, sources }
}

/// `calcFlat`. We cannot know the original roll if a percentage is -100: NaN.
pub fn calc_flat(total: f64, incr_pct: f64, more_pct: f64, inverted: bool) -> f64 {
    if inverted {
        return calc_increased(total, incr_pct, more_pct, false);
    }
    total / (1.0 + more_pct / 100.0) / (1.0 + incr_pct / 100.0)
}

/// `calcIncreased`
pub fn calc_increased(flat: f64, incr_pct: f64, more_pct: f64, inverted: bool) -> f64 {
    if inverted {
        return calc_flat(flat, incr_pct, more_pct, false);
    }
    flat * (1.0 + incr_pct / 100.0) * (1.0 + more_pct / 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::tests::en;

    #[test]
    fn stat_refs_exist() {
        en().check_stat_refs(stat_refs()).unwrap();
    }

    #[test]
    fn rounds_like_javascript() {
        assert_eq!(js_round(2.5), 3.0);
        assert_eq!(js_round(-2.5), -2.0);
        assert_eq!(js_round(2.4999999999999996), 2.0);
    }
}
