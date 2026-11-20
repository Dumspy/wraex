//! The Wraex desktop app.
//!
//! A thin Rune shell for now. The fixture harness (phase 1) and the Windows platform
//! spike (phase 2) come before the price-check UI (phase 5).
//!
//! `cargo run -p wraex-app` opens the window; `-- --snap wraex.png` renders it
//! headlessly.

use rune::tok::type_;
use rune::{AppSpec, Cx, El, col, text};
use wraex_game::Game;
use wraex_poe1::Poe1;
use wraex_poe2::Poe2;

#[derive(Debug, Default)]
struct State;

fn view(_: &State, _: &mut Cx<State>) -> El {
    col()
        .w_full()
        .h_full()
        .center()
        .gap(6.0)
        .child(text("Wraex").font(type_::TITLE))
        .child(text(Poe1.name()))
        .child(text(Poe2.name()))
}

fn main() {
    rune::run(AppSpec::new("Wraex", (420.0, 220.0), State::default, view));
}
