//! `ref/apt/renderer/src/parser/Parser.ts`: `parseClipboard`.
//!
//! The item text is split into sections (`--------`), and each parser in
//! [`PARSERS`] gets the remaining sections in order until one is parsed; virtual
//! parsers work on the item alone. Section parsers are ported one function per
//! TS function, in the same order. Where the reference throws (a `TypeError` on
//! `undefined`, or an explicit `throw`), parsing fails with `item.parse_error`,
//! as its `try/catch` does.

pub mod advanced_mod_desc;
pub mod calc_q20;
pub mod item;
pub mod magic_name;
pub mod meta;
pub mod modifiers;
pub mod stat_translations;

use std::sync::OnceLock;

use wraex_core::js;
use wraex_core::js_regex::Regex;
use wraex_core::{Rarity, data::js_string};

use crate::data::{BaseType, Data, MercenaryBuildRef, Namespace};
use advanced_mod_desc::{
    ENCHANT_LINE, IMPLICIT_LINE, ModifierInfo, ParsedModifier, SCOURGE_LINE, group_lines_by_mod, is_mod_info_line,
    parse_mod_info_line, parse_mod_type,
};
use calc_q20::{calc_prop_percentile, quality_stats};
use item::{
    Corrupted, HeistBlueprint, HeistContract, IncursionRoom, Item, ItemInfluence, Sockets, StackSize, UnknownModifier,
};
use meta::{ItemCategory, JEWELLERY};
use modifiers::{ModifierMechanic, ModifierType, sum_stats_by_mod_type};
use stat_translations::{
    ParsedStat, RollGeneration, StatRollInfo, StatString, lines_to_stat_strings, try_parse_translation,
};

/// Why parsing stopped: the reference's error key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fail(pub &'static str);

impl Fail {
    /// Anything the reference throws on, caught by `parseClipboard`.
    pub const THROW: Fail = Fail("item.parse_error");
    pub const UNKNOWN: Fail = Fail("item.unknown");
}

type R<T> = Result<T, Fail>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Parsed,
    Skipped,
    ParserSkipped,
}

use Section::{Parsed, ParserSkipped, Skipped};

type SectionParser = fn(&[String], &mut Item, &Data) -> R<Section>;
type VirtualParser = fn(&mut Item, &Data) -> R<()>;

enum Step {
    Section(SectionParser),
    Virtual(VirtualParser),
}

use Step::{Section as S, Virtual as V};

/// `parsers`, in order. Each section is parsed by at most one parser.
const PARSERS: &[Step] = &[
    S(parse_unidentified),
    V(parse_superior),
    V(parse_foulborn),
    V(parse_vestigial),
    S(parse_synthesised),
    S(parse_category_by_help_text),
    V(parse_map_tier),
    V(normalize_name),
    V(find_in_database),
    // -----------
    S(parse_item_level),
    S(parse_talisman_tier),
    S(parse_gem),
    S(parse_vaal_gem),
    S(parse_armour),
    S(parse_weapon),
    S(parse_accessory),
    S(parse_flask),
    S(parse_tincture),
    S(parse_stack_size),
    S(parse_corrupted),
    S(parse_imbued_gem),
    S(parse_foil),
    S(parse_influence),
    S(parse_map),
    S(parse_sockets),
    S(parse_heist_contract),
    S(parse_heist_blueprint),
    S(parse_chart),
    S(parse_area_level),
    S(parse_atzoatl_rooms),
    S(parse_mirrored_tablet),
    S(parse_filled_coffin),
    S(parse_mirrored),
    S(parse_split),
    S(parse_sentinel_charge),
    S(parse_scrying_orb),
    S(parse_mercenary),
    S(parse_viscera),
    S(parse_logbook_area),
    S(parse_logbook_area),
    S(parse_logbook_area),
    S(parse_mercenary_gems),
    S(parse_mercenary_gems),
    S(parse_mercenary_gems),
    S(parse_mercenary_gems),
    S(parse_mercenary_gems),
    S(parse_mercenary_gems),
    S(parse_modifiers), // enchant
    S(parse_modifiers), // scourge
    S(parse_modifiers), // implicit
    S(parse_modifiers), // explicit
    V(augment_modifiers),
    V(transform_to_legacy_modifiers),
    V(parse_fractured),
    V(pick_correct_variant),
    V(calc_disenchant_dust),
    V(calc_base_percentile),
];

const VALUE_AUGMENTED: &str = " (augmented)";

/// `parseClipboard`: the item, or the reference's error key.
pub fn parse_clipboard(d: &Data, clipboard: &str) -> Result<Item, Fail> {
    let mut sections = item_text_to_sections(clipboard);

    let first = sections.first().ok_or(Fail::THROW)?;
    if first.get(2).map(String::as_str) == Some(d.s("CANNOT_USE_ITEM")) {
        let mut first = sections.remove(0);
        first.pop(); // the CANNOT_USE_ITEM line
        // Item class and rarity move into the next section.
        let next = sections.first_mut().ok_or(Fail::THROW)?;
        first.append(next);
        *next = first;
    }
    let mut item = parse_name_plate(d, &mut sections[0])?;
    sections.remove(0);
    item.raw_text = clipboard.to_owned();

    for step in PARSERS {
        match step {
            V(parser) => parser(&mut item, d)?,
            S(parser) => {
                let mut i = 0;
                while i < sections.len() {
                    // Works around an upstream bug.
                    parse_eldritch_item(d, &mut sections[i]);
                    match parser(&sections[i], &mut item, d)? {
                        Parsed => {
                            sections.remove(i);
                            break;
                        }
                        ParserSkipped => break,
                        Skipped => i += 1,
                    }
                }
            }
        }
    }
    Ok(item)
}

fn item_text_to_sections(text: &str) -> Vec<Vec<String>> {
    let mut lines: Vec<&str> = text.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l)).collect();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    let mut sections: Vec<Vec<String>> = vec![Vec::new()];
    for line in lines {
        if line == "--------" {
            sections.push(Vec::new());
        } else {
            sections.last_mut().unwrap().push(line.to_owned());
        }
    }
    sections.retain(|s| !s.is_empty());
    sections
}

/// `section[0]` where the reference calls a method on it: throws if missing.
fn first(section: &[String]) -> R<&str> {
    section.first().map(String::as_str).ok_or(Fail::THROW)
}

