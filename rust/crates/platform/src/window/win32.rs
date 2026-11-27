//! The Win32 half of the window hook: what `electron-overlay-window`'s native backend does,
//! written from its documented behaviour (README, its `windows.c`) rather than ported code.
//!
//! One dedicated thread owns every hook and a message loop, because out-of-context
//! `SetWinEventHook` callbacks arrive as window messages to the thread that installed them:
//!
//! - `EVENT_SYSTEM_FOREGROUND` and `EVENT_SYSTEM_MINIMIZEEND` (system-wide): *some* window
//!   came to the foreground. Windows reports these optimistically while it switches, so the
//!   report is only trusted when the window really is the active one (see
//!   `is_really_focused`), as the reference does with an MSAA check.
//! - An 83 ms `SetTimer` that re-reads `GetForegroundWindow`: `ForegroundLockTimeout` (and
//!   applications that steal the foreground) can move the foreground without an event.
//! - `EVENT_OBJECT_NAMECHANGE` on the foreground window: the game's title bar changes once
//!   it is ready, and a window that was already open when Wraex started is found this way.
//! - `EVENT_OBJECT_LOCATIONCHANGE` and `EVENT_OBJECT_DESTROY` on the found window's thread:
//!   moves, resizes (including borderless-windowed and fullscreen changes) and the window
//!   going away, after which the search starts over.
//! - A `WH_MOUSE_LL` low-level mouse hook on the same thread: every mouse move, button press
//!   and wheel notch, in physical screen pixels (the half of `uiohook-napi` the overlay and the
//!   stash scroll use). It only observes: the callback queues the event and calls
//!   `CallNextHookEx`, so the game never waits on it.
//! - A `WH_KEYBOARD_LL` low-level keyboard hook on the same thread: every key's down and up with
//!   the modifiers held, which is `OverlayVisibility`'s Alt peek and `Shortcuts`' key log. Keys
//!   Wraex synthesizes are flagged injected and skipped. Observe-only, like the mouse hook.
//!
//! The reported rectangle is the *client* area (`GetClientRect` + `ClientToScreen`), which is
//! the whole window in borderless-windowed and fullscreen and the content below the title bar
//! otherwise — the reference's `get_content_bounds`.
//!
//! An integrity-level (UIPI) probe tells the host the game runs elevated: a `PostMessage` of
//! a registered message fails with `ERROR_ACCESS_DENIED` across integrity levels, and
//! upstream shows "PoE window - No access" in exactly that case.

// The whole module is Win32 FFI: every call here is an `unsafe` extern function.
#![expect(unsafe_code, reason = "Win32 FFI: window tracking and low-level hooks have no safe API")]

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use windows::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_SUCCESS, GetLastError, HWND, LPARAM, POINT, RECT, SetLastError, WPARAM,
};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VIRTUAL_KEY, VK_CONTROL, VK_MENU, VK_SHIFT};
use windows::Win32::UI::WindowsAndMessaging::{
    CHILDID_SELF, CallNextHookEx, DispatchMessageW, EVENT_OBJECT_DESTROY, EVENT_OBJECT_LOCATIONCHANGE,
    EVENT_OBJECT_NAMECHANGE, EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_MINIMIZEEND, GA_ROOT, GUITHREADINFO, GWL_EXSTYLE,
    GetAncestor, GetClassNameW, GetClientRect, GetForegroundWindow, GetGUIThreadInfo, GetMessageW, GetWindowLongPtrW,
    GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, HHOOK, IsHungAppWindow, IsIconic, IsWindowVisible,
    KBDLLHOOKSTRUCT, KillTimer, LLKHF_INJECTED, MSG, MSLLHOOKSTRUCT, OBJID_WINDOW, PM_NOREMOVE, PeekMessageW,
    PostMessageW, PostThreadMessageW, RegisterWindowMessageW, SW_HIDE, SW_SHOWNOACTIVATE, SWP_FRAMECHANGED,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetForegroundWindow, SetTimer, SetWindowLongPtrW,
    SetWindowPos, SetWindowsHookExW, ShowWindow, TranslateMessage, UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL,
    WINEVENT_OUTOFCONTEXT, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_QUIT,
    WM_RBUTTONDOWN, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_XBUTTONDOWN, WS_EX_TOOLWINDOW,
};
use windows::core::w;

use wraex_core::num::f64_to_i32;

use super::{
    Bounds, GameWindowEvents, KeyEvent, ModifierState, MouseEvent, Point, WakeFn, WatchError, WindowTarget,
    matches_target,
};

/// The reference's foreground timer: 12 times a second, which is what catches foreground
/// changes Windows swallowed (`ForegroundLockTimeout`, another app stealing the foreground).
const FOREGROUND_TIMER_MS: u32 = 83;

