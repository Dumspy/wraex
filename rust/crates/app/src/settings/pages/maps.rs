//! `web/map-check/settings-maps.vue` and `SettingsMatcherEntry.vue`: every area modifier with
//! its per-profile decision (warning, deadly, good), and the profile's own list first.

use rune::{El, col, el, row, text};
use serde_json::Value;
use wraex_core::GameId;

use crate::settings::pages::{tr, tr_fmt};
use crate::settings::{Field, Project, Settings, maps_mut};
use crate::theme::{self, text as type_};
use crate::trade::Mode;
use crate::ui;
use crate::ui::field::Kind;

/// `StatTag`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tag {
    None,
    Outdated,
    HeistExclusive,
    UberMapExclusive,
}

impl Tag {
    /// `tagToShowOrder`: plain stats, then the uber-only ones, then the heist-only ones.
    fn order(self) -> u8 {
        match self {
            Tag::None => 0,
            Tag::UberMapExclusive => 1,
            Tag::HeistExclusive => 2,
            Tag::Outdated => 3,
        }
    }

    /// `t('map.mods.…')`: a literal key with dots, which the i18n lookup can't reach as a path.
    fn label(self, game: GameId) -> &'static str {
        match self {
            Tag::Outdated => "outdated",
            Tag::HeistExclusive => "heist",
            Tag::UberMapExclusive if game == GameId::Poe2 => "T17",
            Tag::UberMapExclusive => "T16+",
            Tag::None => "",
        }
    }
}

/// A `SettingsMatcherEntry` row's stat (`StatMatcher` in `map-check/common.ts`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AreaMod {
    pub matcher: String,
    pub tag: Tag,
}

/// `findAllAreaMods()`: every stat the game marks as an area mod, one row per matcher text.
pub fn area_mods(game: GameId, mode: Mode) -> Vec<AreaMod> {
    let mut out = Vec::new();
    match game {
        GameId::Poe2 => {
            for stat in crate::poe2::price_check::data(mode).stats_iter("AreaMods", &[]) {
                let heist = stat.from_heist_area_mods == Some(true) && stat.from_area_mods != Some(true);
                let uber = stat.from_uber_area_mods == Some(true) && stat.from_area_mods != Some(true);
                let tag = if heist {
                    Tag::HeistExclusive
                } else if uber {
                    Tag::UberMapExclusive
                } else {
                    Tag::None
                };
                for matcher in &stat.matchers {
                    out.push(AreaMod { matcher: matcher.string.clone(), tag });
                }
            }
        }
        _ => {
            for stat in crate::poe1::price_check::data().stats_iter("AreaMods", &[]) {
                let tag = match stat.from_area_mods.as_deref() {
                    Some("heist_exclusive") => Tag::HeistExclusive,
                    Some("ubermap_exclusive") => Tag::UberMapExclusive,
                    Some(_) => Tag::None,
                    None => continue,
                };
                for matcher in &stat.matchers {
                    out.push(AreaMod { matcher: matcher.string.clone(), tag });
                }
            }
        }
    }
    out.sort_by(|a, b| a.tag.order().cmp(&b.tag.order()).then_with(|| a.matcher.cmp(&b.matcher)));
    out
}

/// `isOutdated`: a decided stat whose wording no longer matches the game data.
pub fn is_outdated(game: GameId, mode: Mode, matcher: &str) -> bool {
    match game {
        GameId::Poe2 => crate::poe2::price_check::data(mode).stat_by_match_str(matcher).is_none(),
        _ => crate::poe1::price_check::data().stat_by_match_str_v2(matcher).is_none(),
    }
}

/// The `maps` object's `profile`.
fn profile(settings: &Settings) -> u32 {
    Field::Maps("profile").read(&settings.config).as_u64().unwrap_or(1) as u32
}

/// `decisionSet[profile - 1]`, or `-` for no entry.
fn decision(settings: &Settings, matcher: &str) -> (String, String) {
    let profile = profile(settings);
    let entry = settings
        .config
        .get("widgets")
        .and_then(|widgets| widgets.as_array())
        .and_then(|widgets| widgets.iter().find(|w| w["wmType"] == "item-check"))
        .and_then(|widget| widget.get("maps"))
        .and_then(|maps| maps.get("selectedStats"))
        .and_then(|stats| stats.as_array())
        .and_then(|stats| stats.iter().find(|stat| stat["matcher"] == matcher));
    match entry {
        Some(entry) => {
            let set = entry["decision"].as_str().unwrap_or_default();
            let at = set.chars().nth(profile.saturating_sub(1) as usize).unwrap_or('-');
            (set.to_owned(), at.to_string())
        }
        None => (String::new(), "-".to_owned()),
    }
}

