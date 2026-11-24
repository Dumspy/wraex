//! `UnknownModifier.vue`: a modifier the parser did not recognize, with the button that
//! reloads the trade site's data (`fallback/ReloadTradeData.vue`).

use rune::{El, col, row, text};
use wraex_poe2::parser::item::UnknownModifier;

use crate::poe2::i18n;
use crate::poe2::price_check::view::Project;
use crate::theme::{self, text as type_};
use crate::ui::{self, icon};

/// `UnknownModifier.vue`.
pub fn view<S: 'static>(key: &str, stat: &UnknownModifier, _item_text: &str, _project: Project<S>) -> El {
    row()
        .key(key)
        .w_full()
        .justify_between()
        .border_sides([0.0, 0.0, 1.0, 0.0], theme::gray(700))
        .border_layout()
        .child(
            col()
                .py(theme::rem(0.5))
                .child(
                    row()
                        .w_full()
                        .pb(theme::rem(0.25))
                        .items_center()
                        .child(
                            row()
                                .w(theme::rem(1.25))
                                .shrink()
                                .child(icon::warning(theme::rem(0.75), theme::orange(400))),
                        )
                        .child(
                            ui::item_modifier_text(&format!("{key}-text"), &stat.text, None, type_::SM)
                                .mr(theme::rem(0.25))
                                .truncate(),
                        ),
                )
                .child(
                    row()
                        .key(format!("{key}-reason"))
                        .ml(theme::rem(1.25))
                        .items_center()
                        .child(
                            text(format!("{} — ", i18n::t(stat.kind.as_str())))
                                .key(format!("{key}-type"))
                                .font(theme::leading_none(type_::XS))
                                .color(theme::gray(600)),
                        )
                        .child(
                            text(i18n::t("Not recognized modifier"))
                                .key(format!("{key}-unrecognized"))
                                .font(theme::leading_none(type_::XS))
                                .color(theme::orange(400)),
                        ),
                ),
        )
        .child(reload_trade_data::<S>(format!("{key}-reload")))
}

/// `ReloadTradeData.vue`'s first state: the button that asks for the trade site's data
/// again. The app loads that data once per run (`price_check::data`), so the button is drawn
/// where the reference has it but cannot refresh it yet — its label wraps in the narrow
/// column the row leaves it, as the reference's does.
fn reload_trade_data<S: 'static>(key: String) -> El {
    let label = i18n::t("Reload fallback data");
    col().key(key.clone()).w(theme::rem(4.25)).mx(theme::rem(0.25)).my(theme::rem(0.25)).child(
        row()
            .key(format!("{key}-button"))
            .w_full()
            .px(theme::rem(0.5))
            .py(theme::rem(0.25))
            .justify_center()
            .radius(theme::ROUNDED)
            .fill(theme::gray(700))
            .focusable(true)
            .role("button")
            .label(label.clone())
            .on_press(|_: &mut S| {})
            .child(
                text(label)
                    .key(format!("{key}-label"))
                    .font(theme::leading_none(type_::XS))
                    .color(theme::gray(400))
                    .w(theme::rem(3.75))
                    .wrap(),
            ),
    )
}
