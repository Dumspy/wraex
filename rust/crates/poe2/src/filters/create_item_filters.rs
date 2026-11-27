//! `filters/create-item-filters.ts`: the item filters (base type, rarity, item
//! level, ...) of a preset.

use serde::Deserialize;
use wraex_core::Rarity;
use wraex_core::filters::{ceil_to_bracket, floor_to_bracket};

use super::common::max_useful_item_level;
use super::{
    CorruptedFilter, Disabled, Discriminator, FilterNumeric, InfluenceFilter, ItemEditorSelection, ItemFilters, R,
    RequiresFilter, SearchFilter, TradeFilters, UnidentifiedFilter, ValueFilter, VeiledFilter, missing,
};
use crate::data::{BaseType, Data, Namespace};
use crate::parser::item::Item;
use crate::parser::meta::{ARMOUR, GEM, ItemCategory, is_weapon};
use crate::parser::modifiers::ModifierType;

use ItemCategory as C;

/// `SPECIAL_SUPPORT_GEM`
pub const SPECIAL_SUPPORT_GEM: [&str; 3] = ["Empower Support", "Enlighten Support", "Enhance Support"];

/// `CATEGORIES_WITH_USEFUL_QUALITY`
fn has_useful_quality(category: &ItemCategory) -> bool {
    matches!(category, C::Flask | C::Charm | C::Tincture) || is_weapon(category) || ARMOUR.contains(category)
}

/// `CATEGORY_TO_TRADE_ID` (`web/price-check/trade/pathofexile-trade.ts`)
pub const fn category_to_trade_id(category: &ItemCategory) -> Option<&'static str> {
    Some(match category {
        C::Map => "map",
        C::AbyssJewel => "jewel.abyss",
        C::Amulet => "accessory.amulet",
        C::Belt => "accessory.belt",
        C::BodyArmour => "armour.chest",
        C::Boots => "armour.boots",
        C::Bow => "weapon.bow",
        C::Claw => "weapon.claw",
        C::Dagger => "weapon.dagger",
        C::FishingRod => "weapon.rod",
        C::Flask => "flask",
        C::Gloves => "armour.gloves",
        C::Helmet => "armour.helmet",
        C::Jewel => "jewel",
        C::OneHandedAxe => "weapon.oneaxe",
        C::OneHandedMace => "weapon.onemace",
        C::OneHandedSword => "weapon.onesword",
        C::Quiver => "armour.quiver",
        C::Ring => "accessory.ring",
        C::RuneDagger => "weapon.runedagger",
        C::Sceptre => "weapon.sceptre",
        C::Shield => "armour.shield",
        C::Staff => "weapon.staff",
        C::TwoHandedAxe => "weapon.twoaxe",
        C::TwoHandedMace => "weapon.twomace",
        C::TwoHandedSword => "weapon.twosword",
        C::Wand => "weapon.wand",
        C::Warstaff => "weapon.warstaff",
        C::ClusterJewel => "jewel.cluster",
        C::HeistBlueprint => "heistmission.blueprint",
        C::HeistContract => "heistmission.contract",
        C::HeistTool => "heistequipment.heisttool",
        C::HeistBrooch => "heistequipment.heistreward",
        C::HeistGear => "heistequipment.heistweapon",
        C::HeistCloak => "heistequipment.heistutility",
        C::Trinket => "accessory.trinket",
        C::SanctumRelic => "sanctum.relic",
        C::Tincture => "tincture",
        C::Charm => "flask.charm",
        C::Crossbow => "weapon.crossbow",
        C::SkillGem => "gem.activegem",
        C::SupportGem => "gem.supportgem",
        C::MetaGem => "gem.metagem",
        C::Focus => "armour.focus",
        C::Spear => "weapon.spear",
        C::Flail => "weapon.flail",
        C::Buckler => "armour.buckler",
        C::Tablet => "map.tablet",
        C::MapFragment => "map.fragment",
        C::Talisman => "weapon.talisman",
        C::Waystone => "map.waystone",
        _ => return None,
    })
}

/// `CreateOptions`
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateOptions {
    pub league: String,
    #[serde(default)]
    pub currency: Option<String>,
    /// `securable`, `any`, `online`, `available` or `onlineleague`.
    #[serde(default)]
    pub listing_type: Option<String>,
    /// `app` or `api`.
    pub collapse_listings: String,
    pub activate_stock_filter: bool,
    #[serde(default)]
    pub exact: bool,
    pub use_en: bool,
}

