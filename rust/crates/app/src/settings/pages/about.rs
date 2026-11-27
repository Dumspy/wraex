//! `web/settings/about.vue`: the app's version, the update check's state and the links.
//!
//! The name and links are Wraex's own (the references' pages name their apps and issue
//! trackers); the layout and the update-state wording are upstream's.

use rune::{El, col, el, row, text};
use wraex_core::GameId;
use wraex_core::updates::{NoDownloadReason, UpdateInfo};

use crate::settings::pages::{tr as t_key, tr_fmt};
use crate::settings::{Project, Settings};
use crate::theme::{self, text as type_};
use crate::ui;

const RELEASES: &str = "https://github.com/Dumspy/wraex/releases";
const ISSUES: &str = "https://github.com/Dumspy/wraex/issues";

pub fn view<S: 'static>(settings: &Settings, project: Project<S>) -> El {
    let version = env!("CARGO_PKG_VERSION");

    // The icon, name and version. The art is the reference's own `/images`: the orb in
    // Awakened PoE Trade, the jeweler in EE2 (`w-12 h-12`).
    let icon = match settings.game {
        GameId::Poe2 => "jeweler.png",
        _ => "TransferOrb.png",
    };
    let mut page = col().key("page-about").w_full().h_full().padding(theme::rem(0.5)).items_center();
    let mut heading = col().key("about-heading").items_center().padding(theme::rem(0.5)).mb(theme::rem(1.0));
    heading = heading.child(ui::art::local(settings.game, icon, theme::rem(3.0)).key("about-icon"));
    heading = heading.child(text("Wraex").key("about-name").font(type_::BASE));
    heading = heading.child(text(tr_fmt(settings, "app.version", &[version])).key("about-version").font(type_::SM));
    heading = heading.child(
        row()
            .key("about-links")
            .gap(theme::rem(0.5))
            .mt(theme::rem(0.25))
            .child(link::<S>("about-release-notes", &t_key(settings, "app.release_notes"), RELEASES))
            .child(link::<S>("about-report-bug", &t_key(settings, "app.report_bug"), ISSUES)),
    );
    page = page.child(heading);

    // The update box: `border border-gray-600 rounded p-2 whitespace-nowrap min-w-min w-72`.
    let (str1, str2, action) = update_state(settings);
    let mut box_ = col()
        .key("about-update")
        .w(theme::rem(18.0))
        .padding(theme::rem(0.5))
        .radius(theme::ROUNDED)
        .border(1.0, theme::gray(600))
        .border_layout();
    box_ = box_.child(text(str1).key("about-update-name").font(type_::SM));
    box_ = box_.child(text(str2).key("about-update-state").font(type_::SM));
    if let Some((key, label, action)) = action {
        box_ = box_.child(
            row()
                .key("about-update-action")
                .w_full()
                .mt(theme::rem(0.25))
                .px(theme::rem(0.5))
                .py(theme::rem(0.25))
                .radius(theme::ROUNDED)
                .fill(theme::gray(700))
                .justify_center()
                .focusable(true)
                .role("button")
                .label(label.clone())
                .on_press_tx(move |s: &mut S, tx: &mut rune::Tx| action(project(s), tx))
                .child(text(label).key(key).font(theme::leading_none(type_::SM)).color(theme::gray(300))),
        );
    }
    page = page.child(box_);

    // The contact block, pinned to the bottom: `text-center mt-auto py-8`. EE2 names its own
    // Discord handle and communities (there is no Forbidden Trove row).
    page = page.child(el().grow(1.0));
    let mut footer = col().key("about-contact").items_center().py(theme::rem(2.0));
    footer = footer.child(text(t_key(settings, "app.contact_me")).key("about-contact-text").font(type_::SM));
    let handle = match settings.game {
        GameId::Poe2 => "<@340233612064718851>",
        _ => "<@295216259795124225>",
    };
    footer = footer.child(
        text(handle).key("about-contact-handle").font(type_::SM.family(theme::FAMILY_EXO2)).color(theme::gray(500)),
    );
    let mut communities = row().key("about-communities").gap(theme::rem(1.0)).mt(theme::rem(0.5));
    communities = match settings.game {
        GameId::Poe2 => communities
            .child(community::<S>(
                settings,
                "about-reddit",
                "dc_reddit.png",
                "r/pathofexile",
                "https://old.reddit.com/r/pathofexile2",
            ))
            .child(community::<S>(
                settings,
                "about-discord",
                "discord.png",
                "pathofexile",
                "https://discord.gg/pathofexile",
            )),
        _ => communities
            .child(community::<S>(
                settings,
                "about-forbidden-trove",
                "dc_tft.gif",
                "The Forbidden Trove",
                "https://discord.gg/tftrove",
            ))
            .child(community::<S>(
                settings,
                "about-reddit",
                "dc_reddit.png",
                "r/pathofexile",
                "https://discord.gg/pathofexile",
            )),
    };
    page.child(footer.child(communities))
}

