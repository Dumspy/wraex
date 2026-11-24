//! The Wraex desktop app.
//!
//! A Rune shell over the core crates. So far it has one window, PoE1's price check
//! ([`poe1::price_check`]); the title bar and the widgets it is built from live in [`ui`] and
//! [`theme`], shared with the PoE2 windows that come later.
//!
//! `cargo run -p wraex-app` opens the window; `-- --snap out.png --scene <slug>` renders one of
//! the corpus cases headlessly (`--list-scenes` lists them).

pub mod poe1;
pub mod theme;
pub mod ui;

use rune::{AppSpec, Cx, El};

use crate::poe1::price_check::{PriceCheck, WIDTH};

/// The window's height, in points. Upstream's overlay spans the game's panel; the screenshots
/// in `tools/golden/screens` are 900 tall, so the port's window is too.
pub const HEIGHT: f32 = 900.0;

/// The app's state. One window so far; each new widget adds a field here.
#[derive(Debug)]
pub struct App {
    pub price_check: PriceCheck,
}

impl App {
    /// The app with its price check opened on `text` (the game's copy key produced it).
    pub fn from_text(text: &str) -> Self {
        Self { price_check: PriceCheck::from_text(text) }
    }

    /// Reopen the price check on a corpus case: `<slug>.txt`.
    pub fn load(&mut self, slug: &str) {
        self.price_check = PriceCheck::from_text(&corpus(slug));
    }
}

impl Default for App {
    fn default() -> Self {
        Self::from_text(&corpus(SCENES[0].0))
    }
}

/// `App` → the price check, for the window's handlers.
fn project(app: &mut App) -> &mut PriceCheck {
    &mut app.price_check
}

/// The app's single window, for now.
pub fn view(app: &App, cx: &mut Cx<App>) -> El {
    poe1::price_check::view::view(&app.price_check, cx, project)
}

