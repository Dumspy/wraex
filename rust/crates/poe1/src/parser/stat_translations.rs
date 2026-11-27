//! `ref/apt/renderer/src/parser/stat-translations.ts`: matching stat text from an
//! item against the translations in `stats.ndjson`.

use std::sync::OnceLock;

use serde::Serialize;
use wraex_core::js;
use wraex_core::js_regex::Regex;
pub use wraex_core::parse::StatString;

use super::Fail;
use super::meta::{ARMOUR, HEIST_EQUIPMENT, ItemCategory, WEAPON};
use super::modifiers::ModifierType;
use crate::data::{Data, MergeKind, Stat, StatGroup, StatGroupResolver, StatMatcher, StatOrGroup};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ParsedStat {
    pub stat: Stat,
    pub translation: StatMatcher,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roll: Option<StatRollInfo>,
}

/// `ParsedStat['roll']`
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StatRollInfo {
    pub unscalable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation: Option<RollGeneration>,
    pub dp: bool,
    pub value: f64,
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RollGeneration {
    Legacy,
    Volatile,
    Reflecting,
}

/// Which of the found numbers stay as `#` placeholders, by count of numbers.
const PLACEHOLDER_MAP: [&[&[usize]]; 5] = [
    // 0 # -> max 0 #
    &[&[]],
    // 1 # -> max 1 #
    &[&[0], &[]],
    // 2 # -> max 2 #
    &[&[0, 1], &[0], &[1], &[]],
    // 3 # -> max 2 #
    &[&[0, 1, 2], &[1, 2], &[0, 2], &[0, 1], &[2], &[1], &[0]],
    // 4 # -> max 2 #
    &[
        &[0, 1, 2, 3],
        &[1, 2, 3],
        &[0, 2, 3],
        &[0, 1, 3],
        &[0, 1, 2],
        &[2, 3],
        &[1, 3],
        &[1, 2],
        &[0, 3],
        &[0, 2],
        &[0, 1],
    ],
];

#[derive(Debug, Clone)]
struct PlaceholderValue {
    roll: f64,
    roll_str: String,
    decimal: bool,
    bounds: Option<(f64, f64)>,
}

#[derive(Debug, Clone)]
struct Combination {
    stat: String,
    values: Vec<PlaceholderValue>,
}

#[expect(clippy::expect_used, reason = "a literal pattern, compiled by the tests below")]
fn value_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?<value>(?<!\d|\))[+-]?\d+(?:\.\d+)?)(?:\((?<min>.[^)-]*)(?:-(?<max>[^)]+))?\))?", "g")
            .expect("valid pattern")
    })
}

#[expect(clippy::expect_used, reason = "a literal pattern, compiled by the tests below")]
fn hash_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new("#", "gm").expect("valid pattern"))
}

/// `_statPlaceholderGenerator`: the stat text with every subset of its numbers
/// turned into `#`, most placeholders first, then the exact text.
fn stat_placeholder_combinations(stat: &str) -> Vec<Combination> {
    let mut matches: Vec<PlaceholderValue> = Vec::new();
    let with_placeholders = stat.replace("()", "").replace("(...)", "");
    let with_placeholders = value_regex().replace_all(&with_placeholders, |caps| {
        let Some(roll) = caps.get(1) else { return String::new() };
        let min = caps.get(2);
        // Sextant "# uses remaining", legacy rolls: `(min)` without a max.
        let max = if min.is_some() && caps.get(3).is_none() { min } else { caps.get(3) };
        let num = |s: Option<&str>| s.map_or(f64::NAN, js::number);
        let bounds = (num(min), num(max));
        let decimal =
            roll.contains('.') || min.is_some_and(|m| m.contains('.')) || max.is_some_and(|m| m.contains('.'));
        let numeric = !bounds.0.is_nan() && !bounds.1.is_nan();
        matches.push(PlaceholderValue {
            roll: js::number(roll),
            roll_str: roll.to_owned(),
            decimal,
            bounds: numeric.then_some(bounds),
        });
        match (numeric, min) {
            (true, _) | (false, None) => "#".to_owned(),
            (false, Some(min)) => format!("#({min}-{})", max.unwrap_or("undefined")),
        }
    });

    let mut out = Vec::new();
    if let Some(map) = PLACEHOLDER_MAP.get(matches.len()) {
        for replacements in *map {
            let mut seen = 0_usize;
            let replaced = hash_regex().replace_all(&with_placeholders, |_| {
                let i = seen;
                seen = seen.saturating_add(1);
                match matches.get(i) {
                    Some(m) if replacements.contains(&i) => m.roll_str.clone(),
                    _ => "#".to_owned(),
                }
            });
            let values =
                matches.iter().enumerate().filter(|(i, _)| !replacements.contains(i)).map(|(_, v)| v.clone()).collect();
            out.push(Combination { stat: replaced, values });
        }
    }
    // Fallback to the exact stat text, without any placeholders.
    out.push(Combination { stat: stat.to_owned(), values: Vec::new() });
    out
}

