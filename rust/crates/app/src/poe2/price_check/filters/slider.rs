//! `web/ui/StatRollSlider.vue`: the bar under a unique's roll row, from its bounds to the
//! roll, with a tick at the roll and the bounds' numbers at the ends.
//!
//! A pointer drag sets the roll's min or max (whichever end the mode allows); the port
//! draws the bar and leaves dragging to the min/max fields above it.

use rune::{Color, El, el, row, text};
use wraex_poe2::filters::MinMax;

use crate::theme::{self, text as type_};

/// The track's width: `style="width: 12.5rem"`.
const TRACK: f32 = 12.5 * theme::REM;
/// `p-0.5`: the band is inset by two points either side.
const INSET: f32 = 2.0;

/// How the roll's ends map onto the bounds (`StatRollSlider.vue`'s `state`).
struct State {
    left: f64,
    right: f64,
    min_inclusive: bool,
    max_inclusive: bool,
}

fn state(min: Option<f64>, max: Option<f64>, bounds: MinMax) -> State {
    let percent = |value: f64| ((value - bounds.min) / (bounds.max - bounds.min)) * 100.0;
    match (min, max) {
        (Some(min), None) => State { left: percent(min), right: 0.0, min_inclusive: false, max_inclusive: true },
        (None, Some(max)) => {
            State { left: 0.0, right: percent(bounds.max) - percent(max), min_inclusive: true, max_inclusive: false }
        }
        (Some(min), Some(max)) if min > max => {
            State { left: 0.0, right: 100.0, min_inclusive: false, max_inclusive: false }
        }
        (Some(min), Some(max)) => State {
            left: percent(min),
            right: 100.0 - percent(max),
            min_inclusive: min <= bounds.min,
            max_inclusive: max >= bounds.max,
        },
        (None, None) => State { left: 0.0, right: 100.0, min_inclusive: false, max_inclusive: false },
    }
}

/// `StatRollSlider.vue`: `mr-4`, 12.5 rem wide.
pub fn view(index: usize, roll: f64, min: Option<f64>, max: Option<f64>, bounds: MinMax) -> El {
    let state = state(min, max, bounds);
    let percent_roll = ((roll - bounds.min) / (bounds.max - bounds.min)) * 100.0;
    let inner = f64::from(TRACK - 2.0 * INSET);
    let left = inner * (state.left / 100.0).clamp(0.0, 1.0);
    let right = inner * (state.right / 100.0).clamp(0.0, 1.0);
    let band = (inner - left - right).max(12.0);
    // The tick marks the roll on the whole track, not on the band.
    let tick = (inner * (percent_roll / 100.0) - left - 2.0 * f64::from(INSET)).clamp(0.0, (band - 4.0).max(0.0));
    let (left, right, tick) = (left as f32, right as f32, tick as f32);

    let bound = |value: f64, inclusive: bool, key: String| {
        let color = if inclusive { theme::gray(900) } else { theme::gray(500) };
        text(wraex_core::js::number_to_string(value)).key(key).font(type_::SM).color(color)
    };

    // `bg-gray-700 rounded p-0.5 h-5 items-center`: the bounds' numbers sit at the ends,
    // the grey band covers the rolls the filter excludes, and the tick marks the roll.
    row()
        .key(format!("stat-{index}-slider"))
        .w(TRACK)
        .h(theme::rem(1.25))
        .mr(theme::rem(1.0))
        .items_center()
        .radius(theme::ROUNDED)
        .fill(theme::gray(700))
        .padding(INSET)
        .child(bound(bounds.min, state.min_inclusive, format!("stat-{index}-slider-min")).pl(theme::rem(0.25)))
        .child(
            row()
                .key(format!("stat-{index}-slider-fill"))
                .grow(1.0)
                .h_full()
                .items_center()
                .child(el().w(left))
                .child(
                    row()
                        .grow(1.0)
                        .h_full()
                        .items_center()
                        .radius(INSET)
                        .fill(theme::gray(400))
                        // `background-clip: content-box`: the band is inset again.
                        .padding(INSET)
                        .child(el().w(tick))
                        .child(el().key(format!("stat-{index}-slider-tick")).w(2.0).h_full().fill(Color::BLACK))
                        .child(el().grow(1.0))
                        // The bounds' numbers sit at the track's ends in the reference; the
                        // band covers the right one once the roll's range reaches it, which is
                        // the mode a unique's rows are in.
                        .child(
                            bound(bounds.max, state.max_inclusive, format!("stat-{index}-slider-max"))
                                .pr(theme::rem(0.25)),
                        ),
                )
                .child(el().w(right)),
        )
}