/// `section[0] === value`
fn first_is(section: &[String], value: &str) -> bool {
    section.first().is_some_and(|l| l == value)
}

/// `x` is set and truthy.
fn truthy(x: Option<f64>) -> bool {
    x.is_some_and(js::truthy)
}

fn normalize_name(item: &mut Item, d: &Data) -> R<()> {
    if item.rarity == Some(Rarity::Magic)
        && let Some(base_type) = magic_name::magic_basetype(d, &item.name)?
    {
        item.name = base_type;
    }

    if item.category == Some(ItemCategory::MetamorphSample) {
        for (key, name) in [
            ("METAMORPH_BRAIN", "Metamorph Brain"),
            ("METAMORPH_EYE", "Metamorph Eye"),
            ("METAMORPH_LUNG", "Metamorph Lung"),
            ("METAMORPH_HEART", "Metamorph Heart"),
            ("METAMORPH_LIVER", "Metamorph Liver"),
        ] {
            if d.re(key).is_match(&item.name) {
                item.name = name.to_owned();
                break;
            }
        }
    }
    Ok(())
}

fn find_in_database(item: &mut Item, d: &Data) -> R<()> {
    let base_or_name = item.base_type.clone().unwrap_or_else(|| item.name.clone());
    let info = match &item.category {
        Some(ItemCategory::DivinationCard) => d.item_by_translated(Namespace::DivinationCard, &item.name),
        Some(ItemCategory::CapturedBeast) => d.item_by_translated(Namespace::CapturedBeast, &base_or_name),
        Some(ItemCategory::Gem) => d.item_by_translated(Namespace::Gem, &item.name),
        Some(ItemCategory::MetamorphSample) => d.item_by_translated(Namespace::Item, &item.name),
        Some(ItemCategory::Voidstone) => d.item_by_ref(Namespace::Item, "Charged Compass"),
        _ if item.rarity == Some(Rarity::Unique) && !item.is_unidentified => {
            d.item_by_translated(Namespace::Unique, &item.name)
        }
        _ => d.item_by_translated(Namespace::Item, &base_or_name),
    };
    let mut info = info.filter(|i| !i.is_empty()).ok_or(Fail::UNKNOWN)?;
    if info[0].unique.is_some() {
        let base_types = d
            .item_by_translated(Namespace::Item, js_string(&item.base_type))
            .filter(|b| !b.is_empty())
            .ok_or(Fail::UNKNOWN)?;
        let base_type_ref = &base_types[0].ref_name;
        info.retain(|i| i.unique.as_ref().is_some_and(|u| &u.base == base_type_ref));
    }
    // No variant left: `item.info` is undefined and the next read throws.
    let chosen = info.first().cloned().ok_or(Fail::THROW)?;
    // Choose the first variant; the right one is picked at the end of parsing.
    item.info_variants = Some(info);
    item.info = Some(chosen);
    // Same for every variant.
    if item.category.is_none() {
        let info = item.info();
        if let Some(craftable) = &info.craftable {
            item.category = Some(ItemCategory::from_text(&craftable.category));
        } else if let Some(unique) = &info.unique {
            let base = d.item_by_ref(Namespace::Item, &unique.base).ok_or(Fail::THROW)?;
            let craftable = base.first().and_then(|b| b.craftable.as_ref()).ok_or(Fail::THROW)?;
            item.category = Some(ItemCategory::from_text(&craftable.category));
        }
    }
    Ok(())
}

fn parse_map_tier(item: &mut Item, d: &Data) -> R<()> {
    let text = item.base_type_or_name().to_owned();
    let Some(caps) = d.re("MAP_TIER").captures(&text) else { return Ok(()) };
    item.map_tier = Some(caps.get(1).map_or(f64::NAN, js::number));
    let whole = caps.whole();
    match &mut item.base_type {
        Some(base) if !base.is_empty() => *base = base.replacen(whole, "", 1),
        _ => item.name = item.name.replacen(whole, "", 1),
    }
    Ok(())
}

/// The numbers after `prefix` (`parseInt(line.slice(prefix.length), 10)`).
fn int_after(line: &str, prefix: &str) -> f64 {
    js::parse_int(&line[prefix.len()..])
}

fn parse_area_prop_nested(d: &Data, line: &str, item: &mut Item) -> bool {
    let props: [(&str, &mut Option<f64>); 4] = [
        ("MAP_ITEM_QUANTITY", &mut item.area_item_quantity),
        ("MAP_ATLAS_DROPS", &mut item.area_atlas_drops),
        ("MAP_ITEM_RARITY", &mut item.area_item_rarity),
        ("MAP_MONSTER_PACK_SIZE", &mut item.area_pack_size),
    ];
    for (key, field) in props {
        let prefix = d.s(key);
        if line.starts_with(prefix) {
            *field = Some(int_after(line, prefix));
            return true;
        }
    }
    false
}

fn parse_map(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if item.category != Some(ItemCategory::Map) {
        return Ok(ParserSkipped);
    }
    let mut is_parsed = Skipped;
    for line in section {
        if parse_area_prop_nested(d, line, item) {
            is_parsed = Parsed;
            continue;
        }
        let more: [(&str, &mut Option<f64>); 4] = [
            ("MAP_MORE_MAPS", &mut item.map_more_maps),
            ("MAP_MORE_THREADS", &mut item.map_more_threads),
            ("MAP_MORE_CURRENCY", &mut item.map_more_currency),
            ("MAP_MORE_DIVINATION_CARDS", &mut item.map_more_div_cards),
        ];
        let mut done = false;
        for (key, field) in more {
            let prefix = d.s(key);
            if line.starts_with(prefix) {
                *field = Some(int_after(line, prefix));
                done = true;
                break;
            }
        }
        if done {
            is_parsed = Parsed;
        } else if line.starts_with(d.s("MAP_AREA")) {
            // The reference reads the area name from the section's first line.
            let area_name = first(section)?.get(d.s("MAP_AREA").len()..).unwrap_or("");
            let area = d.item_by_translated(Namespace::Area, area_name).ok_or(Fail::THROW)?;
            item.map_area = area.into_iter().next();
            is_parsed = Parsed;
        } else if let Some(caps) = d.re("MAP_COMPLETION_REWARD").captures(line) {
            let reward_name = caps.get(1).unwrap_or("undefined");
            let reward = d.item_by_translated(Namespace::Unique, reward_name).ok_or(Fail::THROW)?;
            item.map_completion_reward = reward.into_iter().next();
            is_parsed = Parsed;
        }
    }
    Ok(is_parsed)
}