/// The action a state's button runs.
type Action = fn(&mut Settings, &mut rune::Tx);

/// `info`: the two lines and the optional action (`about.vue`'s computed).
fn update_state(settings: &Settings) -> (String, String, Option<(&'static str, String, Action)>) {
    let check = t_key(settings, "updates.check_now");
    let downloads = t_key(settings, "updates.downloads_page");
    let check_action: Option<(&'static str, String, Action)> =
        Some(("about-check-now-label", check, check_now as Action));
    match &settings.update {
        UpdateInfo::Initial => {
            (t_key(settings, "updates.maybe_outdated"), t_key(settings, "updates.never_checked"), check_action)
        }
        UpdateInfo::CheckingForUpdate => (t_key(settings, "updates.checking"), t_key(settings, "please_wait"), None),
        UpdateInfo::UpdateNotAvailable { checked_at } => {
            let now = crate::trade::now(settings.mode);
            let when = wraex_core::relative_time::to_relative_short(*checked_at as i64, now as i64);
            (t_key(settings, "updates.latest"), tr_fmt(settings, "updates.last_checked", &[&when]), check_action)
        }
        UpdateInfo::Error { .. } => (
            t_key(settings, "updates.maybe_outdated"),
            t_key(settings, "updates.error"),
            Some(("about-downloads-label", downloads, open_downloads as Action)),
        ),
        UpdateInfo::UpdateDownloaded { version } => (
            tr_fmt(settings, "updates.available", &[version]),
            t_key(settings, "updates.installed_on_exit"),
            // `quitAndInstall`: the download is verified, so the button restarts into it.
            Some(("about-install-label", t_key(settings, "updates.install_now"), install_now as Action)),
        ),
        UpdateInfo::UpdateAvailable { version, no_download_reason } => (
            tr_fmt(settings, "updates.available", &[version]),
            match no_download_reason {
                Some(NoDownloadReason::NotSupported) => t_key(settings, "updates.download_manually"),
                Some(NoDownloadReason::DisabledByFlag) => t_key(settings, "updates.download_disabled"),
                None => t_key(settings, "updates.downloading"),
            },
            Some(("about-downloads-label", downloads, open_downloads as Action)),
        ),
    }
}

/// `checkForUpdates`: [`UpdateInfo::CheckingForUpdate`] shows at once. A live window asks the
/// shell's worker (see [`Settings::begin_update_check`]); a scene's check lands in
/// [`Settings::poll`].
fn check_now(settings: &mut Settings, _tx: &mut rune::Tx) {
    settings.begin_update_check();
}

/// The "Install now" button: `autoUpdater.quitAndInstall(false)`, which the shell runs from
/// [`Settings::install_requested`]. A scene only sets the flag.
fn install_now(settings: &mut Settings, _tx: &mut rune::Tx) {
    settings.install_requested = true;
}

/// `openDownloadPage`: the Wraex releases page, for a copy that cannot install the update
/// itself (a failed check, a portable build, or `--no-updates`).
fn open_downloads(_settings: &mut Settings, tx: &mut rune::Tx) {
    let _ = tx.open_url(RELEASES);
}

/// An underlined link (`border-b`), opened by pressing it.
fn link<S: 'static>(key: &str, label: &str, url: &'static str) -> El {
    row()
        .key(key.to_owned())
        .border_sides([0.0, 0.0, 1.0, 0.0], theme::gray(400))
        .border_layout()
        .focusable(true)
        .role("link")
        .label(label.to_owned())
        .on_press_tx(move |_s: &mut S, tx: &mut rune::Tx| {
            let _ = tx.open_url(url);
        })
        .child(text(label.to_owned()).key(format!("{key}-label")).font(type_::SM))
}

/// `<li><img class="rounded inline" src="/images/…"> <a class="border-b">…</a></li>`: a
/// community's 32 pt icon beside its link.
fn community<S: 'static>(settings: &Settings, key: &str, icon: &str, label: &str, url: &'static str) -> El {
    row()
        .key(key.to_owned())
        .gap(theme::rem(0.25))
        .items_center()
        .child(ui::art::local(settings.game, icon, theme::rem(2.0)).key(format!("{key}-icon")))
        .child(link::<S>(&format!("{key}-link"), label, url))
}
