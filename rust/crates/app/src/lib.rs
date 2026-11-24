//! The Wraex desktop app.
//!
//! A Rune shell over the core crates. So far it has one window, PoE1's price check
//! ([`poe1::price_check`]); the title bar and the widgets it is built from live in [`ui`] and
//! [`theme`], shared with the PoE2 windows that come later.
//!
//! `cargo run -p wraex-app` opens the window; `-- --snap out.png --scene <slug>` renders one of
//! the corpus cases headlessly (`--list-scenes` lists them).

pub mod poe1;
pub mod replay;
pub mod theme;
pub mod trade;
pub mod ui;

use rune::{AppSpec, Cx, El};

use crate::poe1::price_check::{PriceCheck, WIDTH};
use crate::trade::Mode;

/// The window's height, in points. Upstream's overlay spans the game's panel; the screenshots
/// in `tools/golden/screens` are 900 tall, so the port's window is too.
pub const HEIGHT: f32 = 900.0;

/// The app's state. One window so far; each new widget adds a field here.
#[derive(Debug)]
pub struct App {
    pub price_check: PriceCheck,
    /// Where trade requests go: HTTPS in a window, recorded answers headless.
    mode: Mode,
}

impl App {
    /// The app with its price check opened on `text` (the game's copy key produced it).
    pub fn from_text(text: &str) -> Self {
        Self::with_mode(text, Mode::for_run())
    }

    /// [`App::from_text`] with the trade transport chosen.
    pub fn with_mode(text: &str, mode: Mode) -> Self {
        Self { price_check: PriceCheck::with_mode(text, mode), mode }
    }

    /// A corpus case answered from the recorded screenshots, for tests.
    pub fn replay(slug: &str) -> Self {
        Self::with_mode(&corpus(slug), Mode::Replay)
    }

    /// Reopen the price check on a corpus case: `<slug>.txt`.
    pub fn load(&mut self, slug: &str) {
        self.price_check = PriceCheck::with_mode(&corpus(slug), self.mode);
    }

    /// Reopen the price check and press Search, as the screenshot harness does for the
    /// cases whose reference window does not search by itself (a rare, a map).
    pub fn load_and_search(&mut self, slug: &str) {
        self.load(slug);
        self.price_check.start_search();
    }
}

impl Default for App {
    fn default() -> Self {
        Self::with_mode(&corpus(SCENES[0].0), Mode::for_run())
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
    // `tools/golden/screens/cases.mjs` marks these two `search: true`.
    ("adv-rare-ring-mirrored-catalyst", |app| app.load_and_search("adv-rare-ring-mirrored-catalyst")),
    ("adv-unique-timeless-jewel", |app| app.load("adv-unique-timeless-jewel")),
    ("real-map-rare-t16-max-res", |app| app.load_and_search("real-map-rare-t16-max-res")),
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
        // A live search's answer lands from a worker thread; the waker runs `poll`, which
        // applies it and redraws. Replay answers land in the handler, so `poll` is idle.
        setup: Some(|app, waker| app.price_check.set_waker(waker)),
        poll: Some(|app, _t, _tx| app.price_check.poll()),
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
        let app = App::replay(slug);
        let text = TextSystem::new(theme::FONTS, theme::FAMILIES);
        let mut runtime = Runtime::with_text(app, view, (WIDTH, HEIGHT), Env::new(true, 1.0), text);
        // `AppSpec::poll` in a window; the tests drive frames themselves.
        runtime.poll = Some(|app: &mut App, _t: f64, _tx: &mut rune::Tx| app.price_check.poll());
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
        assert!(App::replay(SCENES[0].0).price_check.item.is_ok());
        for (slug, _) in SCENES {
            let runtime = runtime(slug);
            assert_eq!(text_of(&runtime, "titlebar-title").as_deref(), Some("Standard"), "{slug}");
            assert!(runtime.find("titlebar-close").is_some(), "{slug}: close button");
            assert!(!runtime.elements().is_empty());
        }
    }