/// One reported window, as the hook thread keeps it.
struct HookThread {
    target: WindowTarget,
    events: Sender<GameWindowEvents>,
    /// The low-level mouse hook's queue: `uiohook`'s `mousemove`/`mousedown`/`wheel`.
    mouse: Sender<MouseEvent>,
    /// The low-level keyboard hook's queue: `uiohook`'s `keydown`/`keyup`.
    keys: Sender<KeyEvent>,
    overlay: Arc<Mutex<Option<isize>>>,
    /// The modifiers the keyboard hook has seen held, which the mouse hook's events carry too.
    modifiers: ModifierState,
    /// What the wheel's last ticks left short of a whole notch ([`super::wheel_rotation`]).
    wheel_leftover: i16,
    /// Move events are the bulk of the hook's traffic and are only wanted while a widget's
    /// area is tracked (upstream adds its `mousemove` listener for exactly that time); the
    /// flag keeps the hook from waking the app on every desktop mouse move.
    mouse_moves: Arc<AtomicBool>,
    wake: WakeFn,
    /// The found window, mirrored for [`super::focus_window`] (0 when there is none).
    game: Arc<Mutex<Option<isize>>>,
    /// The tracked window; null until one with the target's title is found.
    hwnd: HWND,
    /// Whether the tracked window currently has the foreground.
    is_focused: bool,
    /// `EVENT_OBJECT_DESTROY` arrived; the next foreground report emits `Detach`.
    is_destroyed: bool,
    /// The last window reported as the foreground (`foreground_window`).
    foreground: HWND,
    location_hook: HWINEVENTHOOK,
    destroy_hook: HWINEVENTHOOK,
    name_hook: HWINEVENTHOOK,
    mouse_hook: HHOOK,
    keyboard_hook: HHOOK,
    uipi_message: u32,
}

thread_local! {
    /// The hook thread's own state. `SetWinEventHook` takes no user data, so the callback
    /// finds it here; out-of-context hooks run on the thread that installed them.
    static HOOK: RefCell<Option<HookThread>> = const { RefCell::new(None) };
}

/// Runs `f` with the hook thread's state, if this is that thread.
///
/// A `try_borrow_mut` rather than a `borrow_mut`: a hook callback cannot arrive while another
/// one runs (they are delivered on this thread's message loop), but a re-entrant delivery must
/// not panic the hook thread if one ever does.
fn with_hook(f: impl FnOnce(&mut HookThread)) {
    HOOK.with(|cell| {
        if let Ok(mut hook) = cell.try_borrow_mut()
            && let Some(hook) = hook.as_mut()
        {
            f(hook);
        }
    });
}

impl HookThread {
    /// Queues one report and wakes the app. A disconnected receiver means the app dropped
    /// the watcher; the thread is already on its way out then.
    fn emit(&self, event: GameWindowEvents) {
        if self.events.send(event).is_ok() {
            (self.wake)();
        }
    }

    /// Queues one mouse event. The hook callback does nothing else: no blocking, no input
    /// injection, and no work that the message loop would have to wait for.
    fn emit_mouse(&self, event: MouseEvent) {
        if self.mouse.send(event).is_ok() {
            (self.wake)();
        }
    }

    /// Queues one key event, with the same observe-only contract as [`Self::emit_mouse`].
    fn emit_key(&self, event: KeyEvent) {
        if self.keys.send(event).is_ok() {
            (self.wake)();
        }
    }

    fn unhook_target(&mut self) {
        if !self.location_hook.is_invalid() {
            // SAFETY: `location_hook` was returned by `SetWinEventHook` on this thread and is
            // unhooked once.
            _ = unsafe { UnhookWinEvent(self.location_hook) };
            self.location_hook = HWINEVENTHOOK::default();
        }
        if !self.destroy_hook.is_invalid() {
            // SAFETY: `destroy_hook` came from the thread's own `SetWinEventHook`.
            _ = unsafe { UnhookWinEvent(self.destroy_hook) };
            self.destroy_hook = HWINEVENTHOOK::default();
        }
    }

    /// The reference's `handle_new_foreground`: a new window has the foreground (or the
    /// title of the foreground window changed). Follow it and look at it.
    fn handle_new_foreground(&mut self, hwnd: HWND) {
        self.foreground = hwnd;
        // The title of the *previous* foreground window is no longer interesting.
        if !self.name_hook.is_invalid() {
            // SAFETY: `name_hook` came from `SetWinEventHook` on this thread and is unhooked
            // once.
            _ = unsafe { UnhookWinEvent(self.name_hook) };
            self.name_hook = HWINEVENTHOOK::default();
        }
        // The game's title bar is filled in after the client window exists, and a game that
        // was already running when Wraex started is found through its next title change.
        if !hwnd.is_invalid() && hwnd != self.hwnd {
            // SAFETY: a plain Win32 query; the thread id is the window's own.
            let thread = unsafe { GetWindowThreadProcessId(hwnd, None) };
            // SAFETY: an out-of-context hook owned by this thread, unhooked above or at exit.
            self.name_hook = unsafe {
                SetWinEventHook(
                    EVENT_OBJECT_NAMECHANGE,
                    EVENT_OBJECT_NAMECHANGE,
                    None,
                    Some(event_proc),
                    0,
                    thread,
                    WINEVENT_OUTOFCONTEXT,
                )
            };
        }
        self.check_and_handle(hwnd);
    }

