//! `ref/ee2/renderer/src/parser/advanced-mod-desc.ts`: the `{ Prefix Modifier
//! "Name" (Tier: 1) — Tags }` lines of the advanced (Ctrl+Alt+C) item text, and
//! the ` (implicit)`-like suffixes on stat lines.

use serde::{Serialize, Serializer};
use wraex_core::js;
pub use wraex_core::parse::{group_lines_by_mod, incr_roll, is_mod_info_line};

use super::Fail;
use super::modifiers::ModifierType;
use super::stat_translations::{ParsedStat, StatRollInfo};
use crate::data::Data;

pub const SCOURGE_LINE: &str = " (scourge)";
pub const AUGMENT_LINE: &str = " (rune)";
pub const ADDED_AUGMENT_LINE: &str = " (added rune)";
pub const ENCHANT_LINE: &str = " (enchant)";
pub const IMPLICIT_LINE: &str = " (implicit)";
pub const DESECRATED_LINE: &str = " (desecrated)";
const CRAFTED_LINE: &str = " (crafted)";
const FRACTURED_LINE: &str = " (fractured)";

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
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank: Option<f64>,
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roll_incr: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added_incr: Option<bool>,
    /// A `Set`, which `JSON.stringify` writes as `{}`.
    #[serde(skip_serializing_if = "Option::is_none", serialize_with = "empty_object")]
    pub hybrid_with_ref: Option<Vec<String>>,
}

fn empty_object<S: Serializer, T>(_: &T, s: S) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeMap;
    s.serialize_map(Some(0))?.end()
}