    /// A stackable currency: it searches straight away (`filters.preset_bulk`), so the
    /// exchange table stands in for the Search row.
    #[test]
    fn the_divine_orb_shows_its_exchange_table() {
        let runtime = runtime("currency-divine-orb");
        assert_eq!(text_of(&runtime, "filter-name-text").as_deref(), Some("Divine Orb"));
        // The item's `?` price row, for everything the trade site has no price yet.
        assert_eq!(text_of(&runtime, "trend-price").as_deref(), Some("?"));
        assert!(runtime.find("search-label").is_none(), "no Search button once `doSearch`");
        // `Matched:` counts the offers per currency: chaos was asked for, divine was not.
        assert_eq!(text_of(&runtime, "trade-matched-label").as_deref(), Some("Matched:"));
        assert_eq!(text_of(&runtime, "bulk-currency-chaos-total").as_deref(), Some("9"));
        assert_eq!(text_of(&runtime, "bulk-currency-divine-total").as_deref(), Some("?"));
        // The recorded exchange answer's first and last offers, cheapest first.
        assert_eq!(text_of(&runtime, "bulk-0-price").as_deref(), Some("800"));
        assert_eq!(text_of(&runtime, "bulk-0-exchange-left").as_deref(), Some("800"));
        assert_eq!(text_of(&runtime, "bulk-0-date").as_deref(), Some("2 hr. ago"));
        assert_eq!(text_of(&runtime, "bulk-8-price").as_deref(), Some("1800"));
        assert!(runtime.find("bulk-9-price").is_none(), "nine offers, the rest of the table is empty");
    }

