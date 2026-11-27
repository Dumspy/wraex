//! Reading the clipboard, and restoring what was there after a copy.
//!
//! Ported from `main/src/shortcuts/HostClipboard.ts`, which differs between the
//! references only in details this module selects by [`GameId`]:
//!
//! - the placeholder written to an empty clipboard on Linux
//!   (`__APT_FORCE_EMPTY_` for PoE1, `__EE2_FORCE_EMPTY_` for PoE2),
//! - EE2's `initialDelay` (`updateDelay`), the wait before the first poll;
//!   `POLL_DELAY` is the wait between polls in both,
//! - EE2's item detector also accepts an uncut skill gem's first line
//!   (`Rarity: …`) and knows Japanese.
//!
//! What it does, in upstream's order: snapshot the clipboard, blank it so an
//! unchanged clipboard cannot be mistaken for a fresh copy, let the caller press
//! the game's copy key, then poll every [`POLL_DELAY`] ms for up to
//! [`POLL_LIMIT`] ms. The previous contents are restored afterwards when
//! `restoreClipboard` is on — after [`RESTORE_AFTER`] ms for text the app wrote
//! itself ([`HostClipboard::restore_shortly`]), which doubles as a throttle
//! against "Too many actions" disconnects.
//!
//! The clock is [`wraex_core::trade::api::Clock`], the same trait the trade client
//! waits with, so the app has one clock to implement. Upstream's
//! `setTimeout(RESTORE_AFTER)` is a [`HostClipboard::pending_restore_deadline`] the
//! app's timer fires, not a blocking sleep: the throttle has to be visible to the
//! next hotkey press.
//!
//! What stays OS-specific: reading and writing the clipboard itself. PLAN.md
//! suggests `arboard`, or Rune's `Tx::copy` for the write side; either has to be
//! checked on the phase 2 spike (the Linux/Proton workarounds below only make
//! sense against the real thing). Clipboard ownership on X11/Wayland differs from
//! Win32 and macOS, so the [`Clipboard`] implementation is per platform.

use wraex_core::GameId;
use wraex_core::game_files::Os;
use wraex_core::trade::api::Clock;
use wraex_core::trade::rate_limit::Millis;

/// The wait before each clipboard poll, in milliseconds.
pub const POLL_DELAY: Millis = 48;
/// How long polling lasts, in milliseconds.
pub const POLL_LIMIT: Millis = 500;
/// PoE must read the clipboard within this time; after it, the app restores what
/// was there. If the game lagged, it would read the restored clipboard, which may
/// hold a password.
pub const RESTORE_AFTER: Millis = 120;

/// The platform's clipboard (`Electron.Clipboard`). `readText` of a clipboard
/// without text is the empty string, like Electron's.
pub trait Clipboard: std::fmt::Debug {
    fn read_text(&mut self) -> String;
    fn write_text(&mut self, text: &str);
}

/// Why a read ended without item text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadError {
    /// The poll reached [`POLL_LIMIT`] (upstream's `Reading clipboard timed out`,
    /// logged as `[ClipboardPoller] No item text found.`).
    Timeout,
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Reading clipboard timed out")
    }
}

impl std::error::Error for ReadError {}

/// The restore that `setTimeout(RESTORE_AFTER)` will write back.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingRestore {
    /// The clipboard's text when the app started writing to it.
    saved: String,
    due: Millis,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Capture {
    saved: String,
    next_poll: Millis,
    timeout: Millis,
}

/// `HostClipboard.ts`: the polling read, and the short-lived restore.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostClipboard {
    game: GameId,
    os: Os,
    should_restore: bool,
    initial_delay: Millis,
    /// `this.pollPromise != null`: a read is in flight.
    capture: Option<Capture>,
    /// `this.isRestored`: no restore is pending, so the next
    /// [`Self::restore_shortly`] may run.
    is_restored: bool,
    pending_restore: Option<PendingRestore>,
}

impl HostClipboard {
    /// `new HostClipboard(logger)`, plus the game (the Linux placeholder and the
    /// item detector's language table) and the OS (the Linux workarounds).
    pub fn new(game: GameId, os: Os) -> Self {
        Self {
            game,
            os,
            should_restore: false,
            initial_delay: POLL_DELAY,
            capture: None,
            is_restored: true,
            pending_restore: None,
        }
    }