/// `t`: the name the trade site is searched with.
fn t(opts: &CreateOptions, info: &BaseType) -> Option<String> {
    if opts.use_en { info.ref_name.clone() } else { info.name.clone() }
}

fn truthy(x: Option<f64>) -> Option<f64> {
    x.filter(|v| *v != 0.0 && !v.is_nan())
}

fn base_type(opts: &CreateOptions, info: &BaseType) -> SearchFilter {
    SearchFilter { base_type: info.name.clone(), base_type_trade: t(opts, info), ..Default::default() }
}

fn first_item(d: &Data, ns: Namespace, name: &str) -> R<BaseType> {
    d.item_by_ref(ns, name).and_then(|v| v.into_iter().next()).ok_or_else(|| missing(name))
}

fn area_level(item: &Item) -> R<f64> {
    item.area_level.ok_or_else(|| missing("areaLevel"))
}

/// `createFilters`
#[expect(
    clippy::too_many_lines,
    reason = "line-for-line port of upstream `createFilters` (filters/create-item-filters.ts)"
)]
pub fn create_filters(d: &Data, item: &Item, opts: &CreateOptions) -> R<ItemFilters> {
    let info = item.info();
    let rarity = item.rarity;
    let is = |c: ItemCategory| item.category.as_ref() == Some(&c);
    let trade_tag = info.trade_tag.as_deref().is_some_and(|t| !t.is_empty());

    let mut filters = ItemFilters {
        trade: TradeFilters {
            offline: false,
            online_in_league: false,
            listing_type: opts.listing_type.clone().unwrap_or_else(|| "securable".to_owned()),
            listed: None,
            currency: opts.currency.clone(),
            league: opts.league.clone(),
            collapse_listings: opts.collapse_listings.clone(),
        },
        ..Default::default()
    };

    if item.category.as_ref().is_some_and(|c| GEM.contains(c)) && !trade_tag {
        return create_gem_filters(d, item, filters, opts);
    }
    if is(C::UncutGem) {
        return create_uncut_gem_filters(d, item, filters, opts);
    }
    if is(C::Currency) && item.trials.is_some() {
        return create_trials_filters(item, filters, opts);
    }
    if is(C::CapturedBeast) {
        // Always English on trade.
        filters.search_exact =
            SearchFilter { base_type: info.name.clone(), base_type_trade: info.ref_name.clone(), ..Default::default() };
        return Ok(filters);
    }
    if item.stack_size.is_some() || trade_tag {
        let value = item.stack_size.as_ref().map(|s| s.value).filter(|v| *v != 0.0 && !v.is_nan()).unwrap_or(1.0);
        let disabled = !(item.stack_size.as_ref().is_some_and(|s| s.value > 1.0) && opts.activate_stock_filter);
        filters.stack_size = Some(FilterNumeric::new(value, disabled));
    }
    if is(C::Invitation) {
        filters.search_exact = base_type(opts, info);
        return Ok(filters);
    }
    if is(C::MetamorphSample) {
        filters.search_exact = base_type(opts, info);
        filters.item_level = Some(FilterNumeric::new(item.item_level.ok_or_else(|| missing("itemLevel"))?, false));
        return Ok(filters);
    }
    if is(C::DivinationCard) || is(C::Currency) || item.ref_name_is("Charged Compass") {
        filters.search_exact = base_type(opts, info);
        if item.ref_name_is("Chronicle of Atzoatl") {
            let value = floor_to_bracket(area_level(item)?, &[1.0, 68.0, 73.0, 75.0, 78.0, 80.0]);
            filters.area_level = Some(FilterNumeric::new(value, false));
        }
        if item.ref_name_is("Mirrored Tablet") || item.ref_name_is("Forbidden Tome") {
            filters.area_level = Some(FilterNumeric::new(area_level(item)?, false));
        }
        if item.ref_name_is("Filled Coffin") {
            filters.item_level = Some(FilterNumeric::new(item.item_level.ok_or_else(|| missing("itemLevel"))?, false));
        }
        return Ok(filters);
    }

    let unique_search = |filters: &mut ItemFilters, base: &str| -> R<()> {
        filters.search_exact = SearchFilter {
            name: info.name.clone(),
            name_trade: t(opts, info),
            base_type_trade: t(opts, &first_item(d, Namespace::Item, base)?),
            ..Default::default()
        };
        Ok(())
    };

    // "Endgame" (map-like) items, uniques too.
    if is(C::Map) {
        if let (Some(unique), Some(Rarity::Unique)) = (&info.unique, rarity) {
            unique_search(&mut filters, &unique.base)?;
        } else {
            filters.search_exact = base_type(opts, info);
            filters.search_relaxed =
                Some(SearchFilter { category: item.category.clone(), disabled: Some(false), ..Default::default() });
        }

        if let Some(blighted) = item.map_blighted {
            filters.map_blighted = Some(ValueFilter { value: blighted });
        }

        filters.map_tier = Some(FilterNumeric::new(item.map_tier.ok_or_else(|| missing("mapTier"))?, false));
    } else if item.ref_name_is("Expedition Logbook") {
        filters.search_exact = base_type(opts, info);
        // Flooring the area level does not seem to matter in PoE2.
        filters.area_level = Some(FilterNumeric::new(area_level(item)?, false));
    } else if is(C::HeistBlueprint) {
        // TODO (upstream): blocked by https://www.pathofexile.com/forum/view-thread/3109852
        filters.search_relaxed =
            Some(SearchFilter { category: item.category.clone(), disabled: Some(true), ..Default::default() });
        filters.search_exact = base_type(opts, info);
        filters.area_level = Some(FilterNumeric::new(area_level(item)?, false));
        if let Some(wings) = truthy(item.heist.as_ref().and_then(|h| h.wings_revealed)) {
            filters.heist_wings_revealed = Some(FilterNumeric::new(wings, false));
        }
    } else if let (Some(Rarity::Unique), Some(unique)) = (rarity, &info.unique) {
        unique_search(&mut filters, &unique.base)?;
    } else {
        filters.search_exact = base_type(opts, info);
        if let Some(category) = item.category.as_ref().filter(|c| category_to_trade_id(c).is_some()) {
            let disabled = match category {
                C::ClusterJewel => true,
                C::SanctumRelic | C::Charm => false,
                _ => opts.exact,
            };
            filters.search_relaxed =
                Some(SearchFilter { category: Some(category.clone()), disabled: Some(disabled), ..Default::default() });
        }
    }

    if let Some(charge) = item.sentinel_charge {
        filters.sentinel_charge = Some(FilterNumeric::new(charge, false));
    }

    if let Some(quality) = truthy(item.quality) {
        if quality >= 20.0 && is(C::Flask) {
            // Shown at 20, enabled only above.
            filters.quality = Some(FilterNumeric::new(quality, quality <= 20.0));
        } else if quality > 20.0 && item.category.as_ref().is_some_and(has_useful_quality) {
            // Exceptional quality only for non-flasks. Not by default on rares:
            // most of the craft is likely done (and finished items don't care).
            filters.quality = Some(FilterNumeric::new(quality, rarity == Some(Rarity::Rare)));
        } else if is(C::Charm) {
            filters.quality = Some(FilterNumeric::new(quality, quality < 10.0));
        }
    }

    if let Some(linked) = item.gem_sockets.as_ref().and_then(|s| s.linked).filter(|l| *l != 0) {
        filters.linked_sockets = Some(FilterNumeric::new(f64::from(linked), false));
    }

    if let Some(white) = truthy(item.gem_sockets.as_ref().map(|s| s.white)) {
        filters.white_sockets = Some(FilterNumeric::new(white, false));
    }

    if let Some(sockets) = &item.augment_sockets
        && let Some(current) = truthy(Some(sockets.current))
    {
        filters.augment_sockets = Some(FilterNumeric::new(current, current <= sockets.normal && !item.is_corrupted));
    }
    if filters.item_editor_selection.is_none() {
        filters.item_editor_selection =
            Some(ItemEditorSelection { disabled: true, editing: false, value: "None".to_owned() });
    }

    if let Some(requires) = &item.requires
        && rarity == Some(Rarity::Rare)
        && !opts.exact
        && truthy(Some(requires.level)).is_some_and(|l| l <= 75.0)
        && truthy(item.item_level).is_some_and(|l| l <= 75.0)
    {
        filters.requires =
            Some(RequiresFilter { level: Some(FilterNumeric::new(requires.level, true)), ..Default::default() });
    }

    // Not `item.isCorrupted`: let the buyer corrupt.
    let for_adorned_jewel = rarity == Some(Rarity::Magic) && (is(C::Jewel) || is(C::AbyssJewel));

    // Waystones (formerly tablets) are ignored: a buyer won't care about a
    // corrupted one with the right mods.
    if item.is_unmodifiable != Some(true) && !is(C::Map) && rarity.is_some() {
        filters.corrupted = Some(CorruptedFilter { value: item.is_corrupted, exact: Some(for_adorned_jewel) });
    }

    if for_adorned_jewel {
        filters.rarity = Some(ValueFilter { value: "magic" });
    } else if rarity == Some(Rarity::Normal) && !item.ref_name_is("Idol of Estazunti") && opts.exact {
        // Chance orbs only work on normal items.
        filters.rarity = Some(ValueFilter { value: "normal" });
    } else if rarity == Some(Rarity::Magic) && opts.exact && !is(C::Tablet) {
        // Tablets compare with rare ones.
        filters.rarity = Some(ValueFilter { value: "magic" });
    } else if matches!(rarity, Some(Rarity::Normal | Rarity::Magic | Rarity::Rare)) {
        filters.rarity = Some(ValueFilter { value: "nonunique" });
    }

    if item.is_mirrored == Some(true) {
        filters.mirrored = Some(Disabled { disabled: false });
    }

    if item.is_sanctified == Some(true) {
        filters.sanctified = Some(Disabled { disabled: false });
    }

    if item.is_fractured != Some(true) && opts.exact {
        filters.fractured = Some(ValueFilter { value: false });
    }

    if item.is_foil == Some(true) {
        filters.foil = Some(Disabled { disabled: true });
    }

    if !item.influences.is_empty() && item.influences.len() <= 2 {
        filters.influences =
            Some(item.influences.iter().map(|&value| InfluenceFilter { value, disabled: !opts.exact }).collect());
    }

    if let Some(level) = truthy(item.item_level) {
        let max_useful = max_useful_item_level(item.category.as_ref());
        #[expect(clippy::float_cmp, reason = "JS `!==` on the item level")]
        if max_useful != 1.0
            && rarity != Some(Rarity::Unique)
            && !is(C::Map)
            && !is(C::HeistBlueprint)
            && !is(C::HeistContract)
            && !is(C::MemoryLine)
            && !item.ref_name_is("Expedition Logbook")
        {
            if is(C::ClusterJewel) {
                filters.item_level = Some(FilterNumeric {
                    value: floor_to_bracket(level, &[1.0, 50.0, 68.0, 75.0, 84.0]),
                    max: Some(ceil_to_bracket(level, &[100.0, 74.0, 67.0, 49.0])),
                    disabled: !opts.exact,
                });
            } else {
                // TODO (upstream): limit level by item type
                filters.item_level =
                    Some(FilterNumeric::new(level.min(max_useful), !opts.exact || is(C::Flask) || is(C::Charm)));
            }
        }

        if rarity == Some(Rarity::Unique) {
            if item.is_unidentified && item.ref_name_is("Watcher's Eye") {
                filters.item_level = Some(FilterNumeric::new(level, false));
            }

            let agnerod = ["Agnerod", "Agnerod East", "Agnerod North", "Agnerod South", "Agnerod West"];
            if level >= 75.0 && agnerod.iter().any(|a| item.ref_name_is(a)) {
                // https://pathofexile.gamepedia.com/The_Vinktar_Square
                let normalized = if level >= 82.0 {
                    82.0
                } else if level >= 80.0 {
                    80.0
                } else if level >= 78.0 {
                    78.0
                } else {
                    75.0
                };
                filters.item_level = Some(FilterNumeric::new(normalized, false));
            }
        }
    }

    if item.is_unidentified {
        if let Some(tier) = truthy(item.unidentified_tier) {
            filters.unidentified_tier = Some(UnidentifiedFilter { value: tier, disabled: tier < 5.0 });
        } else {
            filters.unidentified = Some(UnidentifiedFilter { value: true, disabled: rarity != Some(Rarity::Unique) });
        }
    }

    if item.is_veiled == Some(true) {
        filters.veiled = Some(VeiledFilter {
            stat_refs: item
                .stats_by_type
                .iter()
                .filter(|c| c.kind == ModifierType::Veiled)
                .map(|c| c.stat.stat_ref.clone())
                .collect(),
            veiled_count: wraex_core::num::usize_to_f64(
                item.new_mods.iter().filter(|m| m.info.kind == ModifierType::Veiled).count(),
            ),
            disabled: rarity != Some(Rarity::Unique),
        });

        if rarity != Some(Rarity::Unique)
            && let Some(level) = &mut filters.item_level
        {
            level.disabled = false;
        }
    }

    if rarity.is_some() && item.augment_sockets.as_ref().is_some_and(|s| s.empty > 0.0) {
        filters.temp_augment_storage = Some(Vec::new());
    }

    Ok(filters)
}

