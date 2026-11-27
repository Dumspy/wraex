//! `web/map-check/settings-maps.vue` and `SettingsMatcherEntry.vue`: every area modifier with
//! its per-profile decision (warning, deadly, good), and the profile's own list first.

use rune::{El, col, el, row, text};
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use wraex_core::GameId;
use wraex_core::js::json_value;

use crate::settings::pages::{tr, tr_fmt};
use crate::settings::{Field, Project, Settings, maps, maps_mut};
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
    const fn order(self) -> u8 {
        match self {
            Self::None => 0,
            Self::UberMapExclusive => 1,
            Self::HeistExclusive => 2,
            Self::Outdated => 3,
        }
    }

    /// `t('map.mods.…')`: a literal key with dots, which the i18n lookup can't reach as a path.
    fn label(self, game: GameId) -> &'static str {
        match self {
            Self::Outdated => "outdated",
            Self::HeistExclusive => "heist",
            Self::UberMapExclusive if game == GameId::Poe2 => "T17",
            Self::UberMapExclusive => "T16+",
            Self::None => "",
        }
    }
}

/// A `SettingsMatcherEntry` row's stat (`StatMatcher` in `map-check/common.ts`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AreaMod {
    pub matcher: String,
    pub tag: Tag,
}

/// The item check's `maps` settings (`ItemCheckWidget.maps`), as this page reads them.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct MapsView {
    #[serde(deserialize_with = "crate::settings::array_or_empty")]
    selected_stats: Vec<StatDecision>,
}

/// One `selectedStats` entry (`MapCheckStat`): a matcher and its per-profile decisions.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct StatDecision {
    matcher: String,
    #[serde(default, deserialize_with = "crate::settings::string_or_empty")]
    decision: String,
}

/// A new `selectedStats` entry, written as the template's `{ matcher, decision }` object.
#[derive(Debug, Serialize)]
struct NewStat<'a> {
    matcher: &'a str,
    decision: &'a str,
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
        GameId::Poe1 => {
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
    // `a.matchStr.localeCompare(b.matchStr)`: case-insensitive, and punctuation-weighted as ICU
    // does, so "# metres…" sorts before "#% chance…". Folding case reproduces the reference's
    // order for these English templates.
    out.sort_by(|a, b| {
        a.tag.order().cmp(&b.tag.order()).then_with(|| a.matcher.to_lowercase().cmp(&b.matcher.to_lowercase()))
    });
    out
}

/// `isOutdated`: a decided stat whose wording no longer matches the game data.
pub fn is_outdated(game: GameId, mode: Mode, matcher: &str) -> bool {
    match game {
        GameId::Poe2 => crate::poe2::price_check::data(mode).stat_by_match_str(matcher).is_none(),
        GameId::Poe1 => crate::poe1::price_check::data().stat_by_match_str_v2(matcher).is_none(),
    }
}

/// The `maps` object's `profile`, `1` when unset.
fn profile(settings: &Settings) -> u32 {
    let profile = match Field::Maps("profile").read(&settings.config) {
        Value::Number(number) => number.as_u64(),
        _ => None,
    };
    // `as u32`: the old cast kept the low 32 bits.
    profile.map_or(1, |profile| u32::try_from(profile & u64::from(u32::MAX)).unwrap_or_default())
}

/// The `maps` settings, or the defaults when the config has no item check.
fn maps_view(settings: &Settings) -> MapsView {
    maps(&settings.config).and_then(|maps| MapsView::deserialize(maps).ok()).unwrap_or_default()
}

/// The profile's decision letter (`set.charAt(profile - 1)`).
fn decision_at(set: &str, profile: u32) -> Option<char> {
    usize::try_from(profile.saturating_sub(1)).ok().and_then(|index| set.chars().nth(index))
}

/// `decisionSet[profile - 1]`, or `-` for no entry.
fn decision(settings: &Settings, matcher: &str) -> (String, String) {
    let profile = profile(settings);
    maps_view(settings).selected_stats.iter().find(|stat| stat.matcher == matcher).map_or_else(
        || (String::new(), "-".to_owned()),
        |entry| {
            let set = entry.decision.as_str();
            let at = decision_at(set, profile).unwrap_or('-');
            (set.to_owned(), at.to_string())
        },
    )
}

/// `maps.selectedStats`, writable.
fn selected_stats_mut(maps: &mut Value) -> Option<&mut Vec<Value>> {
    match maps {
        Value::Object(map) => match map.get_mut("selectedStats") {
            Some(Value::Array(stats)) => Some(stats),
            _ => None,
        },
        _ => None,
    }
}

/// `stat.matcher`, when the entry is an object holding a string.
fn stat_matcher(stat: &Value) -> Option<&str> {
    match stat {
        Value::Object(map) => match map.get("matcher") {
            Some(Value::String(matcher)) => Some(matcher),
            _ => None,
        },
        _ => None,
    }
}

/// `decision`'s setter with `decisionCreate`, pushing a new entry when there is none.
fn set_decision(settings: &mut Settings, matcher: &str, value: &str) {
    let profile = profile(settings);
    let (set, _) = decision(settings, matcher);
    let next = set_decided(&set, profile, value);
    let Some(maps) = maps_mut(&mut settings.config) else { return };
    let Some(stats) = selected_stats_mut(maps) else { return };
    match stats.iter().position(|stat| stat_matcher(stat) == Some(matcher)) {
        Some(at) => {
            if let Some(Value::Object(entry)) = stats.get_mut(at) {
                entry.insert("decision".to_owned(), Value::String(next));
            }
        }
        None => stats.push(json_value(&NewStat { matcher, decision: &next })),
    }
}

