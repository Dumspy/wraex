//! `filters/create-item-filters.ts`: the item filters (base type, rarity, item
//! level, ...) of a preset.

use serde::Deserialize;
use wraex_core::Rarity;
use wraex_core::filters::{ceil_to_bracket, floor_to_bracket};

use super::{
    CorruptedFilter, Disabled, FilterNumeric, HiddenFilter, InfluenceFilter, ItemFilters, MapBlighted,
    MapCompletionReward, R, RarityFilter, SearchFilter, TradeFilters, UnidentifiedFilter, ValueFilter, VeiledFilter,
    missing,
};
use crate::data::{BaseType, Data, Namespace};
use crate::parser::calc_q20::get_prop_quality;
use crate::parser::item::Item;
use crate::parser::meta::{CONSUMABLE_CRAFTABLE_ITEM, ItemCategory, JEWELLERY, MAGIC_ONLY_OR_UNIQUE_ITEM};
use crate::parser::modifiers::ModifierType;

use ItemCategory as C;

/// `SPECIAL_SUPPORT_GEM`
pub const SPECIAL_SUPPORT_GEM: [&str; 3] = ["Empower Support", "Enlighten Support", "Enhance Support"];

/// `PERMANENT_SC` (`web/background/Leagues.ts`)
pub const PERMANENT_SC: [&str; 2] = ["Standard", "標準模式"];

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
        C::Charm => "azmeri.charm",
        C::Idol => "idol",
        C::Graft => "graft",
        C::Chart => "chart",
        C::HeadViscera => "viscera.head",
        C::TorsoViscera => "viscera.body",
        C::LowerLimbViscera => "viscera.leg",
        C::UpperLimbViscera => "viscera.arm",
        _ => return None,
    })
}

/// `CreateOptions`, which the reference spells in `create-item-filters.ts`.
#[expect(
    clippy::struct_excessive_bools,
    reason = "mirrors upstream `CreateOptions` (`create-item-filters.ts`) field for field"
)]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateOptions {
    pub league: String,
    pub merchant_only: bool,
    pub currency: Option<String>,
    /// `app` or `api`.
    pub collapse_listings: String,
    pub activate_stock_filter: bool,
    #[serde(default)]
    pub exact: bool,
    pub use_en: bool,
}

/// `t`: the name the trade site is searched with.
fn t(opts: &CreateOptions, info: &BaseType) -> String {
    if opts.use_en { info.ref_name.clone() } else { info.name.clone() }
}

fn is(item: &Item, category: &ItemCategory) -> bool {
    item.category.as_ref() == Some(category)
}

fn in_set(item: &Item, set: &[ItemCategory]) -> bool {
    item.category.as_ref().is_some_and(|c| set.contains(c))
}

fn base_type(opts: &CreateOptions, info: &BaseType) -> SearchFilter {
    SearchFilter { base_type: Some(info.name.clone()), base_type_trade: Some(t(opts, info)), ..Default::default() }
}

/// The `sub` filter of a map area (Scrying Orb, blighted maps, charts).
fn map_area_sub(item: &Item) -> R<Box<SearchFilter>> {
    let area = item.map_area.as_ref().ok_or_else(|| missing("mapArea"))?;
    Ok(Box::new(SearchFilter {
        base_type: Some(area.name.clone()),
        base_type_trade: area.trade_disc.clone(),
        discriminator_trade: item.info().trade_disc.clone(),
        disabled: Some(false),
        ..Default::default()
    }))
}

fn area_level(item: &Item) -> R<f64> {
    item.area_level.ok_or_else(|| missing("areaLevel"))
}

