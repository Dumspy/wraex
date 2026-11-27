//! The OS clipboard behind [`wraex_platform::clipboard::Clipboard`].
//!
//! [`wraex_platform::clipboard`] ports `HostClipboard.ts` as a polling state machine and
//! reads the OS clipboard once per scheduled capture deadline.
//!
//! `arboard` is the same crate Rune uses for its own clipboard on Windows and Linux, so this
//! adds no new platform code. Its `Clipboard` is not `Sync`, so each thread borrows it.

use wraex_platform::clipboard::Clipboard;

/// The system clipboard. A read that fails (no clipboard, no text, a locked one) is the empty
/// string, as Electron's `readText` is.
#[derive(Debug, Default)]
pub struct SystemClipboard;

impl SystemClipboard {
    pub const fn new() -> Self {
        Self
    }
}

impl Clipboard for SystemClipboard {
    fn read_text(&mut self) -> String {
        match arboard::Clipboard::new() {
            Ok(mut clipboard) => clipboard.get_text().unwrap_or_default(),
            Err(error) => {
                wraex_platform::log!("wraex: the clipboard is unavailable: {error}");
                String::new()
            }
        }
    }

    fn write_text(&mut self, text: &str) {
        match arboard::Clipboard::new() {
            Ok(mut clipboard) => {
                if let Err(error) = clipboard.set_text(text) {
                    wraex_platform::log!("wraex: writing to the clipboard failed: {error}");
                }
            }
            Err(error) => wraex_platform::log!("wraex: the clipboard is unavailable: {error}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wraex_core::GameId;
    use wraex_core::game_files::Os;
    use wraex_core::trade::api::Clock;
    use wraex_platform::clipboard::{HostClipboard, is_poe_item};

    /// A clock whose `sleep` is a no-op, so the poll loop runs at once.
    #[derive(Debug)]
    struct Instant;

    impl Clock for Instant {
        fn now(&self) -> u64 {
            0
        }

        fn sleep(&self, _millis: u64) {}
    }

    /// The real clipboard, written and read back. Ignored by default: it takes the user's
    /// clipboard, which a test suite should not do.
    #[test]
    #[ignore = "takes the user's clipboard"]
    fn text_round_trips() {
        let mut clipboard = SystemClipboard::new();
        clipboard.write_text("Rarity: Currency\nDivine Orb\n");
        assert!(is_poe_item(&clipboard.read_text(), GameId::Poe1));
        // And the platform state machine reads it as the item a copy produced.
        let mut host = HostClipboard::new(GameId::Poe1, Os::MacOs);
        let item = clipboard.read_text();
        host.begin_capture(&mut clipboard, &Instant, || {});
        clipboard.write_text(&item);
        let text = host.poll_capture(&mut clipboard, host.capture_deadline().unwrap()).unwrap().unwrap();
        assert_eq!(text, "Rarity: Currency\nDivine Orb\n");
    }
}
