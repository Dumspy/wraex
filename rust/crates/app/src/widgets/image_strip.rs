//! `ref/apt/renderer/src/web/overlay/WidgetImageStrip.vue` in Rune: a titled strip of pictures.
//!
//! The default instance is "Cheat sheets" with one image, `syndicate.jpg` from the reference's
//! `public/images`, drawn `object-fit: cover` in a 12 × 6.5 rem card. Editing the widget (its
//! hover panel's `edit`) turns the title into an input and adds the per-image `fa-times` remove
//! button, the `fa-arrows-alt` drag handle and the "Choose File" card; the file dialog and the
//! drag-and-drop reordering are the shell's ([`Action::ImportImage`]), and the added file's name
//! would arrive as a long path under `/uploads/`, which the port has no store for.
//!
//! Each image is `{ id, url }`: a file name in the reference's `public/images` when it is
//! shorter than 32 characters, a full URL when it contains `://` (`FullscreenImage.vue`'s
//! `resolvedSrc`). The port draws the first through [`crate::ui::art`] and the second as CDN art;
//! an uploads path is the placeholder, as any picture the app cannot load is.

use rune::{Color, El, col, row};
use serde::Deserialize;
use serde_json::{Map, Value};
use wraex_core::num::usize_to_f32;

use super::lenient;
use crate::theme::{self, text as type_};
use crate::widgets::{Action, Overlay, Project, Widget, icon};

/// `WidgetSpec.type`.
pub const WM_TYPE: &str = "image-strip";

/// One `images[]` entry (`WidgetImageStrip.vue`'s `{ id, url }`), read leniently: an odd entry is
/// a card with the fallbacks the widget has always used.
#[derive(Debug, Clone, Default, Deserialize)]
struct StripImage {
    #[serde(default, deserialize_with = "lenient")]
    id: Option<serde_json::Number>,
    #[serde(default, deserialize_with = "lenient")]
    url: Option<String>,
}

/// `config.images`'s field of the widget object.
#[derive(Debug, Deserialize)]
struct ImagesField {
    #[serde(default, deserialize_with = "lenient")]
    images: Option<Vec<Value>>,
}

/// `config.images`, in config order; an entry that is not an object is an empty one.
fn images(widget: &Widget) -> Vec<StripImage> {
    ImagesField::deserialize(&widget.config)
        .ok()
        .and_then(|field| field.images)
        .unwrap_or_default()
        .iter()
        .map(|image| StripImage::deserialize(image).unwrap_or_default())
        .collect()
}

/// `WidgetImageStrip.vue`'s mount: the default cheat sheet, `syndicate.jpg`.
pub fn default_images() -> Value {
    let mut image = Map::new();
    image.insert("id".to_owned(), Value::from(1_u32));
    image.insert("url".to_owned(), Value::String("syndicate.jpg".to_owned()));
    Value::Array(vec![Value::Object(image)])
}

/// `.card`: `width: 12rem; height: 6.5rem; p-1; bg-gray-800; rounded`, at the default 16 pt
/// root font size.
const CARD: (f32, f32) = (12.0 * theme::REM, 6.5 * theme::REM);
/// The picture's box inside it (`w-full h-full` of the card's 0.25 rem padding).
const PICTURE: (f32, f32) = (CARD.0 - 2.0 * theme::REM / 4.0, CARD.1 - 2.0 * theme::REM / 4.0);

/// `WidgetImageStrip.vue`: the title (or its input) and the images.
// `cards * CARD.0 + (cards - 1) * gap` is the reference's own arithmetic: `mul_add` fuses the
// rounding, which would move the strip by a fraction of a point from the reference's CSS.
#[expect(clippy::suboptimal_flops, reason = "the reference's CSS arithmetic is not fused")]
pub fn view<S: 'static>(overlay: &Overlay, widget: &Widget, project: Project<S>) -> El {
    let id = widget.wm_id;
    let editing = overlay.editing == Some(id);
    let images = images(widget);

    // The columns the strip spans: one card per image, plus the "Choose File" card while
    // editing. Upstream's title is a block across the card's width, and its card is as wide as
    // its images, so the title takes the same width here.
    let gap = theme::rem(0.25);
    let cards = images.len().max(1).saturating_add(usize::from(editing));
    let strip_width = usize_to_f32(cards) * CARD.0 + usize_to_f32(cards.saturating_sub(1)) * gap;
    // `m-1` around the title: a block's auto width is its parent's minus its margins.
    let margin = theme::rem(0.25);
    let title_width = strip_width - 2.0 * margin;

    let title = if editing {
        let style = type_::SM;
        let placeholder = crate::settings::t(overlay.game, "widget.title");
        row().key("image-strip-title-edit").mb(theme::rem(0.25)).child(
            rune::text_input(&overlay.title, style)
                .key("image-strip-title-input")
                .single_line()
                // `w-full`: the input spans the card's content, not the title's margins.
                .w(strip_width)
                .min_w(0.0)
                .px(theme::rem(0.25))
                .py(theme::rem(0.25))
                .radius(theme::ROUNDED)
                .fill(theme::gray(700))
                .color(theme::gray(100))
                .caret(theme::gray(300), theme::gray(400).alpha(0.35))
                .placeholder(placeholder.clone(), theme::gray(500))
                .placeholder_font(style)
                .label(placeholder)
                .on_edit(move |s: &mut S| &mut project(s).title)
                .on_input_change(move |s: &mut S, _tx: &mut rune::Tx| {
                    let overlay = project(s);
                    let title = overlay.title.text.clone();
                    overlay.set_title(id, &title);
                }),
        )
    } else {
        // `text-gray-100 m-1 leading-4 text-center`: the title, or "Untitled".
        let label = match widget.wm_title.as_str() {
            "" => "Untitled".to_owned(),
            title => title.to_owned(),
        };
        row().key("image-strip-title").w(title_width).mx(margin).my(margin).child(
            rune::text(label)
                .key("image-strip-title-text")
                .w(title_width)
                .align(rune::Align::Center)
                .font(type_::SM.lh(theme::rem(1.0)))
                .color(theme::gray(100)),
        )
    };

    let mut strip = row().key("image-strip-images").gap(gap);
    for (index, image) in images.iter().enumerate() {
        let url = image.url.clone().unwrap_or_default();
        let image_id = image
            .id
            .as_ref()
            .and_then(serde_json::Number::as_i64)
            .unwrap_or_else(|| i64::try_from(index).unwrap_or(i64::MAX));
        strip = strip.child(image_card(overlay, id, ImageCard { index, id: image_id, url: &url }, project));
    }
    if editing {
        strip = strip.child(choose_file(overlay, id, project));
    }

    // `.widget-default-style p-1` with the reference's `min-width: 5rem`.
    col()
        .key("image-strip")
        .min_w(theme::rem(5.0))
        .padding(theme::rem(0.25))
        .radius(theme::ROUNDED)
        .fill(theme::gray(900))
        .shadow(1.0, 3.0, Color::BLACK.alpha(0.75))
        .child(title)
        .child(strip)
}

