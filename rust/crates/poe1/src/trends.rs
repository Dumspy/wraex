//! `web/price-check/trends/getDetailsId.ts`, plus the `deltaFromGraph` function of
//! `trends/PriceTrend.vue`: which poe.ninja entry an item is, and how its 7-day
//! graph reads.
//!
//! The query is whatever `Prices::find_price` takes ([`DbQuery`]); the variant is
//! built from the item's level, influences, sockets and unique roll, exactly as the
//! reference builds its poe.ninja search string.

use wraex_core::Rarity;
use wraex_core::filters::floor_to_bracket;
use wraex_core::js::number_to_string;

use crate::filters::create_item_filters::SPECIAL_SUPPORT_GEM;
use crate::parser::item::Item;
use crate::parser::meta::{ARMOUR, ItemCategory, JEWELLERY, WEAPON};
use crate::parser::modifiers::{ModifierType, StatCalculated};
use crate::prices::DbQuery;

/// `getDetailsId`'s answer: the namespace, the name and poe.ninja's variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetailsId {
    /// `ITEM`, `UNIQUE`, `GEM`, ...
    pub ns: &'static str,
    /// `item.info.refName` in every branch but the coffin.
    pub name: String,
    pub variant: Option<String>,
}

impl DetailsId {
    /// The `findPriceByQuery` query this id stands for.
    pub fn query(&self) -> DbQuery<'_> {
        DbQuery { ns: self.ns, name: &self.name, variant: self.variant.as_deref() }
    }
}

/// `isValuableBasetype`: a normal, magic or rare jewellery, armour or weapon with a
/// price of its own on poe.ninja.
pub fn is_valuable_basetype(item: &Item) -> bool {
    if !matches!(item.rarity, Some(Rarity::Normal | Rarity::Magic | Rarity::Rare)) {
        return false;
    }
    let Some(category) = item.category.as_ref() else { return false };
    JEWELLERY.contains(category)
        || ARMOUR.contains(category)
        || WEAPON.contains(category)
        || *category == ItemCategory::Quiver
}

/// `getDetailsId`: the poe.ninja entry to price `item` by, or `None` where the
/// reference's `!` would have thrown.
pub fn get_details_id(item: &Item) -> Option<DetailsId> {
    match item.category.as_ref() {
        Some(ItemCategory::Gem) => return Some(for_skill_gem(item)),
        Some(ItemCategory::Map) => {
            let unique = item.rarity == Some(Rarity::Unique);
            let tier = if unique { None } else { item.map_tier };
            let generation = (!unique).then(|| if item.map_tier.is_some() { "Gen-24" } else { "Atlas" });
            let parts = [Some(format!("T{}", number_to_string(tier.unwrap_or(0.0)))), generation.map(str::to_owned)];
            return Some(details(item, variant(parts)));
        }
        _ => {}
    }
    if item.rarity == Some(Rarity::Unique) {
        return for_unique_item(item);
    }
    if is_valuable_basetype(item) {
        return for_basetype(item);
    }
    if item.ref_name_is("Filled Coffin") {
        let stat = item.stats_by_type.first()?;
        let value = stat.sources.first()?.contributes?.value;
        let level = item.item_level?;
        let name = stat.stat.stat_ref.replace('#', &number_to_string(value));
        let bracket = number_to_string(floor_to_bracket(level, &[1.0, 70.0, 80.0, 84.0]));
        return Some(DetailsId { name, ..details(item, Some(bracket)) });
    }
    Some(details(item, None))
}