/// `decision`'s setter with `decisionCreate`, pushing a new entry when there is none.
fn set_decision(settings: &mut Settings, matcher: &str, value: &str) {
    let profile = profile(settings);
    let (set, _) = decision(settings, matcher);
    let next = set_decided(&set, profile, value);
    let Some(maps) = maps_mut(&mut settings.config) else { return };
    let Some(stats) = maps.get_mut("selectedStats").and_then(Value::as_array_mut) else { return };
    match stats.iter_mut().find(|stat| stat["matcher"] == matcher) {
        Some(entry) => entry["decision"] = Value::String(next),
        None => stats.push(serde_json::json!({ "matcher": matcher, "decision": next })),
    }
}

/// `decisionCreate(value, profile, updateSet)`.
fn set_decided(set: &str, profile: u32, value: &str) -> String {
    wraex_poe1::map_check::decision_create(value, profile, set)
}

/// `decisionHasColor`: a decided, not merely seen, profile.
fn decision_has_color(set: &str, profile: u32) -> bool {
    let at = set.chars().nth(profile.saturating_sub(1) as usize);
    at != Some('-') && at != Some('s') && at.is_some()
}

/// The list's search box: `input placeholder="Search..."` over the stats' text. Its text is
/// view state (`search` in `settings-maps.vue`), not a config field.
fn search_field<S: 'static>(settings: &Settings, project: Project<S>) -> El {
    let Some(edit) = settings.edit("maps-search") else {
        return el().key("maps-search").h(theme::rem(1.25));
    };
    let edit_key = "maps-search".to_owned();
    let change_key = "maps-search".to_owned();
    ui::field::text_field(
        edit,
        "maps-search",
        Kind::Grow,
        None,
        &format!("{}...", tr(settings, "Search")),
        move |s: &mut S| project(s).edit_mut(&edit_key),
        move |s: &mut S, _tx: &mut rune::Tx| {
            let text = project(s).edit_text(&change_key);
            project(s).maps_search = text;
        },
    )
}