    /// `updateOptions(restoreClipboard)`: whether the previous clipboard comes
    /// back after a copy.
    pub fn update_options(&mut self, restore_clipboard: bool) {
        self.should_restore = restore_clipboard;
    }

    /// EE2's `updateDelay(initialDelay)`: the wait before the first poll. PoE1
    /// always uses [`POLL_DELAY`].
    pub fn update_delay(&mut self, initial_delay: Millis) {
        self.initial_delay = initial_delay;
    }

    /// `isPolling`.
    pub fn is_polling(&self) -> bool {
        self.capture.is_some()
    }

    /// `shouldRestore`.
    pub fn should_restore(&self) -> bool {
        self.should_restore
    }

    /// The Linux placeholder written over the clipboard,
    /// `__{APT,EE2}_FORCE_EMPTY_{Date.now()}`.
    fn force_empty(&self, clock: &dyn Clock) -> String {
        let marker = match self.game {
            GameId::Poe1 => "APT",
            GameId::Poe2 => "EE2",
        };
        format!("__{marker}_FORCE_EMPTY_{}", clock.now())
    }

    pub fn capture_deadline(&self) -> Option<Millis> {
        self.capture.as_ref().map(|capture| capture.next_poll)
    }

    /// Snapshot before sending the copy chord. A pending capture owns the clipboard.
    pub fn begin_capture(&mut self, clipboard: &mut dyn Clipboard, clock: &dyn Clock, press_keys: impl FnOnce()) {
        if self.is_polling() {
            return;
        }
        let mut text_before = clipboard.read_text();
        if is_poe_item(&text_before, self.game) {
            // An item from the previous copy is not the previous clipboard.
            text_before.clear();
            if self.os != Os::Linux {
                clipboard.write_text("");
            } else {
                // Workaround for KDE's "Prevent empty clipboard": Proton then
                // reads a changed clipboard, not the item's text.
                clipboard.write_text(&self.force_empty(clock));
            }
        } else if self.os == Os::Linux {
            // Workaround for Proton 10+, which ignores an unchanged clipboard.
            clipboard.write_text(&self.force_empty(clock));
        }

        press_keys();

        let next_poll = clock.now().saturating_add(self.initial_delay);
        let timeout = next_poll.saturating_add((POLL_LIMIT.div_ceil(POLL_DELAY) - 1) * POLL_DELAY);
        self.capture = Some(Capture { saved: text_before, next_poll, timeout });
    }

    /// One clipboard read per deadline. Returning None leaves the event loop free until the next poll.
    pub fn poll_capture(&mut self, clipboard: &mut dyn Clipboard, now: Millis) -> Option<Result<String, ReadError>> {
        let capture = self.capture.as_mut()?;
        if now < capture.next_poll {
            return None;
        }
        let text = clipboard.read_text();
        let result = if is_poe_item(&text, self.game) {
            Ok(text)
        } else if now >= capture.timeout {
            Err(ReadError::Timeout)
        } else {
            capture.next_poll = now.saturating_add(POLL_DELAY).min(capture.timeout);
            return None;
        };
        if self.should_restore {
            clipboard.write_text(&capture.saved);
        }
        self.capture = None;
        Some(result)
    }

    /// `restoreShortly(cb)`: run `write` with the clipboard, then restore what was
    /// there [`RESTORE_AFTER`] ms later. While that restore is pending, this
    /// returns `None` and does not run `write` at all — the throttle against
    /// disconnects from "Too many actions". When `restoreClipboard` is off it still
    /// throttles, it just puts nothing back.
    pub fn restore_shortly<T>(
        &mut self,
        clipboard: &mut dyn Clipboard,
        clock: &dyn Clock,
        write: impl FnOnce(&mut dyn Clipboard) -> T,
    ) -> Option<T> {
        self.apply_pending_restore(clipboard, clock);
        if !self.is_restored || self.is_polling() {
            return None;
        }

        self.is_restored = false;
        let saved = clipboard.read_text();
        let result = write(clipboard);
        self.pending_restore = Some(PendingRestore { saved, due: clock.now() + RESTORE_AFTER });
        Some(result)
    }

    /// When the app's timer should call [`Self::apply_pending_restore`].
    pub fn pending_restore_deadline(&self) -> Option<Millis> {
        self.pending_restore.as_ref().map(|p| p.due)
    }

