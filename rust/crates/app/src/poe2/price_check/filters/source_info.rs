//! `SourceInfo.vue`: the modifier a stat comes from, shown while the block's Mods toggle is
//! on.

use rune::{Color, El, col, el, row, text};
use wraex_core::filters::round_roll;
use wraex_core::js;
use wraex_poe2::filters::StatFilter;
use wraex_poe2::parser::advanced_mod_desc::apply_incr;
use wraex_poe2::parser::modifiers::StatSource;

use crate::poe2::i18n;
use crate::theme::{self, text as type_};
use crate::ui;

/// `SourceInfo.vue`.
pub fn view(filter: &StatFilter, source: &StatSource) -> El {
    let info = &source.modifier.info;
    let mut mod_text = i18n::t(&format!("item.mod_{}", info.kind.as_str()));
    if let Some(name) = &info.name {
        mod_text.push_str(" \"");
        mod_text.push_str(name);
        mod_text.push('"');
    }
    if info.tier.is_some() || info.rank.is_some() {
        mod_text.push_str(" (");
        mod_text.push_str(&i18n::fmt("item.mod_tier", &[&tier_text(info.tier)]));
        mod_text.push(')');
    }
    if let Some(rank) = info.rank {
        mod_text.push_str(" (");
        mod_text.push_str(&i18n::fmt("item.mod_rank", &[&js::number_to_string(rank)]));
        mod_text.push(')');
    }

    let (dp, is_negated) = filter.roll.as_ref().map_or((false, false), |roll| (roll.dp, roll.is_negated));
    let contribution = source.contributes.map(|contributes| {
        let value = if is_negated { -contributes.value } else { contributes.value };
        round_roll(value, dp)
    });
    let contributing_ref = &source.stat.stat.stat_ref;
    let mut out =
        col().w_full().child(text(mod_text).key("source-info-mod").font(type_::SM.italic()).color(theme::gray(500)));
    for (index, parsed) in source.modifier.stats.iter().enumerate() {
        let key = format!("source-info-stat-{index}");
        if &parsed.stat.stat_ref != contributing_ref {
            out = out.child(row().w_full().items_center().gap(theme::rem(0.5)).child(
                text(parsed.translation.string.clone()).key(key).font(type_::SM).color(theme::gray(500)).strike(
                    0,
                    parsed.translation.string.len(),
                    None,
                ),
            ));
            continue;
        }
        let applied = apply_incr(info, parsed);
        let parsed = applied.as_ref().unwrap_or(parsed);
        let roll = parsed.roll.as_ref().map(|roll_value| {
            let value = if parsed.translation.negate == Some(true) { -roll_value.value } else { roll_value.value };
            round_roll(value, roll_value.dp)
        });
        let mut line = row().w_full().items_center().gap(theme::rem(0.5)).child(ui::item_modifier_text(
            &key,
            &parsed.translation.string,
            roll,
            type_::SM,
        ));
        if let Some(contribution) = contribution {
            line = line.child(el().grow(1.0)).child(
                row()
                    .w(theme::rem(3.0))
                    .center_y()
                    .justify_center()
                    .radius(theme::ROUNDED)
                    .border(1.0, theme::gray(700))
                    .border_layout()
                    .child(
                        text(js::number_to_string(contribution))
                            .key(format!("{key}-contribution"))
                            .font(type_::SM)
                            .color(Color::WHITE),
                    ),
            );
        }
        out = out.child(line);
    }
    out
}

/// `t('item.mod_tier', [info.tier])` with JavaScript's `String(number)`.
fn tier_text(tier: Option<f64>) -> String {
    // `undefined` in the template's `[info.tier]`.
    tier.map_or_else(|| "undefined".to_owned(), js::number_to_string)
}
