//! `SettingsWindow.vue`'s pages: one module per `selectedComponent`.

pub mod about;
pub mod chat;
pub mod debug;
pub mod general;
pub mod hotkeys;
pub mod item_check;
pub mod leveling;
pub mod maps;
pub mod price_check;

use rune::{El, el};

use crate::settings::{Field, Page, Project, Settings, t};
use crate::theme::{self, text as type_};
use crate::ui;

/// The selected page's content. The page roots copy their Vue template's outer class
/// (`max-w-md p-2`, or the maps list's full-height column).
pub fn view<S: 'static>(settings: &Settings, project: Project<S>) -> El {
    match settings.page {
        Page::Hotkeys => hotkeys::view(settings, project),
        Page::Chat => chat::view(settings, project),
        Page::General => general::view(settings, project),
        Page::PriceCheck => price_check::view(settings, project),
        Page::Maps => maps::view(settings, project),
        Page::ItemCheck => item_check::view(settings, project),
        Page::Leveling => leveling::view(settings, project),
        Page::Debug => debug::view(settings, project),
        Page::About => about::view(settings, project),
    }
}

/// `max-w-md p-2`: every page's column, at most 28 rem wide.
///
/// `w_full` keeps it inside the content column when the window is narrower than upstream's
/// 50 rem (the price-check window is 28.75 rem wide, so a settings scene renders in that).
pub fn page(key: &str) -> El {
    rune::col().key(key.to_owned()).w_full().max_w(theme::rem(28.0)).padding(theme::rem(0.5))
}

/// A field label: `<div class="flex-1 mb-1">{{ t('…') }}</div>`.
pub fn label(key: &str, text: &str) -> El {
    ui::field::label(key, text)
}

/// A [`text_field`]: the element key, the config field it binds and its input class.
#[derive(Debug, Clone, Copy)]
pub struct TextField<'a> {
    /// The element key; the state's edit handle carries the same key.
    pub key: &'a str,
    /// The config field the text is written back to.
    pub field: Field,
    /// Which input class it copies.
    pub kind: ui::field::Kind,
    /// A fixed width, or the column's own for `None`.
    pub width: Option<f32>,
    /// The empty text's placeholder, or `""` for none.
    pub placeholder: &'a str,
}

impl<'a> TextField<'a> {
    /// A full-width text field of `kind`.
    pub const fn new(key: &'a str, field: Field, kind: ui::field::Kind) -> Self {
        Self { key, field, kind, width: None, placeholder: "" }
    }

    /// The empty text's placeholder.
    #[must_use]
    pub const fn placeholder(self, placeholder: &'a str) -> Self {
        Self { placeholder, ..self }
    }
}

/// A text field bound to `field`, writing its (trimmed, `v-model.trim`) text back.
pub fn text_field<S: 'static>(settings: &Settings, props: TextField<'_>, project: Project<S>) -> El {
    let TextField { key, field, kind, width, placeholder } = props;
    let Some(edit) = settings.edit(key) else {
        return el().key(key).h(type_::SM.line_px());
    };
    let edit_key = key.to_owned();
    let change_key = key.to_owned();
    ui::field::text_field(
        &ui::field::TextFieldProps { edit, key, kind, width, placeholder },
        move |s: &mut S| project(s).edit_mut(&edit_key),
        move |s: &mut S, _tx: &mut rune::Tx| {
            let text = project(s).edit_text(&change_key);
            project(s).set_string(field, text.trim());
        },
    )
}

/// A [`number_field`] (`v-model.number`): the element key, the config field it binds and what
/// its setter does with a value.
#[derive(Debug, Clone, Copy)]
pub struct NumberField<'a> {
    /// The element key; the state's edit handle carries the same key.
    pub key: &'a str,
    /// The config field the number is written back to.
    pub field: Field,
    /// The input's fixed width (`w-16`).
    pub width: f32,
    /// What the setter does with a parsed value.
    pub limit: Limit,
}

impl<'a> NumberField<'a> {
    /// A 4 rem number field.
    pub fn new(key: &'a str, field: Field, limit: Limit) -> Self {
        Self { key, field, width: theme::rem(4.0), limit }
    }
}

/// A number field (`v-model.number`): written back only while the text parses, clamped to
/// `range` where the template's computed setter clamps (or ignores values outside it).
pub fn number_field<S: 'static>(settings: &Settings, props: NumberField<'_>, project: Project<S>) -> El {
    let NumberField { key, field, width, limit } = props;
    let Some(edit) = settings.edit(key) else {
        return el().key(key).h(type_::SM.line_px());
    };
    let edit_key = key.to_owned();
    let change_key = key.to_owned();
    ui::field::text_field(
        &ui::field::TextFieldProps { edit, key, kind: ui::field::Kind::Number, width: Some(width), placeholder: "" },
        move |s: &mut S| project(s).edit_mut(&edit_key),
        move |s: &mut S, _tx: &mut rune::Tx| {
            let text = project(s).edit_text(&change_key);
            let Ok(value) = text.trim().parse::<f64>() else { return };
            if let Some(value) = limit.apply(value) {
                project(s).set_f64(field, value);
            }
        },
    )
}

/// What a number field's setter does with a value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Limit {
    /// Any number (`v-model.number`).
    Any,
    /// Ignored outside `min..=max` (`searchStatRange`).
    Within(f64, f64),
    /// Clamped to `min..=max` (`apiLatencySeconds`, `initialDelay`).
    Clamp(f64, f64),
}

impl Limit {
    fn apply(self, value: f64) -> Option<f64> {
        match self {
            Self::Any => Some(value),
            Self::Within(min, max) => (min..=max).contains(&value).then_some(value),
            Self::Clamp(min, max) => Some(value.clamp(min, max)),
        }
    }
}

/// A checkbox bound to a boolean field.
pub fn checkbox<S: 'static>(settings: &Settings, key: &str, field: Field, label: &str, project: Project<S>) -> El {
    let checked = settings.bool(field);
    ui::checkbox(ui::CheckboxProps::new(key, label).checked(checked), move |s: &mut S| {
        let checked = project(s).bool(field);
        project(s).set_bool(field, !checked);
    })
}

/// `t(key)` for the page's game.
pub fn tr(settings: &Settings, key: &str) -> String {
    t(settings.game, key)
}

/// `t(key, args)`.
pub fn tr_fmt(settings: &Settings, key: &str, args: &[&str]) -> String {
    crate::settings::fmt(settings.game, key, args)
}

/// `AppConfig().language === 'en'` style checks the item check's Craft of Exile row makes.
pub fn is_english(settings: &Settings) -> bool {
    settings.string(Field::Root("language")) == "en"
}