    /// The reference's `check_and_handle_window`: blur a window that lost the foreground,
    /// detach a destroyed one, focus the tracked one, or look for the target in this window.
    fn check_and_handle(&mut self, hwnd: HWND) {
        // "Ghost" windows of an unresponsive application can be reported as the foreground;
        // they are not the game.
        // SAFETY: a plain Win32 query.
        if !hwnd.is_invalid() && unsafe { IsHungAppWindow(hwnd) }.as_bool() {
            return;
        }

        if !self.hwnd.is_invalid() {
            if self.hwnd == hwnd {
                // Found (again): the host only needs to hear about a change of focus.
                if !self.is_focused {
                    self.is_focused = true;
                    self.emit(GameWindowEvents::Focus);
                }
                return;
            }
            if self.is_focused {
                self.is_focused = false;
                self.emit(GameWindowEvents::Blur);
            }
            if self.is_destroyed {
                self.unhook_target();
                self.hwnd = HWND::default();
                self.is_destroyed = false;
                if let Ok(mut game) = self.game.lock() {
                    *game = None;
                }
                self.emit(GameWindowEvents::Detach);
            }
        }

        // Not the tracked window: is it the game's? (`strcmp` on the title in the reference.)
        let Some(title) = window_title(hwnd) else { return };
        let class = window_class(hwnd);
        if !matches_target(&self.target, &title, class.as_deref()) {
            return;
        }

        self.unhook_target();
        self.hwnd = hwnd;
        self.is_destroyed = false;
        // SAFETY: a plain Win32 query; the thread id is the window's own.
        let thread = unsafe { GetWindowThreadProcessId(hwnd, None) };
        // SAFETY: an out-of-context hook owned by this thread; `unhook_target` releases it.
        self.location_hook = unsafe {
            SetWinEventHook(
                EVENT_OBJECT_LOCATIONCHANGE,
                EVENT_OBJECT_LOCATIONCHANGE,
                None,
                Some(event_proc),
                0,
                thread,
                WINEVENT_OUTOFCONTEXT,
            )
        };
        // SAFETY: the same, for the window's destruction.
        self.destroy_hook = unsafe {
            SetWinEventHook(
                EVENT_OBJECT_DESTROY,
                EVENT_OBJECT_DESTROY,
                None,
                Some(event_proc),
                0,
                thread,
                WINEVENT_OUTOFCONTEXT,
            )
        };
        let has_access = has_uipi_access(hwnd, self.uipi_message);
        crate::log!(
            "wraex: following window \"{title}\"{} (access: {})",
            class.map(|class| format!(" of class \"{class}\"")).unwrap_or_default(),
            if has_access { "yes" } else { "no, the game may run elevated" }
        );
        self.emit(GameWindowEvents::Attach { has_access: Some(has_access) });
        // The window died right after it came to the foreground: wait for the next one.
        let Some(bounds) = client_bounds(hwnd) else {
            self.unhook_target();
            self.hwnd = HWND::default();
            return;
        };
        if let Ok(mut game) = self.game.lock() {
            *game = Some(hwnd_address(hwnd));
        }
        // A minimised window has no client area to cover; the restore reports it again.
        if bounds.width > 0.0 && bounds.height > 0.0 {
            self.emit(GameWindowEvents::Bounds(bounds));
        }
        self.is_focused = true;
        self.emit(GameWindowEvents::Focus);
    }
}

/// The system's foreground hook callbacks. There is no user data, so the state comes
/// from [`HOOK`].
unsafe extern "system" fn event_proc(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _thread: u32,
    _time: u32,
) {
    with_hook(|hook| match event {
        EVENT_OBJECT_DESTROY => {
            if hwnd == hook.hwnd && id_object == OBJID_WINDOW.0 && id_child == CHILDID_SELF.cast_signed() {
                hook.is_destroyed = true;
                hook.check_and_handle(HWND::default());
            }
        }
        EVENT_OBJECT_LOCATIONCHANGE => {
            if hwnd == hook.hwnd && id_object == OBJID_WINDOW.0 && id_child == CHILDID_SELF.cast_signed() {
                // A minimised window's client rectangle is not where the game is drawn; the
                // restore reports the real one again (`EVENT_SYSTEM_MINIMIZEEND`).
                // SAFETY: a plain Win32 query.
                if unsafe { IsIconic(hwnd) }.as_bool() {
                    return;
                }
                if let Some(bounds) = drawn_bounds(hwnd) {
                    hook.emit(GameWindowEvents::Bounds(bounds));
                }
            }
        }
        EVENT_OBJECT_NAMECHANGE => {
            if hwnd == hook.foreground && id_object == OBJID_WINDOW.0 && id_child == CHILDID_SELF.cast_signed() {
                hook.check_and_handle(hook.foreground);
            }
        }
        EVENT_SYSTEM_FOREGROUND | EVENT_SYSTEM_MINIMIZEEND
            // Windows reports these while it is still switching; only a window that really
            // is active counts, or the host would focus/blur twice for one switch.
            if is_really_focused(hwnd) =>
        {
            hook.handle_new_foreground(hwnd);
        }
        _ => {}
    });
}

