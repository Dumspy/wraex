//! `ref/ee2/renderer/src/parser/stat-translations.ts`: matching stat text from an
//! item against the translations in `stats.ndjson`, with the trade site's stat
//! texts as a fallback.

use std::sync::OnceLock;

use serde::Serialize;
use wraex_core::js;
use wraex_core::js_regex::Regex;
pub use wraex_core::parse::StatString;

use super::Fail;
use super::modifiers::ModifierType;
use crate::data::{BaseType, Data, Stat, StatMatcher, StatTrade, stat_better};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedStat {
    pub stat: Stat,
    pub translation: StatMatcher,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roll: Option<StatRollInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_added_augment: Option<BaseType>,
}

/// `ParsedStat['roll']`
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StatRollInfo {
    pub unscalable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legacy: Option<bool>,
    pub dp: bool,
    pub value: f64,
    pub min: f64,
    pub max: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub option: Option<f64>,
}

/// Which of the found numbers stay literal (listed) or become `#`, by count of
/// numbers. Listed indexes keep their number, so the literal text comes first.
const PLACEHOLDER_MAP: [&[&[usize]]; 5] = [
    // 0 # -> max 0 #
    &[&[]],
    // 1 # -> max 1 #
    &[&[0], &[]],
    // 2 # -> max 2 #
    &[&[0, 1], &[0], &[1], &[]],
    // 3 # -> max 2 #
    &[&[0, 1, 2], &[1, 2], &[0, 2], &[0, 1], &[2], &[1], &[0], &[]],
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
        &[3],
        &[2],
        &[1],
        &[0],
        &[],
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

fn value_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?<value>(?<!\d|\))[+-]?\d+(?:\.\d+)?)(?:\((?<min>.[^)-]*)(?:-(?<max>[^)]+))?\))?", "gm").unwrap()
    })
}

fn hash_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new("#", "gm").unwrap())
}

/// `_statPlaceholderGenerator`: the stat text with subsets of its numbers turned
/// into `#`, then the exact text.
fn stat_placeholder_combinations(stat: &str) -> Vec<Combination> {
    let mut matches: Vec<PlaceholderValue> = Vec::new();
    // "()" when GGG didn't provide an advanced description.
    let with_placeholders = stat.replace("()", "");
    let with_placeholders = value_regex().replace_all(&with_placeholders, |caps| {
        let roll = caps.get(1).unwrap();
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
        for replacements in map.iter() {
            let mut idx: isize = -1;
            let replaced = hash_regex().replace_all(&with_placeholders, |_| {
                idx += 1;
                let i = idx as usize;
                if replacements.contains(&i) { matches[i].roll_str.clone() } else { "#".to_owned() }
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

/// `tryParseTranslation`. A stat the data lacks for this modifier type may still
/// be on the trade site; the first such candidate is the fallback.
pub fn try_parse_translation(d: &Data, stat: &StatString, mod_type: ModifierType) -> Result<Option<ParsedStat>, Fail> {
    let real_type = if mod_type == ModifierType::AddedAugment { ModifierType::Augment } else { mod_type };
    let mut backup: Option<(ParsedStat, Combination)> = None;

    for combination in stat_placeholder_combinations(&stat.string) {
        let found = d.stat_by_match_str(&combination.stat);
        let usable = found.as_ref().is_some_and(|(s, _)| {
            s.trade.ids.as_ref().is_some_and(|ids| ids.contains_key(real_type.as_str()))
                || s.stat_ref.starts_with(d.s_ref("GRANTS_SKILL"))
        });
        if !usable {
            if backup.is_none()
                && let Some(parsed) = try_secondary_parse_translation(d, &combination.stat)
            {
                backup = Some((parsed, combination));
            }
            continue;
        }
        let (found_stat, matcher) = found.unwrap();
        let roll = parse_roll(&found_stat, &matcher, combination, stat)?;
        return Ok(Some(ParsedStat { stat: found_stat, translation: matcher, roll, from_added_augment: None }));
    }
    match backup {
        Some((mut parsed, combination)) => {
            parsed.roll = parse_roll(&parsed.stat, &parsed.translation, combination, stat)?;
            Ok(Some(parsed))
        }
        None => Ok(None),
    }
}

/// `parseRoll`. Modifiers must be upgraded to the new values with a Divine Orb.
fn parse_roll(
    found: &Stat,
    matcher: &StatMatcher,
    mut combination: Combination,
    stat: &StatString,
) -> Result<Option<StatRollInfo>, Fail> {
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

    if found.stat_ref == "# uses remaining" {
        // `combination.values[0]` throws when there is none.
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
    if values.is_empty() {
        return Ok(None);
    }
    let avg = |f: &dyn Fn(&PlaceholderValue) -> f64| get_roll_or_minmax_avg(&values.iter().map(f).collect::<Vec<_>>());
    Ok(Some(StatRollInfo {
        unscalable: stat.unscalable,
        legacy: legacy_stat_rolls.then_some(true),
        dp: found.dp == Some(true) || values.iter().any(|v| v.decimal),
        value: avg(&|v| v.roll),
        min: avg(&|v| v.bounds.map_or(v.roll, |b| b.0)),
        max: avg(&|v| v.bounds.map_or(v.roll, |b| b.1)),
        option: matcher.value,
    }))
}

/// `getRollOrMinmaxAvg`: the average of four or two values, otherwise the first.
pub fn get_roll_or_minmax_avg(values: &[f64]) -> f64 {
    match values.len() {
        4 => (values[0] + values[1] + values[2] + values[3]) / 4.0,
        2 => (values[0] + values[1]) / 2.0,
        _ => values.first().copied().unwrap_or(f64::NAN),
    }
}

/// `trySecondaryParseTranslation`: a naive stat built from the trade site's text.
fn try_secondary_parse_translation(d: &Data, stat: &str) -> Option<ParsedStat> {
    let ids = d.trade_stat_by_match_str(stat)?.clone();
    let matcher = StatMatcher { string: stat.to_owned(), advanced: None, negate: None, value: None, oils: None };
    let stat_line = Stat {
        stat_ref: stat.to_owned(),
        id: None,
        dp: None,
        matchers: vec![matcher.clone()],
        better: stat_better::POSITIVE_ROLL,
        from_area_mods: None,
        from_uber_area_mods: None,
        from_heist_area_mods: None,
        anointments: None,
        trade: StatTrade { inverted: None, option: None, count: None, ids: Some(ids) },
    };
    Some(ParsedStat { stat: stat_line, translation: matcher, roll: None, from_added_augment: None })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_combinations_include_single_placeholders() {
        let c = stat_placeholder_combinations("Adds 1(1-2) to 3(3-4) to 5 Things");
        let texts: Vec<&str> = c.iter().map(|c| c.stat.as_str()).collect();
        assert_eq!(texts.len(), 9);
        assert_eq!(texts[7], "Adds # to # to # Things");
        assert_eq!(texts[8], "Adds 1(1-2) to 3(3-4) to 5 Things");
    }

    #[test]
    fn rolls_average_four_or_two() {
        assert_eq!(get_roll_or_minmax_avg(&[1.0, 2.0, 3.0, 6.0]), 3.0);
        assert_eq!(get_roll_or_minmax_avg(&[3.0, 7.0]), 5.0);
        assert_eq!(get_roll_or_minmax_avg(&[3.0, 7.0, 9.0]), 3.0);
    }
}