fn parse_fractured(item: &mut Item, _: &Data) -> R<()> {
    if item.new_mods.iter().any(|m| m.info.kind == ModifierType::Fractured) {
        item.is_fractured = Some(true);
    }
    Ok(())
}

fn pick_correct_variant(item: &mut Item, d: &Data) -> R<()> {
    let variants = item.info_variants.as_deref().unwrap_or_default();
    let info = match pick_variant(variants, item)? {
        Some(v) => v.clone(),
        None => variants.first().cloned().ok_or(Fail::THROW)?,
    };
    item.info = Some(info);
    if let Some(unique) = &item.info().unique {
        let base_variants = d.item_by_ref(Namespace::Item, &unique.base).ok_or(Fail::THROW)?;
        let base = match pick_variant(&base_variants, item)? {
            Some(v) => Some(v.clone()),
            None => base_variants.first().cloned(),
        };
        item.unique_base = base;
    }
    Ok(())
}

/// `_pickCorrectVariant`: the first variant whose conditions the item meets. It
/// may find none, e.g. a corrupted implicit on a Two-Stone Ring.
fn pick_variant<'a>(variants: &'a [BaseType], item: &Item) -> R<Option<&'a BaseType>> {
    if variants.len() <= 1 {
        return Ok(variants.first());
    }
    for variant in variants {
        let cond = variant.disc.as_ref().ok_or(Fail::THROW)?;
        let set = |flag: Option<bool>| flag == Some(true);

        if set(cond.prop_ar) && !truthy(item.armour_ar) {
            continue;
        }
        if set(cond.prop_ev) && !truthy(item.armour_ev) {
            continue;
        }
        if set(cond.prop_es) && !truthy(item.armour_es) {
            continue;
        }
        if let Some(map_tier) = cond.map_tier.as_deref().filter(|t| !t.is_empty()) {
            // Truthy, so never NaN: the comparisons can be flipped.
            let Some(tier) = item.map_tier.filter(|t| js::truthy(*t)) else { continue };
            if map_tier == "W" && tier > 5.0 {
                continue;
            }
            if map_tier == "Y" && !(6.0..=10.0).contains(&tier) {
                continue;
            }
            if map_tier == "R" && tier < 11.0 {
                continue;
            }
        }
        if let Some(implicit) = &cond.has_implicit
            && !item
                .stats_by_type
                .iter()
                .any(|c| c.kind == ModifierType::Implicit && c.stat.stat_ref == implicit.stat_ref)
        {
            continue;
        }
        if let Some(explicit) = &cond.has_explicit
            && !item
                .stats_by_type
                .iter()
                .any(|c| c.kind == ModifierType::Explicit && c.stat.stat_ref == explicit.stat_ref)
        {
            continue;
        }
        if let Some(text) = cond.section_text.as_deref().filter(|t| !t.is_empty())
            && !item.raw_text.contains(text)
        {
            continue;
        }
        return Ok(Some(variant));
    }
    Ok(None)
}

fn parse_name_plate(d: &Data, section: &mut Vec<String>) -> R<Item> {
    let mut lines = std::mem::take(section).into_iter();
    let mut line = lines.next();
    if !line.as_deref().is_some_and(|l| l.starts_with(d.s("ITEM_CLASS"))) {
        return Err(Fail::THROW);
    }

    line = lines.next();
    let mut rarity_text = None;
    if let Some(l) = line.as_deref().filter(|l| l.starts_with(d.s("RARITY"))) {
        rarity_text = Some(l[d.s("RARITY").len()..].to_owned());
        line = lines.next();
    }

    let name = markup_condition_parser(&line.ok_or(Fail::THROW)?);
    // `line && markupConditionParser(line)`: an empty line stays "".
    let base_type = lines.next().map(|l| if l.is_empty() { l } else { markup_condition_parser(&l) });

    let mut item = Item::empty(name, base_type);
    let rarity = rarity_text.as_deref();
    let is = |key: &str| rarity == Some(d.s(key));
    if is("RARITY_CURRENCY") {
        item.category = Some(ItemCategory::Currency);
    } else if is("RARITY_DIVCARD") {
        item.category = Some(ItemCategory::DivinationCard);
    } else if is("RARITY_GEM") {
        item.category = Some(ItemCategory::Gem);
    } else if is("RARITY_NORMAL") || is("RARITY_QUEST") {
        item.rarity = Some(Rarity::Normal);
    } else if is("RARITY_MAGIC") {
        item.rarity = Some(Rarity::Magic);
    } else if is("RARITY_RARE") {
        item.rarity = Some(Rarity::Rare);
    } else if is("RARITY_UNIQUE") {
        item.rarity = Some(Rarity::Unique);
    }
    Ok(item)
}

fn parse_influence(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if section.len() <= 2 {
        let count_before = item.influences.len();
        for line in section {
            let influence = [
                ("INFLUENCE_CRUSADER", ItemInfluence::Crusader),
                ("INFLUENCE_ELDER", ItemInfluence::Elder),
                ("INFLUENCE_SHAPER", ItemInfluence::Shaper),
                ("INFLUENCE_HUNTER", ItemInfluence::Hunter),
                ("INFLUENCE_REDEEMER", ItemInfluence::Redeemer),
                ("INFLUENCE_WARLORD", ItemInfluence::Warlord),
            ]
            .into_iter()
            .find(|(key, _)| line == d.s(key));
            if let Some((_, influence)) = influence {
                item.influences.push(influence);
            }
        }
        if count_before < item.influences.len() {
            return Ok(Parsed);
        }
    }
    Ok(Skipped)
}

fn parse_corrupted(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if first_is(section, d.s("CORRUPTED")) {
        item.is_corrupted = Some(Corrupted::default());
        Ok(Parsed)
    } else if first_is(section, d.s("TWICE_CORRUPTED")) {
        item.is_corrupted = Some(Corrupted { twice: Some(true) });
        Ok(Parsed)
    } else if first_is(section, d.s("UNMODIFIABLE")) {
        item.is_corrupted = Some(Corrupted::default());
        item.is_unmodifiable = Some(true);
        Ok(Parsed)
    } else {
        Ok(Skipped)
    }
}

