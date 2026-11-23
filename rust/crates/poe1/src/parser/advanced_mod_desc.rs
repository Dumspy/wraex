//! `ref/apt/renderer/src/parser/advanced-mod-desc.ts`: the `{ Prefix Modifier
//! "Name" (Tier: 1) — Tags }` lines of the advanced (Ctrl+Alt+C) item text.

use serde::Serialize;
use wraex_core::js;
pub use wraex_core::parse::{group_lines_by_mod, incr_roll, is_mod_info_line};

use super::Fail;
use super::modifiers::{ModifierMechanic, ModifierType};
use super::stat_translations::{ParsedStat, StatRollInfo};
use crate::data::Data;

pub const SCOURGE_LINE: &str = " (scourge)";
pub const ENCHANT_LINE: &str = " (enchant)";
pub const IMPLICIT_LINE: &str = " (implicit)";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ParsedModifier {
    pub info: ModifierInfo,
    pub stats: Vec<ParsedStat>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModifierInfo {
    #[serde(rename = "type")]
    pub kind: ModifierType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation: Option<Generation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mechanic: Option<ModifierMechanic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank: Option<f64>,
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roll_incr: Option<f64>,
}

impl ModifierInfo {
    /// `{ type, tags: [] }`
    pub fn new(kind: ModifierType) -> Self {
        Self {
            kind,
            generation: None,
            mechanic: None,
            name: None,
            tier: None,
            rank: None,
            tags: Vec::new(),
            roll_incr: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Generation {
    Suffix,
    Prefix,
}

/// `Number(x) || undefined`
fn number_or_none(x: Option<&str>) -> Option<f64> {
    Some(x.map_or(f64::NAN, js::number)).filter(|n| js::truthy(*n))
}

/// `parseModInfoLine`
pub fn parse_mod_info_line(d: &Data, line: &str) -> Result<ModifierInfo, Fail> {
    let inner = &line[1..line.len() - 1];
    let parts: Vec<&str> = inner.split('\u{2014}').map(js::trim).collect();
    let (mod_text, x_text2, x_text3) = (parts[0], parts.get(1).copied(), parts.get(2).copied());

    let mut info = ModifierInfo::new(ModifierType::Explicit);

    let eldritch = d.re("EATER_IMPLICIT").captures(mod_text).or_else(|| d.re("EXARCH_IMPLICIT").captures(mod_text));
    if let Some(m) = eldritch {
        info.kind = ModifierType::Implicit;
        info.mechanic = Some(ModifierMechanic::Eldritch);
        let rank = m.name("rank");
        info.rank = (1..=6).find(|r| rank == Some(d.s(&format!("ELDRITCH_MOD_R{r}")))).map(f64::from);
    } else {
        let m = d.re("MODIFIER_LINE").captures(mod_text).ok_or(Fail::THROW)?;
        let ty = m.name("type");
        let is = |key: &str| ty == Some(d.s(key));

        if is("IMPLICIT_MODIFIER") || is("CORRUPTED_IMPLICIT") || is("VESTIGIAL_IMPLICIT") {
            info.kind = ModifierType::Implicit;
        } else if is("FRACTURED_PREFIX") || is("FRACTURED_SUFFIX") {
            info.kind = ModifierType::Fractured;
        } else if is("CRAFTED_PREFIX") || is("CRAFTED_SUFFIX") {
            info.kind = ModifierType::Crafted;
        } else if is("RARE_ONLY_PREFIX") || is("RARE_ONLY_SUFFIX") {
            info.mechanic = Some(ModifierMechanic::RareOnly);
        }

        if is("PREFIX_MODIFIER") || is("FRACTURED_PREFIX") || is("CRAFTED_PREFIX") || is("RARE_ONLY_PREFIX") {
            info.generation = Some(Generation::Prefix);
        } else if is("SUFFIX_MODIFIER") || is("FRACTURED_SUFFIX") || is("CRAFTED_SUFFIX") || is("RARE_ONLY_SUFFIX") {
            info.generation = Some(Generation::Suffix);
        } else if is("CORRUPTED_IMPLICIT") {
            info.mechanic = Some(ModifierMechanic::Corruption);
        } else if is("FOULBORN_MODIFIER") {
            info.mechanic = Some(ModifierMechanic::Foulborn);
        } else if is("VESTIGIAL_IMPLICIT") {
            info.mechanic = Some(ModifierMechanic::Vestigial);
        }

        info.name = m.name("name").map(str::to_owned);
        info.tier = number_or_none(m.name("tier"));
        info.rank = number_or_none(m.name("rank"));
    }

    if info.mechanic.is_none()
        && let Some(name) = info.name.as_deref().filter(|n| !n.is_empty())
    {
        info.mechanic = mechanic_from_mod_name(d, name);
    }

    let incr_text = match (x_text3, x_text2) {
        (Some(x3), _) => Some(x3),
        (None, Some(x2)) if d.re("MODIFIER_INCREASED").is_match(x2) => Some(x2),
        _ => None,
    };
    let tags_text = x_text2.filter(|x2| incr_text != Some(x2));
    info.tags = match tags_text {
        Some(t) if !t.is_empty() => t.split(", ").map(str::to_owned).collect(),
        _ => Vec::new(),
    };
    info.roll_incr = match incr_text.filter(|t| !t.is_empty()) {
        Some(t) => {
            let caps = d.re("MODIFIER_INCREASED").captures(t).ok_or(Fail::THROW)?;
            Some(caps.get(1).map_or(f64::NAN, js::number))
        }
        None => None,
    };
    Ok(info)
}

/// `parseModType`: the type from the ` (enchant)`-like suffix, and the lines
/// without it. Throws if no line has one.
pub fn parse_mod_type(lines: &[String]) -> Result<(ModifierType, Vec<String>), Fail> {
    for (ending, kind) in [
        (SCOURGE_LINE, ModifierType::Scourge),
        (ENCHANT_LINE, ModifierType::Enchant),
        (IMPLICIT_LINE, ModifierType::Implicit),
    ] {
        if lines.iter().any(|l| l.ends_with(ending)) {
            return Ok((kind, remove_lines_ending(lines, ending)));
        }
    }
    Err(Fail::THROW)
}

fn remove_lines_ending(lines: &[String], ending: &str) -> Vec<String> {
    lines.iter().map(|l| l.strip_suffix(ending).unwrap_or(l).to_owned()).collect()
}

/// Stat values are stored as ints internally; this is the most common formatter.
const DIV_BY_100: i32 = 2;

/// `applyIncr`: the stat with its roll scaled by the modifier's `rollIncr`.
pub fn apply_incr(info: &ModifierInfo, parsed: &ParsedStat) -> Option<ParsedStat> {
    let incr = info.roll_incr.filter(|p| js::truthy(*p))?;
    let roll = parsed.roll.as_ref().filter(|r| !r.unscalable)?;
    let dp = if roll.dp { DIV_BY_100 } else { 0 };
    Some(ParsedStat {
        stat: parsed.stat.clone(),
        translation: parsed.translation.clone(),
        roll: Some(StatRollInfo {
            unscalable: roll.unscalable,
            generation: None,
            dp: roll.dp,
            value: incr_roll(roll.value, incr, dp),
            min: incr_roll(roll.min, incr, dp),
            max: incr_roll(roll.max, incr, dp),
        }),
    })
}

/// `mechanicFromModName`: influence and league mechanics by affix name.
fn mechanic_from_mod_name(d: &Data, name: &str) -> Option<ModifierMechanic> {
    use ModifierMechanic as M;
    [
        ("SHAPER_MODS", M::Shaper),
        ("ELDER_MODS", M::Elder),
        ("HUNTER_MODS", M::Hunter),
        ("WARLORD_MODS", M::Warlord),
        ("REDEEMER_MODS", M::Redeemer),
        ("CRUSADER_MODS", M::Crusader),
        ("DELVE_MODS", M::Delve),
        ("VEILED_MODS", M::Unveiled),
        ("INCURSION_MODS", M::Incursion),
        ("ESSENCE_MODS", M::Essence),
        ("INFAMOUS_MODS", M::Infamous),
    ]
    .into_iter()
    .find(|(key, _)| d.list(key).iter().any(|n| n == name))
    .map(|(_, m)| m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::tests::en;

    #[test]
    fn parses_mod_info_lines() {
        let d = en();
        let info = parse_mod_info_line(d, r#"{ Prefix Modifier "Abbot's" (Tier: 1) — Life, Defences, Energy Shield }"#)
            .unwrap();
        assert_eq!(info.generation, Some(Generation::Prefix));
        assert_eq!(info.name.as_deref(), Some("Abbot's"));
        assert_eq!(info.tier, Some(1.0));
        assert_eq!(info.tags, ["Life", "Defences", "Energy Shield"]);

        let info =
            parse_mod_info_line(d, "{ Eater of Worlds Implicit Modifier (Greater) — Physical, Elemental, Lightning }")
                .unwrap();
        assert_eq!(
            (info.kind, info.mechanic, info.rank),
            (ModifierType::Implicit, Some(ModifierMechanic::Eldritch), Some(2.0))
        );

        let info = parse_mod_info_line(
            d,
            r#"{ Master Crafted Suffix Modifier "of Craft" — Elemental, Cold, Chaos, Resistance }"#,
        )
        .unwrap();
        assert_eq!((info.kind, info.generation), (ModifierType::Crafted, Some(Generation::Suffix)));

        let info =
            parse_mod_info_line(d, r#"{ Prefix Modifier "The Shaper's" (Tier: 1) — Life — 20% Increased }"#).unwrap();
        assert_eq!((info.mechanic, info.roll_incr), (Some(ModifierMechanic::Shaper), Some(20.0)));
        assert!(parse_mod_info_line(d, r#"{ "bad }"#).is_err());
    }

    #[test]
    fn reads_mod_types() {
        let (kind, rest) = parse_mod_type(&["a (enchant)".to_owned(), "b".to_owned()]).unwrap();
        assert_eq!((kind, rest), (ModifierType::Enchant, vec!["a".to_owned(), "b".to_owned()]));
        assert!(parse_mod_type(&["a".to_owned()]).is_err());
    }
}