/// The reference's foreground timer: `GetForegroundWindow` can change without an event.
unsafe extern "system" fn foreground_timer(_hwnd: HWND, _message: u32, _id: usize, _time: u32) {
    with_hook(|hook| {
        // SAFETY: a plain Win32 query.
        let foreground = unsafe { GetForegroundWindow() };
        if foreground != hook.foreground && is_really_focused(foreground) {
            hook.handle_new_foreground(foreground);
        }
    });
}

/// The low-level mouse hook (`WH_MOUSE_LL`), the input half of `uiohook-napi` that
/// `WidgetAreaTracker.ts` and `Shortcuts.ts` use: every mouse move, button press and wheel
/// notch on the desktop, in physical screen pixels.
///
/// It only observes. The callback queues one small event and hands the message on with
/// [`CallNextHookEx`], so typing and aiming in the game are never delayed or blocked.
unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> windows::Win32::Foundation::LRESULT {
    if code >= 0 {
        // A low-level hook's `wParam` is the message id, which Windows widens to `WPARAM`.
        let message = u32::try_from(wparam.0).unwrap_or_default();
        // SAFETY: for `WH_MOUSE_LL` Windows passes a live `MSLLHOOKSTRUCT` in `lparam`.
        let info = unsafe { &*std::ptr::with_exposed_provenance::<MSLLHOOKSTRUCT>(lparam.0.cast_unsigned()) };
        let point = Point { x: f64::from(info.pt.x), y: f64::from(info.pt.y) };
        with_hook(|hook| match message {
            WM_MOUSEMOVE if hook.mouse_moves.load(Ordering::Relaxed) => {
                hook.emit_mouse(MouseEvent::Move { point, ctrl: hook.modifiers.ctrl(), alt: hook.modifiers.alt() });
            }
            WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN => {
                hook.emit_mouse(MouseEvent::Down { point });
            }
            WM_MOUSEWHEEL => {
                // The high word of `mouseData` is the signed wheel delta (`GET_WHEEL_DELTA_WPARAM`),
                // turned into `uiohook`'s rotation: whole notches, inverted (#13). It always
                // fits a `u16`, and its sign is the wheel direction.
                let delta = u16::try_from(info.mouseData >> 16).unwrap_or(0).cast_signed();
                let rotation = super::wheel_rotation(&mut hook.wheel_leftover, delta);
                hook.emit_mouse(MouseEvent::Wheel {
                    point,
                    rotation: i32::from(rotation),
                    ctrl: hook.modifiers.ctrl(),
                });
            }
            _ => {}
        });
    }
    // SAFETY: the hook contract: everything not consumed goes to the next hook in the chain.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// The low-level keyboard hook (`WH_KEYBOARD_LL`), the half of `uiohook-napi` that
/// `OverlayVisibility.ts` (the Alt peek) and `Shortcuts.ts` (the key log) listen to: every key's
/// down and up on the desktop, with the modifiers the hook has seen.
///
/// It only observes. The callback queues one small event and hands the message on with
/// [`CallNextHookEx`], so typing in the game is never delayed or blocked. Keys Wraex synthesizes
/// itself (`SendInput` for the game's copy key) arrive flagged `LLKHF_INJECTED` and are
/// ignored: they must not toggle the peek or report a modifier the user is not holding.
unsafe extern "system" fn keyboard_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    if code >= 0 {
        // A low-level hook's `wParam` is the message id, which Windows widens to `WPARAM`.
        let message = u32::try_from(wparam.0).unwrap_or_default();
        let down = message == WM_KEYDOWN || message == WM_SYSKEYDOWN;
        let up = message == WM_KEYUP || message == WM_SYSKEYUP;
        if down || up {
            // SAFETY: for `WH_KEYBOARD_LL` Windows passes a live `KBDLLHOOKSTRUCT` in `lparam`.
            let info = unsafe { &*std::ptr::with_exposed_provenance::<KBDLLHOOKSTRUCT>(lparam.0.cast_unsigned()) };
            if !info.flags.contains(LLKHF_INJECTED) {
                with_hook(|hook| {
                    // The released key's modifier is cleared before the event is built, which
                    // is the state `OverlayVisibility`'s keyup handler reads (`!e.altKey`).
                    hook.modifiers.set(info.vkCode, down);
                    // SAFETY: a plain Win32 query.
                    let foreground = hwnd_address(unsafe { GetForegroundWindow() });
                    let overlay = hook.overlay.try_lock().ok().and_then(|hwnd| *hwnd);
                    let game = if hook.is_destroyed { 0 } else { hwnd_address(hook.hwnd) };
                    hook.emit_key(super::scoped_key_event(foreground, game, overlay, || {
                        let event = hook.modifiers.input(wraex_core::keys::vk_to_key(info.vkCode));
                        if down { KeyEvent::Down(event) } else { KeyEvent::Up(event) }
                    }));
                });
            }
        }
    }
    // SAFETY: the hook contract: everything not consumed goes to the next hook in the chain.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// The modifiers held right now, for the state before the hook's first key event: a key the
/// user was already holding sends no keydown, and the mouse hook's events need its modifiers.
fn system_modifiers() -> ModifierState {
    let mut modifiers = ModifierState::default();
    // SAFETY: plain Win32 queries, usable from a low-level hook callback.
    let down = |key: VIRTUAL_KEY| unsafe { GetAsyncKeyState(i32::from(key.0)) < 0 };
    modifiers.set(u32::from(VK_SHIFT.0), down(VK_SHIFT));
    modifiers.set(u32::from(VK_CONTROL.0), down(VK_CONTROL));
    modifiers.set(u32::from(VK_MENU.0), down(VK_MENU));
    modifiers
}

/// Whether `hwnd` is the foreground window, or the root of the window that really has the
/// GUI focus. The reference asks MSAA (`AccessibleObjectFromEvent`, `STATE_SYSTEM_FOCUSED`)
/// because `EVENT_SYSTEM_FOREGROUND` can name a window that did not end up focused; the GUI
/// thread's active window answers the same question without an accessibility client.
fn is_really_focused(hwnd: HWND) -> bool {
    if hwnd.is_invalid() {
        return false;
    }
    // SAFETY: a plain Win32 query.
    if unsafe { GetForegroundWindow() } == hwnd {
        return true;
    }
    // SAFETY: a plain Win32 query.
    let thread = unsafe { GetWindowThreadProcessId(hwnd, None) };
    // The structure is small; `cbSize` can never truncate.
    let cb_size = u32::try_from(std::mem::size_of::<GUITHREADINFO>()).unwrap_or_default();
    let mut info = GUITHREADINFO { cbSize: cb_size, ..Default::default() };
    // SAFETY: the call fills `info`, which this function owns.
    if unsafe { GetGUIThreadInfo(thread, &raw mut info) }.is_ok() && info.hwndActive == hwnd {
        return true;
    }
    // SAFETY: a plain Win32 query.
    let foreground = unsafe { GetForegroundWindow() };
    // SAFETY: `GA_ROOT` walks from a handle this thread may query.
    unsafe { GetAncestor(foreground, GA_ROOT) == hwnd }
}

/// The window's title as text, or `None` when it cannot be read.
fn window_title(hwnd: HWND) -> Option<String> {
    // SAFETY: a plain Win32 query.
    let length = unsafe { GetWindowTextLengthW(hwnd) };
    if length <= 0 {
        return None;
    }
    let capacity = usize::try_from(length).unwrap_or(0).saturating_add(1);
    let mut buffer = vec![0u16; capacity];
    // SAFETY: `GetWindowTextW` writes at most the slice's length and returns the count.
    let written = unsafe { GetWindowTextW(hwnd, &mut buffer) };
    if written <= 0 {
        return None;
    }
    Some(String::from_utf16_lossy(buffer.get(..usize::try_from(written).unwrap_or(0))?))
}

/// The window's class name (`GetClassNameW`), for the log and a class-qualified target.
fn window_class(hwnd: HWND) -> Option<String> {
    let mut buffer = [0u16; 256];
    // SAFETY: `GetClassNameW` writes at most the slice's length and returns the count.
    let written = unsafe { GetClassNameW(hwnd, &mut buffer) };
    if written <= 0 {
        return None;
    }
    Some(String::from_utf16_lossy(buffer.get(..usize::try_from(written).unwrap_or(0))?))
}

/// The client area an overlay can be placed over, or `None` when the window has none (it is
/// minimised, or the queries failed).
fn drawn_bounds(hwnd: HWND) -> Option<Bounds> {
    client_bounds(hwnd).filter(|bounds| bounds.width > 0.0 && bounds.height > 0.0)
}

/// The window's client area in physical screen pixels: what the overlay covers. The rectangle
/// may be empty (a minimised window); `None` means the window is gone.
fn client_bounds(hwnd: HWND) -> Option<Bounds> {
    let mut rect = RECT::default();
    // SAFETY: the call fills the rectangle this function owns.
    if unsafe { GetClientRect(hwnd, &raw mut rect) }.is_err() {
        return None;
    }
    let (width, height) = (rect.right.saturating_sub(rect.left), rect.bottom.saturating_sub(rect.top));
    let mut origin = POINT { x: rect.left, y: rect.top };
    // SAFETY: `origin` is a live local the call converts in place.
    if !unsafe { ClientToScreen(hwnd, &raw mut origin) }.as_bool() {
        return None;
    }
    Some(Bounds { x: f64::from(origin.x), y: f64::from(origin.y), width: f64::from(width), height: f64::from(height) })
}

/// Whether this process may post to `hwnd`: an elevated game refuses the post with
/// `ERROR_ACCESS_DENIED`, which is upstream's "no access" case.
fn has_uipi_access(hwnd: HWND, message: u32) -> bool {
    // SAFETY: a thread-safe Win32 call; it only resets this thread's last-error value.
    unsafe { SetLastError(ERROR_SUCCESS) };
    // SAFETY: the game has no handler for the message; posting it only probes UIPI.
    _ = unsafe { PostMessageW(Some(hwnd), message, WPARAM(0), LPARAM(0)) };
    // SAFETY: a thread-safe Win32 query of this thread's last-error value.
    (unsafe { GetLastError() }) != ERROR_ACCESS_DENIED
}

/// What the hook thread reports back when it starts and its message queue exists.
enum Startup {
    Ready(u32),
}

/// What the hook thread starts with: the queues it fills, the wake callback, the handles the
/// `Watcher` shares with it, and the sender for its ready message.
struct HookStart {
    events: Sender<GameWindowEvents>,
    mouse: Sender<MouseEvent>,
    keys: Sender<KeyEvent>,
    overlay: Arc<Mutex<Option<isize>>>,
    mouse_moves: Arc<AtomicBool>,
    wake: WakeFn,
    game: Arc<Mutex<Option<isize>>>,
    startup: Sender<Startup>,
}

/// The hook thread: hooks, the timer, the first look at the foreground, then the message
/// loop until `WM_QUIT` (posted by [`Watcher::drop`]).
fn hook_thread(target: WindowTarget, start: HookStart) {
    let HookStart { events, mouse, keys, overlay, mouse_moves, wake, game, startup } = start;
    // SAFETY: a plain Win32 call that registers a message name for this process.
    let uipi_message = unsafe { RegisterWindowMessageW(w!("WRAEX_UIPI_TEST")) };
    HOOK.with(|cell| {
        *cell.borrow_mut() = Some(HookThread {
            target,
            events,
            mouse,
            keys,
            overlay,
            modifiers: system_modifiers(),
            wheel_leftover: 0,
            mouse_moves,
            wake,
            game,
            hwnd: HWND::default(),
            is_focused: false,
            is_destroyed: false,
            foreground: HWND::default(),
            location_hook: HWINEVENTHOOK::default(),
            destroy_hook: HWINEVENTHOOK::default(),
            name_hook: HWINEVENTHOOK::default(),
            mouse_hook: HHOOK::default(),
            keyboard_hook: HHOOK::default(),
            uipi_message,
        });
    });
    // System-wide: who came to the foreground.
    // SAFETY: an out-of-context hook owned by this thread until it exits.
    unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(event_proc),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );
    }
    // And who came back from minimised.
    // SAFETY: the same, for the restore-from-minimise event.
    unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_MINIMIZEEND,
            EVENT_SYSTEM_MINIMIZEEND,
            None,
            Some(event_proc),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );
    }
    // The global mouse hook, observed only (`mouse_proc` hands every event on).
    // SAFETY: a global low-level hook installed on this thread, unhooked at exit.
    let mouse_hook = unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), None, 0) };
    match mouse_hook {
        Ok(hook) => with_hook(|state| state.mouse_hook = hook),
        Err(error) => crate::log!("wraex: the global mouse hook could not be installed: {error}"),
    }
    // The global keyboard hook, observed only too: the Alt peek and the key log.
    // SAFETY: a global low-level hook installed on this thread, unhooked at exit.
    let keyboard_hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), None, 0) };
    match keyboard_hook {
        Ok(hook) => with_hook(|state| state.keyboard_hook = hook),
        Err(error) => crate::log!("wraex: the global keyboard hook could not be installed: {error}"),
    }
    // SAFETY: a timer on this thread's own message queue, killed before the thread ends.
    let timer = unsafe { SetTimer(None, 0, FOREGROUND_TIMER_MS, Some(foreground_timer)) };
    // Create this thread's message queue before reporting ready, so a stop posted right
    // after `start` returns cannot be lost.
    let mut message = MSG::default();
    // SAFETY: `message` is a live local and `PM_NOREMOVE` only fills it.
    _ = unsafe { PeekMessageW(&raw mut message, None, 0, 0, PM_NOREMOVE) };

    // A game that already runs (and already has the foreground) is found here.
    // SAFETY: a plain Win32 query.
    let foreground = unsafe { GetForegroundWindow() };
    with_hook(|hook| {
        if !foreground.is_invalid() {
            hook.handle_new_foreground(foreground);
        }
    });
    // SAFETY: a plain Win32 query.
    _ = startup.send(Startup::Ready(unsafe { GetCurrentThreadId() }));

    // SAFETY: `message` is a live local the loop owns.
    while unsafe { GetMessageW(&raw mut message, None, 0, 0) }.0 > 0 {
        // SAFETY: the message `GetMessageW` filled.
        _ = unsafe { TranslateMessage(&raw const message) };
        // SAFETY: the same message, dispatched on the thread that owns its window.
        unsafe { DispatchMessageW(&raw const message) };
    }

    // WM_QUIT: the watcher is being dropped.
    // SAFETY: the timer was created on this thread above.
    _ = unsafe { KillTimer(None, timer) };
    with_hook(|hook| {
        hook.unhook_target();
        if !hook.name_hook.is_invalid() {
            // SAFETY: `name_hook` came from this thread's `SetWinEventHook`.
            _ = unsafe { UnhookWinEvent(hook.name_hook) };
        }
        if !hook.mouse_hook.is_invalid() {
            // SAFETY: `mouse_hook` came from this thread's `SetWindowsHookExW`.
            _ = unsafe { UnhookWindowsHookEx(hook.mouse_hook) };
            hook.mouse_hook = HHOOK::default();
        }
        if !hook.keyboard_hook.is_invalid() {
            // SAFETY: `keyboard_hook` came from this thread's `SetWindowsHookExW`.
            _ = unsafe { UnhookWindowsHookEx(hook.keyboard_hook) };
            hook.keyboard_hook = HHOOK::default();
        }
    });
    HOOK.with(|cell| {
        _ = cell.borrow_mut().take();
    });
}