fn parse_foil(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if item.rarity != Some(Rarity::Unique) {
        return Ok(ParserSkipped);
    }
    if first_is(section, d.s("FOIL_UNIQUE")) {
        item.is_foil = Some(true);
        return Ok(Parsed);
    }
    Ok(Skipped)
}

fn parse_unidentified(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if first_is(section, d.s("UNIDENTIFIED")) {
        item.is_unidentified = true;
        return Ok(Parsed);
    }
    Ok(Skipped)
}

fn parse_item_level(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    let prefix = if item.ref_name_is("Filled Coffin") { d.s("CORPSE_LEVEL") } else { d.s("ITEM_LEVEL") };
    for line in section {
        if let Some(rest) = line.strip_prefix(prefix) {
            item.item_level = Some(js::number(rest));
            return Ok(Parsed);
        }
    }
    Ok(Skipped)
}

fn parse_talisman_tier(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if let Some(rest) = first(section)?.strip_prefix(d.s("TALISMAN_TIER")) {
        item.talisman_tier = Some(js::number(rest));
        return Ok(Parsed);
    }
    Ok(Skipped)
}

fn parse_vaal_gem(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if item.category != Some(ItemCategory::Gem) {
        return Ok(ParserSkipped);
    }
    if section.len() == 1
        && let Some(gem) = d.item_by_translated(Namespace::Gem, &section[0])
    {
        item.vaal_gem = gem.into_iter().next();
        return Ok(Parsed);
    }
    Ok(Skipped)
}

fn parse_gem(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if item.category != Some(ItemCategory::Gem) {
        return Ok(ParserSkipped);
    }
    if let Some(level) = section.get(1).and_then(|l| l.strip_prefix(d.s("GEM_LEVEL"))) {
        // "Level: 20 (Max)"
        item.gem_level = Some(js::parse_int(level));
        parse_quality_nested(d, section, item);
        return Ok(Parsed);
    }
    Ok(Skipped)
}

fn parse_imbued_gem(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if item.category != Some(ItemCategory::Gem) {
        return Ok(ParserSkipped);
    }
    if section.len() == 1 {
        let Some((stat, matcher)) = d.stat_by_match_str(&section[0]) else { return Ok(Skipped) };
        item.new_mods.push(ParsedModifier {
            info: ModifierInfo::new(ModifierType::Imbued),
            stats: vec![ParsedStat { stat, translation: matcher, roll: None }],
        });
        item.imbued_gem = Some(true);
        return Ok(Parsed);
    }
    Ok(Skipped)
}

fn parse_stack_size(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if item.rarity != Some(Rarity::Normal)
        && item.category != Some(ItemCategory::Currency)
        && item.category != Some(ItemCategory::DivinationCard)
    {
        return Ok(ParserSkipped);
    }
    if let Some(rest) = first(section)?.strip_prefix(d.s("STACK_SIZE")) {
        // Portal Scroll "Stack Size: 2[localized separator]448/40"
        let digits: String = rest.chars().filter(|c| c.is_ascii_digit() || *c == '/').collect();
        let mut parts = digits.split('/').map(js::number);
        item.stack_size = Some(StackSize { value: parts.next().unwrap(), max: parts.next() });
        return Ok(Parsed);
    }
    Ok(Skipped)
}

fn parse_sockets(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if let Some(rest) = first(section)?.strip_prefix(d.s("SOCKETS")) {
        let sockets = js::trim_end(rest);
        let white = sockets.matches('W').count() as f64;
        let shape: String = sockets.chars().map(|c| if c == ' ' || c == '-' { c } else { '#' }).collect();
        let linked = match shape.as_str() {
            "#-#-#-#-#-#" => Some(6),
            "# #-#-#-#-#" | "#-#-#-#-# #" | "#-#-#-#-#" => Some(5),
            _ => None,
        };
        item.sockets = Some(Sockets { linked, white });
        return Ok(Parsed);
    }
    Ok(Skipped)
}

fn parse_quality_nested(d: &Data, section: &[String], item: &mut Item) -> bool {
    for line in section {
        if line.starts_with(d.s("QUALITY")) {
            // "Quality: +20% (augmented)"
            item.quality = Some(int_after(line, d.s("QUALITY")));
            return true;
        }
    }
    false
}

fn parse_memory_strands_nested(d: &Data, section: &[String], item: &mut Item) -> bool {
    for line in section {
        if line.starts_with(d.s("MEMORY_STRANDS")) {
            item.memory_strands = Some(int_after(line, d.s("MEMORY_STRANDS")));
            return true;
        }
    }
    false
}

fn parse_armour(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    let mut is_parsed = Skipped;
    for line in section {
        let props: [(&str, &mut Option<f64>); 5] = [
            ("ARMOUR", &mut item.armour_ar),
            ("EVASION", &mut item.armour_ev),
            ("ENERGY_SHIELD", &mut item.armour_es),
            ("TAG_WARD", &mut item.armour_ward),
            ("BLOCK_CHANCE", &mut item.armour_block),
        ];
        for (key, field) in props {
            if line.starts_with(d.s(key)) {
                *field = Some(int_after(line, d.s(key)));
                is_parsed = Parsed;
                break;
            }
        }
    }
    if is_parsed == Parsed {
        parse_quality_nested(d, section, item);
        parse_memory_strands_nested(d, section, item);
    }
    Ok(is_parsed)
}

/// `getRollOrMinmaxAvg` over `"min-max"` parsed with `parseInt`.
fn damage_range(text: &str) -> f64 {
    stat_translations::get_roll_or_minmax_avg(&text.split('-').map(js::parse_int).collect::<Vec<_>>())
}

fn parse_weapon(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    let mut is_parsed = Skipped;
    for line in section {
        if let Some(rest) = line.strip_prefix(d.s("CRIT_CHANCE")) {
            item.weapon_crit = Some(js::parse_float(rest));
        } else if let Some(rest) = line.strip_prefix(d.s("ATTACK_SPEED")) {
            item.weapon_as = Some(js::parse_float(rest));
        } else if let Some(rest) = line.strip_prefix(d.s("PHYSICAL_DAMAGE")) {
            item.weapon_physical = Some(damage_range(rest));
        } else if let Some(rest) = line.strip_prefix(d.s("ELEMENTAL_DAMAGE")) {
            item.weapon_elemental = Some(rest.split(", ").map(damage_range).fold(0.0, |sum, x| sum + x));
        } else {
            continue;
        }
        is_parsed = Parsed;
    }
    if is_parsed == Parsed {
        parse_quality_nested(d, section, item);
        parse_memory_strands_nested(d, section, item);
    }
    Ok(is_parsed)
}

