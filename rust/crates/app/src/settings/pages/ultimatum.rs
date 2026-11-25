//! The Ultimatum page: every modifier the player has a decision for, its four tiers, and
//! Remove. There is no static list of Ultimatum modifiers, so a row exists only once the item
//! check (or this page) created one, and each chip sets that one tier for the current profile.

use rune::{El, col, row, text};
use wraex_poe1::map_check;
use wraex_poe1::ultimatum_check::{self, UltimatumCheckConfig, UltimatumModDecision};

use crate::settings::pages::{page, tr};
use crate::settings::{Field, Project, Settings};
use crate::theme::{self, text as type_};
use crate::ui;

/// The four tier numerals, `I`..`IV`.
const TIERS: [&str; 4] = ["I", "II", "III", "IV"];

/// The `maps.profile` both checks decide for.
fn profile(settings: &Settings) -> u32 {
    Field::Maps("profile").read(&settings.config).as_u64().unwrap_or(1) as u32
}

/// The item check widget's `ultimatum`, or the defaults when it has none yet.
fn config(settings: &Settings) -> UltimatumCheckConfig {
    crate::settings::widget(&settings.config, "item-check")
        .and_then(|widget| widget.get("ultimatum"))
        .and_then(|ultimatum| serde_json::from_value(ultimatum.clone()).ok())
        .unwrap_or_default()
}

/// Write the working copy's `ultimatum`, creating the key an old config lacks.
fn store(settings: &mut Settings, config: &UltimatumCheckConfig) {
    let value = serde_json::to_value(config).expect("the Ultimatum check is JSON");
    if let Some(widget) = crate::settings::widget_mut(&mut settings.config, "item-check")
        && let Some(object) = widget.as_object_mut()
    {
        object.insert("ultimatum".to_owned(), value);
    }
}

pub fn view<S: 'static>(settings: &Settings, project: Project<S>) -> El {
    let profile = profile(settings);
    let config = config(settings);

    let numeral = TIERS[usize::try_from(profile.saturating_sub(1)).unwrap_or(0).min(3)];
    let toolbar = row()
        .key("ultimatum-toolbar")
        .w_full()
        .padding(theme::rem(0.25))
        .gap(theme::rem(1.0))
        .items_center()
        .child(
            text(tr(settings, "ultimatum.settings_hint"))
                .key("ultimatum-hint")
                .font(type_::SM)
                .color(theme::gray(500))
                .grow(1.0)
                .min_w(0.0)
                .wrap(),
        )
        .child(
            text(format!("{} {numeral}", tr(settings, "map_check.profile")))
                .key("ultimatum-profile")
                .font(type_::SM)
                .color(theme::gray(400)),
        );

    let mut root = col().key("page-ultimatum").w_full().child(toolbar);
    if config.selected_mods.is_empty() {
        root = root.child(
            row().key("ultimatum-empty").w_full().px(theme::rem(0.5)).py(theme::rem(1.0)).child(
                text(tr(settings, "ultimatum.settings_empty"))
                    .key("ultimatum-empty-text")
                    .font(type_::SM)
                    .color(theme::gray(500))
                    .wrap(),
            ),
        );
    } else {
        let mut list = col().key("ultimatum-list").w_full();
        for (index, modifier) in config.selected_mods.iter().enumerate() {
            list = list.child(entry(settings, modifier, index, profile, project));
        }
        root = root.child(list);
    }
    page("page-ultimatum").child(root)
}

/// One modifier: its name, the four tier chips and Remove.
fn entry<S: 'static>(
    settings: &Settings,
    modifier: &UltimatumModDecision,
    index: usize,
    profile: u32,
    project: Project<S>,
) -> El {
    let mut chips = row().key(format!("ultimatum-{index}-tiers")).gap(theme::rem(0.25)).items_center();
    for tier in 0..TIERS.len() {
        chips = chips.child(chip(&modifier.name, tier, &modifier.tiers[tier], profile, index, project));
    }

    let name = modifier.name.clone();
    let remove = row()
        .key(format!("ultimatum-{index}-remove"))
        .mx(theme::rem(0.25))
        .py(theme::rem(0.25))
        .focusable(true)
        .role("button")
        .label(tr(settings, "Remove"))
        .on_press(move |s: &mut S| {
            let mut config = config(project(s));
            ultimatum_check::remove(&mut config, &name);
            store(project(s), &config);
        })
        .child(ui::icon::times(type_::SM.size, theme::gray(400)));

    row()
        .key(format!("ultimatum-{index}"))
        .w_full()
        .h(theme::rem(2.0))
        .items_center()
        .hover(theme::gray(700))
        .child(
            text(modifier.name.clone())
                .key(format!("ultimatum-{index}-name"))
                .font(theme::leading_none(type_::SM))
                .grow(1.0)
                .min_w(0.0)
                .px(theme::rem(0.5))
                .truncate(),
        )
        .child(chips)
        .child(remove)
}

/// One tier's chip: its numeral on the decision's colour, cycling that tier alone.
fn chip<S: 'static>(name: &str, tier: usize, set: &str, profile: u32, index: usize, project: Project<S>) -> El {
    let at = set.chars().nth(profile.saturating_sub(1) as usize).unwrap_or('-');
    let fill = match at {
        'd' => theme::red(700),
        'w' => theme::orange(600),
        'g' => theme::green(700),
        's' => theme::gray(700),
        _ => theme::gray(800),
    };
    let numeral = TIERS[tier];
    let key = format!("ultimatum-{index}-tier-{}", tier + 1);
    let name = name.to_owned();
    let mut chip = row()
        .key(key.clone())
        .w(theme::rem(1.5))
        .h(type_::SM.line_px())
        .justify_center()
        .radius(theme::ROUNDED)
        .fill(fill)
        .focusable(true)
        .role("button")
        .label(format!("{numeral} {}", decision_label(at)))
        .on_press(move |s: &mut S| {
            let mut config = config(project(s));
            let next = map_check::next_decision(&at.to_string());
            ultimatum_check::set_tier_decision(&mut config, profile, &name, tier as u8 + 1, next);
            store(project(s), &config);
        })
        .child(text(numeral).key(format!("{key}-text")).font(type_::XS).color(theme::gray(200)));
    if at == '-' {
        chip = chip.border(1.0, theme::gray(700)).border_layout();
    }
    chip
}

/// The chip's own words, for the accessibility label only.
fn decision_label(at: char) -> &'static str {
    match at {
        'd' => "deadly",
        'w' => "warning",
        'g' => "good",
        's' => "seen",
        _ => "undecided",
    }
}
