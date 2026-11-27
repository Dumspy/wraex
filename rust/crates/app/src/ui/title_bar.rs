//! `AppTitlebar.vue`: the bar above every widget.

use std::fmt;
use std::rc::Rc;

use rune::{El, row, text};

use crate::theme::{self, text as type_};
use crate::ui::icon;

/// A press handler, as the bar's builder methods take them.
type Handler<S> = Rc<dyn Fn(&mut S)>;

/// The title bar's close glyph (`fa-window-close`), at the 14 pt `text-sm` the bar inherits.
const GLYPH: f32 = 14.0;

/// One widget's title bar: the `<slot />` `AppTitlebar` leads with, its title button and its
/// close button, built in that order as the component lays them out.
///
/// ```ignore
/// ui::title_bar(check.title())
///     .leading(el().w(theme::rem(2.0)).h_full())   // upstream's `<div class="w-8" />`
///     .on_close(|s: &mut App| s.price_check.close())
///     .build()
/// ```
pub struct TitleBar<S> {
    title: String,
    leading: Option<El>,
    trailing: Option<El>,
    on_title_press: Option<Handler<S>>,
    on_close: Option<Handler<S>>,
}

impl<S> fmt::Debug for TitleBar<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TitleBar")
            .field("title", &self.title)
            .field("leading", &self.leading.is_some())
            .field("trailing", &self.trailing.is_some())
            .field("on_title_press", &self.on_title_press.is_some())
            .field("on_close", &self.on_close.is_some())
            .finish()
    }
}

/// A title bar with `title` in the middle. Add handlers and slots, then [`TitleBar::build`].
pub fn title_bar<S>(title: impl Into<String>) -> TitleBar<S> {
    TitleBar { title: title.into(), leading: None, trailing: None, on_title_press: None, on_close: None }
}

impl<S> TitleBar<S> {
    /// The element in `AppTitlebar`'s `<slot />`: the price check puts the divine-orb rate (or
    /// an empty `w-8` spacer that keeps the title centred) there.
    #[must_use]
    pub fn leading(mut self, el: El) -> Self {
        self.leading = Some(el);
        self
    }

    /// An extra element just before the close button.
    #[must_use]
    pub fn trailing(mut self, el: El) -> Self {
        self.trailing = Some(el);
        self
    }

    /// A press on the title. Upstream uses it to open the league selection.
    #[must_use]
    pub fn on_title_press(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_title_press = Some(Rc::new(f));
        self
    }

    /// A press on the close button.
    #[must_use]
    pub fn on_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close = Some(Rc::new(f));
        self
    }
}

impl<S: 'static> TitleBar<S> {
    /// `.titlebar`: `bg-gray-900 text-gray-600`, `justify-content: space-between`, 1.5 rem high.
    pub fn build(mut self) -> El {
        let mut bar =
            row().key("titlebar").w_full().h(theme::rem(1.5)).fill(theme::gray(900)).justify_between().center_y();
        // `AppTitlebar`'s title `<button>` takes no width of its own: with a leading slot the
        // space between it and the close button centres the title (the price check's `w-8`
        // spacer and its rate), and without one (`settings`) the title sits at the left edge.
        let centred = self.leading.is_some();
        if let Some(leading) = self.leading.take() {
            bar = bar.child(leading);
        }
        bar = bar.child(title_button(self.title, self.on_title_press.take(), centred));
        if let Some(trailing) = self.trailing.take() {
            bar = bar.child(trailing);
        }
        bar.child(close_button(self.on_close.take()))
    }
}

/// The title button: it centres its text as a `<button>` does, and truncates when the window
/// is narrow. With `centred` it takes the bar's remaining width (a leading slot keeps the
/// title in the middle); without one it stays content-sized at the bar's left.
fn title_button<S: 'static>(title: String, on_press: Option<Handler<S>>, centred: bool) -> El {
    let label = text(title).key("titlebar-title").font(type_::SM).color(theme::gray(600)).truncate();
    let mut button = row().key("titlebar-caption").h_full().px(theme::rem(0.5)).pt(1.0).center();
    if centred {
        button = button.grow(1.0);
    }
    if let Some(f) = on_press {
        // Upstream also lightens the text on hover; Rune's `hover` tints the fill only.
        button = button.hover(theme::gray(700)).focusable(true).role("button").on_press(move |s: &mut S| f(s));
    }
    button.child(label)
}

/// `title="Close"`: `px-2 pt-px`, and a red fill where upstream turns the icon `red-200`.
fn close_button<S: 'static>(on_close: Option<Handler<S>>) -> El {
    let glyph = icon::close(GLYPH, theme::gray(600));
    let mut button =
        row().key("titlebar-close").h_full().px(theme::rem(0.5)).pt(1.0).center().label("Close").role("button");
    if let Some(f) = on_close {
        button = button.hover(theme::red(500)).focusable(true).on_press(move |s: &mut S| f(s));
    }
    button.child(glyph)
}