/// The Win32 window hook. See the module's documentation.
pub struct Watcher {
    events: Receiver<GameWindowEvents>,
    mouse: Receiver<MouseEvent>,
    keys: Receiver<KeyEvent>,
    mouse_moves: Arc<AtomicBool>,
    game: Arc<Mutex<Option<isize>>>,
    target: WindowTarget,
    thread: Option<JoinHandle<()>>,
    thread_id: u32,
}

impl std::fmt::Debug for Watcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Watcher").field("target", &self.target).field("hwnd", &self.hwnd()).finish_non_exhaustive()
    }
}

impl Watcher {
    /// Start following the window that has `target`'s title, now or whenever it appears.
    pub fn start(target: WindowTarget, overlay: Arc<Mutex<Option<isize>>>, wake: WakeFn) -> Result<Self, WatchError> {
        let (events, receiver) = mpsc::channel();
        let (mouse, mouse_receiver) = mpsc::channel();
        let (keys, key_receiver) = mpsc::channel();
        let mouse_moves: Arc<AtomicBool> = Arc::default();
        let game: Arc<Mutex<Option<isize>>> = Arc::default();
        let (startup, started) = mpsc::channel();
        let thread_game = Arc::clone(&game);
        let thread_target = target.clone();
        let thread_mouse_moves = Arc::clone(&mouse_moves);
        let thread = std::thread::Builder::new()
            .name("wraex-window-hook".to_owned())
            .spawn(move || {
                hook_thread(
                    thread_target,
                    HookStart {
                        events,
                        mouse,
                        keys,
                        overlay,
                        mouse_moves: thread_mouse_moves,
                        wake,
                        game: thread_game,
                        startup,
                    },
                );
            })
            .map_err(|error| WatchError::Thread(error.to_string()))?;
        match started.recv() {
            Ok(Startup::Ready(thread_id)) => Ok(Self {
                events: receiver,
                mouse: mouse_receiver,
                keys: key_receiver,
                mouse_moves,
                game,
                target,
                thread: Some(thread),
                thread_id,
            }),
            Err(_) => {
                _ = thread.join();
                Err(WatchError::Thread("the window hook thread stopped before it started".to_owned()))
            }
        }
    }

