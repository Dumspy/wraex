//! `ref/apt/renderer/src/parser/modifiers.ts`: modifier types and stats summed
//! per modifier type.

use serde::Serialize;

use super::advanced_mod_desc::{ParsedModifier, apply_incr};
use super::stat_translations::ParsedStat;
use crate::data::{Stat, StatMatcher};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StatCalculated {
    pub stat: Stat,
    #[serde(rename = "type")]
    pub kind: ModifierType,
    pub sources: Vec<StatSource>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StatSource {
    pub modifier: ParsedModifier,
    pub stat: ParsedStat,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contributes: Option<StatRoll>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct StatRoll {
    pub value: f64,
    pub min: f64,
    pub max: f64,
}

/// `sumStatsByModType`: one entry per stat and modifier type, with every
/// modifier of that type contributing to it.
pub fn sum_stats_by_mod_type(mods: &[ParsedModifier]) -> Vec<StatCalculated> {
    let mut out: Vec<StatCalculated> = Vec::new();
    for mod_a in mods {
        for stat_a in &mod_a.stats {
            if out.iter().any(|m| m.stat.stat_ref == stat_a.stat.stat_ref && m.kind == mod_a.info.kind) {
                continue;
            }
            let mut sources = Vec::new();
            for mod_b in mods {
                if mod_b.info.kind != mod_a.info.kind {
                    continue;
                }
                let Some(target) = mod_b.stats.iter().find(|s| s.stat.stat_ref == stat_a.stat.stat_ref) else {
                    continue;
                };
                let incr = apply_incr(&mod_b.info, target);
                let roll = incr.as_ref().unwrap_or(target).roll.as_ref();
                sources.push(StatSource {
                    modifier: mod_b.clone(),
                    stat: target.clone(),
                    contributes: roll.map(|r| StatRoll { value: r.value, min: r.min, max: r.max }),
                });
            }
            out.push(StatCalculated { stat: stat_a.stat.clone(), kind: mod_a.info.kind, sources });
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TotalMode {
    Sum,
    Max,
}

/// `statSourcesTotal`. Sources without a roll count as 1 ("flag" semantics).
pub fn stat_sources_total(sources: &[StatSource], mode: TotalMode) -> Option<StatRoll> {
    if sources.iter().all(|s| s.contributes.is_none()) {
        return None;
    }
    if sources.len() == 1 {
        return sources[0].contributes;
    }
    let f = |a: f64, b: f64| match mode {
        TotalMode::Sum => a + b,
        // `Math.max` propagates NaN.
        TotalMode::Max if a.is_nan() || b.is_nan() => f64::NAN,
        TotalMode::Max => a.max(b),
    };
    let mut sum = StatRoll { value: 0.0, min: 0.0, max: 0.0 };
    for source in sources {
        let c = source.contributes.unwrap_or(StatRoll { value: 1.0, min: 1.0, max: 1.0 });
        sum.value = f(sum.value, c.value);
        sum.min = f(sum.min, c.min);
        sum.max = f(sum.max, c.max);
    }
    Some(sum)
}

/// `translateStatWithRoll`'s result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatTranslation {
    pub string: String,
    pub negate: bool,
    /// `undefined` without a roll.
    pub dp: Option<bool>,
}

/// `translateStatWithRoll`: the translation to display a summed stat with.
pub fn translate_stat_with_roll(calc: &StatCalculated, roll: Option<&StatRoll>) -> StatTranslation {
    let matchers = &calc.stat.matchers;
    let translation: &StatMatcher = match roll {
        None => matchers.iter().find(|m| m.value.is_none()).or(matchers.first()).expect("stats have matchers"),
        Some(roll) => matchers
            .iter()
            .find(|m| m.value == Some(roll.value))
            .or_else(|| {
                // TODO (upstream): for some stats reduced is better (m.negate === true)
                let same_sign = js_sign(roll.min) == js_sign(roll.max);
                if same_sign {
                    matchers.iter().find(|m| m.value.is_none() && m.negate.unwrap_or(false) == (roll.value < 0.0))
                } else {
                    matchers.iter().find(|m| m.value.is_none() && !m.negate.unwrap_or(false))
                }
            })
            .or_else(|| matchers.iter().find(|m| m.value.is_none()))
            .expect("never"),
    };
    let dp = roll.map(|_| {
        calc.stat.dp == Some(true)
            || calc
                .sources
                .iter()
                .any(|s| s.stat.stat.stat_ref == calc.stat.stat_ref && s.stat.roll.as_ref().is_some_and(|r| r.dp))
    });
    StatTranslation { string: translation.string.clone(), negate: translation.negate.unwrap_or(false), dp }
}

/// `Math.sign`, compared with `===` (so `NaN` never equals).
fn js_sign(x: f64) -> Option<i8> {
    if x.is_nan() {
        None
    } else if x > 0.0 {
        Some(1)
    } else if x < 0.0 {
        Some(-1)
    } else {
        Some(0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ModifierType {
    Pseudo,
    Explicit,
    Implicit,
    Crafted,
    Enchant,
    Scourge,
    Necropolis,
    Veiled,
    Fractured,
    Imbued,
}

impl ModifierType {
    /// The value, which is also the key in `stat.trade.ids`.
    pub const fn as_str(self) -> &'static str {
        match self {
            ModifierType::Pseudo => "pseudo",
            ModifierType::Explicit => "explicit",
            ModifierType::Implicit => "implicit",
            ModifierType::Crafted => "crafted",
            ModifierType::Enchant => "enchant",
            ModifierType::Scourge => "scourge",
            ModifierType::Necropolis => "necropolis",
            ModifierType::Veiled => "veiled",
            ModifierType::Fractured => "fractured",
            ModifierType::Imbued => "imbued",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum ModifierMechanic {
    #[serde(rename = "synthesised")]
    Synthesised,
    #[serde(rename = "corrupted")]
    Corruption,
    #[serde(rename = "eldritch")]
    Eldritch,
    #[serde(rename = "foulborn")]
    Foulborn,
    #[serde(rename = "vestigial")]
    Vestigial,
    #[serde(rename = "explicit-rareonly")]
    RareOnly,
    #[serde(rename = "explicit-shaper")]
    Shaper,
    #[serde(rename = "explicit-elder")]
    Elder,
    #[serde(rename = "explicit-crusader")]
    Crusader,
    #[serde(rename = "explicit-hunter")]
    Hunter,
    #[serde(rename = "explicit-redeemer")]
    Redeemer,
    #[serde(rename = "explicit-warlord")]
    Warlord,
    #[serde(rename = "explicit-delve")]
    Delve,
    #[serde(rename = "explicit-veiled")]
    Unveiled,
    #[serde(rename = "explicit-incursion")]
    Incursion,
    #[serde(rename = "explicit-infamous")]
    Infamous,
    #[serde(rename = "explicit-essence")]
    Essence,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(roll: Option<f64>) -> StatSource {
        let stat: Stat =
            serde_json::from_str(r#"{"ref":"x","better":1,"matchers":[{"string":"x"}],"trade":{"ids":{}}}"#).unwrap();
        let parsed = ParsedStat {
            stat,
            translation: StatMatcher { string: "x".into(), advanced: None, negate: None, value: None },
            roll: None,
        };
        StatSource {
            modifier: ParsedModifier {
                info: super::super::advanced_mod_desc::ModifierInfo::new(ModifierType::Explicit),
                stats: vec![],
            },
            stat: parsed,
            contributes: roll.map(|v| StatRoll { value: v, min: v, max: v }),
        }
    }

    #[test]
    fn totals_sum_or_max_and_count_flags_as_one() {
        assert_eq!(stat_sources_total(&[source(None)], TotalMode::Sum), None);
        assert_eq!(stat_sources_total(&[source(Some(4.0))], TotalMode::Sum).unwrap().value, 4.0);
        assert_eq!(stat_sources_total(&[source(Some(4.0)), source(None)], TotalMode::Sum).unwrap().value, 5.0);
        assert_eq!(stat_sources_total(&[source(Some(4.0)), source(Some(2.0))], TotalMode::Max).unwrap().value, 4.0);
    }
}