    /// The `setTimeout(..., RESTORE_AFTER)` body. `false` when nothing was due
    /// (no pending restore, or the timer came early); call it again at the
    /// deadline from [`Self::pending_restore_deadline`].
    pub fn apply_pending_restore(&mut self, clipboard: &mut dyn Clipboard, clock: &dyn Clock) -> bool {
        let Some(pending) = self.pending_restore.as_ref() else {
            return false;
        };
        if clock.now() < pending.due {
            return false;
        }

        let pending = self.pending_restore.take().expect("checked above");
        // `shouldRestore` is read here, not when the restore was scheduled.
        if self.should_restore {
            clipboard.write_text(&pending.saved);
        }
        self.is_restored = true;
        true
    }
}

/// `isPoeItem`: does the text start like an item copied from the game? The
/// language table also tells which client the text came from.
pub fn detect_item_language(text: &str, game: GameId) -> Option<&'static str> {
    match game {
        GameId::Poe1 => POE1_ITEM_LINES.iter().find(|(_, line)| text.starts_with(*line)).map(|(lang, _)| *lang),
        GameId::Poe2 => POE2_ITEM_LINES
            .iter()
            .find(|(_, line, gem)| text.starts_with(*line) || text.starts_with(*gem))
            .map(|(lang, _, _)| *lang),
    }
}

/// [`detect_item_language`]'s yes/no.
pub fn is_poe_item(text: &str, game: GameId) -> bool {
    detect_item_language(text, game).is_some()
}

/// `LANGUAGE_DETECTOR` of `ref/apt`: `(language, first line)`.
const POE1_ITEM_LINES: [(&str, &str); 10] = [
    ("en", "Item Class: "),
    ("ru", "Класс предмета: "),
    ("fr", "Classe d'objet: "),
    ("de", "Gegenstandsklasse: "),
    ("pt", "Classe do Item: "),
    ("es", "Clase de objeto: "),
    ("th", "ชนิดไอเทม: "),
    ("ko", "아이템 종류: "),
    ("cmn-Hant", "物品種類: "),
    ("cmn-Hans", "物品类别: "),
];