fn parse_accessory(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    let category = item.category.as_ref();
    if !category.is_some_and(|c| JEWELLERY.contains(c)) && category != Some(&ItemCategory::Quiver) {
        return Ok(ParserSkipped);
    }
    let mut is_parsed = Skipped;
    for line in section {
        let Some(text) = line.strip_suffix(VALUE_AUGMENTED) else { continue };
        let stat = StatString { string: text.to_owned(), unscalable: true };
        if let Some(found) = try_parse_translation(d, &stat, ModifierType::Pseudo, None)?
            && found.stat.jewellery_quality.is_some()
        {
            item.quality = Some(found.roll.as_ref().ok_or(Fail::THROW)?.value);
            item.new_mods.push(ParsedModifier { info: ModifierInfo::new(ModifierType::Pseudo), stats: vec![found] });
            is_parsed = Parsed;
        }
    }
    if parse_memory_strands_nested(d, section, item) {
        is_parsed = Parsed;
    }
    Ok(is_parsed)
}

fn parse_viscera(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if !item.ref_name_is("Viscera") {
        return Ok(ParserSkipped);
    }
    if section.len() == 1 {
        let category = [
            ("VISCERA_HEAD", ItemCategory::HeadViscera),
            ("VISCERA_LOWER_LIMB", ItemCategory::LowerLimbViscera),
            ("VISCERA_UPPER_LIMB", ItemCategory::UpperLimbViscera),
            ("VISCERA_TORSO", ItemCategory::TorsoViscera),
        ]
        .into_iter()
        .find(|(key, _)| section[0] == d.s(key));
        if let Some((_, category)) = category {
            item.category = Some(category);
        }
        // Also true when the category was already set.
        if item.category.is_some() {
            return Ok(Parsed);
        }
    }
    Ok(Skipped)
}

fn plain_mod(kind: ModifierType, stat: crate::data::Stat, translation: crate::data::StatMatcher) -> ParsedModifier {
    ParsedModifier { info: ModifierInfo::new(kind), stats: vec![ParsedStat { stat, translation, roll: None }] }
}

fn parse_logbook_area(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if !item.ref_name_is("Expedition Logbook") {
        return Ok(ParserSkipped);
    }
    if section.len() < 3 {
        return Ok(Skipped);
    }
    // Skip the Logbook Area line, parse the Faction.
    let Some((stat, matcher)) =
        d.stat_by_match_str(&section[1]).filter(|(s, _)| s.stat_ref.starts_with("Has Logbook Faction:"))
    else {
        return Ok(Skipped);
    };
    let mut area_mods = vec![plain_mod(ModifierType::Pseudo, stat, matcher)];

    let (mod_type, lines) = parse_mod_type(&section[2..])?;
    for line in &lines {
        // Area contains an Expedition Boss (#)
        if let Some((stat, matcher)) = d.stat_by_match_str(line)
            && stat.better == crate::data::stat_better::NOT_COMPARABLE
        {
            area_mods.push(plain_mod(mod_type, stat, matcher));
        }
    }
    item.logbook_area_mods.get_or_insert_with(Vec::new).push(area_mods);
    Ok(Parsed)
}

fn parse_mercenary(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if !item.ref_name_is("Mercenary Warrant") {
        return Ok(ParserSkipped);
    }
    for line in section {
        if let Some(rest) = line.strip_prefix(d.s("MERCENARY_LEVEL")) {
            item.item_level = Some(js::number(rest));
        } else if let Some(rest) = line.strip_prefix(d.s("MERCENARY_BUILD")) {
            let mut build = d.item_by_translated(Namespace::MercenaryBuild, rest).ok_or(Fail::THROW)?;
            let first_build = build.first().ok_or(Fail::THROW)?;
            if let Some(MercenaryBuildRef::Ref(ref_name)) = &first_build.mercenary_build {
                build = d.item_by_ref(Namespace::MercenaryBuild, ref_name).ok_or(Fail::THROW)?;
            }
            item.mercenary_build = build.into_iter().next();
        }
    }
    Ok(if item.mercenary_build.is_some() { Parsed } else { Skipped })
}

fn parse_mercenary_gems(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if !item.ref_name_is("Mercenary Warrant") {
        return Ok(ParserSkipped);
    }
    let category = Some(&ItemCategory::MercenaryWarrant);
    let head = first(section)?;
    let skill = StatString { string: head.to_owned(), unscalable: true };
    let Some(skill) = try_parse_translation(d, &skill, ModifierType::Pseudo, category)? else { return Ok(Skipped) };

    let mut group = vec![skill];
    for line in &section[1..] {
        let support = try_parse_translation(
            d,
            &StatString { string: line.clone(), unscalable: true },
            ModifierType::Pseudo,
            category,
        )?;
        let unknown = match &support {
            None => true,
            Some(s) => {
                let merc = s.stat.mercenary.as_ref().ok_or(Fail::THROW)?;
                merc.synthetic_family == Some(true) && merc.tier != Some(3)
            }
        };
        if let Some(support) = support {
            group.push(support);
        }
        if unknown {
            item.unknown_modifiers
                .push(UnknownModifier { text: format!("{line} [{head}]"), kind: ModifierType::Pseudo });
        }
    }
    item.mercenary_skills.get_or_insert_with(Vec::new).push(group);
    Ok(Parsed)
}

fn parse_modifiers(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if !matches!(item.rarity, Some(Rarity::Normal | Rarity::Magic | Rarity::Rare | Rarity::Unique))
        && !item.ref_name_is("Viscera")
    {
        return Ok(ParserSkipped);
    }
    let recognized =
        section.iter().find(|l| l.ends_with(ENCHANT_LINE) || l.ends_with(SCOURGE_LINE) || is_mod_info_line(l));
    let Some(recognized) = recognized else { return Ok(Skipped) };

    if is_mod_info_line(recognized) {
        for (mod_line, stat_lines) in group_lines_by_mod(section) {
            let mut info = parse_mod_info_line(d, &mod_line)?;
            if stat_lines.first().is_some_and(|l| l == d.s("VEILED_PREFIX") || l == d.s("VEILED_SUFFIX")) {
                info.kind = ModifierType::Veiled;
                item.is_veiled = Some(true);
            }
            parse_stats_from_mod(d, &stat_lines, item, info)?;
        }
    } else {
        let (mod_type, lines) = parse_mod_type(section)?;
        parse_stats_from_mod(d, &lines, item, ModifierInfo::new(mod_type))?;
    }
    Ok(Parsed)
}