    /// Everything the OS layer reported since the last call, oldest first.
    pub fn drain(&mut self) -> Vec<GameWindowEvents> {
        self.events.try_iter().collect()
    }

    /// The mouse events since the last call, oldest first (`uiohook`'s listeners).
    pub fn drain_mouse(&mut self) -> Vec<MouseEvent> {
        self.mouse.try_iter().collect()
    }

    /// The keyboard events since the last call, oldest first (`uiohook`'s `keydown`/`keyup`
    /// listeners). Keys Wraex synthesized are not among them.
    pub fn drain_keys(&mut self) -> Vec<KeyEvent> {
        self.keys.try_iter().collect()
    }

    /// Arms or disarms the mouse-move half of the hook. Buttons and wheel keep arriving
    /// either way; moves are only queued while a widget's area is tracked, which is exactly
    /// when upstream adds its `mousemove` listener.
    pub fn set_mouse_moves(&self, enabled: bool) {
        self.mouse_moves.store(enabled, Ordering::Relaxed);
    }

    /// The tracked window's handle, once one was found (and until it is destroyed).
    pub fn hwnd(&self) -> Option<isize> {
        self.game.lock().ok().and_then(|game| *game)
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        // `WM_QUIT` ends the thread's `GetMessageW` loop; the thread unhooks first.
        // SAFETY: posting to a thread of this process that was just told to quit.
        _ = unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
        if let Some(thread) = self.thread.take() {
            _ = thread.join();
        }
    }
}