/// A skill gem's variant: its level, quality and corruption.
fn for_skill_gem(item: &Item) -> DetailsId {
    let ref_name = item.info().ref_name.as_str();
    let special = SPECIAL_SUPPORT_GEM.contains(&ref_name);
    let mut variant = String::new();
    if special
        || ref_name == "Portal"
        || ref_name == "Brand Recall"
        || ref_name == "Blood and Sand"
        || item.gem_level.is_some_and(|level| level >= 20.0)
    {
        variant.push_str(&number_to_string(item.gem_level.unwrap_or(0.0)));
    } else {
        variant.push('1');
    }
    let corrupted = item.is_corrupted.is_some();
    if let Some(quality) = item.quality
        && !special
        && !(ref_name == "Brand Recall" && corrupted)
    {
        // Gem Q20 with up to 4xGCP.
        let quality = if (16.0..=20.0).contains(&quality) { 20.0 } else { quality };
        variant.push('/');
        variant.push_str(&number_to_string(quality));
    }
    if corrupted && ref_name != "Portal" {
        variant.push('c');
    }
    details(item, Some(variant))
}

/// A rare or magic base type: its item level and, when it has exactly one, its influence.
fn for_basetype(item: &Item) -> Option<DetailsId> {
    if item.influences.len() > 1 {
        return None;
    }
    let level = item.item_level?;
    let level = if level >= 86.0 { "86+".to_owned() } else { number_to_string(level) };
    let influence = item.influences.first().map(|influence| influence.as_str().to_owned());
    Some(details(item, variant([Some(level), influence])))
}

/// A unique: its variant, the base type it drops on and its linked sockets.
fn for_unique_item(item: &Item) -> Option<DetailsId> {
    let base = item.unique_base.as_ref()?;
    let base = match item.category.as_ref() {
        Some(ItemCategory::Flask) => None,
        Some(ItemCategory::SanctumRelic) => Some("Relic".to_owned()),
        _ => Some(base.ref_name.clone()),
    };
    let linked = item.sockets.as_ref().and_then(|sockets| sockets.linked).map(|linked| format!("{linked}L"));
    Some(details(item, variant([get_unique_variant(item).map(str::to_owned), base, linked])))
}

fn details(item: &Item, variant: Option<String>) -> DetailsId {
    DetailsId { ns: item.info().namespace.as_str(), name: item.info().ref_name.clone(), variant }
}

/// `variant(props)`: the present parts, joined with `", "`.
fn variant(props: impl IntoIterator<Item = Option<String>>) -> Option<String> {
    let parts: Vec<String> = props.into_iter().flatten().collect();
    if parts.is_empty() { None } else { Some(parts.join(", ")) }
}

/// `hasStat`: the item has a modifier with this ref.
fn has_stat(item: &Item, stat: &str) -> bool {
    item.stats_by_type.iter().any(|parsed| parsed.stat.stat_ref == stat)
}

/// The value the first source of a modifier contributes.
fn contributed_value_of(parsed: &StatCalculated) -> Option<f64> {
    parsed.sources.first()?.contributes.map(|contributes| contributes.value)
}

/// The value the first source of a modifier with this ref contributes.
fn contributed_value(item: &Item, stat: &str) -> Option<f64> {
    contributed_value_of(item.stats_by_type.iter().find(|parsed| parsed.stat.stat_ref == stat)?)
}

/// [`contributed_value`] for an explicit modifier only.
fn explicit_contributed_value(item: &Item, stat: &str) -> Option<f64> {
    contributed_value_of(
        item.stats_by_type
            .iter()
            .find(|parsed| parsed.kind == ModifierType::Explicit && parsed.stat.stat_ref == stat)?,
    )
}

/// `getUniqueVariant`'s `Vessel of Vinktar` branch.
fn vessel_of_vinktar_variant(item: &Item) -> Option<&'static str> {
    [
        ("Adds # to # Lightning Damage to Attacks during Flask effect", "Added Attacks"),
        ("Adds # to # Lightning Damage to Spells during Flask effect", "Added Spells"),
        ("Damage Penetrates #% Lightning Resistance during Flask effect", "Penetration"),
        ("#% of Physical Damage Converted to Lightning during Flask effect", "Conversion"),
    ]
    .into_iter()
    .find_map(|(stat, variant)| has_stat(item, stat).then_some(variant))
}

