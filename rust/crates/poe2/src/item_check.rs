//! `ref/ee2/renderer/src/web/item-check/hotkeyable-actions.ts`: what the item
//! check's hotkeys open (wiki, poe2db, Craft of Exile) or send (stash searches
//! for similar or same-priced items). Opening the URL and sending the event is
//! the app shell's job.

use serde::{Deserialize, Serialize};
use wraex_core::Rarity;
use wraex_core::js::{encode_uri_component, json_string};

use crate::parser::item::Item;
use crate::parser::meta::{ARMOUR, ItemCategory};

/// `CLIENT->MAIN::user-action` with a `stash-search`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UserAction {
    pub name: &'static str,
    pub payload: StashSearch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StashSearch {
    pub action: &'static str,
    /// `JSON.stringify(item.info.name)`: `undefined` for a nameless item.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

fn stash_search(text: Option<String>) -> UserAction {
    UserAction { name: "CLIENT->MAIN::user-action", payload: StashSearch { action: "stash-search", text } }
}

/// `ItemCheckWidget['samePricedType']`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SamePricedType {
    Price,
    Both,
}

/// `POEDB_LANGS`: poe2db's path segment for an app language.
fn poedb_lang(language: &str) -> &'static str {
    match language {
        "en" => "us",
        "ru" => "ru",
        "cmn-Hant" => "tw",
        "ko" => "kr",
        "ja" => "jp",
        "de" => "de",
        "es" => "sp",
        "pt" => "pt",
        "fr" => "fr",
        _ => "undefined",
    }
}

/// `POE2DB_CONVERSIONS`: keyed by category value or by `refName`, as upstream
/// puts both in one map.
fn poe2db_conversion(key: &str) -> Option<&'static str> {
    use ItemCategory as C;
    let category = |c: C| c.as_str().to_owned();
    let pages: [(String, &str); 28] = [
        // One-handed
        (category(C::Claw), "Claws"),
        (category(C::Dagger), "Daggers"),
        (category(C::Wand), "Wands"),
        (category(C::OneHandedSword), "One_Hand_Swords"),
        (category(C::OneHandedAxe), "One_Hand_Axes"),
        (category(C::OneHandedMace), "One_Hand_Maces"),
        (category(C::Sceptre), "Sceptres"),
        (category(C::Spear), "Spears"),
        (category(C::Flail), "Flails"),
        // Two-handed
        (category(C::Bow), "Bows"),
        (category(C::Staff), "Staves"),
        (category(C::TwoHandedSword), "Two_Hand_Swords"),
        (category(C::TwoHandedAxe), "Two_Hand_Axes"),
        (category(C::TwoHandedMace), "Two_Hand_Maces"),
        (category(C::Warstaff), "Quarterstaves"),
        (category(C::Crossbow), "Crossbows"),
        (category(C::Talisman), "Talismans"),
        // Jewellery
        (category(C::Amulet), "Amulets"),
        (category(C::Ring), "Rings"),
        (category(C::Belt), "Belts"),
        // Armour
        (category(C::BodyArmour), "Body_Armours"),
        (category(C::Helmet), "Helmets"),
        (category(C::Gloves), "Gloves"),
        (category(C::Boots), "Boots"),
        // Off-hand
        (category(C::Quiver), "Quivers"),
        (category(C::Shield), "Shields"),
        (category(C::Focus), "Foci"),
        (category(C::Buckler), "Bucklers"),
    ];
    const BY_REF_NAME: [(&str, &str); 22] = [
        // Jewels
        ("Ruby", "Ruby"),
        ("Emerald", "Emerald"),
        ("Sapphire", "Sapphire"),
        ("Time-Lost Ruby", "Time-Lost_Ruby"),
        ("Time-Lost Emerald", "Time-Lost_Emerald"),
        ("Time-Lost Sapphire", "Time-Lost_Sapphire"),
        // Flasks
        ("Life Flask", "Life_Flasks"),
        ("Mana Flask", "Mana_Flasks"),
        ("Charm", "Charms"),
        // Relics
        ("Urn Relic", "Urn_Relic"),
        ("Amphora Relic", "Amphora_Relic"),
        ("Vase Relic", "Vase_Relic"),
        ("Seal Relic", "Seal_Relic"),
        ("Coffer Relic", "Coffer_Relic"),
        ("Tapestry Relic", "Tapestry_Relic"),
        ("Incense Relic", "Incense_Relic"),
        // Tablets
        ("Breach Precursor Tablet", "Breach_Precursor_Tablet"),
        ("Expedition Precursor Tablet", "Expedition_Precursor_Tablet"),
        ("Delirium Precursor Tablet", "Delirium_Precursor_Tablet"),
        ("Ritual Precursor Tablet", "Ritual_Precursor_Tablet"),
        ("Precursor Tablet", "Precursor_Tablet"),
        ("Overseer Precursor Tablet", "Overseer_Precursor_Tablet"),
    ];
    // Waystones
    const WAYSTONES: [(&str, &str); 3] = [
        ("Waystones(Low)", "Waystones_low_tier"),
        ("Waystones(Mid)", "Waystones_mid_tier"),
        ("Waystones(Top)", "Waystones_top_tier"),
    ];
    pages
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| *v)
        .or_else(|| BY_REF_NAME.iter().chain(&WAYSTONES).find(|(k, _)| *k == key).map(|(_, v)| *v))
}