/// A named state for `--scene` (`AppSpec::scenes`).
pub type Scene = (&'static str, fn(&mut App));

/// The PoE1 corpus cases the reference screenshots render
/// (`tools/golden/screens/cases.mjs`), plus a text that isn't an item and three cases that
/// exercise the other kinds of filter row: a magic flask (explicit stats), a cluster jewel
/// (enchant and hidden stats, two presets) and an anointed amulet (pseudo stats and the oil
/// row).
pub const SCENES: &[Scene] = &[
    ("currency-divine-orb", |app| app.load("currency-divine-orb")),
    ("adv-rare-ring-mirrored-catalyst", |app| app.load("adv-rare-ring-mirrored-catalyst")),
    ("adv-unique-timeless-jewel", |app| app.load("adv-unique-timeless-jewel")),
    ("real-map-rare-t16-max-res", |app| app.load("real-map-rare-t16-max-res")),
    ("error-not-an-item", |app| app.load("error-not-an-item")),
    ("adv-magic-flask-charge-recovery", |app| app.load("adv-magic-flask-charge-recovery")),
    ("jewel-cluster-large", |app| app.load("jewel-cluster-large")),
    ("adv-rare-amulet-anointed", |app| app.load("adv-rare-amulet-anointed")),
];

/// The fixture corpus a scene loads (`tools/golden/corpus/poe1/<slug>.txt`).
pub fn corpus(slug: &str) -> String {
    let path = format!("{CORPUS}/{slug}.txt");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
}

const CORPUS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../tools/golden/corpus/poe1");

/// Runs the app: a window, or a headless render with Rune's `--snap`/`--scene`/`--tree`.
pub fn main() {
    rune::run(AppSpec::<App> {
        scenes: SCENES,
        fonts: theme::FONTS,
        families: theme::FAMILIES,
        // The window is exactly the widget: the price check is a fixed 28.75 rem wide, so the
        // window must not shrink past it. (As an overlay it won't be resizable at all.)
        min_size: Some((WIDTH, HEIGHT)),
        ..AppSpec::new("Wraex", (WIDTH, HEIGHT), App::default, view)
    });
}

#[cfg(test)]
mod tests {
    use rune::runtime::ElementInfo;
    use rune::text::TextSystem;
    use rune::{Env, Runtime};

    use super::*;

    fn runtime(slug: &str) -> Runtime<App> {
        let mut app = App::default();
        app.load(slug);
        let text = TextSystem::new(theme::FONTS, theme::FAMILIES);
        let mut runtime = Runtime::with_text(app, view, (WIDTH, HEIGHT), Env::new(true, 1.0), text);
        runtime.frame(0.0);
        runtime
    }

    fn element<'a>(elements: &'a [ElementInfo], key: &str) -> Option<&'a ElementInfo> {
        elements.iter().find(|element| element.key.as_deref() == Some(key))
    }

    fn text_of(runtime: &Runtime<App>, key: &str) -> Option<String> {
        let elements = runtime.elements();
        element(&elements, key)?.text.clone()
    }

    fn click(runtime: &mut Runtime<App>, key: &str) {
        let (x, y) = runtime.find(key).unwrap_or_else(|| panic!("no element keyed `{key}`"));
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0 / 60.0);
    }

    /// Focus a text field, select its contents and paste `text` over them.
    fn replace_input(runtime: &mut Runtime<App>, key: &str, text: &str) {
        click(runtime, key);
        runtime.key(rune::KeyEvent { key: rune::Key::Command('a'), mods: rune::Mods::default() });
        runtime.frame(1.0 / 60.0);
        runtime.paste(text);
        runtime.frame(1.0 / 60.0);
    }

    /// The window frame, in every scene: a title bar over `bg-gray-800`.
    #[test]
    fn every_scene_has_the_title_bar() {
        // With no `--scene`, the window opens on the first scene.
        assert!(App::default().price_check.item.is_ok());
        for (slug, _) in SCENES {
            let runtime = runtime(slug);
            assert_eq!(text_of(&runtime, "titlebar-title").as_deref(), Some("Standard"), "{slug}");
            assert!(runtime.find("titlebar-close").is_some(), "{slug}: close button");
            assert!(!runtime.elements().is_empty());
        }
    }

    /// A stackable currency: it searches straight away (`filters.preset_bulk`), so the Search
    /// row is gone and the listings stand in for it.
    #[test]
    fn the_divine_orb_shows_its_base_type_and_searches_at_once() {
        let runtime = runtime("currency-divine-orb");
        assert_eq!(text_of(&runtime, "filter-name-text").as_deref(), Some("Divine Orb"));
        // The item's `?` price row, for everything the trade site has no price for yet.
        assert_eq!(text_of(&runtime, "trend-price").as_deref(), Some("?"));
        assert!(runtime.find("listings-placeholder").is_some());
        assert!(runtime.find("search-label").is_none(), "no Search button once `doSearch`");
    }

    /// A rare searches by category and waits for the button.
    #[test]
    fn a_rare_ring_labels_its_category_and_offers_search() {
        let mut runtime = runtime("adv-rare-ring-mirrored-catalyst");
        assert_eq!(text_of(&runtime, "filter-name-text").as_deref(), Some("Category: Ring"));
        assert_eq!(text_of(&runtime, "corrupted-label").as_deref(), Some("Not Corrupted"));
        assert_eq!(text_of(&runtime, "search-label").as_deref(), Some("Search"));
        assert!(runtime.find("listings-placeholder").is_none());

        click(&mut runtime, "search");
        assert!(runtime.state.price_check.do_search);
        assert!(runtime.find("listings-placeholder").is_some());
    }

    /// A unique is searched by name, and has the price row.
    #[test]
    fn the_timeless_jewel_shows_its_unique_name() {
        let runtime = runtime("adv-unique-timeless-jewel");
        assert_eq!(text_of(&runtime, "filter-name-text").as_deref(), Some("Lethal Pride"));
        assert_eq!(text_of(&runtime, "corrupted-label").as_deref(), Some("Not Corrupted"));
        assert_eq!(text_of(&runtime, "trend-price").as_deref(), Some("?"));
        assert!(runtime.find("search-label").is_none());
    }

    /// A map has no name to search by: `FilterName` shows its category.
    #[test]
    fn a_map_searches_by_category() {
        let runtime = runtime("real-map-rare-t16-max-res");
        assert_eq!(text_of(&runtime, "filter-name-text").as_deref(), Some("Map"));
        assert_eq!(text_of(&runtime, "search-label").as_deref(), Some("Search"));
    }

    /// Text that isn't an item: the error box, then the raw block.
    #[test]
    fn a_text_that_is_not_an_item_shows_the_error() {
        let runtime = runtime("error-not-an-item");
        assert_eq!(text_of(&runtime, "error-name").as_deref(), Some("An error occurred while parsing the item"));
        assert_eq!(
            text_of(&runtime, "error-message").as_deref(),
            Some("This is probably a bug and you can report it on GitHub.")
        );
        let raw = text_of(&runtime, "error-raw-text").expect("the raw text block");
        assert!(raw.contains("This is not an item"), "{raw}");
        assert!(runtime.find("filter-name-text").is_none());
    }

    /// `FilterName.vue`'s two toggles run on the state.
    #[test]
    fn the_item_name_toggles_the_corruption_filter() {
        let mut runtime = runtime("adv-rare-ring-mirrored-catalyst");
        assert!(!runtime.state.price_check.item_filters().unwrap().corrupted.as_ref().unwrap().value);
        click(&mut runtime, "corrupted");
        assert!(runtime.state.price_check.item_filters().unwrap().corrupted.as_ref().unwrap().value);
        assert_eq!(text_of(&runtime, "corrupted-label").as_deref(), Some("Corrupted"));
    }

    /// The close button hides the widget, as `wm.hide` does upstream.
    #[test]
    fn closing_hides_the_window() {
        let mut runtime = runtime("currency-divine-orb");
        click(&mut runtime, "titlebar-close");
        assert!(!runtime.state.price_check.open);
        assert!(runtime.find("titlebar").is_none());
    }

    /// `FiltersBlock.vue` for the rare ring: its three visible rows, with upstream's labels
    /// and the rolls `ItemModifierText.vue` substitutes.
    #[test]
    fn the_rare_ring_lists_its_stat_rows() {
        let runtime = runtime("adv-rare-ring-mirrored-catalyst");
        // Stats 0, 2 and 3 are the visible rows; 1 (Fire Resistance) and 4 (Quality) are hidden.
        let expected = [
            (0, "94% total Elemental Resistance"),
            (2, "98 total maximum Life"),
            (3, "25% Global Critical Strike Chance"),
        ];
        for (index, label) in expected {
            assert_eq!(text_of(&runtime, &format!("stat-{index}-text")).as_deref(), Some(label), "row {index}");
        }
        assert!(runtime.find("stat-1").is_none());
        let stats = runtime.state.price_check.item_stats();
        let rolls: Vec<Option<f64>> = stats
            .iter()
            .filter_map(|stat| match stat {
                wraex_poe1::filters::FilterOrGroup::Stat(filter) => Some(filter.roll.as_ref().map(|roll| roll.value)),
                wraex_poe1::filters::FilterOrGroup::Group(_) => None,
            })
            .collect();
        assert_eq!(rolls, [Some(94.0), Some(66.0), Some(98.0), Some(25.0), Some(20.0)]);
        // The hidden Fire Resistance and Quality rows stay out of the list until `Hidden`.
        assert!(runtime.find("stat-1").is_none());
        assert!(runtime.find("stat-4").is_none());
        assert_eq!(text_of(&runtime, "btn-stats-label").as_deref(), Some("2 of 5, stats"));
    }

    /// `FilterModifier.vue`'s checkbox writes `filter.disabled`.
    #[test]
    fn a_stat_row_toggles_its_filter() {
        let mut runtime = runtime("adv-rare-ring-mirrored-catalyst");
        let disabled = |runtime: &Runtime<App>, index: usize| match &runtime.state.price_check.item_stats()[index] {
            wraex_poe1::filters::FilterOrGroup::Stat(filter) => filter.disabled,
            wraex_poe1::filters::FilterOrGroup::Group(group) => group.meta.disabled,
        };
        assert!(disabled(&runtime, 3));
        click(&mut runtime, "stat-3-toggle");
        assert!(!disabled(&runtime, 3));
        assert_eq!(text_of(&runtime, "btn-stats-label").as_deref(), Some("3 of 5, stats"));
        click(&mut runtime, "stat-3-toggle");
        assert!(disabled(&runtime, 3));
    }

    /// Typing in a row's min field parses the number back into `filter.roll.min`.
    #[test]
    fn a_stat_row_parses_its_min_field() {
        let mut runtime = runtime("adv-rare-ring-mirrored-catalyst");
        assert_eq!(
            runtime.state.price_check.item_stats().first().and_then(|stat| match stat {
                wraex_poe1::filters::FilterOrGroup::Stat(filter) => filter.roll.as_ref().and_then(|roll| roll.min),
                wraex_poe1::filters::FilterOrGroup::Group(_) => None,
            }),
            Some(84.0)
        );
        replace_input(&mut runtime, "stat-0-min", "100");
        let roll = match &runtime.state.price_check.item_stats()[0] {
            wraex_poe1::filters::FilterOrGroup::Stat(filter) => filter.roll.clone().expect("a roll"),
            wraex_poe1::filters::FilterOrGroup::Group(_) => panic!("a stat row"),
        };
        assert_eq!(roll.min, Some(100.0));
        assert!(!runtime.state.price_check.item_stats().first().is_some_and(|stat| match stat {
            wraex_poe1::filters::FilterOrGroup::Stat(filter) => filter.disabled,
            wraex_poe1::filters::FilterOrGroup::Group(_) => true,
        }));
        // Enter submits the form (`@submit` → `doSearch`).
        click(&mut runtime, "stat-0-max");
        runtime.key(rune::KeyEvent { key: rune::Key::Enter, mods: rune::Mods::default() });
        runtime.frame(1.0 / 60.0);
        assert!(runtime.state.price_check.do_search);
    }

    /// `FiltersBlock.vue`'s preset tabs replace the rows.
    #[test]
    fn switching_preset_replaces_the_rows() {
        let mut runtime = runtime("real-map-rare-t16-max-res");
        assert_eq!(text_of(&runtime, "stat-0-text").as_deref(), Some("Atlas Drops: 67%"));
        assert!(runtime.find("stat-0").is_some());
        click(&mut runtime, "preset-1");
        assert_eq!(
            runtime.state.price_check.active_preset().map(|preset| preset.id.as_str()),
            Some("filters.preset_bulk")
        );
        assert!(runtime.find("stat-0").is_none());
        assert_eq!(text_of(&runtime, "btn-stats-label").as_deref(), Some("Stats ignored"));
        click(&mut runtime, "preset-0");
        assert_eq!(text_of(&runtime, "stat-0-text").as_deref(), Some("Atlas Drops: 67%"));
    }

    /// Collapse hides the rows (the chip and the button both toggle it) and Hidden shows the
    /// rows upstream hides for a reason.
    #[test]
    fn collapse_and_hidden_switch_what_the_block_lists() {
        let mut runtime = runtime("adv-rare-ring-mirrored-catalyst");
        click(&mut runtime, "filters-collapse");
        assert!(runtime.state.price_check.stats_collapsed);
        assert!(runtime.find("stat-0").is_none());
        click(&mut runtime, "btn-stats");
        assert!(!runtime.state.price_check.stats_collapsed);
        assert!(runtime.find("stat-0").is_some());

        assert!(runtime.find("stat-1").is_none());
        click(&mut runtime, "show-hidden");
        assert!(runtime.state.price_check.show_hidden);
        assert_eq!(text_of(&runtime, "stat-1-text").as_deref(), Some("66% total to Fire Resistance"));
        assert_eq!(text_of(&runtime, "stat-4-text").as_deref(), Some("Quality (Life and Mana Modifiers): 20%"));
    }

    /// The Mods toggle shows `SourceInfo.vue` under a row whose sources deserve it.
    #[test]
    fn the_mods_toggle_shows_a_rows_sources() {
        let mut runtime = runtime("adv-rare-ring-mirrored-catalyst");
        assert!(runtime.find("source-info-mod").is_none());
        click(&mut runtime, "show-sources");
        assert!(runtime.state.price_check.show_sources);
        assert!(runtime.find("source-info-mod").is_some());
    }

    /// `FilterGroup.vue`: the chevron collapses the group and its label's `smartToggle`
    /// closes an open, enabled group.
    #[test]
    fn a_group_expands_and_collapses() {
        let mut runtime = runtime("adv-unique-timeless-jewel");
        // The group sits at index 1 (a hidden `explicit` row comes first).
        assert!(runtime.find("stat-1-0").is_some());
        click(&mut runtime, "stat-1m-expand");
        assert!(!group_expanded(&runtime, 1));
        assert!(runtime.find("stat-1-0").is_none());
        click(&mut runtime, "stat-1m-expand");
        assert!(group_expanded(&runtime, 1));

        // Its label is enabled and open, so the first press only closes it.
        click(&mut runtime, "stat-1m-label");
        assert!(!group_expanded(&runtime, 1));
        assert!(!group_disabled(&runtime, 1));
        // Pressing it again turns the group off; a third press turns it back on and opens it.
        click(&mut runtime, "stat-1m-label");
        assert!(group_disabled(&runtime, 1));
        assert!(!group_expanded(&runtime, 1));
        click(&mut runtime, "stat-1m-label");
        assert!(!group_disabled(&runtime, 1));
        assert!(group_expanded(&runtime, 1));
    }

    fn group_expanded(runtime: &Runtime<App>, index: usize) -> bool {
        match &runtime.state.price_check.item_stats()[index] {
            wraex_poe1::filters::FilterOrGroup::Group(group) => group.expanded,
            wraex_poe1::filters::FilterOrGroup::Stat(_) => panic!("a group"),
        }
    }

    fn group_disabled(runtime: &Runtime<App>, index: usize) -> bool {
        match &runtime.state.price_check.item_stats()[index] {
            wraex_poe1::filters::FilterOrGroup::Group(group) => group.meta.disabled,
            wraex_poe1::filters::FilterOrGroup::Stat(_) => panic!("a group"),
        }
    }

    /// The extra scenes render their filter kinds: a magic flask's explicit rows, a cluster
    /// jewel's enchant rows and two presets, and an amulet's pseudo rows.
    #[test]
    fn the_extra_scenes_render_their_filter_rows() {
        let flask = runtime("adv-magic-flask-charge-recovery");
        assert_eq!(text_of(&flask, "stat-0-text").as_deref(), Some("51% increased Charge Recovery"));
        assert_eq!(text_of(&flask, "stat-1-text").as_deref(), Some("#% reduced effect"));

        let jewel = runtime("jewel-cluster-large");
        assert_eq!(text_of(&jewel, "stat-0-text").as_deref(), Some("Adds 8 Passive Skills"));
        assert!(jewel.find("preset-1").is_some());

        let amulet = runtime("adv-rare-amulet-anointed");
        assert_eq!(text_of(&amulet, "stat-0-text").as_deref(), Some("77% total Elemental Resistance"));
        assert!(amulet.find("show-hidden").is_some());
    }
}
