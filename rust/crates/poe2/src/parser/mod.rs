//! `ref/ee2/renderer/src/parser/Parser.ts`: `parseClipboard`.
//!
//! The item text is split into sections (`--------`), and each parser in
//! [`PARSERS`] gets the remaining sections in order until one is parsed; virtual
//! parsers work on the item alone. Section parsers are ported one function per
//! TS function, in the same order. Where the reference throws (a `TypeError` on
//! `undefined`, or an explicit `throw`), parsing fails with `item.parse_error`,
//! as its `try/catch` does.
//!
//! Not ported yet: filling empty augment sockets from the price-check widget's
//! `savedAugments` (`useAugment`, which needs `calc-base.ts` and the item
//! editor). Parsing behaves as with `savedAugments: {}`, the default.

pub mod advanced_mod_desc;
pub mod augment_builder;
pub mod calc_q20;
pub mod item;
pub mod magic_name;
pub mod meta;
pub mod modifiers;
pub mod stat_translations;

use std::borrow::Cow;

use wraex_core::js;
use wraex_core::js_regex::Regex;
use wraex_core::parse::{item_text_to_sections, lines_to_stat_strings, markup_condition_parser};
use wraex_core::{Rarity, data::js_string};

use crate::data::{AugmentData, BaseType, Data, Namespace, TradeItemQuery};
use advanced_mod_desc::{
    AUGMENT_LINE, ENCHANT_LINE, IMPLICIT_LINE, ModifierInfo, ParsedModifier, group_lines_by_mod, is_mod_info_line,
    parse_mod_info_line, parse_mod_type,
};
use augment_builder::build_editor_items;
use calc_q20::{calc_prop_percentile, quality_stats};
use item::{
    AugmentSockets, EditorItem, GemSockets, Heist, IncursionRoom, Item, ItemInfluence, Requires, StackSize, Trials,
    UnknownModifier,
};
use meta::{GEM, ItemCategory};
use modifiers::{ModifierType, StatCalculated, sum_stats_by_mod_type};
use stat_translations::{ParsedStat, StatRollInfo, StatString, get_roll_or_minmax_avg, try_parse_translation};

/// Why parsing stopped: the reference's error key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fail(pub Cow<'static, str>);

impl Fail {
    /// Anything the reference throws on, caught by `parseClipboard`.
    pub const THROW: Self = Self(Cow::Borrowed("item.parse_error"));
    pub const UNKNOWN: Self = Self(Cow::Borrowed("item.unknown"));
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

/// `LANGUAGE_DETECTOR`: `(lang, displayLang, rarityLine)`.
const LANGUAGE_DETECTOR: &[(&str, &str, &str)] = &[
    ("en", "English", "Rarity: "),
    ("ru", "Русский", "Редкость: "),
    ("fr", "Français", "Rareté: "),
    ("de", "Deutsch", "Seltenheit: "),
    ("pt", "Português (Brasil)", "Raridade: "),
    ("es", "Español", "Rareza: "),
    ("th", "ไทย", "Rarity: "),
    ("ko", "한국어", "아이템 희귀도: "),
    ("cmn-Hant", "正體中文", "稀有度: "),
    ("cmn-Hans", "普通话", "Rarity: "),
    ("ja", "日本語", "レアリティ: "),
];

/// `parsers`, in order. Each section is parsed by at most one parser.
const PARSERS: &[Step] = &[
    S(parse_unidentified),
    V(parse_superior),
    V(parse_exceptional),
    V(parse_runeforged),
    S(parse_synthesised),
    S(parse_category_by_help_text),
    V(normalize_name),
    V(find_in_database),
    // -----------
    S(parse_item_level),
    S(parse_requirements),
    S(parse_talisman_tier),
    S(parse_gem),
    S(parse_armour),
    S(parse_weapon),
    S(parse_caster),
    S(parse_flask),
    S(parse_jewelery),
    S(parse_charm_slots),
    S(parse_spirit),
    S(parse_price_note),
    S(parse_timelost_radius),
    S(parse_stack_size),
    S(parse_corrupted),
    S(parse_foil),
    S(parse_influence),
    S(parse_map),
    S(parse_waystone),
    S(parse_sockets),
    S(parse_augment_sockets),
    S(parse_heist_blueprint),
    S(parse_trials),
    S(parse_area_level),
    S(parse_atzoatl_rooms),
    S(parse_mirrored_tablet),
    S(parse_filled_coffin),
    S(parse_mirrored),
    S(parse_sanctified),
    S(parse_sentinel_charge),
    S(parse_logbook_area),
    S(parse_logbook_area),
    S(parse_logbook_area),
    S(parse_modifiers), // enchant
    S(parse_modifiers), // augment
    S(parse_modifiers), // implicit
    S(parse_modifiers), // grant skill
    S(parse_modifiers), // explicit
    V(transform_to_legacy_modifiers),
    V(parse_fractured),
    V(parse_blighted_map),
    V(apply_augment_sockets),
    V(apply_elemental_added),
    V(pick_correct_variant),
    V(calc_base_percentile),
];

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
    let Some(first_section) = sections.first_mut() else { return Err(Fail::THROW) };
    let mut item = parse_name_plate(d, first_section)?;
    sections.remove(0);
    clipboard.clone_into(&mut item.raw_text);

    for step in PARSERS {
        match step {
            V(parser) => parser(&mut item, d)?,
            S(parser) => {
                let mut i = 0;
                while i < sections.len() {
                    let Some(section) = sections.get(i) else { break };
                    match parser(section, &mut item, d)? {
                        Parsed => {
                            sections.remove(i);
                            break;
                        }
                        ParserSkipped => break,
                        Skipped => i = i.saturating_add(1),
                    }
                }
            }
        }
    }
    Ok(item)
}

/// `x` is set and truthy.
fn truthy(x: Option<f64>) -> bool {
    x.is_some_and(js::truthy)
}

/// `str` set and non-empty (truthy).
fn non_empty(s: Option<&String>) -> Option<&str> {
    s.map(String::as_str).filter(|s| !s.is_empty())
}

/// `re.exec(text)![1]`, throwing when it does not match.
fn exec_group1(re: &Regex, text: &str) -> R<String> {
    let caps = re.captures(text).ok_or(Fail::THROW)?;
    Ok(js_string(&caps.get(1).map(str::to_owned)).to_owned())
}

/// `CLIENT_STRINGS_REF[key]` first, then `CLIENT_STRINGS[key]`: the group of the
/// first that matches.
fn ref_then_local(d: &Data, key: &str, text: &str) -> R<Option<String>> {
    for re in [d.re_ref(key), d.re(key)] {
        if re.is_match(text) {
            return exec_group1(re, text).map(Some);
        }
    }
    Ok(None)
}

fn normalize_name(item: &mut Item, d: &Data) -> R<()> {
    if item.rarity == Some(Rarity::Magic)
        && let Some(base_type) = magic_name::magic_basetype(d, &item.name)?
    {
        item.name = base_type;
    }

    if matches!(item.rarity, Some(Rarity::Normal | Rarity::Rare)) {
        let blight = |text: &str| -> R<Option<String>> {
            for re in [
                d.re_ref("MAP_BLIGHTED"),
                d.re_ref("MAP_BLIGHT_RAVAGED"),
                d.re("MAP_BLIGHTED"),
                d.re("MAP_BLIGHT_RAVAGED"),
            ] {
                if re.is_match(text) {
                    return exec_group1(re, text).map(Some);
                }
            }
            Ok(None)
        };
        match non_empty(item.base_type.as_ref()).map(str::to_owned) {
            Some(base) => {
                if let Some(b) = blight(&base)? {
                    item.base_type = Some(b);
                }
            }
            None => {
                if let Some(n) = blight(&item.name)? {
                    item.name = n;
                }
            }
        }
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
                name.clone_into(&mut item.name);
                break;
            }
        }
    }
    Ok(())
}

/// One `findInDatabase` lookup round (`by_ref` or translated names).
fn lookup_info(d: &Data, item: &Item, by_ref: bool) -> Option<Vec<BaseType>> {
    let find = |ns, name: &str| if by_ref { d.item_by_ref(ns, name) } else { d.item_by_translated(ns, name) };
    let base_or_name = item.base_type.as_deref().unwrap_or(&item.name);
    match &item.category {
        Some(ItemCategory::DivinationCard) => find(Namespace::DivinationCard, &item.name),
        Some(ItemCategory::CapturedBeast) => find(Namespace::CapturedBeast, base_or_name),
        Some(c) if GEM.contains(c) => find(Namespace::Gem, &item.name),
        Some(ItemCategory::MetamorphSample) => find(Namespace::Item, &item.name),
        Some(ItemCategory::Voidstone) => find(Namespace::Item, "Charged Compass"),
        _ if item.rarity == Some(Rarity::Unique) && !item.is_unidentified => find(Namespace::Unique, &item.name),
        _ => find(Namespace::Item, base_or_name),
    }
}