/// The profile's own decided stats: outdated ones first, then everything the area-mod list
/// has, filtered by the search and the Only-selected toggle.
fn row_entries(settings: &Settings) -> Vec<AreaMod> {
    let profile = profile(settings);
    let only_selected = settings.maps_only_selected;
    let search = settings.maps_search.to_lowercase();
    let queries: Vec<&str> = search.split(' ').filter(|part| !part.is_empty()).collect();

    let mut rows: Vec<AreaMod> = settings
        .config
        .get("widgets")
        .and_then(|widgets| widgets.as_array())
        .and_then(|widgets| widgets.iter().find(|w| w["wmType"] == "item-check"))
        .and_then(|widget| widget.get("maps"))
        .and_then(|maps| maps.get("selectedStats"))
        .and_then(|stats| stats.as_array())
        .map(|stats| {
            stats
                .iter()
                .filter_map(|stat| Some((stat["matcher"].as_str()?, stat["decision"].as_str().unwrap_or_default())))
                .filter(|(matcher, set)| {
                    decision_has_color(set, profile) && is_outdated(settings.game, settings.mode, matcher)
                })
                .map(|(matcher, _)| AreaMod { matcher: matcher.to_owned(), tag: Tag::Outdated })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let selected: Vec<String> = settings
        .config
        .get("widgets")
        .and_then(|widgets| widgets.as_array())
        .and_then(|widgets| widgets.iter().find(|w| w["wmType"] == "item-check"))
        .and_then(|widget| widget.get("maps"))
        .and_then(|maps| maps.get("selectedStats"))
        .and_then(|stats| stats.as_array())
        .map(|stats| {
            stats
                .iter()
                .filter(|stat| decision_has_color(stat["decision"].as_str().unwrap_or_default(), profile))
                .filter_map(|stat| stat["matcher"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();

    for area in settings.area_mods.iter().flatten() {
        let text = area.matcher.to_lowercase();
        if !queries.iter().all(|part| text.contains(part)) {
            continue;
        }
        if only_selected && !selected.contains(&area.matcher) {
            continue;
        }
        if rows.iter().any(|row| row.matcher == area.matcher) {
            continue;
        }
        rows.push(area.clone());
    }
    rows
}

pub fn view<S: 'static>(settings: &Settings, project: Project<S>) -> El {
    let profile = profile(settings);
    let only_selected = settings.maps_only_selected;
    let rows = row_entries(settings);
    let found = rows.len();

    let mut root = col().key("page-maps").w_full().h_full().min_h(0.0);
    let mut toolbar = row().key("maps-toolbar").w_full().padding(theme::rem(0.25)).items_center().flex_wrap();
    toolbar = toolbar.child(row().key("maps-search-box").w(theme::rem(12.0)).child(search_field(settings, project)));
    toolbar = toolbar.child(row().key("maps-only-selected").mx(theme::rem(0.5)).child(ui::toggle(
        "maps-only-selected-toggle",
        &tr(settings, "map_check.search_selected"),
        only_selected,
        move |s: &mut S| {
            let next = !project(s).maps_only_selected;
            project(s).maps_only_selected = next;
        },
    )));
    toolbar = toolbar.child(row().key("maps-show-new").ml(theme::rem(1.0)).child(ui::toggle(
        "maps-show-new-toggle",
        &tr(settings, "map_check.new_mods_icon"),
        settings.bool(Field::Maps("showNewStats")),
        move |s: &mut S| {
            let on = project(s).bool(Field::Maps("showNewStats"));
            project(s).set_bool(Field::Maps("showNewStats"), !on);
        },
    )));
    root = root.child(toolbar);

    // The list's header: the count, then the three decision colours.
    let mut header = row().key("maps-header").w_full().py(theme::rem(0.25)).items_center();
    header = header.child(
        text(tr_fmt(settings, "map_check.search_stat_col", &[&found.to_string()]))
            .key("maps-stat-col")
            .font(theme::leading_none(type_::SM))
            .grow(1.0)
            .min_w(0.0)
            .pl(theme::rem(0.5))
            .truncate(),
    );
    header = header.child(
        row()
            .key("maps-legend")
            .gap(theme::rem(0.25))
            .items_center()
            .pr(theme::rem(1.0))
            .child(legend("maps-legend-warn", theme::orange(600)))
            .child(legend("maps-legend-deadly", theme::red(700)))
            .child(legend("maps-legend-good", theme::green(700))),
    );
    root = root.child(header);

    let mut list =
        col().key("maps-list").w_full().grow(1.0).min_h(0.0).scroll(rune::ScrollSpec { stick_bottom: false });
    for (index, row_) in rows.iter().enumerate() {
        list = list.child(entry::<S>(settings, row_, index, profile, project));
    }
    root.child(list)
}

/// One coloured legend square, 32 pt wide (`w-8 py-1`).
fn legend(key: &str, color: rune::Color) -> El {
    row()
        .key(key.to_owned())
        .w(theme::rem(2.0))
        .py(theme::rem(0.25))
        .fill(color)
        .justify_center()
        .child(ui::icon::warning(12.0, rune::Color::WHITE))
}

/// `SettingsMatcherEntry.vue`: the tag, the stat text, the three decision radios and Remove.
fn entry<S: 'static>(settings: &Settings, area: &AreaMod, index: usize, profile: u32, project: Project<S>) -> El {
    let (set, at) = decision(settings, &area.matcher);
    let removable = decision_has_color(&set, profile) || area.tag == Tag::Outdated;
    let game = settings.game;
    let matcher = area.matcher.clone();

    let mut label = row().key(format!("maps-{index}-label")).grow(1.0).min_w(0.0).items_center().gap(theme::rem(0.25));
    if area.tag != Tag::None {
        label = label.child(tag_badge(format!("maps-{index}-tag"), area.tag.label(game), area.tag));
    }
    label = label.child(
        text(area.matcher.clone()).key(format!("maps-{index}-matcher")).font(theme::leading_none(type_::SM)).truncate(),
    );

    let mut controls = row().key(format!("maps-{index}-controls")).items_center().gap(theme::rem(1.0));
    for (value, key) in [("w", "warn"), ("d", "deadly"), ("g", "good")] {
        let checked = at == value;
        let matcher = matcher.clone();
        controls = controls.child(ui::radio(&format!("maps-{index}-{key}"), key, checked, false, move |s: &mut S| {
            set_decision(project(s), &matcher, value)
        }));
    }
    if removable {
        let matcher = matcher.clone();
        controls = controls.child(
            row()
                .key(format!("maps-{index}-remove"))
                .mx(theme::rem(0.25))
                .py(theme::rem(0.25))
                .focusable(true)
                .role("button")
                .label(tr(settings, "Remove"))
                .on_press(move |s: &mut S| set_decision(project(s), &matcher, "---"))
                .child(ui::icon::close(
                    12.0,
                    if area.tag == Tag::Outdated { theme::red(400) } else { theme::gray(400) },
                )),
        );
    } else {
        controls = controls.child(el().w(theme::rem(1.5)));
    }

    row()
        .key(format!("maps-{index}"))
        .w_full()
        .h(theme::rem(2.0))
        .px(theme::rem(0.5))
        .items_center()
        .hover(theme::gray(700))
        .child(label)
        .child(row().key(format!("maps-{index}-keys")).items_center().gap(theme::rem(1.0)).child(controls))
}

/// `.tag-heist` (`bg-red-800`), `.tag-uber` (`bg-gray-900` with a gray-700 outline) and
/// `.tag-outdated` (`bg-red-400 text-black`).
fn tag_badge(key: String, label: &str, tag: Tag) -> El {
    let mut badge = row().key(key).px(theme::rem(0.25)).radius(theme::ROUNDED).center_y();
    badge = match tag {
        Tag::HeistExclusive => badge.fill(theme::red(800)).child(text(label.to_owned()).font(type_::XS)),
        Tag::UberMapExclusive => badge
            .fill(theme::gray(900))
            .border(1.0, theme::gray(700))
            .border_layout()
            .child(text(label.to_owned()).font(type_::XS)),
        Tag::Outdated => {
            badge.fill(theme::red(400)).child(text(label.to_owned()).font(type_::XS).color(rune::Color::BLACK))
        }
        Tag::None => badge,
    };
    badge
}