/// `decisionCreate(value, profile, updateSet)`.
fn set_decided(set: &str, profile: u32, value: &str) -> String {
    wraex_poe1::map_check::decision_create(value, profile, set)
}

/// `decisionHasColor`: a decided, not merely seen, profile.
fn decision_has_color(set: &str, profile: u32) -> bool {
    decision_at(set, profile).is_some_and(|decision| decision != '-' && decision != 's')
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
        &ui::field::TextFieldProps {
            edit,
            key: "maps-search",
            kind: Kind::Grow,
            width: None,
            placeholder: &format!("{}...", tr(settings, "Search")),
        },
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
    let view = maps_view(settings);

    let mut rows: Vec<AreaMod> = view
        .selected_stats
        .iter()
        .filter(|stat| {
            decision_has_color(&stat.decision, profile) && is_outdated(settings.game, settings.mode, &stat.matcher)
        })
        .map(|stat| AreaMod { matcher: stat.matcher.clone(), tag: Tag::Outdated })
        .collect();

    let selected: Vec<&str> = view
        .selected_stats
        .iter()
        .filter(|stat| decision_has_color(&stat.decision, profile))
        .map(|stat| stat.matcher.as_str())
        .collect();

    for area in settings.area_mods.iter().flatten() {
        let text = area.matcher.to_lowercase();
        if !queries.iter().all(|part| text.contains(part)) {
            continue;
        }
        if only_selected && !selected.contains(&area.matcher.as_str()) {
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
    toolbar = toolbar.child(row().key("maps-search-box").w(theme::rem(8.4375)).child(search_field(settings, project)));
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
            // `padding-right: calc(0.875rem + 2.350rem)`: the radios' 0.875 rem marker plus
            // the two gaps and boxes the rows put to their right.
            .pr(theme::rem(0.875 + 2.35))
            .child(legend("maps-legend-warn", theme::orange(600), ui::icon::warning))
            .child(legend("maps-legend-deadly", theme::red(700), ui::icon::skull))
            .child(legend("maps-legend-good", theme::green(700), ui::icon::check)),
    );
    root = root.child(header);

    let mut list =
        col().key("maps-list").w_full().grow(1.0).min_h(0.0).scroll(rune::ScrollSpec { stick_bottom: false });
    for (index, row_) in rows.iter().enumerate() {
        list = list.child(entry::<S>(settings, row_, index, profile, project));
    }
    root.child(list)
}

/// One coloured legend square, 32 pt wide (`w-8 py-1`): `fa-exclamation-triangle`,
/// `fa-skull-crossbones` and `fa-check` at the page's 14 pt, in its `text-gray-100`.
fn legend(key: &str, color: rune::Color, glyph: fn(f32, rune::Color) -> El) -> El {
    row()
        .key(key.to_owned())
        .w(theme::rem(2.0))
        .py(theme::rem(0.25))
        .fill(color)
        .justify_center()
        .child(glyph(type_::SM.size, theme::gray(100)))
}

/// `SettingsMatcherEntry.vue`: the tag, the stat text, the three decision radios and Remove.
fn entry<S: 'static>(settings: &Settings, area: &AreaMod, index: usize, profile: u32, project: Project<S>) -> El {
    let (set, at) = decision(settings, &area.matcher);
    let removable = decision_has_color(&set, profile) || area.tag == Tag::Outdated;
    let game = settings.game;
    let matcher = area.matcher.clone();

    let mut label = row()
        .key(format!("maps-{index}-label"))
        .grow(1.0)
        .min_w(0.0)
        .px(theme::rem(0.5))
        .items_center()
        .gap(theme::rem(0.25));
    if area.tag != Tag::None {
        label = label.child(tag_badge(format!("maps-{index}-tag"), area.tag.label(game), area.tag));
    }
    label = label.child(
        text(area.matcher.clone()).key(format!("maps-{index}-matcher")).font(theme::leading_none(type_::SM)).truncate(),
    );

    let mut controls = row().key(format!("maps-{index}-controls")).items_center().gap(theme::rem(1.0));
    // `controls-auto-hide`: upstream lays the decision radios out but `.row:not(:hover)` hides
    // them, so an undecided row shows only its text. A screenshot is never hovered.
    if removable {
        for (value, key) in [("w", "warn"), ("d", "deadly"), ("g", "good")] {
            let checked = at == value;
            let matcher = matcher.clone();
            controls =
                controls.child(ui::radio_icon(&format!("maps-{index}-{key}"), key, checked, move |s: &mut S| {
                    set_decision(project(s), &matcher, value);
                }));
        }
        // `fa-trash-alt` for an outdated wording, `fa-times` otherwise (`w-4`, 14 pt glyph).
        let glyph = if area.tag == Tag::Outdated {
            ui::icon::trash(type_::SM.size, theme::red(400))
        } else {
            ui::icon::times(type_::SM.size, theme::gray(400))
        };
        controls = controls.child(
            row()
                .key(format!("maps-{index}-remove"))
                .mx(theme::rem(0.25))
                .py(theme::rem(0.25))
                .focusable(true)
                .role("button")
                .label(tr(settings, "Remove"))
                .on_press(move |s: &mut S| set_decision(project(s), &matcher, "---"))
                .child(glyph),
        );
    } else {
        controls = controls.child(el().w(theme::rem(1.5)));
    }

    row()
        .key(format!("maps-{index}"))
        .w_full()
        .h(theme::rem(2.0))
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