/// `createGemFilters`
fn create_gem_filters(d: &Data, item: &Item, mut filters: ItemFilters, opts: &CreateOptions) -> R<ItemFilters> {
    let info = item.info();
    let gem = info.gem.as_ref().ok_or_else(|| missing("info.gem"))?;
    if gem.transfigured == Some(true) {
        let normal_variant = gem.normal_variant.as_deref().ok_or_else(|| missing("gem.normalVariant"))?;
        let normal_gem = first_item(d, Namespace::Gem, normal_variant)?;
        filters.search_exact =
            SearchFilter { base_type: info.name.clone(), base_type_trade: t(opts, &normal_gem), ..Default::default() };
        filters.discriminator = Some(Discriminator { value: None, trade: info.trade_disc.clone() });
    } else {
        filters.search_exact = base_type(opts, info);
    }

    filters.corrupted = Some(CorruptedFilter { value: item.is_corrupted, exact: None });

    let gem_level = || item.gem_level.ok_or_else(|| missing("gemLevel"));
    let quality = truthy(item.quality);

    if gem.awakened == Some(true) {
        let level = gem_level()?;
        filters.gem_level = Some(FilterNumeric::new(level, level < 5.0));
        if let (true, Some(quality)) = (item.is_corrupted, quality) {
            filters.quality = Some(FilterNumeric::new(quality, quality < 20.0));
        }
        return Ok(filters);
    }

    if SPECIAL_SUPPORT_GEM.iter().any(|g| item.ref_name_is(g)) {
        let level = gem_level()?;
        filters.gem_level = Some(FilterNumeric::new(level, level < 3.0));
        if let (true, Some(quality)) = (item.is_corrupted, quality) {
            filters.quality = Some(FilterNumeric::new(quality, true));
        }
        return Ok(filters);
    }

    if let Some(sockets) = &item.gem_sockets {
        filters.socket_number = Some(FilterNumeric::new(sockets.number, sockets.number < 3.0));
    }

    if let Some(quality) = quality {
        filters.quality = Some(FilterNumeric::new(quality, quality < 16.0));
    }

    let level = gem_level()?;
    filters.gem_level = Some(FilterNumeric::new(level, level < 19.0));

    Ok(filters)
}

