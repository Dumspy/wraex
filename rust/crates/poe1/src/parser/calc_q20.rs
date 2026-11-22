//! `ref/apt/renderer/src/parser/calc-q20.ts`: item properties at 20% quality, and
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
        flat: &["+# to Armour"],
        incr: &[
            "#% increased Armour",
            "#% increased Armour and Energy Shield",
            "#% increased Armour and Evasion",
            "#% increased Armour, Evasion and Energy Shield",
        ],
    };
    pub const EVASION: PropStats = PropStats {
        flat: &["+# to Evasion Rating"],
        incr: &[
            "#% increased Evasion Rating",
            "#% increased Armour and Evasion",
            "#% increased Evasion and Energy Shield",
            "#% increased Armour, Evasion and Energy Shield",
        ],
    };
    pub const ENERGY_SHIELD: PropStats = PropStats {
        flat: &["+# to maximum Energy Shield"],
        incr: &[
            "#% increased Energy Shield",
            "#% increased Armour and Energy Shield",
            "#% increased Evasion and Energy Shield",
            "#% increased Armour, Evasion and Energy Shield",
        ],
    };
    pub const WARD: PropStats = PropStats { flat: &["+# to Ward"], incr: &["#% increased Ward"] };
    pub const PHYSICAL_DAMAGE: PropStats =
        PropStats { flat: &["Adds # to # Physical Damage"], incr: &["#% increased Physical Damage"] };

    pub const ALL: [PropStats; 5] = [ARMOUR, EVASION, ENERGY_SHIELD, WARD, PHYSICAL_DAMAGE];
}

/// `QUALITY_CHANGING_ENCHANT`
pub const QUALITY_CHANGING_ENCHANT: [&str; 2] =
    ["Quality does not increase Defences", "Quality does not increase Physical Damage"];

/// Every ref this module passes to `stat()`, for [`crate::data::Data::check_stat_refs`].
pub fn stat_refs() -> impl Iterator<Item = &'static str> {
    quality_stats::ALL.into_iter().flat_map(|p| p.flat.iter().chain(p.incr)).copied().chain(QUALITY_CHANGING_ENCHANT)
}

fn has_quality_changing_enchant(item: &Item) -> bool {
    item.stats_by_type.iter().any(|c| QUALITY_CHANGING_ENCHANT.contains(&c.stat.stat_ref.as_str()))
}

/// `getTradeMaxQuality`: the trade site applies 20% to everything else,
/// mirrored items included.
pub fn get_trade_max_quality(item: &Item) -> f64 {
    if has_quality_changing_enchant(item) {
        return 0.0;
    }
    js_max(20.0, item.quality.unwrap_or(0.0))
}

/// `getPropQuality`
pub fn get_prop_quality(item: &Item) -> f64 {
    if has_quality_changing_enchant(item) {
        return 0.0;
    }
    item.quality.unwrap_or(0.0)
}

fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() { f64::NAN } else { a.max(b) }
}

fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() { f64::NAN } else { a.min(b) }
}

/// A property roll and the sources behind it.
#[derive(Debug, Clone, PartialEq)]
pub struct PropRoll {
    pub roll: StatRoll,
    pub sources: Vec<StatSource>,
}

/// `propAt20Quality`
pub fn prop_at_20_quality(total: f64, stats: PropStats, bounds: Option<[f64; 2]>, item: &Item) -> PropRoll {
    let base = calc_prop_base(stats, item);
    let flat_base = calc_flat(total, base.incr.value, get_prop_quality(item)) - base.flat.value;
    let [base_min, base_max] = bounds.unwrap_or([flat_base, flat_base]);
    let quality = get_trade_max_quality(item);
    PropRoll {
        roll: if !flat_base.is_nan() {
            StatRoll {
                value: calc_increased(flat_base + base.flat.value, base.incr.value, quality),
                min: calc_increased(base_min + base.flat.min, base.incr.min, quality),
                max: calc_increased(base_max + base.flat.max, base.incr.max, quality),
            }
        } else {
            StatRoll { value: 0.0, min: 0.0, max: 0.0 }
        },
        sources: base.sources.into_iter().map(|s| StatSource { contributes: None, ..s }).collect(),
    }
}

/// `calcPropBounds`
pub fn calc_prop_bounds(total: f64, stats: PropStats, item: &Item) -> PropRoll {
    let base = calc_prop_base(stats, item);
    let flat_base = calc_flat(total, base.incr.value, 0.0) - base.flat.value;
    let roll = StatRoll {
        value: calc_increased(flat_base + base.flat.value, base.incr.value, 0.0),
        min: calc_increased(flat_base + base.flat.min, base.incr.min, 0.0),
        max: calc_increased(flat_base + base.flat.max, base.incr.max, 0.0),
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
    let roll = calc_flat(total, base.incr.value, get_prop_quality(item)) - base.flat.value;
    let [min, max] = bounds;
    let result = js_round(((roll - min) / (max - min)) * 100.0);
    js_min(js_max(result, 0.0), 100.0)
}

/// `Math.round`: halves round towards +∞.
fn js_round(x: f64) -> f64 {
    if !x.is_finite() || x.fract() == 0.0 {
        return x;
    }
    let floor = x.floor();
    // `-0.4` rounds to `-0`, which prints as `0` anyway.
    if x - floor >= 0.5 { floor + 1.0 } else { floor }
}

struct PropBase {
    incr: StatRoll,
    flat: StatRoll,
    sources: Vec<StatSource>,
}

/// `calcPropBase`
fn calc_prop_base(stats: PropStats, item: &Item) -> PropBase {
    let zero = StatRoll { value: 0.0, min: 0.0, max: 0.0 };
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
        let roll = stat_sources_total(&calc.sources, TotalMode::Sum).unwrap_or(StatRoll {
            value: f64::NAN,
            min: f64::NAN,
            max: f64::NAN,
        });
        total.value += roll.value;
        total.min += roll.min;
        total.max += roll.max;
        sources.extend(calc.sources.iter().cloned());
    }
    PropBase { incr, flat, sources }
}

/// We cannot know the original roll if `more_pct` or `incr_pct` is -100: NaN.
fn calc_flat(total: f64, incr_pct: f64, more_pct: f64) -> f64 {
    total / (1.0 + more_pct / 100.0) / (1.0 + incr_pct / 100.0)
}

fn calc_increased(flat: f64, incr_pct: f64, more_pct: f64) -> f64 {
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
        assert_eq!(js_round(-0.4), -0.0);
        assert_eq!(js_round(2.4999999999999996), 2.0);
    }
}