fn find_in_database(item: &mut Item, d: &Data) -> R<()> {
    let has = |i: &Option<Vec<BaseType>>| i.as_ref().is_some_and(|v| !v.is_empty());
    let mut info = lookup_info(d, item, true);
    if !has(&info) {
        // BUG[UPSTREAM]: https://www.pathofexile.com/forum/view-thread/3913283
        info = lookup_info(d, item, false);
    }
    if !has(&info) {
        // First attempt to find the item in the trade site's items.
        let query = TradeItemQuery {
            name: Some(&item.name),
            category: item.category.as_ref(),
            rarity: item.rarity,
            base_type: item.base_type.as_deref(),
        };
        info = d.trade_item_by_ref(&query, false);
        if !has(&info) {
            return Err(Fail::UNKNOWN);
        }
    }
    let Some(mut info) = info else { return Err(Fail::UNKNOWN) };

    if info.first().is_some_and(|i| i.unique.is_some()) {
        let base_of = |i: &BaseType| i.unique.as_ref().map(|u| u.base.clone());
        let unique_info: Vec<BaseType> = info.iter().filter(|i| base_of(i) == item.base_type).cloned().collect();
        if !unique_info.is_empty() {
            info = unique_info;
        } else if let Some(base_type) = non_empty(item.base_type.as_ref())
            && let Some(base_info) = d.item_by_translated(Namespace::Item, base_type).filter(|b| !b.is_empty())
        {
            let base_ref = base_info.first().and_then(|b| b.ref_name.clone());
            info.retain(|i| base_of(i) == base_ref);
        }
    }
    // No variant left: `item.info` is undefined and the next read throws.
    let chosen = info.first().cloned().ok_or(Fail::THROW)?;
    item.info_variants = Some(info);
    item.info = Some(chosen);
    // Same for every variant.
    let wombgift = item.info().craftable.as_ref().is_some_and(|c| c.category == ItemCategory::Wombgift.as_str());
    if item.category.is_none() || wombgift {
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

#[expect(clippy::unnecessary_wraps, reason = "`SectionParser` table entry: `PARSERS` fixes the signature")]
fn parse_map(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if let Some(rest) = section.first().and_then(|l| l.strip_prefix(d.s("MAP_TIER"))) {
        item.map_tier = Some(js::number(rest));
        return Ok(Parsed);
    }
    Ok(Skipped)
}

#[expect(clippy::unnecessary_wraps, reason = "`SectionParser` table entry: `PARSERS` fixes the signature")]
fn parse_waystone(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    let tier = item.info().map.as_ref().and_then(|m| m.tier).filter(|t| *t != 0);
    let (true, Some(tier)) = (section.first().is_some_and(|l| l.starts_with(d.s("WAYSTONE_REVIVES"))), tier) else {
        return Ok(Skipped);
    };
    // We are a map now.
    item.map_tier = Some(f64::from(tier));
    for line in section {
        let props: [(&str, &mut Option<f64>); 8] = [
            ("WAYSTONE_REVIVES", &mut item.map_revives),
            ("WAYSTONE_PACK_SIZE", &mut item.map_pack_size),
            ("WAYSTONE_MAGIC_MONSTERS", &mut item.map_magic_monsters),
            ("WAYSTONE_RARE_MONSTERS", &mut item.map_rare_monsters),
            ("WAYSTONE_DROP_CHANCE", &mut item.map_drop_chance),
            ("WAYSTONE_RARITY", &mut item.map_item_rarity),
            ("WAYSTONE_MONSTER_RARITY", &mut item.map_monster_rarity),
            ("WAYSTONE_EFFECTIVENESS", &mut item.map_effectiveness),
        ];
        for (key, field) in props {
            if let Some(rest) = line.strip_prefix(d.s(key)) {
                *field = Some(js::parse_int(rest));
                break;
            }
        }
    }
    Ok(Parsed)
}

fn parse_blighted_map(item: &mut Item, d: &Data) -> R<()> {
    if item.category != Some(ItemCategory::Map) {
        return Ok(());
    }
    let calc = item.stats_by_type.iter().find(|c| {
        c.kind == ModifierType::Implicit && c.stat.stat_ref.starts_with("Area is infested with Fungal Growths")
    });
    let Some(calc) = calc else { return Ok(()) };
    let value = calc.sources.first().and_then(|s| s.contributes).ok_or(Fail::THROW)?.value;
    #[expect(clippy::float_cmp, reason = "JS `===` on the rolled value")]
    let (blighted, map) =
        if value == 9.0 { ("Blight-ravaged", "Blight-ravaged Map") } else { ("Blighted", "Blighted Map") };
    let icon = d.item_by_ref(Namespace::Item, map).and_then(|m| m.into_iter().next()).ok_or(Fail::THROW)?.icon;
    item.map_blighted = Some(blighted);
    // `item.info` is still `infoVariants[0]` here: the same object upstream.
    if let Some(first) = item.info_variants.as_mut().and_then(|v| v.first_mut()) {
        icon.clone_into(&mut first.icon);
    }
    if let Some(info) = item.info.as_mut() {
        info.icon = icon;
    }
    Ok(())
}

#[expect(clippy::unnecessary_wraps, reason = "`VirtualParser` table entry: `PARSERS` fixes the signature")]
fn parse_fractured(item: &mut Item, _: &Data) -> R<()> {
    if item.new_mods.iter().any(|m| m.info.kind == ModifierType::Fractured) {
        item.is_fractured = Some(true);
    }
    Ok(())
}

/// `pickCorrectVariant`: the last variant whose conditions the item meets. It may
/// find none, e.g. a corrupted implicit on a Two-Stone Ring.
fn pick_correct_variant(item: &mut Item, _: &Data) -> R<()> {
    if item.info().disc.is_none() {
        return Ok(());
    }
    let set = |flag: Option<bool>| flag == Some(true);
    let tier_is = |f: &dyn Fn(f64) -> bool| item.map_tier.is_some_and(f);
    let mut picked = None;
    for variant in item.info_variants.as_deref().unwrap_or_default() {
        let cond = variant.disc.as_ref().ok_or(Fail::THROW)?;
        if set(cond.prop_ar) && !truthy(item.armour_ar)
            || set(cond.prop_ev) && !truthy(item.armour_ev)
            || set(cond.prop_es) && !truthy(item.armour_es)
        {
            continue;
        }
        match cond.map_tier.as_deref() {
            Some("W") if !tier_is(&|t| t <= 5.0) => continue,
            Some("Y") if !tier_is(&|t| (6.0..=10.0).contains(&t)) => continue,
            Some("R") if !tier_is(&|t| t >= 11.0) => continue,
            _ => {}
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
        picked = Some(variant.clone());
    }
    if let Some(variant) = picked {
        item.info = Some(variant);
    }
    Ok(())
}

/// `isItemMissingItemClass`: meta skill gems come without an item class line.
fn is_item_missing_item_class(d: &Data, section: &[String]) -> bool {
    (2..=3).contains(&section.len()) && section.first().is_some_and(|l| l.starts_with(d.s("RARITY")))
}

fn parse_name_plate(d: &Data, section: &mut Vec<String>) -> R<Item> {
    let mut lines: std::collections::VecDeque<String> = std::mem::take(section).into();
    let line = lines.pop_front();
    let mut missing_item_class = false;

    if !line.as_deref().is_some_and(|l| l.starts_with(d.s("ITEM_CLASS"))) {
        // HACK (upstream): meta skill gems. `line && section.unshift(line) && ...`:
        // only a non-empty line goes back and gets checked.
        let truthy_line = line.filter(|l| !l.is_empty());
        let checked = truthy_line.is_some();
        if let Some(l) = truthy_line {
            lines.push_front(l);
        }
        let section: Vec<String> = lines.iter().cloned().collect();
        if checked && is_item_missing_item_class(d, &section) {
            missing_item_class = true;
        } else {
            // A wrong language, or a bug.
            let second = section.get(1).ok_or(Fail::THROW)?;
            if let Some((_, display, _)) = LANGUAGE_DETECTOR.iter().find(|(_, _, rarity)| second.starts_with(rarity)) {
                let configured = LANGUAGE_DETECTOR.iter().find(|(lang, _, _)| *lang == d.lang()).ok_or(Fail::THROW)?.1;
                return Err(Fail(format!("item.wrong_language|{display}|{configured}").into()));
            }
            return Err(Fail::THROW);
        }
    }

    let mut line = lines.pop_front();
    let mut rarity_text = None;
    if let Some(l) = line.as_deref().filter(|l| l.starts_with(d.s("RARITY"))) {
        rarity_text = Some(l.get(d.s("RARITY").len()..).unwrap_or_default().to_owned());
        line = lines.pop_front();
    }
    let name = markup_condition_parser(&line.ok_or(Fail::THROW)?);
    // `line && markupConditionParser(line)`: an empty line stays "".
    let base_type = lines.pop_front().map(|l| if l.is_empty() { l } else { markup_condition_parser(&l) });

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
    if missing_item_class {
        item.category = Some(ItemCategory::Gem);
    }
    Ok(item)
}

#[expect(clippy::unnecessary_wraps, reason = "`SectionParser` table entry: `PARSERS` fixes the signature")]
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
            .find(|(key, _)| d.client_strings.text(key) == Some(line.as_str()));
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

#[expect(clippy::unnecessary_wraps, reason = "`SectionParser` table entry: `PARSERS` fixes the signature")]
fn parse_corrupted(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    let Some(line) = section.first() else { return Ok(Skipped) };
    let first = js::trim(line);
    if first == d.s("CORRUPTED") || first == d.s("DOUBLE_CORRUPTED") {
        item.is_corrupted = true;
        Ok(Parsed)
    } else if line == d.s("UNMODIFIABLE") {
        item.is_corrupted = true;
        item.is_unmodifiable = Some(true);
        Ok(Parsed)
    } else {
        Ok(Skipped)
    }
}

#[expect(clippy::unnecessary_wraps, reason = "`SectionParser` table entry: `PARSERS` fixes the signature")]
fn parse_foil(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if item.rarity != Some(Rarity::Unique) {
        return Ok(ParserSkipped);
    }
    if section.first().is_some_and(|l| l == d.s("FOIL_UNIQUE")) {
        item.is_foil = Some(true);
        return Ok(Parsed);
    }
    Ok(Skipped)
}

#[expect(clippy::unnecessary_wraps, reason = "`SectionParser` table entry: `PARSERS` fixes the signature")]
fn parse_unidentified(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    let Some(m) = section.first().and_then(|l| d.re("UNIDENTIFIED").captures(l)) else { return Ok(Skipped) };
    item.is_unidentified = true;
    if let Some(tier) = m.name("tier").filter(|t| !t.is_empty()) {
        item.unidentified_tier = Some(js::number(tier));
    }
    Ok(Parsed)
}

#[expect(clippy::unnecessary_wraps, reason = "`SectionParser` table entry: `PARSERS` fixes the signature")]
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

fn parse_requirements(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    let Some(first) = section.first() else { return Ok(Skipped) };
    if !first.starts_with(d.s("REQUIRES")) || item.category.as_ref().is_some_and(|c| GEM.contains(c)) {
        return Ok(Skipped);
    }
    // TODO (upstream): remove the throw once validated in other languages.
    let m = d.re("REQUIRES_LINE").captures(first).ok_or(Fail::THROW)?;
    let n = |group: &str| js::parse_int(m.name(group).unwrap_or("0"));
    item.requires = Some(Requires { level: n("level"), str: n("str"), dex: n("dex"), int: n("int") });
    Ok(Parsed)
}

#[expect(clippy::unnecessary_wraps, reason = "`SectionParser` table entry: `PARSERS` fixes the signature")]
fn parse_talisman_tier(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if let Some(rest) = section.first().and_then(|l| l.strip_prefix(d.s("TALISMAN_TIER"))) {
        item.talisman_tier = Some(js::number(rest));
        return Ok(Parsed);
    }
    Ok(Skipped)
}

fn parse_gem(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    let line = match item.category {
        Some(ItemCategory::Gem | ItemCategory::MetaGem) => 1,
        Some(ItemCategory::UncutGem) => 0,
        _ => return Ok(ParserSkipped),
    };
    if let Some(level) = section.get(line).and_then(|l| l.strip_prefix(d.s("GEM_LEVEL"))) {
        // "Level: 20 (Max)"
        item.gem_level = Some(js::parse_int(level));
        parse_quality_nested(d, section, item)?;
        return Ok(Parsed);
    }
    Ok(Skipped)
}

#[expect(clippy::unnecessary_wraps, reason = "`SectionParser` table entry: `PARSERS` fixes the signature")]
fn parse_stack_size(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if item.rarity != Some(Rarity::Normal)
        && !matches!(
            item.category,
            Some(ItemCategory::Currency | ItemCategory::DivinationCard | ItemCategory::MapFragment)
        )
    {
        return Ok(ParserSkipped);
    }
    if let Some(rest) = section.first().and_then(|l| l.strip_prefix(d.s("STACK_SIZE"))) {
        // Portal Scroll "Stack Size: 2[localized separator]448/40"
        let digits: String = rest.chars().filter(|c| c.is_ascii_digit() || *c == '/').collect();
        let mut parts = digits.split('/').map(js::number);
        let size = StackSize { value: parts.next().unwrap_or_default(), max: parts.next() };
        if !item.ref_name_is("Idol of Estazunti") {
            item.stack_size = Some(size);
        }
        return Ok(Parsed);
    }
    Ok(Skipped)
}

fn empty_sockets(count: f64) -> Vec<Option<EditorItem>> {
    vec![None; wraex_core::num::f64_to_usize(count)]
}

#[expect(clippy::unnecessary_wraps, reason = "`SectionParser` table entry: `PARSERS` fixes the signature")]
fn parse_augment_sockets(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    let category_max = get_max_sockets(item);
    if category_max == 0.0 {
        return Ok(ParserSkipped);
    }
    if let Some(rest) = section.first().and_then(|l| l.strip_prefix(d.s("SOCKETS"))) {
        let current = wraex_core::num::usize_to_f64(js::trim_end(rest).matches('S').count());
        let slots = if item.is_modifiable() { category_max.max(current) } else { current };
        item.augment_sockets =
            Some(AugmentSockets { empty: 0.0, current, normal: category_max, augments: empty_sockets(slots) });
        return Ok(Parsed);
    }
    // None yet, possibly.
    if item.is_modifiable() {
        item.augment_sockets = Some(AugmentSockets {
            empty: category_max,
            current: 0.0,
            normal: category_max,
            augments: empty_sockets(category_max),
        });
    }
    Ok(Skipped)
}

#[expect(clippy::unnecessary_wraps, reason = "`SectionParser` table entry: `PARSERS` fixes the signature")]
fn parse_sockets(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    let is_gem = item.category.as_ref().is_some_and(|c| GEM.contains(c));
    let Some(rest) = section.first().and_then(|l| l.strip_prefix(d.s("SOCKETS"))).filter(|_| is_gem) else {
        return Ok(Skipped);
    };
    let shape: String = js::trim_end(rest).chars().map(|c| if c == ' ' || c == '-' { c } else { '#' }).collect();
    let linked = match shape.as_str() {
        "#-#-#-#-#-#" => Some(6),
        "# #-#-#-#-#" | "#-#-#-#-# #" | "#-#-#-#-#" => Some(5),
        _ => None,
    };
    // Counted after every socket became `#`, so `white` is always 0 upstream.
    item.gem_sockets = Some(GemSockets {
        number: wraex_core::num::usize_to_f64(shape.matches('#').count()),
        linked,
        white: wraex_core::num::usize_to_f64(shape.matches('W').count()),
    });
    Ok(Parsed)
}

fn parse_quality_nested(d: &Data, section: &[String], item: &mut Item) -> R<()> {
    for line in section {
        let Some(rest) = line.strip_prefix(d.s("QUALITY")) else { continue };
        // "Quality: +20% (augmented)"
        item.quality = Some(js::parse_int(rest));
        if matches!(item.category, Some(ItemCategory::Ring | ItemCategory::Amulet | ItemCategory::Jewel)) {
            let catalysts = if item.category == Some(ItemCategory::Jewel) {
                &d.catalyst_types().refined
            } else {
                &d.catalyst_types().normal
            };
            for catalyst in catalysts {
                // `CATALYST_TO_TAG[catalyst.tags[1]]` must exist, or the loop throws.
                let tag_key = catalyst.tags.get(1).ok_or(Fail::THROW)?;
                let tags = d.catalyst_to_tag(tag_key).ok_or(Fail::THROW)?;
                if tags.iter().any(|t| line.contains(t.as_str())) {
                    item.quality_type = Some(tag_key.clone());
                    return Ok(());
                }
            }
        }
        return Ok(());
    }
    Ok(())
}

fn parse_armour(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    let mut is_parsed = Skipped;
    for line in section {
        let props: [(&str, &mut Option<f64>); 5] = [
            ("ARMOUR", &mut item.armour_ar),
            ("EVASION", &mut item.armour_ev),
            ("ENERGY_SHIELD", &mut item.armour_es),
            ("BLOCK_CHANCE", &mut item.armour_block),
            ("RUNIC_WARD", &mut item.armour_rw),
        ];
        for (key, field) in props {
            if let Some(rest) = line.strip_prefix(d.s(key)) {
                *field = Some(js::parse_int(rest));
                is_parsed = Parsed;
                break;
            }
        }
    }
    if is_parsed == Parsed {
        parse_quality_nested(d, section, item)?;
    }
    if item.rarity == Some(Rarity::Unique) {
        // Undo everything.
        item.armour_ar = None;
        item.armour_ev = None;
        item.armour_es = None;
        item.armour_rw = None;
        item.armour_block = None;
    }
    Ok(is_parsed)
}

/// `"min-max"` per element (`, `-separated), digits only, averaged and summed.
fn damage_sum(d: &Data, text: &str) -> f64 {
    text.split(", ").map(|element| damage_avg(d, element)).fold(0.0, |sum, x| sum + x)
}

fn damage_avg(d: &Data, text: &str) -> f64 {
    let values: Vec<f64> = text
        .split(d.s("HYPHEN"))
        .map(|s| js::parse_int(&s.chars().filter(char::is_ascii_digit).collect::<String>()))
        .collect();
    get_roll_or_minmax_avg(&values)
}

fn parse_weapon(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    let mut is_parsed = Skipped;
    for line in section {
        if let Some(rest) = line.strip_prefix(d.s("CRIT_CHANCE")) {
            item.weapon_crit = Some(js::parse_float(rest));
        } else if let Some(rest) = line.strip_prefix(d.s("ATTACK_SPEED")) {
            item.weapon_as = Some(js::parse_float(rest));
        } else if let Some(rest) = line.strip_prefix(d.s("PHYSICAL_DAMAGE")) {
            item.weapon_physical = Some(damage_avg(d, rest));
        } else if let Some(rest) = line.strip_prefix(d.s("ELEMENTAL_DAMAGE")) {
            item.weapon_elemental = Some(damage_sum(d, rest));
        } else if let Some(rest) =
            ["FIRE_DAMAGE", "COLD_DAMAGE", "LIGHTNING_DAMAGE"].into_iter().find_map(|key| line.strip_prefix(d.s(key)))
        {
            let damage = damage_sum(d, rest);
            item.weapon_elemental =
                Some(item.weapon_elemental.filter(|e| js::truthy(*e)).map_or(damage, |e| damage + e));
        } else if let Some(rest) = line.strip_prefix(d.s("RELOAD_SPEED")) {
            item.weapon_reload = Some(js::parse_float(rest));
        } else if let Some(rest) = line.strip_prefix(d.s("BASE_SPIRIT")) {
            item.weapon_spirit = Some(js::parse_int(rest));
        } else {
            continue;
        }
        is_parsed = Parsed;
    }
    if is_parsed == Parsed {
        parse_quality_nested(d, section, item)?;
    }
    if item.rarity == Some(Rarity::Unique) {
        // Undo everything.
        item.weapon_elemental = None;
        item.weapon_as = None;
        item.weapon_physical = None;
        item.weapon_cold = None;
        item.weapon_lightning = None;
        item.weapon_fire = None;
        item.weapon_crit = None;
        item.weapon_reload = None;
        item.weapon_spirit = None;
    }
    Ok(is_parsed)
}

fn parse_caster(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if !matches!(item.category, Some(ItemCategory::Wand | ItemCategory::Sceptre | ItemCategory::Staff)) {
        return Ok(ParserSkipped);
    }
    if section.len() == 1 && section.first().is_some_and(|l| l.starts_with(d.s("QUALITY"))) {
        parse_quality_nested(d, section, item)?;
        return Ok(Parsed);
    }
    Ok(Skipped)
}

fn parse_logbook_area(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if !item.ref_name_is("Expedition Logbook") {
        return Ok(ParserSkipped);
    }
    if section.len() < 3 {
        return Ok(Skipped);
    }
    // Skip the area, parse the faction (which is then dropped again).
    if section.get(1).is_none_or(|faction| d.stat_by_match_str(faction).is_none()) {
        return Ok(Skipped);
    }
    let Some(rest) = section.get(2..) else { return Ok(Skipped) };
    let (mod_type, lines) = parse_mod_type(d, rest);
    let mut area_mods = Vec::new();
    for line in &lines {
        let stat = StatString { string: line.clone(), unscalable: false };
        if let Some(found) = try_parse_translation(d, &stat, mod_type)? {
            area_mods.push(ParsedModifier { info: ModifierInfo::new(mod_type), stats: vec![found] });
        }
    }
    if !area_mods.is_empty() {
        item.logbook_area_mods.get_or_insert_with(Vec::new).push(area_mods);
    }
    Ok(Parsed)
}

fn parse_modifiers(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if !matches!(item.rarity, Some(Rarity::Normal | Rarity::Magic | Rarity::Rare | Rarity::Unique)) {
        return Ok(ParserSkipped);
    }
    let grants_skill = d.s("GRANTS_SKILL");
    let recognized = section.iter().find(|l| {
        l.ends_with(ENCHANT_LINE) || l.ends_with(AUGMENT_LINE) || l.starts_with(grants_skill) || is_mod_info_line(l)
    });
    let Some(recognized) = recognized else { return Ok(Skipped) };

    if is_mod_info_line(recognized) {
        for (mod_line, stat_lines) in group_lines_by_mod(section) {
            let (mod_type, lines) = parse_mod_type(d, &stat_lines);
            let mut info = parse_mod_info_line(d, &mod_line, mod_type)?;
            if item.category == Some(ItemCategory::Relic) && info.kind == ModifierType::Explicit {
                info.kind = ModifierType::Sanctum;
            }
            parse_stats_from_mod(d, &lines, item, info, None)?;
            if mod_type == ModifierType::Veiled {
                item.is_veiled = Some(true);
            }
        }
    } else {
        let (_, lines) = parse_mod_type(d, section);
        let kind = if recognized.ends_with(ENCHANT_LINE) {
            ModifierType::Enchant
        } else if recognized.starts_with(grants_skill) {
            ModifierType::Skill
        } else {
            ModifierType::Augment
        };
        parse_stats_from_mod(d, &lines, item, ModifierInfo::new(kind), None)?;
    }
    Ok(Parsed)
}

#[expect(clippy::unnecessary_wraps, reason = "`SectionParser` table entry: `PARSERS` fixes the signature")]
fn parse_mirrored(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if section.len() == 1 && section.first().is_some_and(|l| l == d.s("MIRRORED")) {
        item.is_mirrored = Some(true);
        return Ok(Parsed);
    }
    Ok(Skipped)
}

#[expect(clippy::unnecessary_wraps, reason = "`SectionParser` table entry: `PARSERS` fixes the signature")]
fn parse_sanctified(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if section.len() == 1 && section.first().is_some_and(|l| l == d.s("SANCTIFIED")) {
        item.is_sanctified = Some(true);
        return Ok(Parsed);
    }
    Ok(Skipped)
}

/// Consumes flask buffs so they are not recognised as modifiers. Upstream checks
/// `if (isParsed)` on a non-empty string, so quality is read from every section
/// this parser visits.
fn parse_flask(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    let is_parsed = if section.iter().any(|l| d.re("FLASK_CHARGES").is_match(l)) { Parsed } else { Skipped };
    parse_quality_nested(d, section, item)?;
    Ok(is_parsed)
}

#[expect(clippy::unnecessary_wraps, reason = "`SectionParser` table entry: `PARSERS` fixes the signature")]
fn parse_jewelery(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if !matches!(item.category, Some(ItemCategory::Amulet | ItemCategory::Ring | ItemCategory::Belt)) {
        return Ok(ParserSkipped);
    }
    // `QUALITY.substring(0, QUALITY.indexOf(":"))`: empty without a colon.
    let quality = d.s("QUALITY");
    let prefix = quality.find(':').map_or("", |i| quality.get(..i).unwrap_or_default());
    Ok(if section.iter().any(|l| l.starts_with(prefix)) { Parsed } else { Skipped })
}

/// Consumes "Charm Slots" sections so they are not recognised as modifiers.
#[expect(clippy::unnecessary_wraps, reason = "`SectionParser` table entry: `PARSERS` fixes the signature")]
fn parse_charm_slots(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if item.category != Some(ItemCategory::Belt) {
        return Ok(ParserSkipped);
    }
    Ok(if section.iter().any(|l| l.starts_with(d.s("CHARM_SLOTS"))) { Parsed } else { Skipped })
}

/// Consumes "Spirit: 100" sections so they are not recognised as modifiers.
#[expect(clippy::unnecessary_wraps, reason = "`SectionParser` table entry: `PARSERS` fixes the signature")]
fn parse_spirit(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if item.category != Some(ItemCategory::Sceptre) {
        return Ok(ParserSkipped);
    }
    Ok(if section.iter().any(|l| l.starts_with(d.s("BASE_SPIRIT"))) { Parsed } else { Skipped })
}

#[expect(clippy::unnecessary_wraps, reason = "`SectionParser` table entry: `PARSERS` fixes the signature")]
fn parse_price_note(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    for line in section {
        if let Some(note) = line.strip_prefix(d.s("PRICE_NOTE")) {
            item.note = Some(note.to_owned());
            return Ok(Parsed);
        }
    }
    Ok(Skipped)
}

#[expect(clippy::unnecessary_wraps, reason = "`SectionParser` table entry: `PARSERS` fixes the signature")]
fn parse_timelost_radius(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if item.category != Some(ItemCategory::Jewel) {
        return Ok(ParserSkipped);
    }
    Ok(if section.iter().any(|l| l.starts_with(d.s("TIMELESS_RADIUS"))) { Parsed } else { Skipped })
}

#[expect(clippy::unnecessary_wraps, reason = "`SectionParser` table entry: `PARSERS` fixes the signature")]
fn parse_sentinel_charge(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if item.category != Some(ItemCategory::Sentinel) {
        return Ok(ParserSkipped);
    }
    if section.len() == 1
        && let Some(rest) = section.first().and_then(|l| l.strip_prefix(d.s("SENTINEL_CHARGE")))
    {
        item.sentinel_charge = Some(js::parse_int(rest));
        return Ok(Parsed);
    }
    Ok(Skipped)
}

fn parse_synthesised(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if section.len() == 1 && section.first().is_some_and(|l| l == d.s("SECTION_SYNTHESISED")) {
        item.is_synthesised = Some(true);
        let exec = |text: &str| -> R<String> {
            match d.re_ref("ITEM_SYNTHESISED").captures(text) {
                Some(_) => exec_group1(d.re_ref("ITEM_SYNTHESISED"), text),
                None => exec_group1(d.re("ITEM_SYNTHESISED"), text),
            }
        };
        match non_empty(item.base_type.as_ref()).map(str::to_owned) {
            Some(base) => item.base_type = Some(exec(&base)?),
            None => item.name = exec(&item.name)?,
        }
        return Ok(Parsed);
    }
    Ok(Skipped)
}

/// `Normal`, or an unidentified magic, rare or unique item.
const fn may_have_quality_prefix(item: &Item) -> bool {
    match item.rarity {
        Some(Rarity::Normal) => true,
        Some(Rarity::Magic | Rarity::Rare | Rarity::Unique) => item.is_unidentified,
        None => false,
    }
}

fn parse_superior(item: &mut Item, d: &Data) -> R<()> {
    if may_have_quality_prefix(item)
        && let Some(name) = ref_then_local(d, "ITEM_SUPERIOR", &item.name)?
    {
        item.name = name;
    }
    Ok(())
}

fn parse_exceptional(item: &mut Item, d: &Data) -> R<()> {
    if may_have_quality_prefix(item)
        && let Some(name) = ref_then_local(d, "ITEM_EXCEPTIONAL", &item.name)?
    {
        item.name = name;
    }
    Ok(())
}

/// Runeforged items have a different base type upstream, so nothing is done.
#[expect(clippy::unnecessary_wraps, reason = "`VirtualParser` table entry: `PARSERS` fixes the signature")]
const fn parse_runeforged(_: &mut Item, _: &Data) -> R<()> {
    Ok(())
}

#[expect(clippy::unnecessary_wraps, reason = "`SectionParser` table entry: `PARSERS` fixes the signature")]
fn parse_category_by_help_text(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    let category = [
        ("BEAST_HELP", ItemCategory::CapturedBeast),
        ("METAMORPH_HELP", ItemCategory::MetamorphSample),
        ("VOIDSTONE_HELP", ItemCategory::Voidstone),
    ]
    .into_iter()
    .find(|(key, _)| section.first().is_some_and(|l| l == d.s(key)));
    match category {
        Some((_, category)) => {
            item.category = Some(category);
            Ok(Parsed)
        }
        None => Ok(Skipped),
    }
}

#[expect(clippy::unnecessary_wraps, reason = "`SectionParser` table entry: `PARSERS` fixes the signature")]
fn parse_heist_blueprint(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if item.category != Some(ItemCategory::HeistBlueprint) {
        return Ok(ParserSkipped);
    }
    parse_area_level_nested(d, section, item);
    if !truthy(item.area_level) {
        return Ok(Skipped);
    }
    let mut heist = Heist::default();
    for line in section {
        if let Some(target) = line.strip_prefix(d.s("HEIST_TARGET")) {
            let found = [
                ("HEIST_BLUEPRINT_ENCHANTS", "Enchants"),
                ("HEIST_BLUEPRINT_GEMS", "Gems"),
                ("HEIST_BLUEPRINT_REPLICAS", "Replicas"),
                ("HEIST_BLUEPRINT_TRINKETS", "Trinkets"),
            ]
            .into_iter()
            .find(|(key, _)| target == d.s(key));
            if let Some((_, target)) = found {
                heist.target = Some(target);
            }
        } else if let Some(rest) = line.strip_prefix(d.s("HEIST_WINGS_REVEALED")) {
            heist.wings_revealed = Some(js::parse_int(rest));
        }
    }
    item.heist = Some(heist);
    Ok(Parsed)
}

#[expect(clippy::unnecessary_wraps, reason = "`SectionParser` table entry: `PARSERS` fixes the signature")]
fn parse_trials(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if !item.ref_name_is("Djinn Barya") && !item.ref_name_is("Inscribed Ultimatum") {
        return Ok(ParserSkipped);
    }
    parse_area_level_nested(d, section, item);
    if !truthy(item.area_level) {
        return Ok(Skipped);
    }
    let ultimatum = item.ref_name_is("Inscribed Ultimatum");
    let mut trials = Trials::default();
    for line in section {
        if let Some(rest) = line.strip_prefix(d.s("TRIAL_COUNT")) {
            trials.number_of_trials = Some(js::number(rest));
        } else if ultimatum {
            if line.starts_with(d.s("ULTIMATUM_VICTORIOUS")) {
                trials.ultimatum_hint = Some("Victorious");
            } else if line.starts_with(d.s("ULTIMATUM_COWARDLY")) {
                trials.ultimatum_hint = Some("Cowardly");
            } else if line.starts_with(d.s("ULTIMATUM_DEADLY")) {
                trials.ultimatum_hint = Some("Deadly");
            }
        }
    }
    item.trials = Some(trials);
    Ok(Parsed)
}

fn parse_area_level_nested(d: &Data, section: &[String], item: &mut Item) {
    if let Some(rest) = section.iter().find_map(|l| l.strip_prefix(d.s("AREA_LEVEL"))) {
        item.area_level = Some(js::number(rest));
    }
}

#[expect(clippy::unnecessary_wraps, reason = "`SectionParser` table entry: `PARSERS` fixes the signature")]
fn parse_area_level(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if !["Chronicle of Atzoatl", "Expedition Logbook", "Mirrored Tablet", "Forbidden Tome"]
        .iter()
        .any(|r| item.ref_name_is(r))
    {
        return Ok(ParserSkipped);
    }
    parse_area_level_nested(d, section, item);
    Ok(if truthy(item.area_level) { Parsed } else { Skipped })
}

#[expect(clippy::unnecessary_wraps, reason = "`SectionParser` table entry: `PARSERS` fixes the signature")]
fn parse_atzoatl_rooms(section: &[String], item: &mut Item, d: &Data) -> R<Section> {
    if !item.ref_name_is("Chronicle of Atzoatl") {
        return Ok(ParserSkipped);
    }
    if section.first().is_none_or(|l| l != d.s("INCURSION_OPEN")) {
        return Ok(Skipped);
    }
    let mut state = IncursionRoom::Open;
    let Some(rooms) = section.get(1..) else { return Ok(Skipped) };
    for line in rooms {
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
                let value = f64::from(state.value());
                item.new_mods.push(ParsedModifier {
                    info: ModifierInfo::new(ModifierType::Pseudo),
                    stats: vec![ParsedStat {
                        stat,
                        translation: crate::data::StatMatcher {
                            string,
                            advanced: None,
                            negate: None,
                            value: None,
                            oils: None,
                        },
                        roll: Some(StatRollInfo {
                            unscalable: true,
                            legacy: None,
                            dp: false,
                            value,
                            min: value,
                            max: value,
                            option: None,
                        }),
                        from_added_augment: None,
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
        match try_parse_translation(d, &stat, ModifierType::Pseudo)? {
            Some(found) => {
                item.new_mods
                    .push(ParsedModifier { info: ModifierInfo::new(ModifierType::Pseudo), stats: vec![found] });
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
    let (_, lines) = parse_mod_type(d, section);
    parse_stats_from_mod(d, &lines, item, ModifierInfo::new(ModifierType::Necropolis), None)?;
    Ok(Parsed)
}

/// Built-in skills that are skipped entirely.
const BUILTIN_SKILLS: [&str; 3] = ["Grants Skill: Parry", "Grants Skill: Raise Shield", "Grants Skill: Spear Throw"];

/// `parseStatsFromMod`: adds the modifier with every stat its lines parse to.
pub fn parse_stats_from_mod(
    d: &Data,
    lines: &[String],
    item: &mut Item,
    info: ModifierInfo,
    added_augment: Option<&BaseType>,
) -> R<bool> {
    let mut modifier = ParsedModifier { info, stats: Vec::new() };
    if modifier.info.kind == ModifierType::Veiled {
        item.new_mods.push(modifier);
        return Ok(true);
    }

    let kind = modifier.info.kind;
    let from_nothing = item.ref_name_is("From Nothing");
    let not_parsed = lines_to_stat_strings(lines, d.s("UNSCALABLE_VALUE"), |stat| {
        let mut stat = stat.clone();
        if from_nothing {
            stat.string = stat.string.replacen("()", "", 1);
        }
        let Some(mut parsed) = try_parse_translation(d, &stat, kind)? else { return Ok(false) };
        // Fully skip built-in skills.
        if BUILTIN_SKILLS.contains(&parsed.stat.stat_ref.as_str()) {
            return Ok(true);
        }
        // Keep the augment base type on the stat.
        if kind == ModifierType::AddedAugment {
            parsed.from_added_augment = added_augment.cloned();
        }
        modifier.stats.push(parsed);
        Ok(true)
    })?;
    item.new_mods.push(modifier);
    item.unknown_modifiers.extend(not_parsed.into_iter().map(|text| UnknownModifier { text, kind }));
    Ok(true)
}

/// `transformToLegacyModifiers` (deprecated upstream).
#[expect(clippy::unnecessary_wraps, reason = "`VirtualParser` table entry: `PARSERS` fixes the signature")]
fn transform_to_legacy_modifiers(item: &mut Item, _: &Data) -> R<()> {
    item.stats_by_type = sum_stats_by_mod_type(&item.new_mods);
    Ok(())
}

/// `combinations` (`utils.ts`): every subset, in the reference's order.
fn combinations(n: usize) -> Vec<Vec<usize>> {
    let mut acc: Vec<Vec<usize>> = vec![Vec::new()];
    for curr in 0..n {
        let extended: Vec<Vec<usize>> = acc.iter().map(|a| [a.as_slice(), &[curr]].concat()).collect();
        acc.extend(extended);
    }
    acc
}

/// `applyAugmentSockets`: which runes sit in the sockets, worked out from the
/// item's rune stats.
fn apply_augment_sockets(item: &mut Item, d: &Data) -> R<()> {
    let Some(sockets) = &item.augment_sockets else { return Ok(()) };
    let augment_mod = item.new_mods.iter().find(|m| m.info.kind == ModifierType::Augment).cloned();
    let augment_stats: Vec<&StatCalculated> = item
        .stats_by_type
        .iter()
        .filter(|c| c.kind == ModifierType::Augment && !c.stat.stat_ref.starts_with("Bonded"))
        .collect();

    let mut combos: Vec<Vec<usize>> = combinations(augment_stats.len()).into_iter().filter(|c| !c.is_empty()).collect();
    // `toSorted` is stable.
    combos.sort_by_key(|c| std::cmp::Reverse(c.len()));
    let mut augs: Vec<Vec<BaseType>> = Vec::new();
    let mut i = 0;
    while i < combos.len() {
        let Some(combo) = combos.get(i) else { break };
        let stats: Vec<&StatCalculated> = combo.iter().filter_map(|&s| augment_stats.get(s).copied()).collect();
        let used = combo.clone();
        let found = determine_augments(d, augment_mod.as_ref().ok_or(Fail::THROW)?, &stats)?;
        if !found.is_empty() {
            augs.push(found);
            for used in used {
                // Drop every combination that uses one of these stats.
                combos.retain(|c| !c.contains(&used));
            }
            i = 0;
            continue;
        }
        i = i.saturating_add(1);
    }

    let category = item.category.clone().unwrap_or(ItemCategory::Unknown);
    let mut augments: Vec<Option<EditorItem>> =
        augs.iter().flat_map(|group| build_editor_items(group, &category, true)).map(Some).collect();
    let (current, normal) = (sockets.current, sockets.normal);
    let len = wraex_core::num::usize_to_f64(augments.len());
    if len < current.max(normal) {
        // Possibly empty sockets.
        let empty = if item.is_corrupted { current - len } else { current.max(normal) - len };
        for _ in 0..wraex_core::num::f64_to_usize(empty.max(0.0)) {
            augments.push(None);
        }
    }
    if let Some(sockets) = item.augment_sockets.as_mut() {
        sockets.empty = wraex_core::num::usize_to_f64(augments.iter().filter(|a| a.is_none()).count());
        sockets.augments = augments;
    }
    // Filling empty sockets from `savedAugments` comes with the item editor; with
    // the default `savedAugments: {}` upstream returns here.
    Ok(())
}

/// `modifiedBfs`: the fewest augment values summing to `remaining`, preferring
/// repeats of one value.
fn modified_bfs(remaining: f64, combo: &mut Vec<f64>, available: &[f64]) -> Option<Vec<f64>> {
    if remaining == 0.0 {
        return Some(combo.clone());
    }
    if combo.len() >= 7 || available.len() > 8 {
        return Some(Vec::new());
    }
    #[expect(clippy::float_cmp, reason = "JS `===` when counting equal values")]
    let max_repeats = |v: &[f64]| {
        v.iter()
            .map(|x| wraex_core::num::usize_to_f64(v.iter().filter(|y| *y == x).count()))
            .fold(f64::NEG_INFINITY, f64::max)
    };
    let distinct =
        |v: &[f64]| v.iter().enumerate().filter(|(i, x)| !v.get(..*i).is_some_and(|head| head.contains(x))).count();
    let mut best: Option<Vec<f64>> = None;
    for &value in available {
        if value > remaining {
            continue;
        }
        combo.push(value);
        let result = modified_bfs(remaining - value, combo, available);
        combo.pop();
        let Some(result) = result else { continue };
        match &best {
            None => best = Some(result),
            Some(b) if result.len() < b.len() => best = Some(result),
            Some(b) if result.len() == b.len() => {
                let (rm, bm) = (max_repeats(&result), max_repeats(b));
                #[expect(clippy::float_cmp, reason = "JS `===` on the repeat counts")]
                if rm > bm || (rm == bm && distinct(&result) < distinct(b)) {
                    best = Some(result);
                }
            }
            Some(_) => {}
        }
    }
    best.map(|mut b| {
        b.sort_by(|x, y| y.partial_cmp(x).unwrap_or(std::cmp::Ordering::Equal));
        b
    })
}

/// `SameValueZero` membership, as `Set#has` does for numbers.
#[expect(clippy::float_cmp, reason = "JS `===` membership, plus the `NaN` case the reference adds")]
fn set_has(values: &[f64], x: f64) -> bool {
    values.iter().any(|v| *v == x || (v.is_nan() && x.is_nan()))
}

fn augment_item(d: &Data, ref_name: &str) -> R<BaseType> {
    d.item_by_ref(Namespace::Item, ref_name).and_then(|i| i.into_iter().next()).ok_or(Fail::THROW)
}

/// `determineAugments`: the runes that explain a set of rune stats.
fn determine_augments(d: &Data, modifier: &ParsedModifier, stat_calcs: &[&StatCalculated]) -> R<Vec<BaseType>> {
    if modifier.info.kind != ModifierType::Augment {
        return Ok(Vec::new());
    }
    let applied: Vec<f64> =
        stat_calcs.iter().filter_map(|s| s.sources.first().and_then(|src| src.contributes).map(|c| c.value)).collect();

    let mut all_possible: Vec<&[AugmentData]> = Vec::new();
    for calc in stat_calcs {
        let id = calc
            .stat
            .trade
            .ids
            .as_ref()
            .and_then(|ids| ids.get(ModifierType::Augment.as_str()))
            .and_then(|ids| ids.first())
            .ok_or(Fail::THROW)?;
        if let Some(group) = d.augment_data_by_trade_id(id) {
            all_possible.push(group);
        }
    }
    // Intersect the possible augments of every stat.
    let ref_sets: Vec<Vec<&str>> =
        all_possible.iter().map(|g| g.iter().map(|a| a.ref_name.as_str()).collect()).collect();
    let in_all = |r: &str| ref_sets.iter().all(|set| set.contains(&r));
    let Some(possible) = all_possible
        .iter()
        .map(|g| g.iter().filter(|a| in_all(&a.ref_name)).collect::<Vec<_>>())
        .find(|g| !g.is_empty())
    else {
        return Ok(Vec::new());
    };

    // Something like "Raven-Touched".
    if applied.is_empty() {
        let Some(first) = possible.first() else { return Ok(Vec::new()) };
        return Ok(vec![augment_item(d, &first.ref_name)?]);
    }

    if applied.len() > 1 {
        let likely = possible.iter().find(|aug| {
            let mut common: Vec<f64> = Vec::new();
            for v in &aug.values {
                if set_has(&applied, *v) && !set_has(&common, *v) {
                    common.push(*v);
                }
            }
            common.len() == applied.len()
        });
        return match likely {
            Some(aug) => Ok(vec![augment_item(d, &aug.ref_name)?]),
            None => Ok(Vec::new()),
        };
    }

    // How many of this augment are in the item.
    let mut available: Vec<f64> = possible
        .iter()
        .map(|aug| {
            if let [value] = aug.values.as_slice() {
                return *value;
            }
            // Stats like "# to # added Lightning Damage".
            if aug.base_stat.matches('#').count() > 1 {
                let avg = aug.values.iter().sum::<f64>() / wraex_core::num::usize_to_f64(aug.values.len());
                return if js::truthy(avg) { avg } else { 0.0 };
            }
            // Everything else; an empty list reads `values[0]` as undefined.
            aug.values.first().copied().unwrap_or(f64::NAN)
        })
        .collect();
    available.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));

    // Search all combinations with the fewest augments.
    let Some(first_applied) = applied.first() else { return Ok(Vec::new()) };
    let likely = modified_bfs(*first_applied, &mut Vec::new(), &available).unwrap_or_default();
    likely
        .iter()
        .map(|&v| {
            #[expect(clippy::float_cmp, reason = "JS `===` on the rolled average")]
            let matches_value = |aug: &AugmentData| {
                aug.values.contains(&v)
                    || aug.values.iter().sum::<f64>() / wraex_core::num::usize_to_f64(aug.values.len()) == v
            };
            let aug = possible.iter().find(|aug| matches_value(aug)).ok_or(Fail::THROW)?;
            augment_item(d, &aug.ref_name)
        })
        .collect()
}

fn apply_elemental_added(item: &mut Item, _: &Data) -> R<()> {
    if !truthy(item.weapon_elemental) || item.rarity == Some(Rarity::Unique) {
        return Ok(());
    }
    let add = |field: &mut Option<f64>, v: f64| {
        *field = Some(field.filter(|x| js::truthy(*x)).map_or(v, |x| v + x));
    };
    for calc in &item.stats_by_type {
        let field = match calc.stat.stat_ref.as_str() {
            "Adds # to # Lightning Damage" => &mut item.weapon_lightning,
            "Adds # to # Cold Damage" => &mut item.weapon_cold,
            "Adds # to # Fire Damage" => &mut item.weapon_fire,
            _ => continue,
        };
        for source in &calc.sources {
            add(field, source.contributes.ok_or(Fail::THROW)?.value);
        }
    }
    Ok(())
}

fn calc_base_percentile(item: &mut Item, d: &Data) -> R<()> {
    // The ref name, since `unique.base` is one.
    let armour = match &item.info().unique {
        Some(unique) => augment_item(d, &unique.base)?.armour,
        None => item.info().armour.clone(),
    };
    let Some(info) = armour else { return Ok(()) };
    let bounds = |b: [u32; 2]| [f64::from(b[0]), f64::from(b[1])];
    // Base percentile is the same for all defences. AR/EV -> ES order: larger
    // rolls give a more precise result. No ward: it is not used anymore.
    let truthy_value = |x: Option<f64>| x.filter(|v| js::truthy(*v));
    let percentile = if let (Some(value), Some(ar)) = (truthy_value(item.armour_ar), info.ar) {
        calc_prop_percentile(value, bounds(ar), quality_stats::ARMOUR, item)
    } else if let (Some(value), Some(ev)) = (truthy_value(item.armour_ev), info.ev) {
        calc_prop_percentile(value, bounds(ev), quality_stats::EVASION, item)
    } else if let (Some(value), Some(es)) = (truthy_value(item.armour_es), info.es) {
        calc_prop_percentile(value, bounds(es), quality_stats::ENERGY_SHIELD, item)
    } else {
        return Ok(());
    };
    item.base_percentile = Some(percentile);
    Ok(())
}

/// `getMaxSockets`: augment sockets an item has without corruption.
pub fn get_max_sockets(item: &Item) -> f64 {
    use ItemCategory as C;
    let by_name = match item.info().ref_name.as_deref() {
        Some("Atziri's Splendour") => Some(6.0),
        Some("Runeseeker's Call") => Some(5.0),
        Some("Greymake" | "Morior Invictus" | "The Bringer of Rain") => Some(4.0),
        Some("Serle's Grit") => Some(3.0),
        Some("Darkness Enthroned" | "Mahuxotl's Machination") => Some(2.0),
        Some("Grasping Ring" | "Corona Amulet" | "Stalking Belt") => Some(1.0),
        _ => None,
    };
    if let Some(n) = by_name {
        return n;
    }
    match item.category {
        Some(
            C::BodyArmour
            | C::TwoHandedAxe
            | C::TwoHandedMace
            | C::TwoHandedSword
            | C::Crossbow
            | C::Bow
            | C::Warstaff
            | C::Staff
            | C::Talisman,
        ) => 2.0,
        Some(
            C::Helmet
            | C::Shield
            | C::Gloves
            | C::Boots
            | C::OneHandedAxe
            | C::OneHandedMace
            | C::OneHandedSword
            | C::Claw
            | C::Dagger
            | C::Focus
            | C::Spear
            | C::Flail
            | C::Wand
            | C::Buckler
            | C::Sceptre,
        ) => 1.0,
        _ => 0.0,
    }
}

/// `isArmourOrWeaponOrCaster`
pub fn is_armour_or_weapon_or_caster(category: Option<&ItemCategory>) -> Option<&'static str> {
    use ItemCategory as C;
    match category? {
        C::BodyArmour | C::Boots | C::Gloves | C::Helmet | C::Shield | C::Focus | C::Buckler => Some("armour"),
        C::OneHandedAxe
        | C::OneHandedMace
        | C::OneHandedSword
        | C::Quiver
        | C::Claw
        | C::Dagger
        | C::Sceptre
        | C::TwoHandedAxe
        | C::TwoHandedMace
        | C::TwoHandedSword
        | C::Crossbow
        | C::Bow
        | C::Warstaff
        | C::Spear
        | C::Flail
        | C::Talisman => Some("weapon"),
        C::Wand | C::Staff => Some("caster"),
        _ => None,
    }
}

/// `parseAffixStrings`: `[Ref|Text]` markup to its text (or the ref alone).
#[expect(
    clippy::expect_used,
    reason = "a literal pattern from the reference: a compile failure is a bug, not an input"
)]
pub fn parse_affix_strings(text: &str) -> String {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"\[([^\]|]+)\|?([^\]]*)\]", "g").expect("valid pattern"));
    re.replace_all(text, |caps| {
        let part2 = caps.get(2).unwrap_or("");
        if part2.is_empty() { caps.get(1).unwrap_or("").to_owned() } else { part2.to_owned() }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::tests::en;

    #[test]
    fn combinations_follow_the_reference_order() {
        assert_eq!(combinations(2), vec![vec![], vec![0], vec![1], vec![0, 1]]);
        assert_eq!(combinations(3).len(), 8);
        assert_eq!(combinations(3)[4], vec![2]);
    }

    #[test]
    fn modified_bfs_prefers_fewest_and_repeats() {
        assert_eq!(modified_bfs(60.0, &mut Vec::new(), &[30.0, 20.0, 10.0]), Some(vec![30.0, 30.0]));
        assert_eq!(modified_bfs(35.0, &mut Vec::new(), &[30.0, 20.0]), None);
        assert_eq!(modified_bfs(0.0, &mut Vec::new(), &[1.0]), Some(vec![]));
    }

    #[test]
    fn affix_markup_takes_the_text() {
        assert_eq!(parse_affix_strings("[Fire|Fire Damage] and [Cold]"), "Fire Damage and Cold");
    }

    // --- `ref/ee2/renderer/specs/Parser/*.test.ts` ---

    /// The clipboard text of a fixture's case (`corpus/poe2/<slug>.txt`).
    fn case_input(slug: &str) -> String {
        wraex_golden::cases(wraex_core::GameId::Poe2)
            .into_iter()
            .find(|case| case.slug == slug)
            .unwrap_or_else(|| panic!("no fixture {slug}"))
            .input
    }

    /// The case's sections, as `itemTextToSections` splits them.
    fn sections(slug: &str) -> Vec<Vec<String>> {
        item_text_to_sections(&case_input(slug))
    }

    /// A hand-built item with the name plate's rarity and category.
    fn test_item(rarity: Rarity, category: ItemCategory) -> Item {
        let mut item = Item::test_item();
        item.rarity = Some(rarity);
        item.category = Some(category);
        item
    }

    /// `makeCalcStat` (`specs/helper.ts`): a summed stat with one source.
    fn make_calc_stat(d: &Data, stat_ref: &str, value: f64, kind: ModifierType) -> StatCalculated {
        let stat = d.stat_by_ref(stat_ref).unwrap_or_else(|| panic!("no stat {stat_ref}"));
        let parsed = ParsedStat {
            stat: stat.clone(),
            translation: stat.matchers[0].clone(),
            roll: Some(StatRollInfo {
                unscalable: false,
                legacy: None,
                dp: stat.dp == Some(true),
                value,
                min: value,
                max: value,
                option: None,
            }),
            from_added_augment: None,
        };
        StatCalculated {
            stat,
            kind,
            sources: vec![crate::parser::modifiers::StatSource {
                modifier: ParsedModifier { info: ModifierInfo::new(kind), stats: vec![parsed.clone()] },
                stat: parsed,
                contributes: Some(crate::parser::modifiers::StatRoll { value, min: value, max: value, option: None }),
            }],
        }
    }

    /// The rarity and category the test items' name plates carry.
    fn test_item_props(slug: &str) -> (Rarity, ItemCategory) {
        match slug {
            "ee2-magic-item" => (Rarity::Magic, ItemCategory::TwoHandedMace),
            "ee2-rare-item" => (Rarity::Rare, ItemCategory::Bow),
            "ee2-unique-item" => (Rarity::Unique, ItemCategory::Focus),
            "ee2-rare-with-implicit" => (Rarity::Rare, ItemCategory::Ring),
            "ee2-high-damage-rare-item" => (Rarity::Rare, ItemCategory::Crossbow),
            "ee2-armour-high-value-rare-item" => (Rarity::Rare, ItemCategory::BodyArmour),
            "ee2-wand-rare-item" => (Rarity::Rare, ItemCategory::Wand),
            "ee2-normal-shield" => (Rarity::Normal, ItemCategory::Shield),
            "ee2-two-implicit-item" => (Rarity::Rare, ItemCategory::Belt),
            "ee2-two-line-one-implicit-item" => (Rarity::Rare, ItemCategory::Tablet),
            other => panic!("no test item {other}"),
        }
    }

    fn parsed_test_item(slug: &str) -> Item {
        let (rarity, category) = test_item_props(slug);
        test_item(rarity, category)
    }

    fn lines_of(text: &str) -> Vec<String> {
        text.split('\n').map(str::to_owned).collect()
    }

    /// `specs/Parser/parsers.test.ts` (`itemTextToSections`): an empty text has no
    /// sections, and the test items split into the expected counts.
    #[test]
    fn an_empty_item_text_has_no_sections() {
        assert_eq!(item_text_to_sections(""), Vec::<Vec<String>>::new());
    }

    #[test]
    fn test_items_split_into_the_expected_sections() {
        for (slug, expected) in [
            ("ee2-rare-item", 5),
            ("ee2-magic-item", 5),
            ("ee2-normal-item", 4),
            ("ee2-unique-item", 6),
            ("ee2-rare-with-implicit", 5),
            ("ee2-high-damage-rare-item", 9),
            ("ee2-armour-high-value-rare-item", 8),
            ("ee2-wand-rare-item", 6),
        ] {
            assert_eq!(sections(slug).len(), expected, "{slug}");
        }
    }

    /// `specs/Parser/parsers.test.ts` (`parseWeapon`).
    #[test]
    fn parses_the_test_items_weapons() {
        let d = en();
        #[expect(clippy::type_complexity, reason = "one row per `parseWeapon` test case")]
        let cases: [(&str, f64, Option<f64>, f64, f64, Option<f64>, Option<f64>); 3] = [
            // physical, elemental, attacks per second, crit, reload, quality
            ("ee2-magic-item", 53.5, Some(25.5), 1.2, 5.0, None, None),
            ("ee2-rare-item", 48.5, Some(86.0), 1.2, 5.0, None, None),
            ("ee2-high-damage-rare-item", 728.5, None, 2.07, 5.0, Some(0.6), Some(29.0)),
        ];
        for (slug, physical, elemental, aps, crit, reload, quality) in cases {
            let mut item = Item::test_item();
            let section = &sections(slug)[1];
            assert_eq!(parse_weapon(section, &mut item, d), Ok(Parsed), "{slug}");
            assert_eq!(item.weapon_physical, Some(physical), "{slug}: physical");
            assert_eq!(item.weapon_elemental, elemental, "{slug}: elemental");
            assert_eq!(item.weapon_as, Some(aps), "{slug}: attack speed");
            assert_eq!(item.weapon_crit, Some(crit), "{slug}: crit");
            assert_eq!(item.weapon_reload, reload, "{slug}: reload");
            assert_eq!(item.quality, quality, "{slug}: quality");
        }
    }

    /// `specs/Parser/parsers.test.ts` (`parseArmour`).
    #[test]
    fn parses_the_test_items_armour() {
        let d = en();
        #[expect(clippy::type_complexity, reason = "one row per `parseArmour` test case")]
        let cases: [(&str, Option<f64>, Option<f64>, Option<f64>, Option<f64>, Option<f64>); 3] = [
            // armour, evasion, energy shield, quality, block
            ("ee2-normal-item", Some(174.0), None, Some(60.0), Some(9.0), None),
            ("ee2-unique-item", None, None, Some(44.0), None, None),
            ("ee2-armour-high-value-rare-item", Some(3075.0), None, None, Some(20.0), None),
        ];
        for (slug, ar, ev, es, quality, block) in cases {
            let mut item = Item::test_item();
            let section = &sections(slug)[1];
            assert_eq!(parse_armour(section, &mut item, d), Ok(Parsed), "{slug}");
            assert_eq!(item.armour_ar, ar, "{slug}: armour");
            assert_eq!(item.armour_ev, ev, "{slug}: evasion");
            assert_eq!(item.armour_es, es, "{slug}: energy shield");
            assert_eq!(item.quality, quality, "{slug}: quality");
            assert_eq!(item.armour_block, block, "{slug}: block");
        }
    }

    /// `specs/Parser/parsers.test.ts` (`parseRequirements`).
    #[test]
    fn parses_the_test_items_requirements() {
        let d = en();
        let cases = [
            ("ee2-normal-item", Requires { level: 75.0, str: 67.0, dex: 0.0, int: 67.0 }),
            ("ee2-magic-item", Requires { level: 28.0, str: 57.0, dex: 0.0, int: 0.0 }),
            ("ee2-rare-item", Requires { level: 51.0, str: 0.0, dex: 103.0, int: 0.0 }),
            ("ee2-unique-item", Requires { level: 26.0, str: 0.0, dex: 0.0, int: 43.0 }),
            ("ee2-rare-with-implicit", Requires { level: 45.0, str: 0.0, dex: 0.0, int: 0.0 }),
            ("ee2-high-damage-rare-item", Requires { level: 79.0, str: 89.0, dex: 89.0, int: 0.0 }),
            ("ee2-armour-high-value-rare-item", Requires { level: 65.0, str: 121.0, dex: 0.0, int: 0.0 }),
            ("ee2-wand-rare-item", Requires { level: 90.0, str: 0.0, dex: 0.0, int: 125.0 }),
            ("ee2-requires-one-attribute", Requires { level: 0.0, str: 0.0, dex: 0.0, int: 78.0 }),
        ];
        for (slug, requires) in cases {
            let mut item = Item::test_item();
            let requires_prefix = d.s("REQUIRES");
            let section = sections(slug)
                .into_iter()
                .find(|section| section.iter().any(|line| line.starts_with(requires_prefix)))
                .unwrap_or_else(|| panic!("{slug}: no requirements section"));
            assert_eq!(parse_requirements(&section, &mut item, d), Ok(Parsed), "{slug}");
            assert_eq!(item.requires, Some(requires), "{slug}");
        }
    }

    /// `specs/Parser/parsers.test.ts` (`parseRequirements`): the localized lines.
    #[test]
    fn parses_localized_requirements_lines() {
        let cases = [
            ("en", "Requires: Level 28, 57 (augmented) Str", Requires { level: 28.0, str: 57.0, dex: 0.0, int: 0.0 }),
            ("cmn-Hant", "需求: 等級 80, 108 (unmet) 智慧", Requires { level: 80.0, str: 0.0, dex: 0.0, int: 108.0 }),
            ("ja", "装備条件：レベル 72, 70 筋力, 70 知性", Requires { level: 72.0, str: 70.0, dex: 0.0, int: 70.0 }),
            (
                "ko",
                "요구 사항: 레벨 78, 89 힘, 89 (unmet) 민첩",
                Requires { level: 78.0, str: 89.0, dex: 89.0, int: 0.0 },
            ),
            ("cmn-Hant", "需求: 等級 78, 54 力量, 138 智慧", Requires { level: 78.0, str: 54.0, dex: 0.0, int: 138.0 }),
            (
                "ru",
                "Требуется: Уровень 80, 59 (unmet) Ловк, 59 Инт",
                Requires { level: 80.0, str: 0.0, dex: 59.0, int: 59.0 },
            ),
        ];
        for (lang, line, requires) in cases {
            let data = Data::load(&crate::data::reference_source(), lang, Some(crate::data::reference_trade_data()))
                .unwrap_or_else(|e| panic!("{lang}: {e}"));
            let mut item = Item::test_item();
            assert_eq!(parse_requirements(&[line.to_owned()], &mut item, &data), Ok(Parsed), "{lang}");
            assert_eq!(item.requires, Some(requires), "{lang}: {line}");
        }
    }

    /// `specs/Parser/modifiersParsers.test.ts` (`[e2e] parseModifiers`): every
    /// listed mod section is recognized.
    #[test]
    fn every_test_items_mod_sections_are_recognized() {
        let d = en();
        let cases: [(&str, &[usize]); 9] = [
            ("ee2-magic-item", &[4]),
            ("ee2-rare-item", &[4]),
            ("ee2-unique-item", &[4]),
            ("ee2-rare-with-implicit", &[3, 4]),
            ("ee2-high-damage-rare-item", &[5, 6, 7]),
            ("ee2-armour-high-value-rare-item", &[5, 6]),
            ("ee2-wand-rare-item", &[3, 4]),
            ("ee2-two-implicit-item", &[3, 4]),
            ("ee2-two-line-one-implicit-item", &[2, 3]),
        ];
        for (slug, indexes) in cases {
            let item_sections = sections(slug);
            for &index in indexes {
                let mut item = parsed_test_item(slug);
                assert_eq!(parse_modifiers(&item_sections[index], &mut item, d), Ok(Parsed), "{slug} section {index}");
            }
        }
    }

    /// `specs/Parser/modifiersParsers.test.ts` (`[e2e] parseModifiers`): a section
    /// adds the right number of modifiers and leaves no unknown ones.
    #[test]
    fn every_test_items_mod_section_adds_the_expected_mods() {
        let d = en();
        let cases = [
            ("ee2-magic-item", 4, 2),
            ("ee2-rare-item", 4, 4),
            ("ee2-unique-item", 4, 5),
            ("ee2-rare-with-implicit", 3, 1),
            ("ee2-rare-with-implicit", 4, 4),
            ("ee2-high-damage-rare-item", 5, 1),
            ("ee2-high-damage-rare-item", 6, 1),
            ("ee2-high-damage-rare-item", 7, 6),
            ("ee2-armour-high-value-rare-item", 5, 1),
            ("ee2-armour-high-value-rare-item", 6, 6),
            ("ee2-wand-rare-item", 3, 1),
            ("ee2-wand-rare-item", 4, 4),
            ("ee2-normal-shield", 4, 1),
            ("ee2-two-implicit-item", 3, 2),
            ("ee2-two-implicit-item", 4, 5),
            ("ee2-two-line-one-implicit-item", 2, 1),
            ("ee2-two-line-one-implicit-item", 3, 4),
        ];
        for (slug, index, expected) in cases {
            let mut item = parsed_test_item(slug);
            assert_eq!(parse_modifiers(&sections(slug)[index], &mut item, d), Ok(Parsed), "{slug} section {index}");
            assert_eq!(item.new_mods.len(), expected, "{slug} section {index}: mods");
            assert!(item.unknown_modifiers.is_empty(), "{slug} section {index}: unknown mods");
        }
    }

    /// `specs/Parser/modifiersParsers.test.ts` (`[unit] parseModifiers`): explicit
    /// modifier sections, including a fractured and a desecrated affix.
    #[test]
    fn parses_an_explicit_modifier_section() {
        let section = lines_of(
            "{ Fractured Prefix Modifier \"Electrocuting\" (Tier: 2) — Damage, Elemental, Lightning, Attack }
Adds 10(1-16) to 273(239-300) Lightning Damage
{ Prefix Modifier \"Razor-sharp\" (Tier: 3) — Damage, Physical, Attack }
Adds 24(23-35) to 51(39-59) Physical Damage
{ Master Crafted Prefix Modifier \"Overpowering\" (Tier: 2) — Damage, Elemental, Attack }
109(100-119)% increased Elemental Damage with Attacks
{ Suffix Modifier \"of the Drought\" (Tier: 1) — Mana, Physical, Attack }
Leeches 7.59(7-7.9)% of Physical Damage as Mana
{ Desecrated Suffix Modifier \"of Siphoning\" (Tier: 3) — Mana }
Gain 21(21-27) Mana per enemy killed
{ Suffix Modifier \"of Acclaim\" (Tier: 1) — Attack, Speed }
18(17-19)% increased Attack Speed
",
        );
        let mut item = test_item(Rarity::Rare, ItemCategory::Bow);
        assert_eq!(parse_modifiers(&section, &mut item, en()), Ok(Parsed));
    }

    /// `specs/Parser/modifiersParsers.test.ts` (`[unit] parseModifiers`): implicit,
    /// granted skill, augment and enchant sections.
    #[test]
    fn parses_the_other_modifier_section_kinds() {
        let d = en();
        let cases = [
            ("{ Implicit Modifier — Attack }\nLoads an additional bolt\n", ModifierType::Implicit),
            ("Grants Skill: Level 18 Cackling Companions\n", ModifierType::Skill),
            ("18% increased Physical Damage (rune)\nGain 24 Mana per enemy killed (rune)\n", ModifierType::Augment),
            ("{ Enhancement }\nAllocates Core of the Guardian — Unscalable Value\n", ModifierType::Enchant),
        ];
        for (text, kind) in cases {
            let mut item = test_item(Rarity::Rare, ItemCategory::Bow);
            assert_eq!(parse_modifiers(&lines_of(text), &mut item, d), Ok(Parsed), "{text}");
            assert_eq!(item.new_mods.len(), 1, "{text}");
            assert!(item.unknown_modifiers.is_empty(), "{text}");
            assert_eq!(item.new_mods[0].info.kind, kind, "{text}");
        }

        let mut item = test_item(Rarity::Rare, ItemCategory::Bow);
        assert_eq!(
            parse_modifiers(&lines_of("45% increased Elemental Damage with Attacks (enchant)\n"), &mut item, d),
            Ok(Parsed)
        );
    }

    /// `specs/Parser/fractureParsers.test.ts` (`parseFractured`).
    #[test]
    fn a_fractured_mod_marks_the_item_fractured() {
        let d = en();
        let mut item = Item::test_item();
        item.new_mods = [ModifierType::Fractured, ModifierType::Explicit, ModifierType::Explicit]
            .into_iter()
            .map(|kind| ParsedModifier { info: ModifierInfo::new(kind), stats: vec![] })
            .collect();
        parse_fractured(&mut item, d).unwrap();
        assert_eq!(item.is_fractured, Some(true));

        let mut item = Item::test_item();
        item.new_mods = [ModifierType::Implicit, ModifierType::Explicit]
            .into_iter()
            .map(|kind| ParsedModifier { info: ModifierInfo::new(kind), stats: vec![] })
            .collect();
        parse_fractured(&mut item, d).unwrap();
        assert_eq!(item.is_fractured, None);
    }

    /// `specs/Parser/mapParser.test.ts` (`parseMap`): the waystone section sets the
    /// map tier and its properties.
    #[test]
    fn parses_a_waystones_properties() {
        let d = en();
        #[expect(clippy::type_complexity, reason = "one row per `parseMap` test case")]
        let cases: [(&str, u32, f64, f64, Option<f64>, f64, f64, Option<f64>); 2] = [
            // tier, revives, pack size, magic monsters, rare monsters, drop chance
            ("ee2-rare-map", 14, 2.0, 34.0, None, 28.0, 75.0, None),
            ("ee2-rare-map-fake-all-props", 16, 0.0, 20.0, Some(30.0), 71.0, 90.0, Some(17.0)),
        ];
        for (slug, tier, revives, pack, magic, rare, drop, rarity) in cases {
            let mut item = Item::test_item();
            item.category = Some(ItemCategory::Map);
            item.info.as_mut().unwrap().map = Some(crate::data::MapInfo { screenshot: None, tier: Some(tier) });
            let section = &sections(slug)[1];
            assert_eq!(parse_waystone(section, &mut item, d), Ok(Parsed), "{slug}");
            assert_eq!(item.map_tier, Some(f64::from(tier)), "{slug}: tier");
            assert_eq!(item.map_revives, Some(revives), "{slug}: revives");
            assert_eq!(item.map_pack_size, Some(pack), "{slug}: pack size");
            assert_eq!(item.map_magic_monsters, magic, "{slug}: magic monsters");
            assert_eq!(item.map_rare_monsters, Some(rare), "{slug}: rare monsters");
            assert_eq!(item.map_drop_chance, Some(drop), "{slug}: drop chance");
            assert_eq!(item.map_item_rarity, rarity, "{slug}: item rarity");
        }
    }

    /// `specs/Parser/sectionUnit.test.ts` (`parseUnidentified`).
    #[test]
    fn parses_unidentified_sections() {
        let d = en();
        let mut item = Item::test_item();
        assert_eq!(parse_unidentified(&["Unidentified".to_owned()], &mut item, d), Ok(Parsed));
        assert!(item.is_unidentified);
        assert_eq!(item.unidentified_tier, None);

        let mut item = Item::test_item();
        assert_eq!(parse_unidentified(&["Unidentified (Tier 4)".to_owned()], &mut item, d), Ok(Parsed));
        assert!(item.is_unidentified);
        assert_eq!(item.unidentified_tier, Some(4.0));

        for lines in [
            vec!["Item Level: 54".to_owned()],
            vec![
                "{ Implicit Modifier — Elemental, Cold, Resistance }".to_owned(),
                "+23(20-30)% to Cold Resistance".to_owned(),
            ],
            vec!["Item Class: Rings".to_owned(), "Rarity: Magic".to_owned(), "Sapphire Ring".to_owned()],
        ] {
            let mut item = Item::test_item();
            assert_eq!(parse_unidentified(&lines, &mut item, d), Ok(Skipped), "{lines:?}");
            assert!(!item.is_unidentified);
            assert_eq!(item.unidentified_tier, None);
        }
    }

    /// `specs/Parser/sectionUnit.test.ts` (`parseTrials`).
    #[test]
    #[expect(clippy::type_complexity, reason = "one row per `parseTrials` test case")]
    fn parses_djinn_barya_and_inscribed_ultimatum_trials() {
        let d = en();
        let cases: [(&str, &[&str], f64, f64, Option<&str>); 10] = [
            ("Djinn Barya", &["Area Level: 25", "Number of Trials: 1"], 25.0, 1.0, None),
            ("Djinn Barya", &["Area Level: 58", "Number of Trials: 2"], 58.0, 2.0, None),
            ("Djinn Barya", &["Area Level: 70", "Number of Trials: 3"], 70.0, 3.0, None),
            ("Djinn Barya", &["Area Level: 80", "Number of Trials: 4"], 80.0, 4.0, None),
            ("Inscribed Ultimatum", &["Area Level: 27", "Number of Trials: 1"], 27.0, 1.0, None),
            ("Inscribed Ultimatum", &["Area Level: 56", "Number of Trials: 2"], 56.0, 2.0, None),
            ("Inscribed Ultimatum", &["Area Level: 69", "Number of Trials: 3"], 69.0, 3.0, None),
            (
                "Inscribed Ultimatum",
                &["Area Level: 75", "Number of Trials: 4", "Victorious"],
                75.0,
                4.0,
                Some("Victorious"),
            ),
            (
                "Inscribed Ultimatum",
                &["Area Level: 76", "Number of Trials: 4", "Cowardly"],
                76.0,
                4.0,
                Some("Cowardly"),
            ),
            ("Inscribed Ultimatum", &["Area Level: 79", "Number of Trials: 4", "Deadly"], 79.0, 4.0, Some("Deadly")),
        ];
        for (ref_name, lines, area, count, hint) in cases {
            let mut item = test_item(Rarity::Rare, ItemCategory::Unknown);
            item.info.as_mut().unwrap().ref_name = Some(ref_name.to_owned());
            let lines: Vec<String> = lines.iter().map(|l| (*l).to_owned()).collect();
            assert_eq!(parse_trials(&lines, &mut item, d), Ok(Parsed), "{ref_name} {lines:?}");
            assert_eq!(item.area_level, Some(area), "{ref_name} {lines:?}: area");
            let trials = item.trials.expect("trials");
            assert_eq!(trials.number_of_trials, Some(count), "{ref_name} {lines:?}: count");
            assert_eq!(trials.ultimatum_hint, hint, "{ref_name} {lines:?}: hint");
        }

        let skipped: [(&str, &[&str]); 6] = [
            ("Divine Crown", &["Area Level: 27", "Number of Trials: 1"]),
            ("Withered Wand", &["Area Level: 56", "Number of Trials: 2"]),
            ("Uncut Support Gem (Level 5)", &["Area Level: 69", "Number of Trials: 3"]),
            ("Ornate Belt", &["Area Level: 75", "Number of Trials: 4", "Victorious"]),
            ("Rider Bow", &["Area Level: 76", "Number of Trials: 4", "Cowardly"]),
            ("", &["Area Level: 79", "Number of Trials: 4", "Deadly"]),
        ];
        for (ref_name, lines) in skipped {
            let mut item = Item::test_item();
            item.info.as_mut().unwrap().ref_name = Some(ref_name.to_owned());
            let lines: Vec<String> = lines.iter().map(|l| (*l).to_owned()).collect();
            assert_eq!(parse_trials(&lines, &mut item, d), Ok(ParserSkipped), "{ref_name}");
            assert_eq!(item.area_level, None, "{ref_name}");
            assert!(item.trials.is_none(), "{ref_name}");
        }
    }

    /// `specs/Parser/skillGem.test.ts` (`isItemMissingItemClass`): meta skill gems
    /// come without an item class line.
    #[test]
    fn recognizes_meta_skill_gems_without_an_item_class() {
        let d = en();
        assert!(is_item_missing_item_class(d, &sections("ee2-meta-skill-gem")[0]));
        assert!(!is_item_missing_item_class(d, &sections("ee2-rare-item")[0]));
    }

    /// `specs/Parser/skillGem.test.ts`: uncut gems parse without a gem level, meta
    /// skill gems with theirs.
    #[test]
    fn parses_uncut_and_meta_gems() {
        let d = en();
        for (slug, category, gem_level) in [
            ("ee2-uncut-skill-gem", ItemCategory::Currency, None),
            ("ee2-uncut-spirit-gem", ItemCategory::Currency, None),
            ("ee2-uncut-support-gem", ItemCategory::Currency, None),
            ("ee2-meta-skill-gem", ItemCategory::Gem, Some(14.0)),
        ] {
            let item = parse_clipboard(d, &case_input(slug)).unwrap();
            assert_eq!(item.category, Some(category), "{slug}");
            assert_eq!(item.gem_level, gem_level, "{slug}");
        }
    }

    /// `specs/Parser/augment.test.ts` (`determineAugments`).
    #[test]
    fn determines_the_augments_that_explain_rune_stats() {
        let d = en();
        let augment_mod = ParsedModifier { info: ModifierInfo::new(ModifierType::Augment), stats: vec![] };
        let names = |calcs: &[StatCalculated]| {
            let refs: Vec<&StatCalculated> = calcs.iter().collect();
            let mut found = determine_augments(d, &augment_mod, &refs).unwrap();
            found.sort_by_key(|a| std::cmp::Reverse(a.ref_name.as_deref().unwrap_or("").to_owned()));
            found.into_iter().map(|a| a.ref_name.unwrap_or_default()).collect::<Vec<_>>()
        };

        let two_iron = names(&[make_calc_stat(d, "#% increased Physical Damage", 36.0, ModifierType::Augment)]);
        assert_eq!(two_iron, ["Greater Iron Rune", "Greater Iron Rune"]);

        let storm = names(&[make_calc_stat(d, "#% to Lightning Resistance", 10.0, ModifierType::Augment)]);
        assert_eq!(storm, ["Lesser Storm Rune"]);

        let range = names(&[make_calc_stat(d, "Adds # to # Lightning Damage", 15.5, ModifierType::Augment)]);
        assert_eq!(range, ["Greater Storm Rune"]);

        let bonded = names(&[make_calc_stat(
            d,
            "Idols socketed in this item gain the benefits of their Bonded modifiers",
            1.0,
            ModifierType::Augment,
        )]);
        assert_eq!(bonded, ["Fox Idol"]);
    }

    /// `specs/Parser/augment.test.ts` (`BFS`, `modifiedBfs`).
    #[test]
    fn modified_bfs_finds_the_fewest_repeated_augment_values() {
        let bfs = |target: f64, options: &[f64]| modified_bfs(target, &mut Vec::new(), options);
        assert_eq!(bfs(36.0, &[14.0, 16.0, 18.0, 20.0]), Some(vec![18.0, 18.0]));
        assert_eq!(bfs(40.0, &[14.0, 16.0, 18.0, 20.0]), Some(vec![20.0, 20.0]));
        assert_eq!(bfs(20.0, &[14.0, 16.0, 18.0, 20.0]), Some(vec![20.0]));
        assert_eq!(bfs(19.0, &[14.0, 16.0, 18.0, 20.0]), None);
        assert_eq!(bfs(15.0, &[5.0, 7.5, 10.0]), Some(vec![7.5, 7.5]));
        assert_eq!(bfs(34.0, &[14.0, 16.0, 18.0]), Some(vec![18.0, 16.0]));
        assert_eq!(bfs(10.0, &[10.0]), Some(vec![10.0]));
        assert_eq!(bfs(10.0, &[]), None);
        assert_eq!(bfs(40.0, &[10.0, 20.0, 30.0]), Some(vec![20.0, 20.0]));
        // No infinite loop when nothing fits.
        assert_eq!(bfs(100.0, &[0.0; 100]).unwrap_or_default().len(), 0);
        assert_eq!(bfs(15.5, &[1.0; 4]).unwrap_or_default().len(), 0);
    }

    /// `specs/Parser/augment.test.ts` (`parseAugmentSockets`).
    #[test]
    fn parses_augment_sockets_from_the_sockets_line() {
        let d = en();
        let sockets_prefix = d.s("SOCKETS");
        for (line, current) in [("S", 1.0), ("S S", 2.0), ("S S S", 3.0), ("S S S S", 4.0), ("S S S S S", 5.0)] {
            let mut item = test_item(Rarity::Rare, ItemCategory::Gloves);
            let lines = vec![format!("{sockets_prefix}{line}")];
            assert_eq!(parse_augment_sockets(&lines, &mut item, d), Ok(Parsed), "{line}");
            let sockets = item.augment_sockets.expect("sockets");
            assert_eq!((sockets.normal, sockets.empty, sockets.current), (1.0, 0.0, current), "{line}");
            assert_eq!(sockets.augments.len(), wraex_core::num::f64_to_usize(current), "{line}");
            assert!(sockets.augments.iter().all(Option::is_none), "{line}");
        }

        let mut item = test_item(Rarity::Rare, ItemCategory::Gloves);
        assert_eq!(parse_augment_sockets(&["text".to_owned()], &mut item, d), Ok(Skipped));
        let sockets = item.augment_sockets.expect("sockets");
        assert_eq!((sockets.normal, sockets.empty, sockets.current), (1.0, 1.0, 0.0));
        assert_eq!(sockets.augments.len(), 1);
        assert!(sockets.augments.iter().all(Option::is_none));

        let mut item = Item::test_item();
        assert_eq!(parse_augment_sockets(&[], &mut item, d), Ok(ParserSkipped));
    }

    /// `specs/Parser/augment.test.ts` (`applyAugmentSockets`).
    #[test]
    fn applies_the_augments_that_fit_the_sockets() {
        let d = en();
        let sockets =
            |empty, current, normal, len| AugmentSockets { empty, current, normal, augments: vec![None; len] };
        let editor_names = |item: &Item| {
            item.augment_sockets
                .as_ref()
                .unwrap()
                .augments
                .iter()
                .map(|a| a.as_ref().and_then(|a| a.ref_name.clone()))
                .collect::<Vec<_>>()
        };

        let mut item = Item::test_item();
        let before = item.clone();
        apply_augment_sockets(&mut item, d).unwrap();
        assert_eq!(item, before);

        let mut item = test_item(Rarity::Rare, ItemCategory::Gloves);
        item.augment_sockets = Some(sockets(0.0, 2.0, 2.0, 2));
        apply_augment_sockets(&mut item, d).unwrap();
        assert_eq!(item.augment_sockets.as_ref().unwrap().empty, 2.0);
        assert!(item.augment_sockets.as_ref().unwrap().augments.iter().all(Option::is_none));

        let mut item = test_item(Rarity::Rare, ItemCategory::Bow);
        item.augment_sockets = Some(sockets(0.0, 2.0, 2.0, 2));
        item.stats_by_type = vec![make_calc_stat(d, "#% increased Physical Damage", 18.0, ModifierType::Augment)];
        item.new_mods = vec![ParsedModifier {
            info: ModifierInfo::new(ModifierType::Augment),
            stats: vec![item.stats_by_type[0].sources[0].stat.clone()],
        }];
        apply_augment_sockets(&mut item, d).unwrap();
        assert_eq!(editor_names(&item), [Some("Greater Iron Rune".to_owned()), None]);
        assert_eq!(item.augment_sockets.as_ref().unwrap().empty, 1.0);

        let mut item = test_item(Rarity::Rare, ItemCategory::Bow);
        item.augment_sockets = Some(sockets(0.0, 2.0, 2.0, 2));
        item.stats_by_type = vec![
            make_calc_stat(d, "#% increased Physical Damage", 18.0, ModifierType::Augment),
            make_calc_stat(d, "Bow Attacks fire # additional Arrows", 1.0, ModifierType::Augment),
        ];
        item.new_mods = vec![ParsedModifier {
            info: ModifierInfo::new(ModifierType::Augment),
            stats: item.stats_by_type.iter().map(|c| c.sources[0].stat.clone()).collect(),
        }];
        apply_augment_sockets(&mut item, d).unwrap();
        let names = editor_names(&item);
        assert!(names.iter().any(|n| n.as_deref() == Some("Greater Iron Rune")), "{names:?}");
        assert!(names.iter().any(|n| n.as_deref() == Some("Countess Seske's Rune of Archery")), "{names:?}");
        assert_eq!(item.augment_sockets.as_ref().unwrap().empty, 0.0);

        // Multiline augments: the stats split across the runes that explain them.
        let mut item = test_item(Rarity::Rare, ItemCategory::Staff);
        item.augment_sockets = Some(sockets(0.0, 2.0, 2.0, 2));
        item.stats_by_type = vec![
            make_calc_stat(d, "Meta Skills gain #% increased Energy", 40.0, ModifierType::Augment),
            make_calc_stat(d, "#% increased Spirit", -25.0, ModifierType::Augment),
            make_calc_stat(d, "#% increased Mana Regeneration Rate", 20.0, ModifierType::Augment),
        ];
        item.new_mods = vec![ParsedModifier {
            info: ModifierInfo::new(ModifierType::Augment),
            stats: item.stats_by_type.iter().map(|c| c.sources[0].stat.clone()).collect(),
        }];
        apply_augment_sockets(&mut item, d).unwrap();
        let names = editor_names(&item);
        assert!(names.iter().any(|n| n.as_deref() == Some("Idol of the Martyr")), "{names:?}");
        assert!(names.iter().any(|n| n.as_deref() == Some("Lesser Inspiration Rune")), "{names:?}");
        assert_eq!(item.augment_sockets.as_ref().unwrap().empty, 0.0);

        // Bonded stats are not socketed runes themselves.
        let mut item = test_item(Rarity::Rare, ItemCategory::BodyArmour);
        item.augment_sockets = Some(sockets(1.0, 1.0, 2.0, 2));
        item.stats_by_type = vec![
            make_calc_stat(d, "Bonded: #% to Quality of all Skills", 5.0, ModifierType::Augment),
            make_calc_stat(
                d,
                "Idols socketed in this item gain the benefits of their Bonded modifiers",
                1.0,
                ModifierType::Augment,
            ),
        ];
        item.new_mods = vec![ParsedModifier {
            info: ModifierInfo::new(ModifierType::Augment),
            stats: item.stats_by_type.iter().map(|c| c.sources[0].stat.clone()).collect(),
        }];
        apply_augment_sockets(&mut item, d).unwrap();
        assert_eq!(editor_names(&item), [Some("Fox Idol".to_owned()), None]);
        let sockets = item.augment_sockets.as_ref().unwrap();
        assert_eq!((sockets.empty, sockets.current, sockets.normal), (1.0, 1.0, 2.0));
    }
}