/// `getUniqueVariant`'s `Atziri's Splendour` branch: every stat of a row is required.
fn atziris_splendour_variant(item: &Item) -> Option<&'static str> {
    [
        (&["#% increased Armour, Evasion and Energy Shield"][..], "Armour/Evasion/ES"),
        (&["#% increased Evasion and Energy Shield", "+# to maximum Energy Shield"][..], "Evasion/ES"),
        (&["#% increased Evasion and Energy Shield", "+# to maximum Life"][..], "Evasion/ES/Life"),
        (&["#% increased Armour and Energy Shield", "+# to maximum Energy Shield"][..], "Armour/ES"),
        (&["#% increased Armour and Energy Shield", "+# to maximum Life"][..], "Armour/ES/Life"),
        (&["#% increased Armour and Evasion", "+# to maximum Life"][..], "Armour/Evasion"),
        (&["+# to maximum Energy Shield", "#% increased Energy Shield"][..], "ES"),
        (&["#% increased Evasion Rating", "+# to maximum Life"][..], "Evasion"),
        (&["#% increased Armour", "+# to maximum Life"][..], "Armour"),
    ]
    .into_iter()
    .find_map(|(stats, variant)| stats.iter().all(|stat| has_stat(item, stat)).then_some(variant))
}

/// `getUniqueVariant`'s `Volkuur's Guidance` branch.
fn volkuurs_guidance_variant(item: &Item) -> Option<&'static str> {
    [
        ("Adds # to # Cold Damage to Spells and Attacks", "Cold"),
        ("Adds # to # Fire Damage to Spells and Attacks", "Fire"),
        ("Adds # to # Lightning Damage to Spells and Attacks", "Lightning"),
    ]
    .into_iter()
    .find_map(|(stat, variant)| has_stat(item, stat).then_some(variant))
}

/// `getUniqueVariant`'s `Yriel's Fostering` branch.
fn yriels_fostering_variant(item: &Item) -> Option<&'static str> {
    [
        ("Projectiles from Attacks have #% chance to Maim on Hit while\nyou have a Bestial Minion", "Maim"),
        ("Projectiles from Attacks have #% chance to Poison on Hit while\nyou have a Bestial Minion", "Poison"),
        (
            "Projectiles from Attacks have #% chance to inflict Bleeding on Hit while\nyou have a Bestial Minion",
            "Bleeding",
        ),
    ]
    .into_iter()
    .find_map(|(stat, variant)| has_stat(item, stat).then_some(variant))
}

/// `getUniqueVariant`'s `Doryani's Invitation` branch.
fn doryanis_invitation_variant(item: &Item) -> Option<&'static str> {
    [
        ("#% increased Global Physical Damage", "Physical"),
        ("#% increased Fire Damage", "Fire"),
        ("#% increased Cold Damage", "Cold"),
        ("#% increased Lightning Damage", "Lightning"),
    ]
    .into_iter()
    .find_map(|(stat, variant)| has_stat(item, stat).then_some(variant))
}

/// `getUniqueVariant`'s `Impresence` branch.
fn impresence_variant(item: &Item) -> Option<&'static str> {
    [
        ("Adds # to # Cold Damage", "Cold"),
        ("Adds # to # Chaos Damage", "Chaos"),
        ("Adds # to # Fire Damage", "Fire"),
        ("Adds # to # Lightning Damage", "Lightning"),
        ("Adds # to # Physical Damage", "Physical"),
    ]
    .into_iter()
    .find_map(|(stat, variant)| has_stat(item, stat).then_some(variant))
}

/// `getUniqueVariant`'s `The First Crest` branch.
fn the_first_crest_variant(item: &Item) -> Option<&'static str> {
    [
        ("Aureus Coins are converted to Experience upon defeating the Herald of the Scourge", "Experience"),
        ("Aureus Coins are converted to Relics upon defeating the Herald of the Scourge", "Relics"),
        ("Aureus Coins are converted to Tainted Currency upon defeating the Herald of the Scourge", "Tainted Currency"),
    ]
    .into_iter()
    .find_map(|(stat, variant)| has_stat(item, stat).then_some(variant))
}