/// `createUncutGemFilters`
pub fn create_uncut_gem_filters(
    d: &Data,
    item: &Item,
    mut filters: ItemFilters,
    opts: &CreateOptions,
) -> R<ItemFilters> {
    let info = item.info();
    let ref_name = info.ref_name.as_deref().ok_or_else(|| missing("refName"))?;
    let normal_gem = first_item(d, Namespace::Item, ref_name)?;
    filters.search_exact =
        SearchFilter { base_type: info.name.clone(), base_type_trade: t(opts, &normal_gem), ..Default::default() };
    let gem_level = item.gem_level.ok_or_else(|| missing("gemLevel"))?;
    let range = if gem_level < 18.0 && ref_name != "Uncut Support Gem" { 1.0 } else { 0.0 };

    let mut level = FilterNumeric::new(gem_level - range, false);
    if range != 0.0 {
        level.max = Some(gem_level + range);
    }
    filters.gem_level = Some(level);

    Ok(filters)
}

/// `createTrialsFilters`
pub fn create_trials_filters(item: &Item, mut filters: ItemFilters, opts: &CreateOptions) -> R<ItemFilters> {
    let info = item.info();
    let ref_name = info.ref_name.as_deref().unwrap_or("undefined");
    filters.search_exact = base_type(opts, info);
    let area_level = area_level(item)?;

    filters.awarded_ascendancy_points =
        Some(FilterNumeric::new(ascendancy_points_by_area_level(ref_name, area_level), false));
    filters.area_level = Some(FilterNumeric::new(area_level, false));

    if ref_name == "Inscribed Ultimatum"
        && let Some(hint) = item.trials.as_ref().and_then(|t| t.ultimatum_hint)
    {
        filters.ultimatum_hint = Some(UnidentifiedFilter { value: hint, disabled: true });
    }

    Ok(filters)
}