/// A Win32 window handle as the `isize` the app passes around (0 when there is none).
fn hwnd_address(window: HWND) -> isize {
    window.0.addr().cast_signed()
}

/// An `isize` from the app as the window handle Win32 takes (0 is the invalid handle).
const fn to_hwnd(handle: isize) -> HWND {
    // SAFETY: an `HWND` only carries the address; the calls that take it validate it, and 0
    // means "no window" to them, as it does to `HWND::default`.
    HWND(std::ptr::with_exposed_provenance_mut(handle.cast_unsigned()))
}

/// Put the overlay window over the game's client area, in physical pixels.
///
/// This is the reference's `setBounds`, which Electron does in DIPs; the hook reports physical
/// pixels, so this does too. `false` when the window is not there any more.
pub fn place_overlay(hwnd: isize, bounds: Bounds) -> bool {
    // SAFETY: `hwnd` belongs to this process' Rune window for as long as the app runs.
    unsafe {
        SetWindowPos(
            to_hwnd(hwnd),
            None,
            f64_to_i32(bounds.x.round()),
            f64_to_i32(bounds.y.round()),
            f64_to_i32(bounds.width.round()),
            f64_to_i32(bounds.height.round()),
            SWP_NOACTIVATE | SWP_NOZORDER,
        )
        .is_ok()
    }
}