/// `getUniqueVariant`: the roll that picks one poe.ninja entry for a unique with
/// several versions.
fn get_unique_variant(item: &Item) -> Option<&'static str> {
    match item.info().ref_name.as_str() {
        "Vessel of Vinktar" => vessel_of_vinktar_variant(item),
        "Atziri's Splendour" => atziris_splendour_variant(item),
        "Bubonic Trail" | "Lightpoacher" | "Shroud of the Lightless" | "Tombfist" => {
            match explicit_contributed_value(item, "Has # Abyssal Sockets")? {
                2.0 => Some("2 Jewels"),
                1.0 => Some("1 Jewel"),
                _ => None,
            }
        }
        "Volkuur's Guidance" => volkuurs_guidance_variant(item),
        "Yriel's Fostering" => yriels_fostering_variant(item),
        "Doryani's Invitation" => doryanis_invitation_variant(item),
        "Impresence" => impresence_variant(item),
        "Voices" => match contributed_value(item, "Adds # Small Passive Skills which grant nothing")? {
            7.0 => Some("7 passives"),
            5.0 => Some("5 passives"),
            3.0 => Some("3 passives"),
            _ => None,
        },
        "The First Crest" => the_first_crest_variant(item),
        _ => None,
    }
}

/// `deltaFromGraph`'s forecast: the chevron `PriceTrend.vue` draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Forecast {
    Up,
    Down,
    Const,
}

/// `deltaFromGraph`: how the 7-day graph moved, and the box the sparkline draws in.
#[derive(Debug, Clone, PartialEq)]
pub struct TrendChange {
    /// The graph without its `null`s.
    pub points: Vec<f64>,
    pub draw_min: f64,
    pub draw_max: f64,
    pub forecast: Forecast,
    /// `"6 %"`, with upstream's thin space before the percent sign.
    pub text: String,
}

/// `deltaFromGraph(graphPoints)`, or `None` with fewer than two numbers.
pub fn delta_from_graph(graph: &[Option<f64>]) -> Option<TrendChange> {
    let points: Vec<f64> = graph.iter().flatten().copied().collect();
    if points.len() < 2 {
        return None;
    }

    let mut forecast = Forecast::Const;
    if points.len() == 7 {
        // The last three points, the whole graph when it is shorter (it is not).
        let last_three = points.get(4..).unwrap_or_default();
        if points.iter().filter(|point| **point > 0.0).count() >= 4 || last_three.iter().all(|point| *point > 0.0) {
            forecast = Forecast::Up;
        } else if points.iter().filter(|point| **point < 0.0).count() >= 4
            || last_three.iter().all(|point| *point < 0.0)
        {
            forecast = Forecast::Down;
        }
    }

    let count = wraex_core::num::usize_to_f64(points.len());
    let mean = points.iter().sum::<f64>() / count;
    let squares: f64 = points.iter().map(|point| (point - mean).powi(2)).sum();
    let change = (squares / (count - 1.0)).sqrt();
    // `changeVal || 1`: a flat graph still gets a box to draw in.
    let pad = if change == 0.0 { 1.0 } else { change };
    let min = points.iter().copied().fold(f64::INFINITY, f64::min);
    let max = points.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    // `Math.round(changeVal * 2)`: halves toward +∞, with the product rounded first.
    let doubled = change * 2.0;
    let rounded = (doubled + 0.5).floor();
    Some(TrendChange {
        points,
        draw_min: min - pad,
        draw_max: max + pad,
        forecast,
        text: format!("{}\u{2009}%", number_to_string(rounded)),
    })
}

#[cfg(test)]
mod tests {
    use wraex_core::GameId;

    use super::*;
    use crate::data::{Data, reference_source};
    use crate::prices::NinjaDenseInfo;