fn parse_mirrored(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if first_is(section, d.s("MIRRORED")) {
        item.is_mirrored = Some(true);
        return Ok(Parsed);
    }
    Ok(Skipped)
}

fn parse_split(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if section.len() == 1 && section[0] == d.s("SPLIT") {
        item.is_split = Some(true);
        return Ok(Parsed);
    }
    Ok(Skipped)
}

fn parse_flask(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if item.category != Some(ItemCategory::Flask) {
        return Ok(ParserSkipped);
    }
    // Consumes flask buffs, so they are not recognised as modifiers.
    if section.iter().any(|l| d.re("FLASK_CHARGES").is_match(l)) {
        parse_quality_nested(d, section, item);
        return Ok(Parsed);
    }
    Ok(Skipped)
}

fn parse_tincture(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if item.category != Some(ItemCategory::Tincture) {
        return Ok(ParserSkipped);
    }
    Ok(if parse_quality_nested(d, section, item) { Parsed } else { Skipped })
}

fn parse_sentinel_charge(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if item.category != Some(ItemCategory::Sentinel) {
        return Ok(ParserSkipped);
    }
    if section.len() == 1 && section[0].starts_with(d.s("SENTINEL_CHARGE")) {
        item.sentinel_charge = Some(int_after(&section[0], d.s("SENTINEL_CHARGE")));
        return Ok(Parsed);
    }
    Ok(Skipped)
}

fn parse_scrying_orb(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if !item.ref_name_is("Scrying Orb") {
        return Ok(ParserSkipped);
    }
    if section.len() == 1
        && let Some(area_name) = section[0].strip_prefix(d.s("MAP_AREA"))
    {
        let area = d.item_by_translated(Namespace::Area, area_name).ok_or(Fail::THROW)?;
        item.map_area = area.into_iter().next();
        return Ok(Parsed);
    }
    Ok(Skipped)
}

/// `parseEldritchItem`: these lines have no section of their own and trail the
/// last-ish one (explicit mods, Corrupted or Mirrored).
fn parse_eldritch_item(d: &Data, section: &mut Vec<String>) {
    while section.last().is_some_and(|l| l == d.s("ITEM_EATER") || l == d.s("ITEM_EXARCH")) {
        section.pop();
    }
}

/// `exec(text)![1]`, throwing when it does not match.
fn exec_group1(re: &Regex, text: &str) -> R<String> {
    let caps = re.captures(text).ok_or(Fail::THROW)?;
    Ok(js_string(&caps.get(1).map(str::to_owned)).to_owned())
}

fn parse_synthesised(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if section.len() == 1 && section[0] == d.s("SECTION_SYNTHESISED") {
        item.is_synthesised = Some(true);
        let re = d.re("ITEM_SYNTHESISED");
        match &item.base_type {
            Some(base) if !base.is_empty() => item.base_type = Some(exec_group1(re, base)?),
            _ => item.name = exec_group1(re, &item.name)?,
        }
        return Ok(Parsed);
    }
    Ok(Skipped)
}

fn parse_superior(item: &mut Item, d: &Data) -> R<()> {
    let unid = item.is_unidentified;
    let applies = match item.rarity {
        Some(Rarity::Normal) => true,
        Some(Rarity::Magic | Rarity::Rare | Rarity::Unique) => unid,
        None => false,
    };
    if applies && d.re("ITEM_SUPERIOR").is_match(&item.name) {
        item.name = exec_group1(d.re("ITEM_SUPERIOR"), &item.name)?;
    }
    Ok(())
}

fn parse_foulborn(item: &mut Item, d: &Data) -> R<()> {
    if item.rarity != Some(Rarity::Unique) || item.is_unidentified {
        return Ok(());
    }
    if d.re("FOULBORN_NAME").is_match(&item.name) {
        item.name = exec_group1(d.re("FOULBORN_NAME"), &item.name)?;
        item.is_foulborn = Some(true);
    }
    Ok(())
}

fn parse_vestigial(item: &mut Item, d: &Data) -> R<()> {
    if item.rarity != Some(Rarity::Unique) {
        return Ok(());
    }
    let Some(base) = item.base_type.as_deref().filter(|b| !b.is_empty()) else { return Ok(()) };
    if d.re("VESTIGIAL_NAME").is_match(base) {
        item.base_type = Some(exec_group1(d.re("VESTIGIAL_NAME"), base)?);
        item.is_vestigial = Some(true);
    }
    Ok(())
}

fn parse_category_by_help_text(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if first_is(section, d.s("BEAST_HELP")) {
        item.category = Some(ItemCategory::CapturedBeast);
    } else if first_is(section, d.s("METAMORPH_HELP")) {
        item.category = Some(ItemCategory::MetamorphSample);
    } else if first_is(section, d.s("VOIDSTONE_HELP")) {
        item.category = Some(ItemCategory::Voidstone);
    } else if first_is(section, d.s("VISCERA_HELP")) {
        item.name = "Viscera".to_owned();
    } else {
        return Ok(Skipped);
    }
    Ok(Parsed)
}

fn parse_heist_contract(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if item.category != Some(ItemCategory::HeistContract) {
        return Ok(ParserSkipped);
    }
    if !parse_area_level_nested(d, section, item) {
        return Ok(Skipped);
    }
    let mut contract = HeistContract::default();
    for line in section {
        if let Some(job) = d.re("HEIST_CONTRACT_JOB").captures(line) {
            let required = [
                ("HEIST_JOB_LOCKPICKING", "Lockpicking"),
                ("HEIST_JOB_BRUTEFORCE", "Brute Force"),
                ("HEIST_JOB_PERCEPTION", "Perception"),
                ("HEIST_JOB_DEMOLITION", "Demolition"),
                ("HEIST_JOB_COUNTERTHAUMATURGY", "Counter-Thaumaturgy"),
                ("HEIST_JOB_TRAPDISARMAMENT", "Trap Disarmament"),
                ("HEIST_JOB_AGILITY", "Agility"),
                ("HEIST_JOB_DECEPTION", "Deception"),
                ("HEIST_JOB_ENGINEERING", "Engineering"),
            ]
            .into_iter()
            .find(|(key, _)| job.name("job") == Some(d.s(key)));
            if let Some((_, required)) = required {
                contract.required_job = Some(required);
            }
            contract.job_level = Some(job.name("level").map_or(f64::NAN, js::number));
            continue;
        }
        if let Some(target) = d.re("HEIST_CONTRACT_TARGET").captures(line)
            && target.get(1) == Some(d.s("HEIST_TARGET_PRICELESS"))
        {
            contract.target_value = Some("Priceless");
        }
    }
    item.heist_contract = Some(contract);
    Ok(Parsed)
}