    /// A rare searches by category and waits for the button; pressing it lands the
    /// recorded answer's twenty rows.
    #[test]
    fn a_rare_ring_labels_its_category_and_offers_search() {
        let mut runtime = runtime("adv-rare-ring-mirrored-catalyst");
        assert_eq!(text_of(&runtime, "filter-name-text").as_deref(), Some("Category: Ring"));
        assert_eq!(text_of(&runtime, "corrupted-label").as_deref(), Some("Not Corrupted"));
        assert_eq!(text_of(&runtime, "search-label").as_deref(), Some("Search"));
        assert!(runtime.find("trade-table").is_none());

        click(&mut runtime, "search");
        assert!(runtime.state.price_check.do_search);
        assert!(runtime.state.price_check.trade.error.is_none(), "{:?}", runtime.state.price_check.trade.error);
        assert!(runtime.find("trade-table").is_some());
        assert_eq!(text_of(&runtime, "trade-matched-total").as_deref(), Some("10000"));
        assert_eq!(text_of(&runtime, "listing-0-price").as_deref(), Some("1 chaos"));
        assert_eq!(text_of(&runtime, "listing-0-ilvl").as_deref(), Some("84"));
        assert_eq!(text_of(&runtime, "listing-0-date").as_deref(), Some("2 days ago"));
        assert_eq!(text_of(&runtime, "listing-19-price").as_deref(), Some("3 chaos"));
        assert!(runtime.find("listing-20-price").is_none(), "twenty rows, no more");
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

    /// The Mods toggle shows `SourceInfo.vue` as an overlay while a row whose sources
    /// deserve it is hovered (`FilterModifier.vue`'s `.mods`).
    #[test]
    fn the_mods_toggle_shows_a_rows_sources_on_hover() {
        let mut runtime = runtime("adv-rare-ring-mirrored-catalyst");
        assert!(runtime.find("source-info-mod").is_none());
        click(&mut runtime, "show-sources");
        assert!(runtime.state.price_check.show_sources);
        assert!(runtime.find("source-info-mod").is_none(), "nothing while the pointer is away");

        let (x, y) = runtime.find("stat-0").expect("the first row");
        runtime.pointer_move(x, y);
        runtime.frame(1.0 / 60.0);
        assert_eq!(runtime.state.price_check.source_hover, Some(crate::poe1::price_check::StatPath::Top(0)));
        assert!(runtime.find("stat-0-sources").is_some(), "the overlay under the hovered row");
        assert!(runtime.find("source-info-mod").is_some());

        runtime.pointer_move(2.0, 2.0);
        runtime.frame(1.0 / 60.0);
        assert!(runtime.state.price_check.source_hover.is_none());
        assert!(runtime.find("source-info-mod").is_none());
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

    /// `FiltersBlock.vue` passes `statsVisibility.disabled` as the stats chip's `collapse`,
    /// and `FilterBtnLogical.vue` only draws an icon when it is truthy.
    #[test]
    fn the_stats_chip_draws_its_chevron_only_when_collapsed() {
        let mut runtime = runtime("real-map-rare-t16-max-res");
        assert!(runtime.find("btn-stats-chevron").is_none());
        click(&mut runtime, "btn-stats");
        assert!(runtime.state.price_check.stats_collapsed);
        assert!(runtime.find("btn-stats-chevron").is_some());
        // The Collapse button is inside the block the chip just hid; the chip brings it back.
        click(&mut runtime, "btn-stats");
        assert!(!runtime.state.price_check.stats_collapsed);
        assert!(runtime.find("btn-stats-chevron").is_none());
    }

    /// `{{ t('filters.collapse') }} <i>`: a space, then the chevron.
    #[test]
    fn collapse_keeps_a_space_before_its_chevron() {
        let runtime = runtime("real-map-rare-t16-max-res");
        assert_eq!(text_of(&runtime, "filters-collapse-label").as_deref(), Some("Collapse "));
    }

    /// `FilterBtnNumeric.vue`'s `.input` is transparent: the chip draws the background and
    /// border, so a value like `Map Tier: 16` is plain text, not a box of its own.
    #[test]
    fn a_numeric_chip_draws_its_value_without_a_box() {
        let runtime = runtime("real-map-rare-t16-max-res");
        let elements = runtime.elements();
        let field = element(&elements, "btn-map_tier-min").expect("the map tier field");
        assert_eq!(field.text.as_deref(), Some("16"));
        assert_eq!(field.fill, rune::Color::TRANSPARENT, "the chip's background shows through");
        assert!((field.rect[3] - 20.0).abs() < 0.5, "inside the chip's 22 pt box: {:?}", field.rect);
        // A stat row's roll keeps `FilterModifier.vue`'s gray-900 box.
        let roll = element(&elements, "stat-0-max").expect("the first row's max field");
        assert_eq!(roll.fill, theme::gray(900));
    }

    /// `FilterBtnNumeric.vue`'s `inputMinBlur`: an emptied min field shows 0 and turns its
    /// filter off when the caret goes elsewhere.
    #[test]
    fn an_empty_min_field_shows_zero_and_turns_off_on_blur() {
        use crate::poe1::price_check::NumericFilter;
        let mut runtime = runtime("real-map-rare-t16-max-res");
        click(&mut runtime, "btn-map_tier-min");
        // `poll` records the focused field one frame after the click.
        runtime.frame(1.0 / 60.0);
        runtime.key(rune::KeyEvent { key: rune::Key::Command('a'), mods: rune::Mods::default() });
        runtime.frame(1.0 / 60.0);
        runtime.key(rune::KeyEvent { key: rune::Key::Backspace, mods: rune::Mods::default() });
        runtime.frame(1.0 / 60.0);
        assert_eq!(text_of(&runtime, "btn-map_tier-min").as_deref(), Some(""));

        click(&mut runtime, "stat-0-max");
        runtime.frame(1.0 / 60.0);
        assert_eq!(text_of(&runtime, "btn-map_tier-min").as_deref(), Some("0"));
        assert!(runtime.state.price_check.numeric(NumericFilter::MapTier).expect("the chip").disabled);
    }

    /// `OnlineFilter.vue`'s label opens `OnlineFilterCore.vue` under it.
    #[test]
    fn the_online_filter_opens_its_panel() {
        let mut runtime = runtime("adv-rare-ring-mirrored-catalyst");
        click(&mut runtime, "search");
        assert!(runtime.find("online-filter-panel").is_none());
        assert_eq!(text_of(&runtime, "online-filter-label").as_deref(), Some("Merchant Only"));

        click(&mut runtime, "online-filter");
        assert!(runtime.state.price_check.online_open);
        let label = runtime.find("online-filter").expect("the label");
        let panel = runtime.find("online-filter-panel").expect("the panel");
        assert!(panel.1 > label.1, "under the label: {panel:?} vs {label:?}");
        assert_eq!(text_of(&runtime, "online-filter-merchant-label").as_deref(), Some("Merchant Only"));
        assert_eq!(text_of(&runtime, "online-filter-listed-1day-label").as_deref(), Some("1 Day Ago"));

        click(&mut runtime, "online-filter");
        assert!(runtime.find("online-filter-panel").is_none());
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