/// `tryParseTranslation`
pub fn try_parse_translation(
    d: &Data,
    stat: &StatString,
    mod_type: ModifierType,
    item_category: Option<&ItemCategory>,
) -> Result<Option<ParsedStat>, Fail> {
    for mut combination in stat_placeholder_combinations(&stat.string) {
        let roll = match combination.values.as_slice() {
            [value] => Some(value.roll),
            _ => None,
        };
        let found = find_and_resolve_translation(d, &combination.stat, mod_type, item_category, roll)?;
        let Some((found_stat, matcher)) = found else { continue };
        if !found_stat.trade.ids.contains_key(mod_type.as_str()) {
            continue;
        }

        // Modifiers must be upgraded to the new values with a Divine Orb.
        let mut legacy_stat_rolls = false;

        if matcher.negate == Some(true) {
            for v in &mut combination.values {
                v.roll *= -1.0;
                if let Some((min, max)) = &mut v.bounds {
                    *min *= -1.0;
                    *max *= -1.0;
                }
            }
        }

        if found_stat.stat_ref == "# uses remaining" {
            let uses = combination.values.first_mut().ok_or(Fail::THROW)?;
            uses.bounds = Some((1.0, uses.bounds.map_or(uses.roll, |b| b.1)));
        }

        for v in &mut combination.values {
            let Some((min, max)) = &mut v.bounds else { continue };
            if *min > *max {
                // Some stats granted by legacy modifiers (not legacy rolls) share
                // translations with new modifiers, with the positive and negative
                // strings swapped. Not a legacy roll.
                std::mem::swap(min, max);
            }
            if v.roll > *max {
                *max = v.roll;
                legacy_stat_rolls = true;
            }
            if v.roll < *min {
                *min = v.roll;
                legacy_stat_rolls = true;
            }
        }

        if let (true, Some(value)) = (combination.values.is_empty(), matcher.value.filter(|v| js::truthy(*v))) {
            combination.values = vec![PlaceholderValue {
                roll: value,
                roll_str: String::new(),
                decimal: false,
                bounds: Some((value, value)),
            }];
        }

        let values = &combination.values;
        let roll = (!values.is_empty()).then(|| StatRollInfo {
            unscalable: stat.unscalable,
            generation: legacy_stat_rolls.then_some(RollGeneration::Legacy),
            dp: found_stat.dp == Some(true) || values.iter().any(|v| v.decimal),
            value: get_roll_or_minmax_avg(&values.iter().map(|v| v.roll).collect::<Vec<_>>()),
            min: get_roll_or_minmax_avg(&values.iter().map(|v| v.bounds.map_or(v.roll, |b| b.0)).collect::<Vec<_>>()),
            max: get_roll_or_minmax_avg(&values.iter().map(|v| v.bounds.map_or(v.roll, |b| b.1)).collect::<Vec<_>>()),
        });
        return Ok(Some(ParsedStat { stat: found_stat, translation: matcher, roll }));
    }
    Ok(None)
}