fn parse_heist_blueprint(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if item.category != Some(ItemCategory::HeistBlueprint) {
        return Ok(ParserSkipped);
    }
    if !parse_area_level_nested(d, section, item) {
        return Ok(Skipped);
    }
    let mut blueprint = HeistBlueprint::default();
    for line in section {
        if let Some(target) = line.strip_prefix(d.s("HEIST_BLUEPRINT_TARGET")) {
            let found = [
                ("HEIST_BLUEPRINT_ENCHANTS", "Enchants"),
                ("HEIST_BLUEPRINT_GEMS", "Gems"),
                ("HEIST_BLUEPRINT_REPLICAS", "Replicas"),
                ("HEIST_BLUEPRINT_TRINKETS", "Trinkets"),
            ]
            .into_iter()
            .find(|(key, _)| target == d.s(key));
            if let Some((_, target)) = found {
                blueprint.target = Some(target);
            }
        } else if let Some(rest) = line.strip_prefix(d.s("HEIST_WINGS_REVEALED")) {
            let mut parts = rest.split('/');
            blueprint.wings_revealed = Some(js::parse_int(parts.next().unwrap()));
            blueprint.wings_total = Some(parts.next().map_or(f64::NAN, js::parse_int));
        }
    }
    item.heist_blueprint = Some(blueprint);
    Ok(Parsed)
}

fn parse_chart(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if item.category != Some(ItemCategory::Chart) {
        return Ok(ParserSkipped);
    }
    if !parse_area_level_nested(d, section, item) {
        return Ok(Skipped);
    }
    let area = d.item_by_translated(Namespace::Area, first(section)?).ok_or(Fail::THROW)?;
    item.map_area = area.into_iter().next();
    for line in section {
        if parse_area_prop_nested(d, line, item) {
            // line parsed
        } else if line.starts_with(d.s("CHART_SULPHUR")) {
            item.chart_sulphur = Some(int_after(line, d.s("CHART_SULPHUR")));
        }
    }
    Ok(Parsed)
}

fn parse_area_level_nested(d: &Data, section: &[String], item: &mut Item) -> bool {
    for line in section {
        if let Some(rest) = line.strip_prefix(d.s("AREA_LEVEL")) {
            item.area_level = Some(js::number(rest));
            return true;
        }
    }
    false
}

fn parse_area_level(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if !["Chronicle of Atzoatl", "Expedition Logbook", "Mirrored Tablet", "Forbidden Tome"]
        .iter()
        .any(|r| item.ref_name_is(r))
    {
        return Ok(ParserSkipped);
    }
    Ok(if parse_area_level_nested(d, section, item) { Parsed } else { Skipped })
}

fn parse_atzoatl_rooms(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if !item.ref_name_is("Chronicle of Atzoatl") {
        return Ok(ParserSkipped);
    }
    if !first_is(section, d.s("INCURSION_OPEN")) {
        return Ok(Skipped);
    }
    let mut state = IncursionRoom::Open;
    for line in &section[1..] {
        if line == d.s("INCURSION_OBSTRUCTED") {
            state = IncursionRoom::Obstructed;
            continue;
        }
        match d.stat_by_match_str(line) {
            Some((stat, matcher)) => {
                let string = match state {
                    IncursionRoom::Open => matcher.string,
                    IncursionRoom::Obstructed => format!("{} {}", d.s("INCURSION_OBSTRUCTED"), matcher.string),
                };
                let value = f64::from(state as u8);
                item.new_mods.push(ParsedModifier {
                    info: ModifierInfo::new(ModifierType::Pseudo),
                    stats: vec![ParsedStat {
                        stat,
                        translation: crate::data::StatMatcher { string, advanced: None, negate: None, value: None },
                        roll: Some(StatRollInfo {
                            unscalable: true,
                            generation: None,
                            dp: false,
                            value,
                            min: value,
                            max: value,
                        }),
                    }],
                });
            }
            None => item.unknown_modifiers.push(UnknownModifier { text: line.clone(), kind: ModifierType::Pseudo }),
        }
    }
    Ok(Parsed)
}

fn parse_mirrored_tablet(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if !item.ref_name_is("Mirrored Tablet") {
        return Ok(ParserSkipped);
    }
    if section.len() < 8 {
        return Ok(Skipped);
    }
    for line in section {
        let stat = StatString { string: line.clone(), unscalable: true };
        match try_parse_translation(d, &stat, ModifierType::Pseudo, None)? {
            Some(found) => {
                item.new_mods.push(ParsedModifier { info: ModifierInfo::new(ModifierType::Pseudo), stats: vec![found] })
            }
            None => item.unknown_modifiers.push(UnknownModifier { text: line.clone(), kind: ModifierType::Pseudo }),
        }
    }
    Ok(Parsed)
}

fn parse_filled_coffin(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if !item.ref_name_is("Filled Coffin") {
        return Ok(ParserSkipped);
    }
    if !section.iter().any(|l| l.ends_with(IMPLICIT_LINE)) {
        return Ok(Skipped);
    }
    let (_, lines) = parse_mod_type(section)?;
    parse_stats_from_mod(d, &lines, item, ModifierInfo::new(ModifierType::Necropolis))?;
    Ok(Parsed)
}

fn set_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new("<<set:.+?>>", "g").unwrap())
}

fn condition_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new("<(if:.+?|elif:.+?|else)>{(.+?)}", "g").unwrap())
}

/// `markupConditionParser`: ignores `<<set:__>>` state and always takes the first
/// `<if:__>{...}` branch.
fn markup_condition_parser(text: &str) -> String {
    let text = set_regex().replace_all(text, |_| String::new());
    condition_regex().replace_all(&text, |caps| {
        if caps.get(1).is_some_and(|t| t.starts_with("if:")) {
            caps.get(2).unwrap_or("").to_owned()
        } else {
            String::new()
        }
    })
}