/// Hide the overlay window from the taskbar and Alt+Tab, as Electron's `skipTaskbar` does:
/// `WS_EX_TOOLWINDOW`, which Rune's window options do not expose.
pub fn configure_overlay(hwnd: isize) {
    let window = to_hwnd(hwnd);
    // SAFETY: `window` is this process' Rune window; the style is only read.
    let style = unsafe { GetWindowLongPtrW(window, GWL_EXSTYLE) };
    // `WS_EX_TOOLWINDOW` is a `u32` style bit; a `LONG_PTR` holds it on every target.
    let wanted = style | isize::try_from(WS_EX_TOOLWINDOW.0).unwrap_or(0);
    if wanted != style {
        // The taskbar keeps the button of a window that was already shown when its style
        // changed ("Managing Taskbar Buttons"): hide it for the change and show it again,
        // without activating it, so the button goes.
        // SAFETY: a plain Win32 query.
        let visible = unsafe { IsWindowVisible(window) }.as_bool();
        if visible {
            // SAFETY: the window belongs to this process.
            _ = unsafe { ShowWindow(window, SW_HIDE) };
        }
        // SAFETY: the style is applied in place with `SWP_FRAMECHANGED` below.
        _ = unsafe { SetWindowLongPtrW(window, GWL_EXSTYLE, wanted) };
        // SAFETY: the window belongs to this process; the flags leave its place and size be.
        _ = unsafe {
            SetWindowPos(
                window,
                None,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            )
        };
        if visible {
            // SAFETY: the window belongs to this process, and is shown without activation.
            _ = unsafe { ShowWindow(window, SW_SHOWNOACTIVATE) };
        }
    }
}

/// Give `hwnd` the foreground: the overlay's focus hand-back (the reference's
/// `focusTarget` → `SetForegroundWindow`).
///
/// Windows only honours the call from the process that owns the foreground window, or with
/// the user's input; when it refuses, the calling thread attaches to the foreground window's
/// input queue for the call, which Windows accepts, and detaches again.
pub fn focus_window(hwnd: isize) -> bool {
    let window = to_hwnd(hwnd);
    if window.is_invalid() {
        return false;
    }
    // SAFETY: the handle belongs to this desktop; Windows may refuse the request.
    if unsafe { SetForegroundWindow(window) }.as_bool() {
        return true;
    }
    // SAFETY: a plain Win32 query.
    let foreground = unsafe { GetForegroundWindow() };
    // SAFETY: a plain Win32 query.
    let our_thread = unsafe { GetCurrentThreadId() };
    let their_thread = if foreground.is_invalid() {
        0
    } else {
        // SAFETY: a plain Win32 query; the process id is not needed.
        unsafe { GetWindowThreadProcessId(foreground, None) }
    };
    if their_thread == 0 || their_thread == our_thread {
        return false;
    }
    // SAFETY: both thread ids belong to windows of this desktop; the attach is undone below.
    let attached = unsafe { AttachThreadInput(our_thread, their_thread, true) }.as_bool();
    // SAFETY: with the input queues attached, Windows accepts this thread's request.
    let focused = unsafe { SetForegroundWindow(window) }.as_bool();
    if attached {
        // SAFETY: undoes the attach made above.
        _ = unsafe { AttachThreadInput(our_thread, their_thread, false) };
    }
    focused
}

/// The window's DPI scale (1.0 at 96 DPI): converts the hook's physical pixels to the logical
/// points Rune lays out in.
pub fn dpi_scale(hwnd: isize) -> f64 {
    // SAFETY: a plain Win32 query; `0` means the window has no DPI (not shown yet).
    let dpi = unsafe { GetDpiForWindow(to_hwnd(hwnd)) };
    if dpi == 0 { 1.0 } else { f64::from(dpi) / 96.0 }
}
