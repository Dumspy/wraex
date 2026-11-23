//! `ref/ee2/renderer/src/parser/modifiers.ts`: modifier types and stats summed
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub option: Option<f64>,
}

/// Several allocations of the same notable stay separate.
fn is_allocation(stat_ref: &str) -> bool {
    stat_ref == "Allocates #" || stat_ref == "Legacy of #"
}

/// `sumStatsByModType`: one entry per stat and (grouped) modifier type, with
/// every modifier of that type contributing to it.
pub fn sum_stats_by_mod_type(mods: &[ParsedModifier]) -> Vec<StatCalculated> {
    let mut out: Vec<StatCalculated> = Vec::new();
    for mod_a in mods {
        for stat_a in &mod_a.stats {
            let a_ref = &stat_a.stat.stat_ref;
            if out.iter().any(|merged| {
                &merged.stat.stat_ref == a_ref
                    && types_can_be_grouped(merged.kind, mod_a.info.kind)
                    && (!is_allocation(a_ref)
                        || merged.sources.iter().any(|src| src.stat.translation.string == stat_a.translation.string))
            }) {
                continue;
            }
            let mut has_fractured = false;
            let mut sources = Vec::new();
            for mod_b in mods {
                if !types_can_be_grouped(mod_b.info.kind, mod_a.info.kind) {
                    continue;
                }
                let target = mod_b.stats.iter().find(|stat_b| {
                    &stat_b.stat.stat_ref == a_ref
                        && (!is_allocation(a_ref) || stat_b.translation.string == stat_a.translation.string)
                });
                let Some(target) = target else { continue };
                if mod_b.info.kind == ModifierType::Fractured {
                    has_fractured = true;
                }
                let incr = apply_incr(&mod_b.info, target);
                let roll = incr.as_ref().unwrap_or(target).roll.as_ref();
                sources.push(StatSource {
                    modifier: mod_b.clone(),
                    stat: target.clone(),
                    contributes: roll.map(|r| StatRoll { value: r.value, min: r.min, max: r.max, option: r.option }),
                });
            }
            out.push(StatCalculated {
                stat: stat_a.stat.clone(),
                kind: if has_fractured { ModifierType::Fractured } else { mod_a.info.kind },
                sources,
            });
        }
    }
    out
}

/// `typesCanBeGrouped`: the same type, or two explicit-like types.
pub fn types_can_be_grouped(a: ModifierType, b: ModifierType) -> bool {
    a == b || (EXPLICIT_MOD_TYPES.contains(&a) && EXPLICIT_MOD_TYPES.contains(&b))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TotalMode {
    Sum,
    Max,
    Count,
}

/// `statSourcesTotal`. Sources without a roll count as 1 ("flag" semantics).
pub fn stat_sources_total(sources: &[StatSource], mode: TotalMode) -> Option<StatRoll> {
    if mode == TotalMode::Count {
        let count = sources.len() as f64;
        let option = if sources.is_empty() { None } else { sources[0].contributes.and_then(|c| c.option) };
        return Some(StatRoll { value: count, min: count, max: count, option });
    }
    if sources.len() == 1 {
        return sources[0].contributes;
    }
    let f = |a: f64, b: f64| match mode {
        TotalMode::Max if a.is_nan() || b.is_nan() => f64::NAN,
        TotalMode::Max => a.max(b),
        _ => a + b,
    };
    let mut sum = StatRoll { value: 0.0, min: 0.0, max: 0.0, option: None };
    for source in sources {
        let c = source.contributes.unwrap_or(StatRoll { value: 1.0, min: 1.0, max: 1.0, option: None });
        sum.value = f(sum.value, c.value);
        sum.min = f(sum.min, c.min);
        sum.max = f(sum.max, c.max);
        sum.option = c.option;
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
    let bug = StatMatcher {
        string: format!("BUG_STAT_ID: {}", calc.stat.stat_ref),
        advanced: None,
        negate: None,
        value: None,
        oils: None,
    };
    let translation: &StatMatcher = match roll {
        None => matchers.iter().find(|m| m.value.is_none()).or(matchers.first()).expect("stats have matchers"),
        Some(roll) => {
            let wanted = roll.option.filter(|o| wraex_core::js::truthy(*o)).unwrap_or(roll.value);
            matchers
                .iter()
                .find(|m| m.value == Some(wanted))
                .or_else(|| {
                    // TODO (upstream): for some stats reduced is better (m.negate === true)
                    if js_sign(roll.min) == js_sign(roll.max) {
                        matchers.iter().find(|m| m.value.is_none() && m.negate.unwrap_or(false) == (roll.value < 0.0))
                    } else {
                        matchers.iter().find(|m| m.value.is_none() && !m.negate.unwrap_or(false))
                    }
                })
                .or_else(|| matchers.iter().find(|m| m.value.is_none()))
                .unwrap_or(&bug)
        }
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

/// `modsEqual`
pub fn mods_equal(a: &ParsedModifier, b: &ParsedModifier) -> bool {
    a.stats.len() == b.stats.len()
        && a.info.name == b.info.name
        && a.info.tier == b.info.tier
        && a.info.generation == b.info.generation
        && a.info.kind == b.info.kind
        && a.stats.iter().zip(&b.stats).all(|(x, y)| {
            let (rx, ry) = (x.roll.as_ref(), y.roll.as_ref());
            x.stat.stat_ref == y.stat.stat_ref
                && rx.map(|r| r.min) == ry.map(|r| r.min)
                && rx.map(|r| r.max) == ry.map(|r| r.max)
                && rx.map(|r| r.value) == ry.map(|r| r.value)
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum ModifierType {
    #[serde(rename = "pseudo")]
    Pseudo,
    #[serde(rename = "explicit")]
    Explicit,
    #[serde(rename = "implicit")]
    Implicit,
    #[serde(rename = "crafted")]
    Crafted,
    #[serde(rename = "enchant")]
    Enchant,
    #[serde(rename = "scourge")]
    Scourge,
    #[serde(rename = "necropolis")]
    Necropolis,
    #[serde(rename = "veiled")]
    Veiled,
    #[serde(rename = "fractured")]
    Fractured,
    #[serde(rename = "rune")]
    Augment,
    #[serde(rename = "added-rune")]
    AddedAugment,
    #[serde(rename = "sanctum")]
    Sanctum,
    #[serde(rename = "desecrated")]
    Desecrated,
    #[serde(rename = "skill")]
    Skill,
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
            ModifierType::Augment => "rune",
            ModifierType::AddedAugment => "added-rune",
            ModifierType::Sanctum => "sanctum",
            ModifierType::Desecrated => "desecrated",
            ModifierType::Skill => "skill",
        }
    }
}

pub const EXPLICIT_MOD_TYPES: [ModifierType; 6] = [
    ModifierType::Explicit,
    ModifierType::Fractured,
    ModifierType::Veiled,
    ModifierType::Desecrated,
    ModifierType::Crafted,
    ModifierType::Sanctum,
];

pub const INTERNAL_AUGMENT_TYPES: [ModifierType; 2] = [ModifierType::Augment, ModifierType::AddedAugment];
