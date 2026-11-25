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

    /// `specs/Parser/advanced-mod-desc.test.ts` (`parseModType`): the type comes
    /// from a suffix on any line or from the mod line itself, and is explicit
    /// otherwise. Implicit, fractured, desecrated and crafted lines may still be
    /// reported as explicit here; `parseModInfoLine` settles them.
    #[test]
    fn parses_the_mod_type_of_every_test_mod() {
        let d = en();
        for &(info_line, stats, kind, ..) in TEST_MODS {
            let (mod_type, _) = parse_mod_type(d, &mod_lines(info_line, stats));
            let settled_later = matches!(
                kind,
                ModifierType::Implicit | ModifierType::Fractured | ModifierType::Desecrated | ModifierType::Crafted
            );
            if settled_later {
                assert!(
                    mod_type == ModifierType::Explicit || mod_type == kind,
                    "{info_line}: expected explicit or {kind:?}, got {mod_type:?}"
                );
            } else {
                assert_eq!(mod_type, kind, "{info_line}");
            }
        }
    }

    /// `specs/Parser/advanced-mod-desc.test.ts` (`parseModInfoLine`): type,
    /// generation, name, tags and tier of every test mod.
    #[test]
    fn parses_the_mod_info_line_of_every_test_mod() {
        let d = en();
        for &(info_line, stats, kind, generation, name, tier, tags) in TEST_MODS {
            let (mod_type, lines) = parse_mod_type(d, &mod_lines(info_line, stats));
            let info = parse_mod_info_line(d, &lines[0], mod_type).unwrap();
            assert_eq!(info.kind, kind, "{info_line}: type");
            assert_eq!(info.generation, generation, "{info_line}: generation");
            assert_eq!(info.name.as_deref(), name, "{info_line}: name");
            assert_eq!(info.tags, tags.iter().map(|t| (*t).to_owned()).collect::<Vec<_>>(), "{info_line}: tags");
            assert_eq!(info.tier, tier, "{info_line}: tier");
        }
    }

    /// Every test mod's raw text, without the trailing empty line.
    fn mod_lines(info_line: &str, stats: &[&str]) -> Vec<String> {
        std::iter::once(info_line).chain(stats.iter().copied()).map(str::to_owned).collect()
    }

    /// `createTestModInfo()` (`specs/Parser/mods.ts`): the mod info line with its
    /// stat lines, the type the parser settles on, and the fields it reads.
    #[allow(clippy::type_complexity)]
    const TEST_MODS: &[(&str, &[&str], ModifierType, Option<Generation>, Option<&str>, Option<f64>, &[&str])] = &[
        (
            r#"{ Prefix Modifier "Crackling" (Tier: 7) — Damage, Elemental, Lightning, Attack }"#,
            &[r#"Adds 1(1-4) to 50(46-66) Lightning Damage"#],
            ModifierType::Explicit,
            Some(Generation::Prefix),
            Some("Crackling"),
            Some(7.0),
            &["Damage", "Elemental", "Lightning", "Attack"],
        ),
        (
            r#"{ Suffix Modifier "of the Brute" (Tier: 8) — Attribute }"#,
            &[r#"+8(5-8) to Strength"#],
            ModifierType::Explicit,
            Some(Generation::Suffix),
            Some("of the Brute"),
            Some(8.0),
            &["Attribute"],
        ),
        (
            r#"{ Prefix Modifier "Shocking" (Tier: 4) — Damage, Elemental, Lightning, Attack }"#,
            &[r#"Adds 5(1-5) to 82(62-89) Lightning Damage"#],
            ModifierType::Explicit,
            Some(Generation::Prefix),
            Some("Shocking"),
            Some(4.0),
            &["Damage", "Elemental", "Lightning", "Attack"],
        ),
        (
            r#"{ Prefix Modifier "Scorching" (Tier: 5) — Damage, Elemental, Fire, Attack }"#,
            &[r#"Adds 27(20-30) to 36(31-46) Fire Damage"#],
            ModifierType::Explicit,
            Some(Generation::Prefix),
            Some("Scorching"),
            Some(5.0),
            &["Damage", "Elemental", "Fire", "Attack"],
        ),
        (
            r#"{ Prefix Modifier "Icy" (Tier: 8) — Damage, Elemental, Cold, Attack }"#,
            &[r#"Adds 9(6-9) to 13(10-15) Cold Damage"#],
            ModifierType::Explicit,
            Some(Generation::Prefix),
            Some("Icy"),
            Some(8.0),
            &["Damage", "Elemental", "Cold", "Attack"],
        ),
        (
            r#"{ Suffix Modifier "of Radiance" (Tier: 1) — Attack }"#,
            &[r#"+57(41-60) to Accuracy Rating"#, r#"15% increased Light Radius"#],
            ModifierType::Explicit,
            Some(Generation::Suffix),
            Some("of Radiance"),
            Some(1.0),
            &["Attack"],
        ),
        (
            r#"{ Unique Modifier — Defences }"#,
            &[r#"56(50-70)% increased Energy Shield"#],
            ModifierType::Explicit,
            None,
            None,
            None,
            &["Defences"],
        ),
        (
            r#"{ Unique Modifier — Mana }"#,
            &[r#"40% increased Mana Regeneration Rate while stationary"#],
            ModifierType::Explicit,
            None,
            None,
            None,
            &["Mana"],
        ),
        (
            r#"{ Unique Modifier — Elemental, Lightning, Resistance }"#,
            &[r#"+26(20-30)% to Lightning Resistance"#],
            ModifierType::Explicit,
            None,
            None,
            None,
            &["Elemental", "Lightning", "Resistance"],
        ),
        (
            r#"{ Unique Modifier — Elemental, Lightning, Resistance }"#,
            &[r#"+5% to Maximum Lightning Resistance"#],
            ModifierType::Explicit,
            None,
            None,
            None,
            &["Elemental", "Lightning", "Resistance"],
        ),
        (
            r#"{ Unique Modifier — Mana }"#,
            &[r#"40% increased Mana Regeneration Rate"#],
            ModifierType::Explicit,
            None,
            None,
            None,
            &["Mana"],
        ),
        (
            r#"{ Prefix Modifier "Vaporous" (Tier: 3) — Defences }"#,
            &[r#"+143(124-151) to Evasion Rating"#],
            ModifierType::Explicit,
            Some(Generation::Prefix),
            Some("Vaporous"),
            Some(3.0),
            &["Defences"],
        ),
        (
            r#"{ Suffix Modifier "of the Wrestler" (Tier: 7) — Attribute }"#,
            &[r#"+12(9-12) to Strength"#],
            ModifierType::Explicit,
            Some(Generation::Suffix),
            Some("of the Wrestler"),
            Some(7.0),
            &["Attribute"],
        ),
        (
            r#"{ Suffix Modifier "of Warmth" (Tier: 3) — Mana }"#,
            &[r#"8(8-12)% increased Mana Regeneration Rate"#, r#"5% increased Light Radius"#],
            ModifierType::Explicit,
            Some(Generation::Suffix),
            Some("of Warmth"),
            Some(3.0),
            &["Mana"],
        ),
        (
            r#"{ Suffix Modifier "of the Penguin" (Tier: 7) — Elemental, Cold, Resistance }"#,
            &[r#"+15(11-15)% to Cold Resistance"#],
            ModifierType::Explicit,
            Some(Generation::Suffix),
            Some("of the Penguin"),
            Some(7.0),
            &["Elemental", "Cold", "Resistance"],
        ),
        (
            r#"{ Implicit Modifier — Elemental, Fire, Cold, Lightning, Resistance }"#,
            &[r#"+8(7-10)% to all Elemental Resistances"#],
            ModifierType::Implicit,
            None,
            None,
            None,
            &["Elemental", "Fire", "Cold", "Lightning", "Resistance"],
        ),
        (
            r#"{ Prefix Modifier "Merciless" (Tier: 1) — Damage, Physical, Attack }"#,
            &[r#"173(170-179)% increased Physical Damage"#],
            ModifierType::Explicit,
            Some(Generation::Prefix),
            Some("Merciless"),
            Some(1.0),
            &["Damage", "Physical", "Attack"],
        ),
        (
            r#"{ Prefix Modifier "Dictator's" (Tier: 1) — Damage, Physical, Attack }"#,
            &[r#"78(75-79)% increased Physical Damage"#, r#"+175(175-200) to Accuracy Rating"#],
            ModifierType::Explicit,
            Some(Generation::Prefix),
            Some("Dictator's"),
            Some(1.0),
            &["Damage", "Physical", "Attack"],
        ),
        (
            r#"{ Prefix Modifier "Flaring" (Tier: 1) — Damage, Physical, Attack }"#,
            &[r#"Adds 54(37-55) to 94(63-94) Physical Damage (desecrated)"#],
            ModifierType::Desecrated,
            Some(Generation::Prefix),
            Some("Flaring"),
            Some(1.0),
            &["Damage", "Physical", "Attack"],
        ),
        (
            r#"{ Suffix Modifier "of Infamy" — Attack, Speed }"#,
            &[r#"25(23-25)% increased Attack Speed (fractured)"#],
            ModifierType::Fractured,
            Some(Generation::Suffix),
            Some("of Infamy"),
            None,
            &["Attack", "Speed"],
        ),
        (
            r#"{ Suffix Modifier "of the Sniper" (Tier: 1) }"#,
            &[r#"+7 to Level of all Projectile Skills"#],
            ModifierType::Explicit,
            Some(Generation::Suffix),
            Some("of the Sniper"),
            Some(1.0),
            &[],
        ),
        (
            r#"{ Suffix Modifier "of Bursting" (Tier: 1) — Attack }"#,
            &[r#"Loads 2 additional bolts"#],
            ModifierType::Explicit,
            Some(Generation::Suffix),
            Some("of Bursting"),
            Some(1.0),
            &["Attack"],
        ),
        (
            r#"{ Prefix Modifier "Impenetrable" (Tier: 1) — Defences }"#,
            &[r#"103(101-110)% increased Armour"#],
            ModifierType::Explicit,
            Some(Generation::Prefix),
            Some("Impenetrable"),
            Some(1.0),
            &["Defences"],
        ),
        (
            r#"{ Prefix Modifier "Hardened" (Tier: 1) — Defences }"#,
            &[r#"+70(70-86) to Armour"#, r#"41(39-42)% increased Armour"#],
            ModifierType::Explicit,
            Some(Generation::Prefix),
            Some("Hardened"),
            Some(1.0),
            &["Defences"],
        ),
        (
            r#"{ Prefix Modifier "Unmoving" (Tier: 2) — Defences }"#,
            &[r#"+256(226-256) to Armour (desecrated)"#],
            ModifierType::Desecrated,
            Some(Generation::Prefix),
            Some("Unmoving"),
            Some(2.0),
            &["Defences"],
        ),
        (
            r#"{ Suffix Modifier "of the Titan" (Tier: 1) — Attribute }"#,
            &[r#"+32(31-33) to Strength"#],
            ModifierType::Explicit,
            Some(Generation::Suffix),
            Some("of the Titan"),
            Some(1.0),
            &["Attribute"],
        ),
        (
            r#"{ Suffix Modifier "of Allaying" (Tier: 3) — Physical, Ailment }"#,
            &[r#"48(50-46)% reduced Duration of Bleeding on You"#],
            ModifierType::Explicit,
            Some(Generation::Suffix),
            Some("of Allaying"),
            Some(3.0),
            &["Physical", "Ailment"],
        ),
        (
            r#"{ Suffix Modifier "of the Essence" (Tier: 1) }"#,
            &[r#"Hits against you have 44(40-50)% reduced Critical Damage Bonus"#],
            ModifierType::Explicit,
            Some(Generation::Suffix),
            Some("of the Essence"),
            Some(1.0),
            &[],
        ),
        (
            r#"{ Prefix Modifier "Malignant" (Tier: 4) — Damage, Chaos }"#,
            &[r#"71(65-74)% increased Chaos Damage"#],
            ModifierType::Explicit,
            Some(Generation::Prefix),
            Some("Malignant"),
            Some(4.0),
            &["Damage", "Chaos"],
        ),
        (
            r#"{ Prefix Modifier "Frostbound" (Tier: 1) — Damage, Elemental, Cold }"#,
            &[r#"Gain 28(28-30)% of Damage as Extra Cold Damage"#],
            ModifierType::Explicit,
            Some(Generation::Prefix),
            Some("Frostbound"),
            Some(1.0),
            &["Damage", "Elemental", "Cold"],
        ),
        (
            r#"{ Suffix Modifier "of the Hearth" (Tier: 1) — Mana }"#,
            &[r#"22(18-22)% increased Mana Regeneration Rate"#, r#"15% increased Light Radius"#],
            ModifierType::Explicit,
            Some(Generation::Suffix),
            Some("of the Hearth"),
            Some(1.0),
            &["Mana"],
        ),
        (
            r#"{ Suffix Modifier "of the Apt" (Tier: 4) }"#,
            &[r#"20% reduced Attribute Requirements"#],
            ModifierType::Explicit,
            Some(Generation::Suffix),
            Some("of the Apt"),
            Some(4.0),
            &[],
        ),
        (
            r#"{ Implicit Modifier }"#,
            &[r#"18(12-20)% increased Rarity of Items found"#],
            ModifierType::Implicit,
            None,
            None,
            None,
            &[],
        ),
        (
            r#"{ Prefix Modifier "Lady's" (Tier: 5) }"#,
            &[r#"+30(30-33) to Spirit"#],
            ModifierType::Explicit,
            Some(Generation::Prefix),
            Some("Lady's"),
            Some(5.0),
            &[],
        ),
        (
            r#"{ Prefix Modifier "Gentian" (Tier: 6) — Mana }"#,
            &[r#"+90(90-104) to maximum Mana"#],
            ModifierType::Explicit,
            Some(Generation::Prefix),
            Some("Gentian"),
            Some(6.0),
            &["Mana"],
        ),
        (
            r#"{ Prefix Modifier "Incanter's" (Tier: 1) — Damage, Caster }"#,
            &[r#"29(27-30)% increased Spell Damage"#],
            ModifierType::Explicit,
            Some(Generation::Prefix),
            Some("Incanter's"),
            Some(1.0),
            &["Damage", "Caster"],
        ),
        (
            r#"{ Fractured Suffix Modifier "of the Ice" (Tier: 2) — Elemental, Cold, Resistance }"#,
            &[r#"+37(36-40)% to Cold Resistance"#],
            ModifierType::Fractured,
            Some(Generation::Suffix),
            Some("of the Ice"),
            Some(2.0),
            &["Elemental", "Cold", "Resistance"],
        ),
        (
            r#"{ Suffix Modifier "of the Sorcerer" (Tier: 1) — Caster, Gem }"#,
            &[r#"+3 to Level of all Spell Skills"#],
            ModifierType::Explicit,
            Some(Generation::Suffix),
            Some("of the Sorcerer"),
            Some(1.0),
            &["Caster", "Gem"],
        ),
        (
            r#"{ Desecrated Suffix Modifier "of Amanamu" (Tier: 1) — Elemental, Fire, Chaos, Resistance }"#,
            &[r#"+17(13-17)% to Fire and Chaos Resistances"#],
            ModifierType::Desecrated,
            Some(Generation::Suffix),
            Some("of Amanamu"),
            Some(1.0),
            &["Elemental", "Fire", "Chaos", "Resistance"],
        ),
        (
            r#"{ Crafted Prefix Modifier "Gentian" (Tier: 6) — Mana }"#,
            &[r#"+90(90-104) to maximum Mana"#],
            ModifierType::Crafted,
            Some(Generation::Prefix),
            Some("Gentian"),
            Some(6.0),
            &["Mana"],
        ),
    ];
}