/// `ASCENDANCY_POINTS`: the area level each point count needs.
fn ascendancy_points(ref_name: &str) -> Option<&'static [(u32, f64)]> {
    match ref_name {
        "Inscribed Ultimatum" => Some(&[(1, 1.0), (2, 60.0), (3, 75.0)]),
        "Djinn Barya" => Some(&[(1, 1.0), (2, 45.0), (3, 60.0), (4, 75.0)]),
        _ => None,
    }
}

/// `areaLevelByAscendancyPoints`: the area level an item needs to award `points`.
/// `None` is `undefined`: no entry for that many points.
pub fn area_level_by_ascendancy_points(ref_name: &str, points: f64) -> Option<f64> {
    let Some(levels) = ascendancy_points(ref_name) else { return Some(0.0) };
    let max_possible_points = if ref_name == "Inscribed Ultimatum" { 3.0 } else { 4.0 };

    // Cases not possible.
    if points < 1.0 {
        return Some(1.0);
    } else if points > max_possible_points {
        return Some(75.0);
    }

    #[expect(clippy::float_cmp, reason = "JS `===` on the requested point count")]
    levels.iter().find(|&&(p, _)| f64::from(p) == points).map(|&(_, level)| level)
}

/// `ascendancyPointsByAreaLevel`
pub fn ascendancy_points_by_area_level(ref_name: &str, area_level: f64) -> f64 {
    let Some(levels) = ascendancy_points(ref_name) else { return 0.0 };
    let max_possible_points = if ref_name == "Inscribed Ultimatum" { 3 } else { 4 };

    // Cases not possible.
    if area_level < 1.0 {
        return 1.0;
    } else if area_level > 80.0 {
        return f64::from(max_possible_points);
    }

    for i in (1..=max_possible_points).rev() {
        if levels.iter().any(|&(points, level)| points == i && area_level >= level) {
            return f64::from(i);
        }
    }
    1.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::tests::en;

    fn test_options() -> CreateOptions {
        // `createTestCreateOptions()` (`specs/helper.ts`).
        CreateOptions {
            league: "Standard".into(),
            currency: None,
            listing_type: None,
            collapse_listings: "app".into(),
            activate_stock_filter: false,
            exact: false,
            use_en: true,
        }
    }

    /// `specs/web/price-check/filters/create-item-filters.test.ts`
    /// (`Ascendancy Points calcs`): points awarded for a trial's area level.
    #[test]
    fn ascendancy_points_follow_area_level() {
        for (ref_name, area_level, expected) in [
            ("Inscribed Ultimatum", 1.0, 1.0),
            ("Inscribed Ultimatum", 23.0, 1.0),
            ("Inscribed Ultimatum", 34.0, 1.0),
            ("Inscribed Ultimatum", 42.0, 1.0),
            ("Inscribed Ultimatum", 48.0, 1.0),
            ("Inscribed Ultimatum", 55.0, 1.0),
            ("Inscribed Ultimatum", 70.0, 2.0),
            ("Inscribed Ultimatum", 78.0, 3.0),
            ("Inscribed Ultimatum", 80.0, 3.0),
            ("Inscribed Ultimatum", 100.0, 3.0),
            ("Djinn Barya", 1.0, 1.0),
            ("Djinn Barya", 23.0, 1.0),
            ("Djinn Barya", 34.0, 1.0),
            ("Djinn Barya", 42.0, 1.0),
            ("Djinn Barya", 48.0, 2.0),
            ("Djinn Barya", 55.0, 2.0),
            ("Djinn Barya", 70.0, 3.0),
            ("Djinn Barya", 78.0, 4.0),
            ("Djinn Barya", 80.0, 4.0),
            ("Djinn Barya", 100.0, 4.0),
        ] {
            assert_eq!(ascendancy_points_by_area_level(ref_name, area_level), expected, "{ref_name} {area_level}");
        }
        assert_eq!(ascendancy_points_by_area_level("Other", 75.0), 0.0);
    }

    /// `specs/web/price-check/filters/create-item-filters.test.ts`
    /// (`Ascendancy Points calcs`): the area level for a point count.
    #[test]
    fn area_levels_follow_ascendancy_points() {
        for (ref_name, points, expected) in [
            ("Inscribed Ultimatum", 0.0, 1.0),
            ("Inscribed Ultimatum", 1.0, 1.0),
            ("Inscribed Ultimatum", 2.0, 60.0),
            ("Inscribed Ultimatum", 3.0, 75.0),
            ("Inscribed Ultimatum", 4.0, 75.0),
            ("Inscribed Ultimatum", 5.0, 75.0),
            ("Djinn Barya", 0.0, 1.0),
            ("Djinn Barya", 1.0, 1.0),
            ("Djinn Barya", 2.0, 45.0),
            ("Djinn Barya", 3.0, 60.0),
            ("Djinn Barya", 4.0, 75.0),
            ("Djinn Barya", 5.0, 75.0),
        ] {
            assert_eq!(area_level_by_ascendancy_points(ref_name, points), Some(expected), "{ref_name} {points}");
        }
    }

    /// `specs/web/price-check/filters/create-item-filters.test.ts` (`unidentified
    /// item tests`): the unidentified and unidentified-tier filters.
    #[test]
    fn unidentified_filters_follow_the_item() {
        let d = en();
        let opts = test_options();

        let mut item = Item::test_item();
        item.is_unidentified = false;
        assert!(create_filters(d, &item, &opts).unwrap().unidentified.is_none());

        let mut item = Item::test_item();
        item.is_unidentified = true;
        let filters = create_filters(d, &item, &opts).unwrap();
        assert!(filters.unidentified.as_ref().is_some_and(|u| u.value));
        assert!(filters.unidentified_tier.is_none());

        let mut item = Item::test_item();
        item.is_unidentified = true;
        item.unidentified_tier = Some(4.0);
        let filters = create_filters(d, &item, &opts).unwrap();
        assert!(filters.unidentified.is_none());
        assert_eq!(filters.unidentified_tier.as_ref().map(|t| t.value), Some(4.0));
    }

    /// `specs/web/price-check/filters/create-item-filters.test.ts` (`unidentified
    /// item tests`): enabled by default only on uniques.
    #[test]
    fn unidentified_filters_are_enabled_only_on_uniques() {
        let d = en();
        let opts = test_options();
        for (rarity, disabled) in [(Rarity::Magic, true), (Rarity::Rare, true), (Rarity::Unique, false)] {
            let mut item = Item::test_item();
            item.is_unidentified = true;
            item.rarity = Some(rarity);
            let filters = create_filters(d, &item, &opts).unwrap();
            assert_eq!(filters.unidentified.as_ref().map(|u| u.disabled), Some(disabled), "{rarity:?}");
        }
    }

    /// `specs/web/price-check/filters/create-item-filters.test.ts` (`unidentified
    /// item tests`): enabled by default only on tier 5.
    #[test]
    fn unidentified_tier_filters_are_enabled_only_on_tier_five() {
        let d = en();
        let opts = test_options();
        for (tier, disabled) in [(2.0, true), (3.0, true), (4.0, true), (5.0, false)] {
            let mut item = Item::test_item();
            item.is_unidentified = true;
            item.unidentified_tier = Some(tier);
            let filters = create_filters(d, &item, &opts).unwrap();
            let filter = filters.unidentified_tier.expect("unidentified tier filter");
            assert_eq!((filter.value, filter.disabled), (tier, disabled), "tier {tier}");
        }
    }

    /// `specs/Parser/skillGem.test.ts` (`Create Filter for uncut gems`): uncut gems
    /// search by name, without a gem level filter.
    #[test]
    fn uncut_gems_search_by_name_and_have_no_gem_level_filter() {
        let d = en();
        for (name, level) in [("Uncut Skill Gem", 19.0), ("Uncut Spirit Gem", 16.0), ("Uncut Support Gem", 5.0)] {
            let mut item = Item::test_item();
            item.category = Some(ItemCategory::Currency);
            item.gem_level = Some(level);
            let info = item.info.as_mut().unwrap();
            info.name = Some(name.to_owned());
            info.ref_name = Some(name.to_owned());
            let filters = create_filters(d, &item, &test_options()).unwrap();
            assert_eq!(filters.search_exact.base_type.as_deref(), Some(name), "{name}");
            assert!(filters.gem_level.is_none(), "{name}");
        }
    }
}
