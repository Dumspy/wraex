//! `web/ui/ItemQuickPrice.vue`: an item (or a base type) and what it is worth.
//!
//! ```ignore
//! // `PriceTrend.vue`: the item's icon, its price and the chaos (or divine) icon.
//! ui::item_quick_price("trend", Some(price)).build()
//! ```

use rune::{El, Span, rich, row, text};
use wraex_core::GameId;
use wraex_poe1::prices::{CurrencyValue, display_rounding};

use crate::theme::{self, text as type_};
use crate::ui::art::{self, Picture};
use crate::ui::icon;

/// The item image's size (`w-8 h-8` unless the base type is taller or wider).
const ICON: f32 = 32.0;

/// One `ItemQuickPrice.vue`: `item → price × currency`.
pub struct ItemQuickPrice {
    key: String,
    price: Option<CurrencyValue>,
    fraction: bool,
    currency_text: bool,
    slot: Option<El>,
    /// `itemImg`: the item's art, `/images/wisdom.png` when the caller has none.
    item_img: Option<Picture>,
    icon_w: f32,
    icon_h: f32,
    color: Option<rune::Color>,
}

impl std::fmt::Debug for ItemQuickPrice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ItemQuickPrice")
            .field("key", &self.key)
            .field("price", &self.price)
            .field("fraction", &self.fraction)
            .field("currency_text", &self.currency_text)
            .finish()
    }
}

impl ItemQuickPrice {
    /// `item → price`, with the item's image standing in for `itemImg`.
    pub fn new(key: impl Into<String>, price: Option<CurrencyValue>) -> Self {
        Self {
            key: key.into(),
            price,
            fraction: false,
            currency_text: false,
            slot: None,
            item_img: None,
            icon_w: ICON,
            icon_h: ICON,
            color: None,
        }
    }

    /// `itemImg`: the item's art (`item.info.icon` on the CDN, or one of the reference's
    /// `/images` files). Without it the reference's default, `wisdom.png`, is drawn.
    pub fn item_img(mut self, picture: Picture) -> Self {
        self.item_img = Some(picture);
        self
    }

    /// The price's colour. Popped over the white tippy frame the window's inherited
    /// `text-gray-200` would be invisible (`PriceCheckWindow.vue`'s exchange-rate popover).
    pub fn color(mut self, color: rune::Color) -> Self {
        self.color = Some(color);
        self
    }

    /// `fraction`: `1⁄2` instead of `0.5` (`filters.stackSize != null` in the trend row).
    pub fn fraction(mut self, fraction: bool) -> Self {
        self.fraction = fraction;
        self
    }

    /// `currency-text`: the price's currency as text, with no `×` and no image
    /// (`PriceTrend.vue`'s fallback row).
    pub fn currency_text(mut self) -> Self {
        self.currency_text = true;
        self
    }

    /// Replace the item image with something else (`PriceTrend.vue`'s "Base item").
    pub fn slot(mut self, slot: El) -> Self {
        self.slot = Some(slot);
        self
    }

    /// `itemBase`'s footprint, for an item whose image is not square.
    pub fn item_size(mut self, w: Option<u32>, h: Option<u32>) -> Self {
        match (w.unwrap_or(1), h.unwrap_or(1)) {
            // `h-10 -my-1` and `w-12`: the image grows into the row without moving it.
            (_, h) if h > 1 => {
                self.icon_w = ICON;
                self.icon_h = theme::rem(2.5);
            }
            (w, _) if w > 1 => {
                self.icon_w = theme::rem(3.0);
                self.icon_h = ICON;
            }
            _ => {}
        }
        self
    }

    pub fn build(self) -> El {
        let Self { key, price, fraction, currency_text, slot, item_img, icon_w, icon_h, color } = self;
        // `displayRounding` of both ends: no price is the reference's `?`.
        let min = price.map_or_else(|| "?".to_owned(), |price| display_rounding(price.min, fraction));
        let max = price.map_or_else(|| "?".to_owned(), |price| display_rounding(price.max, fraction));
        let range = min != max;

        let slot = slot.unwrap_or_else(|| {
            let picture = item_img.unwrap_or_else(|| Picture::local("wisdom.png"));
            art::picture(GameId::Poe1, &picture, (icon_w, icon_h)).key(format!("{key}-item-icon"))
        });
        let mut label = row().key(format!("{key}-text")).items_center().min_w(0.0);
        // `<span :class="{ 'px-1': (minText === '?') }">`
        let mut first = text(min.clone()).key(format!("{key}-price")).font(type_::BASE);
        if min == "?" {
            first = first.px(theme::rem(0.25));
        }
        if let Some(color) = color {
            first = first.color(color);
        }
        label = label.child(first);
        if range {
            // `<span class="text-gray-600 font-sans"> ~ </span>`
            label = label.child(
                rich(" ~ ", vec![Span::new(0, 3).mono().color(theme::gray(600))])
                    .key(format!("{key}-range-sep"))
                    .font(type_::BASE),
            );
            let mut last = text(max).key(format!("{key}-price-max")).font(type_::BASE);
            if let Some(color) = color {
                last = last.color(color);
            }
            label = label.child(last);
        }
        if !currency_text {
            // `<span class="font-sans"> ×</span>`, then the chaos or divine icon.
            let span = match color {
                Some(color) => Span::new(0, 3).mono().color(color),
                None => Span::new(0, 3).mono(),
            };
            label = label.child(rich(" \u{d7}", vec![span]).key(format!("{key}-times")).font(type_::BASE));
        } else if let Some(price) = price {
            // `&nbsp;{{ price.currency }}`
            let mut currency =
                text(format!("\u{a0}{}", price.currency)).key(format!("{key}-currency")).font(type_::BASE);
            if let Some(color) = color {
                currency = currency.color(color);
            }
            label = label.child(currency);
        }

        let mut root = row()
            .key(key.clone())
            .items_center()
            .gap(theme::rem(0.25))
            .child(slot)
            // `<i class="fas fa-arrow-right text-gray-600 px-1 text-sm">`
            .child(icon::arrow_right(theme::rem(0.875), theme::rem(0.875), theme::gray(600)).mx(theme::rem(0.25)))
            .child(label);
        if !currency_text {
            // `isValuable`: `/images/divine.png` for a divine price, `/images/chaos.png` otherwise.
            let icon = if price.is_some_and(|price| price.currency == "div") { "divine.png" } else { "chaos.png" };
            root = root.child(art::local(GameId::Poe1, icon, theme::rem(2.0)).key(format!("{key}-currency-icon")));
        }
        root
    }
}