    fn item(slug: &str) -> Item {
        let data = Data::load(&reference_source(), "en").expect("data");
        let cases = wraex_golden::cases(GameId::Poe1);
        let case = cases.iter().find(|case| case.slug == slug).unwrap_or_else(|| panic!("no case {slug}"));
        crate::parser::parse_clipboard(&data, &case.input).expect("a parsed item")
    }

    fn id(slug: &str) -> DetailsId {
        get_details_id(&item(slug)).unwrap_or_else(|| panic!("{slug} has an id"))
    }

    /// The queries the app looks the default cases up with: they are what the recorded
    /// poe.ninja blob holds.
    #[test]
    fn the_details_ids_are_the_dense_blob_entries() {
        let divine = id("currency-divine-orb");
        assert_eq!((divine.ns, divine.name.as_str(), divine.variant), ("ITEM", "Divine Orb", None));

        // A rare's base type, with its item level as the variant.
        let ring = id("adv-rare-ring-mirrored-catalyst");
        assert_eq!((ring.ns, ring.name.as_str(), ring.variant.as_deref()), ("ITEM", "Diamond Ring", Some("85")));

        // A unique: the base type it drops on.
        let jewel = id("adv-unique-timeless-jewel");
        assert_eq!(
            (jewel.ns, jewel.name.as_str(), jewel.variant.as_deref()),
            ("UNIQUE", "Lethal Pride", Some("Timeless Jewel"))
        );

        // A map: its tier and the generation it was added in.
        let map = id("real-map-rare-t16-max-res");
        assert_eq!(map.variant.as_deref(), Some("T16, Gen-24"));
    }

    /// `isValuableBasetype`: jewellery, armour and weapons, not uniques or currency.
    #[test]
    fn valuable_base_types_are_jewellery_armour_and_weapons() {
        assert!(is_valuable_basetype(&item("adv-rare-ring-mirrored-catalyst")));
        assert!(!is_valuable_basetype(&item("adv-unique-timeless-jewel")));
        assert!(!is_valuable_basetype(&item("currency-divine-orb")));
    }

    /// The recorded graphs, as `PriceTrend.vue` reads them.
    #[test]
    fn the_graph_becomes_a_forecast_and_a_sparkline_box() {
        let divine = delta_from_graph(&[
            Some(-6.37),
            Some(-10.54),
            Some(-10.14),
            Some(-9.13),
            Some(-10.97),
            Some(-8.86),
            Some(-2.95),
        ])
        .expect("seven points");
        assert_eq!(divine.forecast, Forecast::Down);
        assert_eq!(divine.text, "6\u{2009}%");

        let jewel =
            delta_from_graph(&[Some(0.0), Some(25.0), Some(0.0), Some(25.0), Some(25.0), Some(25.0), Some(23.88)])
                .expect("seven points");
        assert_eq!(jewel.forecast, Forecast::Up);
        assert_eq!(jewel.text, "24\u{2009}%");

        // A flat graph: no chevrons change, and the box is one point either side.
        let flat = delta_from_graph(&[Some(0.0); 7]).expect("seven points");
        assert_eq!(flat.forecast, Forecast::Const);
        assert_eq!(flat.text, "0\u{2009}%");
        assert_eq!((flat.draw_min, flat.draw_max), (-1.0, 1.0));

        // Fewer than two numbers: no trend row.
        assert!(delta_from_graph(&[Some(1.0), None]).is_none());
        assert!(delta_from_graph(&[]).is_none());
    }

    /// The dense blob's own `graph` arrays parse into the same numbers.
    #[test]
    fn a_dense_info_keeps_its_graph() {
        let info: NinjaDenseInfo = serde_json::from_str(
            r#"{"name":"Lethal Pride","variant":"Timeless Jewel","chaos":99.1,"graph":[0,25,0,25,25,25,23.88]}"#,
        )
        .unwrap();
        assert_eq!(delta_from_graph(&info.graph).map(|change| change.text), Some("24\u{2009}%".to_owned()));
    }
}