/// `getRollOrMinmaxAvg`: the average of two values, otherwise the first.
pub fn get_roll_or_minmax_avg(values: &[f64]) -> f64 {
    match values {
        [a, b] => (a + b) / 2.0,
        _ => values.first().copied().unwrap_or(f64::NAN),
    }
}

fn has_matcher(stat: &Stat, match_str: &str) -> bool {
    stat.matchers.iter().any(|m| m.string == match_str || m.advanced.as_deref() == Some(match_str))
}

/// `findAndResolveTranslation`
fn find_and_resolve_translation(
    d: &Data,
    match_str: &str,
    mod_type: ModifierType,
    item_category: Option<&ItemCategory>,
    roll: Option<f64>,
) -> Result<Option<(Stat, StatMatcher)>, Fail> {
    let Some(stat_or_group) = d.stat_by_match_str_v2(match_str) else { return Ok(None) };
    let stat = match stat_or_group {
        StatOrGroup::Stat(stat) => Some(stat),
        StatOrGroup::Group(group) => resolve_translation(group, match_str, mod_type, item_category, roll)?,
    };
    let Some(stat) = stat else { return Ok(None) };
    let matcher =
        stat.matchers.iter().find(|m| m.string == match_str || m.advanced.as_deref() == Some(match_str)).cloned();
    Ok(matcher.map(|m| (stat, m)))
}

/// `_resolveTranslation`: picks (or merges) the stat of a group the text means.
pub(crate) fn resolve_translation(
    group: StatGroup,
    match_str: &str,
    mod_type: ModifierType,
    item_category: Option<&ItemCategory>,
    roll: Option<f64>,
) -> Result<Option<Stat>, Fail> {
    let StatGroup { resolve, mut stats } = group;
    let kind_index = |kind: &[MergeKind], k: MergeKind| {
        kind.iter().position(|x| *x == k).filter(|&i| i < stats.len()).ok_or(Fail::THROW)
    };

    if let StatGroupResolver::Select { test } = &resolve {
        // Give priority to an exact match, fall back to any match.
        let idx = test
            .iter()
            .position(|expected| expected.as_deref().is_some_and(|e| test_item_category(item_category, e)))
            .or_else(|| test.iter().position(Option::is_none));
        return Ok(idx.and_then(|i| stats.get(i).cloned()));
    }

    let on_trade: Vec<usize> = stats
        .iter()
        .enumerate()
        .filter(|(_, stat)| stat.trade.ids.contains_key(mod_type.as_str()))
        .map(|(i, _)| i)
        .collect();
    if let [index] = on_trade.as_slice() {
        return Ok(Some(stats.swap_remove(*index)));
    }

    match &resolve {
        // Handled above.
        StatGroupResolver::Select { .. } => Ok(None),
        StatGroupResolver::TrivialMerge => {
            let with_match_str: Vec<usize> = if match_str.is_empty() {
                on_trade
            } else {
                on_trade.into_iter().filter(|&i| stats.get(i).is_some_and(|s| has_matcher(s, match_str))).collect()
            };
            let Some((&first, rest)) = with_match_str.split_first() else { return Ok(None) };
            let mut merged = stats.get(first).cloned().ok_or(Fail::THROW)?;
            for &i in rest {
                let Some(stat) = stats.get(i) else { continue };
                merge_trade_ids_into(&mut merged, stat, None);
            }
            Ok(Some(merged))
        }
        StatGroupResolver::PercentMerge { kind } => {
            let pct = kind_index(kind, MergeKind::Percent)?;
            let roll100 = stats
                .get(pct)
                .ok_or(Fail::THROW)?
                .matchers
                .iter()
                .find(|m| m.string == match_str || m.advanced.as_deref() == Some(match_str))
                .is_some_and(|m| m.value == Some(100.0));
            if roll100 {
                let other = stats.get(kind_index(kind, MergeKind::Value)?).cloned().ok_or(Fail::THROW)?;
                let flag = matches!(other.matchers.as_slice(), [m] if !m.string.contains('#'));
                let mut pct_stat = stats.swap_remove(pct);
                merge_trade_ids_into(&mut pct_stat, &other, Some(if flag { "{empty_if_100}" } else { "{div_by_100}" }));
                return Ok(Some(pct_stat));
            }
            Ok(stats.into_iter().find(|s| has_matcher(s, match_str)))
        }
        StatGroupResolver::FlagMerge { kind } => {
            let Some(roll) = roll else { return Ok(None) };
            let val = kind_index(kind, MergeKind::Value)?;
            let flag_stat = stats.get(kind_index(kind, MergeKind::Flag)?).cloned().ok_or(Fail::THROW)?;
            let flag_roll = flag_stat.matchers.first().ok_or(Fail::THROW)?.value;
            let mut val_stat = stats.swap_remove(val);
            if flag_roll == Some(roll) {
                merge_trade_ids_into(&mut val_stat, &flag_stat, Some("{empty}"));
            }
            Ok(Some(val_stat))
        }
    }
}

