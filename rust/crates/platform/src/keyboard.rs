//! Submit one Win32 keyboard event and verify Windows inserted it.

#[cfg(any(windows, test))]
fn insert_one(send: impl FnOnce() -> u32) -> std::io::Result<()> {
    match send() {
        1 => Ok(()),
        _ => Err(std::io::Error::other("SendInput did not insert the keyboard event")),
    }
}

#[cfg(windows)]
pub fn send_key(vk: u16, extended: bool, down: bool) -> std::io::Result<()> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP,
        SendInput, VIRTUAL_KEY,
    };
    let mut flags = KEYBD_EVENT_FLAGS::default();
    if extended {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    if !down {
        flags |= KEYEVENTF_KEYUP;
    }
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VIRTUAL_KEY(vk), dwFlags: flags, ..Default::default() } },
    };
    // SAFETY: the generated INPUT contains its active keyboard variant and lives through the call.
    insert_one(|| unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) })
}

#[cfg(test)]
mod tests {
    #[test]
    fn rejected_keyboard_event_returns_an_error() {
        assert!(super::insert_one(|| 0).is_err());
        assert!(super::insert_one(|| 1).is_ok());
    }
}