/// One image's card: the config entry's `{ id, url }` and the index its element keys count from.
#[derive(Clone, Copy)]
struct ImageCard<'a> {
    index: usize,
    id: i64,
    url: &'a str,
}

/// One `.card` with its `FullscreenImage`; while editing, its remove button on top and the drag
/// handle in the middle.
fn image_card<S: 'static>(overlay: &Overlay, wm_id: u32, image: ImageCard<'_>, project: Project<S>) -> El {
    let ImageCard { index, id: image_id, url } = image;
    // The widget's `Widget.vue` modes: edit draws the remove button and drag handle, and the
    // handle hides while the widget is being moved.
    let editing = overlay.editing == Some(wm_id);
    let moving = overlay.moving == Some(wm_id);
    let pad = theme::rem(0.25);
    let picture = crate::widgets::strip_picture(overlay.game, url, PICTURE);
    let mut card = row()
        .key(format!("image-strip-image-{index}"))
        .size(CARD.0, CARD.1)
        .radius(theme::ROUNDED)
        .fill(theme::gray(800))
        .child(
            // `.card p-1` then `rounded overflow-hidden w-full h-full`: the picture is scaled to
            // cover its 184 × 96 box and the overflow is cropped.
            row()
                .key(format!("image-strip-picture-{index}"))
                .absolute(pad, pad)
                .size(PICTURE.0, PICTURE.1)
                .radius(theme::ROUNDED)
                .clip_rounded(theme::ROUNDED)
                .child(picture.key(format!("image-strip-picture-{index}-img"))),
        );
    if editing {
        // `bg-gray-800 absolute top-0 right-0 rounded-bl text-red-500 px-2 py-1`.
        card = card.child(
            row()
                .key(format!("image-strip-remove-{index}"))
                .pin(None, Some(0.0), Some(0.0), None)
                .px(theme::rem(0.5))
                .py(theme::rem(0.25))
                .corners([0.0, 0.0, 0.0, theme::ROUNDED])
                .fill(theme::gray(800))
                .focusable(true)
                .role("button")
                .label("remove image")
                .on_press(move |s: &mut S| project(s).remove_image(wm_id, image_id))
                .child(icon::times(type_::SM.size, theme::red(500))),
        );
        // `fa-arrows-alt` in a `w-8 h-8` box at the card's centre
        // (`top: calc(50% - 1rem); left: calc(50% - 1rem)`).
        if !moving {
            let handle = theme::rem(2.0);
            card = card.child(
                row()
                    .key(format!("image-strip-drag-{index}"))
                    .absolute((CARD.0 - handle) / 2.0, (CARD.1 - handle) / 2.0)
                    .size(handle, handle)
                    .radius(theme::ROUNDED)
                    .fill(theme::gray(900))
                    .center()
                    .child(icon::arrows_alt(theme::rem(1.0), theme::gray(400))),
            );
        }
    }
    card
}

/// The footer card: `Choose File`, which asks the shell for a file
/// (`Host.importFile`).
fn choose_file<S: 'static>(overlay: &Overlay, wm_id: u32, project: Project<S>) -> El {
    row()
        .key("image-strip-choose-file")
        .size(CARD.0, CARD.1)
        .padding(theme::rem(0.25))
        .radius(theme::ROUNDED)
        .fill(theme::gray(800))
        .center()
        .child(
            row()
                .key("image-strip-choose-file-button")
                .px(theme::rem(0.5))
                .py(theme::rem(0.25))
                .radius(theme::ROUNDED)
                .focusable(true)
                .role("button")
                .label(crate::settings::t(overlay.game, "choose_file"))
                .hover(theme::gray(700))
                .on_press(move |s: &mut S| project(s).actions.push(Action::ImportImage(wm_id)))
                .gap(theme::rem(0.25))
                .items_center()
                .child(icon::file_import(type_::SM.size, theme::gray(400)))
                .child(
                    rune::text(crate::settings::t(overlay.game, "choose_file"))
                        .key("image-strip-choose-file-label")
                        .font(type_::SM)
                        .color(theme::gray(400)),
                ),
        )
}