/// `_mergeTradeIdsInto`: adds `source`'s first trade id of each type to `dest`.
fn merge_trade_ids_into(dest: &mut Stat, source: &Stat, prefix: Option<&str>) {
    for (mod_type, ids) in &source.trade.ids {
        let first = ids.first().map_or("undefined", String::as_str);
        let trade_id = format!("{}{first}", prefix.unwrap_or(""));
        match dest.trade.ids.get_mut(mod_type) {
            Some(dest_ids) => {
                if !dest_ids.contains(&trade_id) {
                    dest_ids.push(trade_id);
                }
            }
            None => {
                dest.trade.ids.insert(mod_type.clone(), vec![trade_id]);
            }
        }
    }
}

/// `testItemCategory`
fn test_item_category(actual: Option<&ItemCategory>, expected: &str) -> bool {
    let Some(actual) = actual else { return false };
    match expected {
        "WEAPON" => WEAPON.contains(actual),
        "ARMOUR" => ARMOUR.contains(actual),
        "HEIST_EQUIPMENT" => HEIST_EQUIPMENT.contains(actual),
        _ => expected == actual.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_combinations_follow_the_map() {
        let c = stat_placeholder_combinations("+17(16-25) to maximum Life");
        let texts: Vec<&str> = c.iter().map(|c| c.stat.as_str()).collect();
        // Listed indexes keep their number: the literal text is tried first.
        assert_eq!(texts, ["+17 to maximum Life", "# to maximum Life", "+17(16-25) to maximum Life"]);
        assert!(c[0].values.is_empty());
        assert_eq!(c[1].values[0].bounds, Some((16.0, 25.0)));
        assert_eq!(c[1].values[0].roll, 17.0);
        let c = stat_placeholder_combinations("Adds 3(2-4) to 7(6-8) Damage");
        let texts: Vec<&str> = c.iter().map(|c| c.stat.as_str()).collect();
        assert_eq!(
            texts,
            [
                "Adds 3 to 7 Damage",
                "Adds 3 to # Damage",
                "Adds # to 7 Damage",
                "Adds # to # Damage",
                "Adds 3(2-4) to 7(6-8) Damage"
            ]
        );
        // A range written in words keeps its text.
        let c = stat_placeholder_combinations("1(High-Low) thing");
        assert_eq!(c[1].stat, "#(High-Low) thing");
        assert_eq!(c[1].values[0].bounds, None);
    }

    #[test]
    fn rolls_average_pairs() {
        assert_eq!(get_roll_or_minmax_avg(&[3.0, 7.0]), 5.0);
        assert_eq!(get_roll_or_minmax_avg(&[3.0, 7.0, 9.0]), 3.0);
    }
}