/// `LANGUAGE_DETECTOR` of `ref/ee2`: `(language, first line, uncut skill gem
/// line)`. PoE2's uncut skill gems are copied without an `Item Class:` line.
const POE2_ITEM_LINES: [(&str, &str, &str); 11] = [
    ("en", "Item Class: ", "Rarity: "),
    ("ru", "Класс предмета: ", "Редкость: "),
    ("fr", "Classe d'objet: ", "Rareté: "),
    ("de", "Gegenstandsklasse: ", "Seltenheit: "),
    ("pt", "Classe do Item: ", "Raridade: "),
    ("es", "Clase de objeto: ", "Rareza: "),
    ("th", "ชนิดไอเทม: ", "Rarity: "),
    ("ko", "아이템 종류: ", "아이템 희귀도: "),
    ("cmn-Hant", "物品種類: ", "稀有度: "),
    ("cmn-Hans", "物品类别: ", "Rarity: "),
    ("ja", "アイテムクラス: ", "レアリティ: "),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{FakeClipboard, FakeClock};

    const ITEM: &str = "Item Class: Rings\nRarity: Rare\nFoo";
    const OTHER: &str = "Item Class: Rings\nRarity: Rare\nBar";

    fn copy(
        host: &mut HostClipboard,
        clipboard: &mut FakeClipboard,
        clock: &dyn Clock,
        presses: &mut usize,
    ) -> Result<String, ReadError> {
        host.begin_capture(clipboard, clock, || *presses += 1);
        loop {
            clock.sleep(host.capture_deadline().unwrap().saturating_sub(clock.now()));
            if let Some(result) = host.poll_capture(clipboard, clock.now()) {
                return result;
            }
        }
    }

    #[test]
    fn an_item_after_the_copy_key_is_returned_and_the_clipboard_restored() {
        let mut host = HostClipboard::new(GameId::Poe1, Os::Windows);
        host.update_options(true);
        let mut clipboard = FakeClipboard::scripted(&["my password", ITEM]);
        let clock = FakeClock::default();
        let mut presses = 0;

        assert_eq!(copy(&mut host, &mut clipboard, &clock, &mut presses), Ok(ITEM.to_owned()));
        assert_eq!(presses, 1, "the copy keys go out before the first poll");
        // Nothing was blanked (the text was not an item), the item reads on the
        // first poll, then the password comes back.
        assert_eq!(clipboard.written(), vec!["my password"]);
        assert_eq!(clock.elapsed(), POLL_DELAY, "one poll");
        assert!(!host.is_polling());
    }

    #[test]
    fn an_item_already_on_the_clipboard_is_not_the_previous_clipboard() {
        let mut host = HostClipboard::new(GameId::Poe1, Os::Windows);
        host.update_options(true);
        let mut clipboard = FakeClipboard::scripted(&[OTHER, ITEM]);
        let clock = FakeClock::default();
        let mut presses = 0;

        assert_eq!(copy(&mut host, &mut clipboard, &clock, &mut presses), Ok(ITEM.to_owned()));
        // The stale item blanked the clipboard, and the blank string is what the
        // restore puts back — never the old item.
        assert_eq!(clipboard.written(), vec!["", ""]);
    }

    #[test]
    fn on_linux_the_clipboard_gets_the_force_empty_placeholder() {
        let mut host = HostClipboard::new(GameId::Poe1, Os::Linux);
        let mut clipboard = FakeClipboard::scripted(&["my password", ITEM]);
        let clock = FakeClock::default();
        let mut presses = 0;

        let _ = copy(&mut host, &mut clipboard, &clock, &mut presses);
        assert_eq!(clipboard.written(), vec!["__APT_FORCE_EMPTY_0"]);

        let mut host = HostClipboard::new(GameId::Poe2, Os::Linux);
        let mut clipboard = FakeClipboard::scripted(&["my password", ITEM]);
        let clock = FakeClock::default();
        let _ = copy(&mut host, &mut clipboard, &clock, &mut presses);
        assert_eq!(clipboard.written(), vec!["__EE2_FORCE_EMPTY_0"]);
    }

    #[test]
    fn a_non_item_clipboard_is_blanked_only_where_the_game_needs_it() {
        // Proton 10+ ignores an unchanged clipboard, so the app always writes.
        let mut host = HostClipboard::new(GameId::Poe1, Os::Linux);
        let mut clipboard = FakeClipboard::scripted(&["my password", ITEM]);
        let clock = FakeClock::default();
        let mut presses = 0;
        let _ = copy(&mut host, &mut clipboard, &clock, &mut presses);
        assert_eq!(clipboard.written()[0], "__APT_FORCE_EMPTY_0");

        // On Windows a non-item clipboard is left alone, and with
        // `restoreClipboard` off nothing is put back either.
        let mut host = HostClipboard::new(GameId::Poe1, Os::Windows);
        let mut clipboard = FakeClipboard::scripted(&["my password", ITEM]);
        let mut presses = 0;
        let _ = copy(&mut host, &mut clipboard, &clock, &mut presses);
        assert_eq!(clipboard.written(), Vec::<String>::new());
    }

    #[test]
    fn polling_stops_after_the_limit_and_restores_the_clipboard() {
        let mut host = HostClipboard::new(GameId::Poe1, Os::Windows);
        host.update_options(true);
        let mut clipboard = FakeClipboard::scripted(&["my password", "still not an item"]);
        let clock = FakeClock::default();
        let mut presses = 0;

        assert_eq!(copy(&mut host, &mut clipboard, &clock, &mut presses), Err(ReadError::Timeout));
        // Polls at 48, 96, ... 528 ms: eleven reads, the last one already past
        // `POLL_LIMIT`.
        assert_eq!(clock.elapsed(), 11 * POLL_DELAY);
        assert_eq!(clipboard.written(), vec!["my password"]);
        assert!(!host.is_polling());
    }

    #[test]
    fn ee2_waits_its_initial_delay_but_keeps_polling_every_poll_delay() {
        let mut host = HostClipboard::new(GameId::Poe2, Os::Windows);
        host.update_delay(100);
        let mut clipboard = FakeClipboard::scripted(&["", ITEM]);
        let clock = FakeClock::default();
        let mut presses = 0;

        assert_eq!(copy(&mut host, &mut clipboard, &clock, &mut presses), Ok(ITEM.to_owned()));
        assert_eq!(clock.elapsed(), 100, "the first wait is `initialDelay`");

        let mut clipboard = FakeClipboard::scripted(&["", "", ""]);
        let clock = FakeClock::default();
        let mut presses = 0;
        assert_eq!(copy(&mut host, &mut clipboard, &clock, &mut presses), Err(ReadError::Timeout));
        assert_eq!(clock.elapsed(), 100 + 10 * POLL_DELAY, "then 48 ms per poll");
    }

    #[test]
    fn restore_shortly_puts_the_previous_text_back_after_the_delay() {
        let mut host = HostClipboard::new(GameId::Poe1, Os::Windows);
        host.update_options(true);
        let mut clipboard = FakeClipboard::scripted(&["a password"]);
        let clock = FakeClock::default();

        let wrote = host.restore_shortly(&mut clipboard, &clock, |clipboard| clipboard.write_text("/hideout"));
        assert_eq!(wrote, Some(()));
        assert_eq!(clipboard.written(), vec!["/hideout"]);
        assert_eq!(host.pending_restore_deadline(), Some(RESTORE_AFTER));

        // The timer has not fired yet.
        assert!(!host.apply_pending_restore(&mut clipboard, &clock));
        clock.advance(RESTORE_AFTER - 1);
        assert!(!host.apply_pending_restore(&mut clipboard, &clock));
        clock.advance(1);
        assert!(host.apply_pending_restore(&mut clipboard, &clock));
        assert_eq!(clipboard.written(), vec!["/hideout", "a password"]);
        assert_eq!(host.pending_restore_deadline(), None);
        // Nothing left to do, and no repeat write.
        assert!(!host.apply_pending_restore(&mut clipboard, &clock));
    }

    #[test]
    fn a_pending_restore_throttles_the_next_call() {
        let mut host = HostClipboard::new(GameId::Poe1, Os::Windows);
        let mut clipboard = FakeClipboard::scripted(&["a password"]);
        let clock = FakeClock::default();

        assert_eq!(host.restore_shortly(&mut clipboard, &clock, |cb| cb.write_text("first")), Some(()));
        // "Too many actions": the second call neither writes nor sends keys.
        assert_eq!(host.restore_shortly(&mut clipboard, &clock, |cb| cb.write_text("second")), None);
        assert_eq!(clipboard.written(), vec!["first"]);

        clock.advance(RESTORE_AFTER);
        assert!(host.apply_pending_restore(&mut clipboard, &clock));
        assert_eq!(host.restore_shortly(&mut clipboard, &clock, |cb| cb.write_text("third")), Some(()));
        assert_eq!(clipboard.written(), vec!["first", "third"], "restoreClipboard is off");
    }

    #[test]
    fn without_restore_the_short_restore_still_throttles() {
        let mut host = HostClipboard::new(GameId::Poe1, Os::Windows);
        let mut clipboard = FakeClipboard::scripted(&["a password"]);
        let clock = FakeClock::default();

        host.restore_shortly(&mut clipboard, &clock, |cb| cb.write_text("/hideout"));
        clock.advance(RESTORE_AFTER);
        assert!(host.apply_pending_restore(&mut clipboard, &clock));
        // The user's text stays: only the throttle window passed.
        assert_eq!(clipboard.written(), vec!["/hideout"]);
    }

    #[test]
    fn items_are_recognised_by_the_first_line_of_each_language() {
        assert_eq!(detect_item_language(ITEM, GameId::Poe1), Some("en"));
        assert_eq!(detect_item_language("Класс предмета: Кольца\n", GameId::Poe1), Some("ru"));
        assert_eq!(detect_item_language("아이템 종류: 반지\n", GameId::Poe2), Some("ko"));
        assert_eq!(detect_item_language("my password", GameId::Poe1), None);
        assert_eq!(detect_item_language("my password", GameId::Poe2), None);
    }

    #[test]
    fn only_poe2_accepts_an_uncut_skill_gem_line() {
        // PoE2 copies uncut skill gems without an `Item Class:` line.
        assert!(!is_poe_item("Rarity: Uncut Skill Gem\n", GameId::Poe1));
        assert!(is_poe_item("Rarity: Uncut Skill Gem\n", GameId::Poe2));
        assert!(is_poe_item("レアリティ: アンカット・スキルジェム\n", GameId::Poe2), "EE2 knows Japanese");
        assert!(!is_poe_item("レアリティ: アンカット・スキルジェム\n", GameId::Poe1));
    }
}