/// `parseStatsFromMod`: adds the modifier with every stat its lines parse to.
fn parse_stats_from_mod(d: &Data, lines: &[String], item: &mut Item, info: ModifierInfo) -> R<()> {
    let mut modifier = ParsedModifier { info, stats: Vec::new() };

    if modifier.info.kind == ModifierType::Veiled {
        let name = modifier.info.name.clone().ok_or(Fail::THROW)?;
        match d.stat_by_match_str(&name) {
            Some((stat, matcher)) => modifier.stats.push(ParsedStat { stat, translation: matcher, roll: None }),
            None => item.unknown_modifiers.push(UnknownModifier { text: name, kind: modifier.info.kind }),
        }
        item.new_mods.push(modifier);
        return Ok(());
    }

    let kind = modifier.info.kind;
    let category = item.category.clone();
    let not_parsed = lines_to_stat_strings(lines, d.s("UNSCALABLE_VALUE"), |stat| {
        Ok(match try_parse_translation(d, stat, kind, category.as_ref())? {
            Some(parsed) => {
                modifier.stats.push(parsed);
                true
            }
            None => false,
        })
    })?;
    item.new_mods.push(modifier);
    item.unknown_modifiers.extend(not_parsed.into_iter().map(|text| UnknownModifier { text, kind }));
    Ok(())
}

fn augment_modifiers(item: &mut Item, _: &Data) -> R<()> {
    let synthesised = item.is_synthesised == Some(true);
    let corrupted_unique = item.rarity == Some(Rarity::Unique) && item.is_corrupted.is_some();
    let reflecting = item.rarity == Some(Rarity::Rare)
        && item.is_mirrored == Some(true)
        && matches!(item.category, Some(ItemCategory::Ring | ItemCategory::Amulet));
    for m in &mut item.new_mods {
        if synthesised && m.info.kind == ModifierType::Implicit {
            m.info.mechanic.get_or_insert(ModifierMechanic::Synthesised);
        }
        for stat in &mut m.stats {
            let Some(roll) = &mut stat.roll else { continue };
            if roll.generation != Some(RollGeneration::Legacy) || roll.unscalable {
                continue;
            }
            if corrupted_unique {
                roll.generation = Some(RollGeneration::Volatile);
            } else if reflecting {
                roll.generation = Some(RollGeneration::Reflecting);
            }
        }
    }
    Ok(())
}

/// `transformToLegacyModifiers` (deprecated upstream).
fn transform_to_legacy_modifiers(item: &mut Item, _: &Data) -> R<()> {
    item.stats_by_type = sum_stats_by_mod_type(&item.new_mods);
    Ok(())
}

fn calc_base_percentile(item: &mut Item, _: &Data) -> R<()> {
    let Some(info) = item.unique_base.as_ref().and_then(|b| b.armour.as_ref()).or(item.info().armour.as_ref()) else {
        return Ok(());
    };
    let bounds = |b: [u32; 2]| [f64::from(b[0]), f64::from(b[1])];
    // Base percentile is the same for all defences. AR/EV -> ES -> WARD order:
    // larger rolls give a more precise result.
    let percentile = if let (true, Some(ar)) = (truthy(item.armour_ar), info.ar) {
        calc_prop_percentile(item.armour_ar.unwrap(), bounds(ar), quality_stats::ARMOUR, item)
    } else if let (true, Some(ev)) = (truthy(item.armour_ev), info.ev) {
        calc_prop_percentile(item.armour_ev.unwrap(), bounds(ev), quality_stats::EVASION, item)
    } else if let (true, Some(es)) = (truthy(item.armour_es), info.es) {
        calc_prop_percentile(item.armour_es.unwrap(), bounds(es), quality_stats::ENERGY_SHIELD, item)
    } else if let (true, Some(ward)) = (truthy(item.armour_ward), info.ward) {
        calc_prop_percentile(item.armour_ward.unwrap(), bounds(ward), quality_stats::WARD, item)
    } else {
        return Ok(());
    };
    item.base_percentile = Some(percentile);
    Ok(())
}

fn calc_disenchant_dust(item: &mut Item, _: &Data) -> R<()> {
    let Some(value) = item.info().unique.as_ref().and_then(|u| u.disenchant_value).filter(|v| js::truthy(*v)) else {
        return Ok(());
    };
    // +50% per influence type, +2% per 1% quality, +50% per corruption implicit.
    let mut increase = item.influences.len() as f64 * 50.0;
    if let Some(q) = item.quality.filter(|q| js::truthy(*q)) {
        increase += q * 2.0;
    }
    if item.is_corrupted.is_some() {
        increase += 50.0
            * item.new_mods.iter().filter(|m| m.info.mechanic == Some(ModifierMechanic::Corruption)).count() as f64;
    }
    let clamp = |x: f64, lo: f64, hi: f64| if x.is_nan() { f64::NAN } else { x.max(lo).min(hi) };
    let ilvl = item.item_level.unwrap_or(f64::NAN);
    let factors_multi = (increase + 100.0) / 100.0;
    let term1 = 50.0; // ilvl 46 and below
    let term2 = 2.0 * (clamp(ilvl, 46.0, 68.0) - 46.0); // ilvl 47 to 68
    let term3 = (3.0 * (clamp(ilvl, 46.0, 68.0) - 46.0) / 11.0).floor(); // ilvl 47 to 68
    let term4 = 25.0 * (clamp(ilvl, 68.0, 84.0) - 68.0); // ilvl 69 to 84
    let total_multi = 5.0 * (term1 + term2 + term3 + term4) * factors_multi;
    item.dust_equivalent = Some((value * total_multi).floor());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_sections() {
        let sections = item_text_to_sections("a\r\nb\n--------\n--------\nc\n");
        assert_eq!(sections, vec![vec!["a".to_owned(), "b".to_owned()], vec!["c".to_owned()]]);
        assert!(item_text_to_sections("").is_empty());
    }

    #[test]
    fn markup_conditions_take_the_first_branch() {
        assert_eq!(
            markup_condition_parser("<<set:MS>><if:MS>{Mother}<elif:FS>{Father} of Spirits"),
            "Mother of Spirits"
        );
        assert_eq!(markup_condition_parser("Plain"), "Plain");
    }
}