impl ModifierInfo {
    /// `{ type, tags: [] }`
    pub fn new(kind: ModifierType) -> Self {
        Self {
            kind,
            generation: None,
            name: None,
            tier: None,
            rank: None,
            tags: Vec::new(),
            roll_incr: None,
            added_incr: None,
            hybrid_with_ref: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Generation {
    Suffix,
    Prefix,
    Corrupted,
    Eldritch,
    Mutated,
}

/// `Number(x) || undefined`
fn number_or_none(x: Option<&str>) -> Option<f64> {
    Some(x.map_or(f64::NAN, js::number)).filter(|n| js::truthy(*n))
}

/// `parseModInfoLine(line, type)`: `kind` is the type from the stat lines'
/// suffixes, which the mod line can override.
pub fn parse_mod_info_line(d: &Data, line: &str, mut kind: ModifierType) -> Result<ModifierInfo, Fail> {
    let inner = &line[1..line.len() - 1];
    let parts: Vec<&str> = inner.split('\u{2014}').map(js::trim).collect();
    let (mod_text, x_text2, x_text3) = (parts[0], parts.get(1).copied(), parts.get(2).copied());

    let mut generation = None;
    let (mut name, mut tier) = (None, None);
    let rank;

    let eldritch = d.re("EATER_IMPLICIT").captures(mod_text).or_else(|| d.re("EXARCH_IMPLICIT").captures(mod_text));
    if let Some(m) = eldritch {
        generation = Some(Generation::Eldritch);
        let r = m.name("rank");
        rank = (1..=6).find(|i| r == Some(d.s(&format!("ELDRITCH_MOD_R{i}")))).map(f64::from);
    } else {
        let m = d.re("MODIFIER_LINE").captures(mod_text).ok_or(Fail::THROW)?;
        let mut ty: String = m.name("type").ok_or(Fail::THROW)?.to_owned();
        // `type2` only exists in some languages' regex ("mostly for spanish").
        let mut ty2: Option<String> = m.name("type2").map(str::to_owned);
        let strip = |s: &str, prefix: &str| js::trim(&s[prefix.len()..]).to_owned();

        let fractured = d.s("FRACTURED_MODIFIER");
        if ty.starts_with(fractured) {
            ty = strip(&ty, fractured);
            kind = ModifierType::Fractured;
        } else if let Some(t2) = ty2.as_deref().filter(|t| t.starts_with(fractured)) {
            ty2 = Some(strip(t2, fractured));
            kind = ModifierType::Fractured;
        }

        let (desecrated, crafted) = (d.s("DESECRATED_MODIFIER"), d.s("CRAFTED_MODIFIER"));
        let not_fractured = |k: ModifierType, to: ModifierType| if k == ModifierType::Fractured { k } else { to };
        if ty.starts_with(desecrated) {
            ty = strip(&ty, desecrated);
            kind = not_fractured(kind, ModifierType::Desecrated);
        } else if ty.starts_with(crafted) {
            ty = strip(&ty, crafted);
            kind = not_fractured(kind, ModifierType::Crafted);
        } else if ty2.as_deref().is_some_and(|t| t.starts_with(desecrated)) {
            kind = not_fractured(kind, ModifierType::Desecrated);
        } else if ty2.as_deref().is_some_and(|t| t.starts_with(crafted)) {
            kind = not_fractured(kind, ModifierType::Crafted);
        }

        let is = |key: &str| ty == d.s(key);
        if is("PREFIX_MODIFIER") {
            generation = Some(Generation::Prefix);
        } else if is("SUFFIX_MODIFIER") {
            generation = Some(Generation::Suffix);
        } else if is("CORRUPTED_MODIFIER") || is("CORRUPTED_IMPLICIT") {
            generation = Some(Generation::Corrupted);
            kind = ModifierType::Enchant;
        } else if is("IMPLICIT_MODIFIER") {
            kind = ModifierType::Implicit;
        } else if is("ENCHANT_MODIFIER") {
            kind = ModifierType::Enchant;
        } else if is("VAAL_UNIQUE_MODIFIER") {
            generation = Some(Generation::Mutated);
        }

        name = m.name("name").filter(|n| !n.is_empty()).map(str::to_owned);
        tier = number_or_none(m.name("tier"));
        rank = number_or_none(m.name("rank"));
    }

    let incr_text = match (x_text3, x_text2) {
        (Some(x3), _) => Some(x3),
        (None, Some(x2)) if d.re("MODIFIER_INCREASED").is_match(x2) => Some(x2),
        _ => None,
    };
    let tags_text = x_text2.filter(|x2| incr_text != Some(x2));
    let tags = match tags_text {
        Some(t) if !t.is_empty() => t.split(", ").map(str::to_owned).collect(),
        _ => Vec::new(),
    };
    let roll_incr = match incr_text.filter(|t| !t.is_empty()) {
        Some(t) => {
            let caps = d.re("MODIFIER_INCREASED").captures(t).ok_or(Fail::THROW)?;
            Some(caps.get(1).map_or(f64::NAN, js::number))
        }
        None => None,
    };
    Ok(ModifierInfo { kind, generation, name, tier, rank, tags, roll_incr, added_incr: None, hybrid_with_ref: None })
}

/// `parseModType`: the type from the first line (veiled) or from a suffix on any
/// line, and the lines without that suffix. Explicit if none.
pub fn parse_mod_type(d: &Data, lines: &[String]) -> (ModifierType, Vec<String>) {
    if lines.first().is_some_and(|l| l == d.s("VEILED_PREFIX") || l == d.s("VEILED_SUFFIX")) {
        return (ModifierType::Veiled, lines.to_vec());
    }
    for (ending, kind) in [
        (SCOURGE_LINE, ModifierType::Scourge),
        (ENCHANT_LINE, ModifierType::Enchant),
        (IMPLICIT_LINE, ModifierType::Implicit),
        (FRACTURED_LINE, ModifierType::Fractured),
        (CRAFTED_LINE, ModifierType::Crafted),
        (AUGMENT_LINE, ModifierType::Augment),
        (ADDED_AUGMENT_LINE, ModifierType::AddedAugment),
        (DESECRATED_LINE, ModifierType::Desecrated),
    ] {
        if lines.iter().any(|l| l.ends_with(ending)) {
            return (kind, remove_lines_ending(lines, ending));
        }
    }
    (ModifierType::Explicit, lines.to_vec())
}

/// `removeLinesEnding` (`Parser.ts`)
pub fn remove_lines_ending(lines: &[String], ending: &str) -> Vec<String> {
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
            legacy: None,
            dp: roll.dp,
            value: incr_roll(roll.value, incr, dp),
            min: incr_roll(roll.min, incr, dp),
            max: incr_roll(roll.max, incr, dp),
            option: None,
        }),
        from_added_augment: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::tests::en;

    #[test]
    fn parses_mod_info_lines() {
        let d = en();
        let info = parse_mod_info_line(
            d,
            r#"{ Desecrated Prefix Modifier "Razor-sharp" (Tier: 3) — Damage, Physical, Attack }"#,
            ModifierType::Explicit,
        )
        .unwrap();
        assert_eq!(
            (info.kind, info.generation, info.tier),
            (ModifierType::Desecrated, Some(Generation::Prefix), Some(3.0))
        );
        assert_eq!(info.tags, ["Damage", "Physical", "Attack"]);
        let info = parse_mod_info_line(d, "{ Implicit Modifier }", ModifierType::Explicit).unwrap();
        assert_eq!(info.kind, ModifierType::Implicit);
        let info =
            parse_mod_info_line(d, r#"{ Fractured Crafted Suffix Modifier "of Calamity" }"#, ModifierType::Explicit)
                .unwrap();
        assert_eq!((info.kind, info.generation), (ModifierType::Fractured, Some(Generation::Suffix)));
    }

    #[test]
    fn reads_mod_types() {
        let d = en();
        let lines = |l: &[&str]| l.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(parse_mod_type(d, &lines(&["a (rune)", "b"])), (ModifierType::Augment, lines(&["a", "b"])));
        assert_eq!(parse_mod_type(d, &lines(&["a"])), (ModifierType::Explicit, lines(&["a"])));
        assert_eq!(parse_mod_type(d, &lines(&["Desecrated Prefix"])).0, ModifierType::Veiled);
    }
}