/// `createFilters`
#[expect(clippy::too_many_lines, reason = "line-for-line port of upstream `createFilters` (`create-item-filters.ts`)")]
pub fn create_filters(d: &Data, item: &Item, opts: &CreateOptions) -> R<ItemFilters> {
    let info = item.info();
    let rarity = item.rarity;
    let is_unique = rarity == Some(Rarity::Unique);
    let craftable = info.craftable.is_some();
    let corrupted = item.is_corrupted.is_some();
    let mirrored = item.is_mirrored == Some(true);
    let fractured = item.is_fractured == Some(true);
    let trade_tag = info.trade_tag.as_deref().is_some_and(|t| !t.is_empty());

    let mut filters = ItemFilters {
        search_exact: SearchFilter::default(),
        search_relaxed: None,
        rarity: None,
        linked_sockets: None,
        white_sockets: None,
        corrupted: None,
        fractured: None,
        imbued_gem: None,
        mirrored: None,
        split: None,
        not_catalysed: None,
        foil: None,
        foulborn: None,
        vestigial: None,
        influences: None,
        quality: None,
        gem_level: None,
        map_tier: None,
        map_blighted: None,
        map_completion_reward: None,
        item_level: None,
        stack_size: None,
        unidentified: None,
        veiled: None,
        area_level: None,
        sentinel_charge: None,
        trade: TradeFilters {
            offline: false,
            online_in_league: false,
            // Divination Cards, and some items at the start of a league that
            // are on Currency Exchange but not yet in the site's Bulk section.
            merchant_only: opts.merchant_only && (info.exchangeable != Some(true) || trade_tag),
            listed: None,
            currency: opts.currency.clone(),
            league: opts.league.clone(),
            collapse_listings: opts.collapse_listings.clone(),
            collapse_merchant: false,
        },
    };

    if (!craftable || in_set(item, CONSUMABLE_CRAFTABLE_ITEM)) && !is_unique {
        if opts.currency.as_deref().is_none_or(str::is_empty) {
            filters.trade.currency = Some("chaos_divine".to_owned());
        }
        if !item.ref_name_is("Mercenary Warrant") && !item.ref_name_is("Viscera") {
            filters.trade.collapse_merchant = true;
        }
    }

    if is(item, &C::Gem) {
        return create_gem_filters(d, item, filters, opts);
    }
    if is(item, &C::CapturedBeast) {
        filters.search_exact = base_type(opts, info);
        return Ok(filters);
    }
    if item.stack_size.is_some() || trade_tag || info.exchangeable == Some(true) {
        let value = item.stack_size.as_ref().map(|s| s.value).filter(|v| *v != 0.0 && !v.is_nan()).unwrap_or(1.0);
        let disabled = !(item.stack_size.as_ref().is_some_and(|s| s.value > 1.0) && opts.activate_stock_filter);
        filters.stack_size = Some(FilterNumeric::new(value, disabled));
    }
    if is(item, &C::Invitation) {
        filters.search_exact = base_type(opts, info);
        return Ok(filters);
    }
    if is(item, &C::MetamorphSample) {
        filters.search_exact = base_type(opts, info);
        let level = item.item_level.ok_or_else(|| missing("itemLevel"))?;
        filters.item_level = Some(FilterNumeric::new(level, false));
        return Ok(filters);
    }
    if item.ref_name_is("Scrying Orb") {
        filters.search_exact = SearchFilter { sub: Some(map_area_sub(item)?), ..base_type(opts, info) };
        return Ok(filters);
    }
    if is(item, &C::DivinationCard) || is(item, &C::Currency) || item.ref_name_is("Charged Compass") {
        filters.search_exact = base_type(opts, info);
        if item.ref_name_is("Chronicle of Atzoatl") {
            let value = floor_to_bracket(area_level(item)?, &[1.0, 68.0, 73.0, 75.0, 78.0, 80.0]);
            filters.area_level = Some(FilterNumeric::new(value, false));
        } else if item.ref_name_is("Mirrored Tablet") {
            filters.area_level = Some(FilterNumeric::new(area_level(item)?, false));
        } else if item.ref_name_is("Forbidden Tome") {
            let level = area_level(item)?;
            let mut filter = FilterNumeric::new(level, false);
            if level < 83.0 {
                filter.max = Some(level);
            }
            filters.area_level = Some(filter);
        } else if let Some(level) = item.item_level.filter(|l| *l != 0.0 && !l.is_nan()) {
            // Incubators, Wombgifts
            filters.item_level = Some(FilterNumeric::new(level, false));
        }
        return Ok(filters);
    }
    if item.ref_name_is("Viscera") {
        filters.search_exact = SearchFilter { category: item.category.clone(), ..Default::default() };
        return Ok(filters);
    }

    let unique_base = item.unique_base.as_ref();
    if is(item, &C::Map) {
        if info.area.as_ref().is_some_and(|a| a.blighted == Some(true)) {
            let map = d
                .item_by_ref(Namespace::Item, "Map")
                .and_then(|v| v.into_iter().next())
                .ok_or_else(|| missing("ITEM Map"))?;
            filters.search_exact = SearchFilter {
                base_type: Some(info.name.clone()),
                base_type_trade: Some(t(opts, &map)),
                discriminator_trade: Some("map".to_owned()),
                sub: Some(map_area_sub(item)?),
                ..Default::default()
            };
        } else if let (true, Some(base)) = (is_unique, unique_base) {
            filters.search_exact = SearchFilter {
                name: Some(info.name.clone()),
                name_trade: Some(t(opts, info)),
                base_type_trade: Some(t(opts, base)),
                ..Default::default()
            };
        } else {
            filters.search_exact = base_type(opts, info);
        }

        if item.ref_name_is("Map") || unique_base.is_some_and(|b| b.ref_name == "Map") {
            filters.search_exact.discriminator_trade = Some("map".to_owned());
        }

        if item.ref_name_is("Blighted Map") {
            filters.map_blighted = Some(ValueFilter { value: MapBlighted::Blighted });
        } else if item.ref_name_is("Blight-ravaged Map") {
            filters.map_blighted = Some(ValueFilter { value: MapBlighted::BlightRavaged });
        } else if item.ref_name_is("Map") {
            filters.map_blighted = Some(ValueFilter { value: MapBlighted::No });
        }

        if let Some(reward) = &item.map_completion_reward {
            filters.map_completion_reward =
                Some(MapCompletionReward { name: reward.name.clone(), name_trade: t(opts, reward) });
        }

        if let Some(tier) = item.map_tier.filter(|t| *t != 0.0 && !t.is_nan()) {
            filters.map_tier = Some(FilterNumeric::new(tier, false));
        }
    } else if item.ref_name_is("Expedition Logbook") {
        filters.search_exact = base_type(opts, info);
        let value = floor_to_bracket(area_level(item)?, &[1.0, 68.0, 73.0, 78.0, 81.0, 83.0]);
        filters.area_level = Some(FilterNumeric::new(value, false));
    } else if is(item, &C::Chart) {
        filters.search_exact = base_type(opts, info);
        filters.search_relaxed = Some(SearchFilter {
            category: item.category.clone(),
            disabled: Some(false),
            sub: Some(map_area_sub(item)?),
            ..Default::default()
        });
    } else if let (true, Some(base)) = (is_unique, unique_base) {
        filters.search_exact = SearchFilter {
            name: Some(info.name.clone()),
            name_trade: Some(t(opts, info)),
            base_type_trade: Some(t(opts, base)),
            ..Default::default()
        };
    } else {
        filters.search_exact = base_type(opts, info);
        if let Some(category) = item.category.as_ref().filter(|c| category_to_trade_id(c).is_some()) {
            let disabled = match category {
                C::ClusterJewel | C::Idol | C::Graft | C::HeistBlueprint => true,
                C::SanctumRelic | C::Charm | C::HeistContract => false,
                _ => opts.exact,
            };
            filters.search_relaxed =
                Some(SearchFilter { category: Some(category.clone()), disabled: Some(disabled), ..Default::default() });
        }
    }

    if let Some(charge) = item.sentinel_charge {
        filters.sentinel_charge = Some(FilterNumeric::new(charge, false));
    }

    let quality = item.quality.filter(|q| *q != 0.0 && !q.is_nan());
    if in_set(item, JEWELLERY) && quality.is_none() {
        filters.not_catalysed = Some(Disabled { disabled: false });
    }

    if let Some(quality) = quality.filter(|q| *q >= 20.0)
        && !in_set(item, JEWELLERY)
        && (is(item, &C::Flask) || is(item, &C::Tincture) || get_prop_quality(item) == 0.0 || opts.exact)
    {
        // `|| opts.exact`: for weapons and armour
        filters.quality = Some(FilterNumeric::new(quality, quality <= 20.0));
    }

    if let Some(linked) = item.sockets.as_ref().and_then(|s| s.linked).filter(|l| *l != 0) {
        filters.linked_sockets = Some(FilterNumeric::new(f64::from(linked), false));
    }

    // Not `item.isCorrupted`: let the buyer corrupt.
    let for_adorned_jewel = rarity == Some(Rarity::Magic) && (is(item, &C::Jewel) || is(item, &C::AbyssJewel));

    if item.is_unmodifiable != Some(true) && (craftable || is_unique) {
        filters.corrupted = Some(CorruptedFilter {
            value: corrupted,
            twice: item.is_corrupted.as_ref().is_some_and(|c| c.twice == Some(true)),
            exact: Some(for_adorned_jewel),
        });
    }

    if for_adorned_jewel {
        filters.rarity = Some(RarityFilter { value: "magic", disabled: false });
    } else if opts.exact
        && rarity == Some(Rarity::Magic)
        && !in_set(item, CONSUMABLE_CRAFTABLE_ITEM)
        && !in_set(item, MAGIC_ONLY_OR_UNIQUE_ITEM)
    {
        filters.rarity = Some(RarityFilter { value: "magic", disabled: true });
    } else if craftable && matches!(rarity, Some(Rarity::Normal | Rarity::Magic | Rarity::Rare)) {
        filters.rarity = Some(RarityFilter { value: "nonunique", disabled: false });
    }

    if mirrored {
        filters.mirrored = Some(HiddenFilter { disabled: false, hidden: false });
    } else if craftable && !corrupted {
        filters.mirrored = Some(HiddenFilter { disabled: true, hidden: true });
    }

    if item.is_split == Some(true) {
        filters.split = Some(HiddenFilter { disabled: false, hidden: false });
    } else if (!PERMANENT_SC.contains(&opts.league.as_str()) || opts.exact)
        && craftable
        && !corrupted
        && !mirrored
        && item.is_synthesised != Some(true)
        && !fractured
        && item.influences.is_empty()
    {
        filters.split = Some(HiddenFilter { disabled: true, hidden: true });
    }

    if !fractured && craftable && !corrupted && !mirrored {
        filters.fractured = Some(ValueFilter { value: false });
    }

    if item.is_foil == Some(true) {
        filters.foil = Some(Disabled { disabled: false });
    }

    if !item.influences.is_empty() && item.influences.len() <= 2 {
        filters.influences =
            Some(item.influences.iter().map(|&value| InfluenceFilter { value, disabled: !opts.exact }).collect());
    }

    if let Some(level) = item.item_level.filter(|l| *l != 0.0 && !l.is_nan()) {
        let excluded = [
            C::Map,
            // https://pathofexile.gamepedia.com/Jewel#Affixes
            C::Jewel,
            C::HeistBlueprint,
            C::HeistContract,
            C::Chart,
            C::MemoryLine,
            C::SanctumRelic,
            C::Charm,
            C::Idol,
        ];
        if !is_unique && !in_set(item, &excluded) && !item.ref_name_is("Expedition Logbook") {
            if is(item, &C::ClusterJewel) {
                filters.item_level = Some(FilterNumeric {
                    value: floor_to_bracket(level, &[1.0, 50.0, 68.0, 75.0, 84.0]),
                    max: Some(ceil_to_bracket(level, &[100.0, 74.0, 67.0, 49.0])),
                    disabled: !opts.exact,
                });
            } else {
                // TODO (upstream): limit level by item type
                filters.item_level = Some(FilterNumeric::new(
                    level.min(86.0),
                    !opts.exact || is(item, &C::Flask) || is(item, &C::Tincture),
                ));
            }
        }

        if is_unique {
            if item.is_unidentified && item.ref_name_is("Watcher's Eye") {
                filters.item_level = Some(FilterNumeric::new(level, false));
            }

            let agnerod = ["Agnerod", "Agnerod East", "Agnerod North", "Agnerod South", "Agnerod West"];
            if level >= 75.0 && agnerod.contains(&info.ref_name.as_str()) {
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
        filters.unidentified = Some(UnidentifiedFilter { value: true, disabled: !is_unique });
    }

    if item.is_veiled == Some(true) {
        let stat_refs = item
            .stats_by_type
            .iter()
            .filter(|c| c.kind == ModifierType::Veiled)
            .map(|c| c.stat.stat_ref.clone())
            .collect();
        filters.veiled = Some(VeiledFilter { stat_refs, disabled: false });

        if !is_unique && let Some(level) = &mut filters.item_level {
            level.disabled = false;
        }
    }

    if is_unique {
        filters.foulborn = Some(ValueFilter { value: item.is_foulborn == Some(true) });
        filters.vestigial = Some(ValueFilter { value: item.is_vestigial == Some(true) });
    }

    if (is(item, &C::HeistContract) || is(item, &C::HeistBlueprint) || is(item, &C::Chart)) && !is_unique {
        filters.area_level = Some(FilterNumeric::new(area_level(item)?, false));
    }

    Ok(filters)
}

/// `createGemFilters`
fn create_gem_filters(d: &Data, item: &Item, mut filters: ItemFilters, opts: &CreateOptions) -> R<ItemFilters> {
    let info = item.info();
    let gem = info.gem.as_ref().ok_or_else(|| missing("info.gem"))?;
    if gem.transfigured == Some(true) {
        let normal_variant = gem.normal_variant.as_deref().ok_or_else(|| missing("gem.normalVariant"))?;
        let normal_gem = d
            .item_by_ref(Namespace::Gem, normal_variant)
            .and_then(|v| v.into_iter().next())
            .ok_or_else(|| missing("normal gem"))?;
        filters.search_exact = SearchFilter {
            base_type: Some(info.name.clone()),
            base_type_trade: Some(t(opts, &normal_gem)),
            discriminator_trade: info.trade_disc.clone(),
            ..Default::default()
        };
    } else {
        filters.search_exact = base_type(opts, info);
    }

    if let Some(vaal) = &item.vaal_gem {
        filters.search_exact.sub = Some(Box::new(SearchFilter {
            base_type: Some(vaal.name.clone()),
            base_type_trade: Some(t(opts, vaal)),
            discriminator_trade: info.trade_disc.clone(),
            disabled: Some(false),
            ..Default::default()
        }));
    }

    filters.corrupted = Some(CorruptedFilter {
        value: item.is_corrupted.is_some(),
        twice: item.is_corrupted.as_ref().is_some_and(|c| c.twice == Some(true)),
        exact: None,
    });

    let gem_level = item.gem_level.ok_or_else(|| missing("gemLevel"))?;
    if item.imbued_gem != Some(true) && item.is_corrupted.is_some() && gem_level >= 20.0 {
        filters.imbued_gem = Some(Disabled { disabled: true });
    }

    let max_level = f64::from(gem.max_level);
    filters.gem_level = Some(FilterNumeric::new(gem_level, gem_level < max_level));

    if let Some(quality) = item.quality.filter(|q| *q != 0.0 && !q.is_nan()) {
        let disabled = if gem.max_level == 1 {
            false
        } else if gem.max_level == 20 && gem.transfigured != Some(true) {
            quality < 16.0
        } else {
            quality < 20.0
        };
        filters.quality = Some(FilterNumeric::new(quality, disabled));
    }

    Ok(filters)
}