/// `encodePoe2dbUri`: spaces to underscores, no apostrophes.
fn encode_poe2db_uri(text: &str) -> String {
    text.replace(' ', "_").replace('\'', "")
}

/// `getPoe2dbPath`: a unique's page, a base's modifier calculator, or the
/// category's (armour by its defences). `None` (`undefined`) without a category.
fn get_poe2db_path(item: &Item) -> Option<String> {
    let category = item.category.as_ref()?;
    let ref_name = item.info().ref_name.as_deref().unwrap_or("undefined");
    if item.rarity == Some(Rarity::Unique) {
        return Some(encode_poe2db_uri(ref_name));
    }
    if let Some(page) = poe2db_conversion(ref_name) {
        return Some(format!("{page}#ModifiersCalc"));
    }
    // No known modifiers page for the category.
    let Some(page) = poe2db_conversion(category.as_str()) else { return Some(encode_poe2db_uri(ref_name)) };

    let mut path = page.to_owned();
    if ARMOUR.contains(category) && !matches!(category, ItemCategory::Buckler | ItemCategory::Focus) {
        let truthy = |x: Option<f64>| x.is_some_and(|v| v != 0.0 && !v.is_nan());
        if truthy(item.armour_ar) {
            path.push_str("_str");
        }
        if truthy(item.armour_ev) {
            path.push_str("_dex");
        }
        if truthy(item.armour_es) {
            path.push_str("_int");
        }
    }
    Some(path + "#ModifiersCalc")
}

/// `openWiki`
pub fn open_wiki(item: &Item) -> String {
    format!("https://www.poe2wiki.net/wiki/{}", item.info().ref_name.as_deref().unwrap_or("undefined"))
}

/// `openPoedb`, for the app's `language`.
pub fn open_poedb(item: &Item, language: &str) -> String {
    let path = get_poe2db_path(item).unwrap_or_else(|| "undefined".to_owned());
    format!("https://poe2db.tw/{}/{path}", poedb_lang(language))
}

/// `openCoE`: the whole item text goes to Craft of Exile.
pub fn open_coe(item: &Item) -> String {
    format!("https://craftofexile.com/?game=poe2&eimport={}", encode_uri_component(&item.raw_text))
}

/// `findSimilarItems`: a stash search for the item's name.
pub fn find_similar_items(item: &Item) -> UserAction {
    stash_search(item.info().name.as_deref().map(json_string))
}

/// `findSamePricedItems`: a stash search for the item's price note (and its
/// name, if both fit the search box's 50 characters). Nothing without a note.
pub fn find_same_priced_items(item: &Item, same_priced_type: SamePricedType) -> Option<UserAction> {
    let note = item.note.as_deref().filter(|n| !n.is_empty())?;
    let note = json_string(note);
    // `JSON.stringify(undefined)` is `undefined`, whose `length` throws; a
    // nameless item searches by note only.
    let text = match item.info().name.as_deref().map(json_string) {
        Some(name)
            if same_priced_type == SamePricedType::Both
                && note.encode_utf16().count() + name.encode_utf16().count() < 51 =>
        {
            format!("{note}{name}")
        }
        _ => note,
    };
    Some(stash_search(Some(text)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `specs/web/item-check.test.ts` (`encodePoe2dbUri`).
    #[test]
    fn encodes_poe2db_uris() {
        assert_eq!(encode_poe2db_uri("a b c"), "a_b_c");
        assert_eq!(encode_poe2db_uri("a'b'c"), "abc");
        assert_eq!(encode_poe2db_uri("a'b c"), "ab_c");
        assert_eq!(encode_poe2db_uri("A/b-cö"), "A/b-cö");
    }
}
