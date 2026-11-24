//! `UnknownModifier.vue`: a modifier the parser did not recognize (`item.unknownModifiers`).

use rune::{El, col, row, text};
use wraex_poe1::parser::item::UnknownModifier;

use crate::poe1::i18n;
use crate::theme::{self, text as type_};
use crate::ui::{self, icon};

/// `UnknownModifier.vue`.
pub fn view(key: &str, stat: &UnknownModifier) -> El {
    col()
        .key(key)
        .w_full()
        .py(theme::rem(0.5))
        .border_sides([0.0, 0.0, 1.0, 0.0], theme::gray(700))
        .border_layout()
        .child(
            row()
                .w_full()
                .pb(theme::rem(0.25))
                .items_center()
                .child(row().w(theme::rem(1.25)).child(icon::warning(theme::rem(0.875), theme::orange(400))))
                .child(ui::item_modifier_text(&format!("{key}-text"), &stat.text, None, type_::SM).truncate()),
        )
        .child(
            row()
                .key(format!("{key}-reason"))
                .pl(theme::rem(1.25))
                .items_center()
                .child(
                    text(format!("{} —", i18n::t(stat.kind.as_str())))
                        .key(format!("{key}-type"))
                        .font(theme::leading_none(type_::XS))
                        .color(theme::gray(600)),
                )
                .child(
                    text(i18n::t("Not recognized modifier"))
                        .key(format!("{key}-unrecognized"))
                        .font(theme::leading_none(type_::XS))
                        .color(theme::orange(400))
                        .pl(theme::rem(0.25)),
                ),
        )
}
