//! `ref/apt/renderer/src/web/item-check/hotkeyable-actions.ts`: what the item
//! check's hotkeys open (wiki, poedb, Craft of Exile) or send (a stash search).
//! Opening the URL and sending the event is the app shell's job.

use serde::Serialize;
use wraex_core::js::{encode_uri_component, json_string};

use crate::parser::item::Item;

/// `CLIENT->MAIN::user-action` with a `stash-search`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UserAction {
    pub name: &'static str,
    pub payload: StashSearch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StashSearch {
    pub action: &'static str,
    pub text: String,
}

/// `POEDB_LANGS`: poedb's path segment for an app language.
fn poedb_lang(language: &str) -> &'static str {
    match language {
        "en" => "us",
        "ru" => "ru",
        "cmn-Hant" => "tw",
        "ko" => "kr",
        _ => "undefined",
    }
}

/// `openWiki`
pub fn open_wiki(item: &Item) -> String {
    format!("https://www.poewiki.net/wiki/{}", item.info().ref_name)
}

/// `openPoedb`, for the app's `language`.
pub fn open_poedb(item: &Item, language: &str) -> String {
    let slug = encode_uri_component(&item.info().ref_name.replace('\'', "").replace(' ', "_"));
    format!("https://poedb.tw/{}/{slug}", poedb_lang(language))
}

/// `openCoE`: the whole item text goes to Craft of Exile.
pub fn open_coe(item: &Item) -> String {
    format!("https://craftofexile.com/?game=poe1&eimport={}", encode_uri_component(&item.raw_text))
}

/// `findSimilarItems`: a stash search for the item's name.
pub fn find_similar_items(item: &Item) -> UserAction {
    UserAction {
        name: "CLIENT->MAIN::user-action",
        payload: StashSearch { action: "stash-search", text: json_string(&item.info().name) },
    }
}
