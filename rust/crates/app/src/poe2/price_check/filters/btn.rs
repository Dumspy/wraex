//! `FilterBtnNumeric.vue` and `FilterBtnLogical.vue`: the chips above the stat rows.

use rune::{Color, El, row, text};

use crate::poe2::i18n;
use crate::poe2::price_check::view::Project;
use crate::poe2::price_check::{InputField, NumericFilter, NumericPart, PriceCheck};
use crate::theme::{self, text as type_};
use crate::ui::art;
use crate::ui::icon;

use super::{FieldStyle, LINE, number_field, number_width};

/// `FilterBtnNumeric.vue`: the label toggles the filter, the min/max fields edit it.
pub fn numeric<S: 'static>(check: &PriceCheck, which: NumericFilter, project: Project<S>) -> El {
    let Some(filter) = check.numeric(which) else {
        return row().key(format!("btn-{}", which.key()));
    };
    let active = !filter.disabled;
    let has_max = filter.max.is_some();
    let min_field = InputField::Numeric(which, NumericPart::Min);
    let min_width = number_width(check.input(&min_field.key()).map_or("", |edit| edit.text.as_str()));
    let corners = if has_max { [theme::ROUNDED, 0.0, 0.0, theme::ROUNDED] } else { [theme::ROUNDED; 4] };
    let mut chip = row()
        .key(format!("btn-{}", which.key()))
        .h(LINE)
        .items_center()
        .pr(theme::rem(0.25))
        .fill(theme::gray(900))
        .radius(theme::ROUNDED)
        .border(1.0, if active { theme::gray(500) } else { Color::TRANSPARENT })
        .border_layout()
        .child(
            row()
                .key(format!("btn-{}-name", which.key()))
                .h_full()
                .pl(theme::rem(0.5))
                .items_center()
                .focusable(true)
                .role("button")
                .label(numeric_label(which))
                .on_press(move |s: &mut S| project(s).toggle_numeric(which))
                .child(
                    text(numeric_label(which))
                        .key(format!("btn-{}-label", which.key()))
                        .font(type_::SM)
                        .color(theme::gray(200)),
                ),
        )
        .child(number_field(check, min_field, min_width, "", corners, FieldStyle::Chip, project));
    if has_max {
        let max_field = InputField::Numeric(which, NumericPart::Max);
        let max_width = number_width(check.input(&max_field.key()).map_or("", |edit| edit.text.as_str()));
        chip = chip.child(text("–").font(type_::SM).color(theme::gray(400))).child(number_field(
            check,
            max_field,
            max_width,
            "…",
            [0.0, theme::ROUNDED, theme::ROUNDED, 0.0],
            FieldStyle::Chip,
            project,
        ));
    }
    chip
}

/// `FilterBtnLogical.vue`: one boolean chip. `active` overrides the filter's own state for
/// the chips that are always on (Mirrored, Sanctified), `readonly` chips do nothing. `image`
/// is the chip's `img` prop (an influence badge).
pub fn logical<S: 'static>(
    key: &str,
    label: &str,
    active: Option<bool>,
    collapse: Option<bool>,
    image: Option<art::Picture>,
    on_press: impl Fn(&mut S) + 'static,
) -> El {
    let mut chip = row()
        .key(format!("btn-{key}"))
        .h(LINE)
        .pl(theme::rem(0.25))
        .pr(theme::rem(0.5))
        .items_center()
        .fill(theme::gray(900))
        .radius(theme::ROUNDED)
        .border(1.0, if active == Some(true) { theme::gray(500) } else { Color::TRANSPARENT })
        .border_layout()
        .focusable(true)
        .role("button")
        .label(label)
        .on_press(on_press);
    if let Some(image) = image {
        chip = chip.child(super::image(image, theme::rem(1.25)));
    }
    chip = chip.child(
        text(label).key(format!("btn-{key}-label")).font(type_::SM).color(theme::gray(200)).pl(theme::rem(0.25)),
    );
    // `v-if="collapse"` draws the chevron only while the block is collapsed, and its class
    // is `filter.disabled ? fa-chevron-down : fa-chevron-up` for `statsVisibility`, which is
    // disabled exactly then: always the down chevron.
    if collapse == Some(true) {
        chip = chip.child(
            icon::chevron(theme::rem(0.75), false, theme::gray(400))
                .key(format!("btn-{key}-chevron"))
                .pl(theme::rem(0.5)),
        );
    }
    chip
}

/// A read-only chip: `:readonly` and `:filter="{ disabled: false }"` in `FiltersBlock.vue`,
/// so it is drawn as active and a press does nothing.
pub fn readonly<S: 'static>(key: &str, label: &str) -> El {
    logical::<S>(key, label, Some(true), None, None, |_: &mut S| {})
}

/// `t('item.item_level')` without its `{0}`: the value is the chip's own field.
fn numeric_label(which: NumericFilter) -> String {
    let message = i18n::t(&format!("item.{}", which.key()));
    message.replace("{0}", "").trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chip_label_drops_the_value_placeholder() {
        assert_eq!(numeric_label(NumericFilter::AreaLevel), "Area Level:");
        assert_eq!(numeric_label(NumericFilter::AscendancyPoints), "Points:");
        assert_eq!(numeric_label(NumericFilter::RequiresLevel), "Req. Level:");
    }
}
