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
//!
//! The reported rectangle is the *client* area (`GetClientRect` + `ClientToScreen`), which is
//! the whole window in borderless-windowed and fullscreen and the content below the title bar
//! otherwise — the reference's `get_content_bounds`.
//!
//! An integrity-level (UIPI) probe tells the host the game runs elevated: a `PostMessage` of
//! a registered message fails with `ERROR_ACCESS_DENIED` across integrity levels, and
//! upstream shows "PoE window - No access" in exactly that case.

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
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VIRTUAL_KEY, VK_CONTROL, VK_MENU};
use windows::Win32::UI::WindowsAndMessaging::{
    CHILDID_SELF, CallNextHookEx, DispatchMessageW, EVENT_OBJECT_DESTROY, EVENT_OBJECT_LOCATIONCHANGE,
    EVENT_OBJECT_NAMECHANGE, EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_MINIMIZEEND, GA_ROOT, GUITHREADINFO, GWL_EXSTYLE,
    GetAncestor, GetClassNameW, GetClientRect, GetForegroundWindow, GetGUIThreadInfo, GetMessageW, GetWindowLongPtrW,
    GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, HHOOK, IsHungAppWindow, IsIconic, KillTimer, MSG,
    MSLLHOOKSTRUCT, OBJID_WINDOW, PM_NOREMOVE, PeekMessageW, PostMessageW, PostThreadMessageW, RegisterWindowMessageW,
    SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetForegroundWindow, SetTimer,
    SetWindowLongPtrW, SetWindowPos, SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, WH_MOUSE_LL,
    WINEVENT_OUTOFCONTEXT, WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_QUIT, WM_RBUTTONDOWN,
    WM_XBUTTONDOWN, WS_EX_TOOLWINDOW,
};
use windows::core::w;

use super::{Bounds, GameWindowEvents, MouseEvent, Point, WakeFn, WatchError, WindowTarget, matches_target};

/// The reference's foreground timer: 12 times a second, which is what catches foreground
/// changes Windows swallowed (`ForegroundLockTimeout`, another app stealing the foreground).
const FOREGROUND_TIMER_MS: u32 = 83;

/// One reported window, as the hook thread keeps it.
struct Hook {
    target: WindowTarget,
    events: Sender<GameWindowEvents>,
    /// The low-level mouse hook's queue: `uiohook`'s `mousemove`/`mousedown`/`wheel`.
    mouse: Sender<MouseEvent>,
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
    uipi_message: u32,
}

thread_local! {
    /// The hook thread's own state. `SetWinEventHook` takes no user data, so the callback
    /// finds it here; out-of-context hooks run on the thread that installed them.
    static HOOK: RefCell<Option<Hook>> = const { RefCell::new(None) };
}

/// Runs `f` with the hook thread's state, if this is that thread.
///
/// A `try_borrow_mut` rather than a `borrow_mut`: a hook callback cannot arrive while another
/// one runs (they are delivered on this thread's message loop), but a re-entrant delivery must
/// not panic the hook thread if one ever does.
fn with_hook(f: impl FnOnce(&mut Hook)) {
    HOOK.with(|cell| {
        if let Ok(mut hook) = cell.try_borrow_mut()
            && let Some(hook) = hook.as_mut()
        {
            f(hook);
        }
    });
}

impl Hook {
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

    fn unhook_target(&mut self) {
        // SAFETY: both handles were returned by `SetWinEventHook` on this thread and are
        // unhooked once.
        unsafe {
            if !self.location_hook.is_invalid() {
                let _ = UnhookWinEvent(self.location_hook);
                self.location_hook = HWINEVENTHOOK::default();
            }
            if !self.destroy_hook.is_invalid() {
                let _ = UnhookWinEvent(self.destroy_hook);
                self.destroy_hook = HWINEVENTHOOK::default();
            }
        }
    }

    /// The reference's `handle_new_foreground`: a new window has the foreground (or the
    /// title of the foreground window changed). Follow it and look at it.
    fn handle_new_foreground(&mut self, hwnd: HWND) {
        self.foreground = hwnd;
        // The title of the *previous* foreground window is no longer interesting.
        // SAFETY: the handle came from `SetWinEventHook` on this thread.
        unsafe {
            if !self.name_hook.is_invalid() {
                let _ = UnhookWinEvent(self.name_hook);
                self.name_hook = HWINEVENTHOOK::default();
            }
        }
        // The game's title bar is filled in after the client window exists, and a game that
        // was already running when Wraex started is found through its next title change.
        if !hwnd.is_invalid() && hwnd != self.hwnd {
            // SAFETY: a plain Win32 query; the thread id is the window's own.
            let thread = unsafe { GetWindowThreadProcessId(hwnd, None) };
            // SAFETY: out-of-context hook owned by this thread, unhooked above or at exit.
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
            if self.hwnd != hwnd {
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
            } else {
                // Found (again): the host only needs to hear about a change of focus.
                if !self.is_focused {
                    self.is_focused = true;
                    self.emit(GameWindowEvents::Focus);
                }
                return;
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
        // SAFETY: a plain Win32 query; hooks as above.
        let thread = unsafe { GetWindowThreadProcessId(hwnd, None) };
        // SAFETY: out-of-context hooks owned by this thread; unhooked by `unhook_target`.
        unsafe {
            self.location_hook = SetWinEventHook(
                EVENT_OBJECT_LOCATIONCHANGE,
                EVENT_OBJECT_LOCATIONCHANGE,
                None,
                Some(event_proc),
                0,
                thread,
                WINEVENT_OUTOFCONTEXT,
            );
            self.destroy_hook = SetWinEventHook(
                EVENT_OBJECT_DESTROY,
                EVENT_OBJECT_DESTROY,
                None,
                Some(event_proc),
                0,
                thread,
                WINEVENT_OUTOFCONTEXT,
            );
        }
        let has_access = has_uipi_access(hwnd, self.uipi_message);
        eprintln!(
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
            *game = Some(hwnd.0 as isize);
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
            if hwnd == hook.hwnd && id_object == OBJID_WINDOW.0 && id_child == CHILDID_SELF as i32 {
                hook.is_destroyed = true;
                hook.check_and_handle(HWND::default());
            }
        }
        EVENT_OBJECT_LOCATIONCHANGE => {
            if hwnd == hook.hwnd && id_object == OBJID_WINDOW.0 && id_child == CHILDID_SELF as i32 {
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
            if hwnd == hook.foreground && id_object == OBJID_WINDOW.0 && id_child == CHILDID_SELF as i32 {
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
        let message = wparam.0 as u32;
        // SAFETY: for `WH_MOUSE_LL` Windows passes a live `MSLLHOOKSTRUCT` in `lparam`.
        let info = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
        let point = Point { x: f64::from(info.pt.x), y: f64::from(info.pt.y) };
        with_hook(|hook| match message {
            WM_MOUSEMOVE if hook.mouse_moves.load(Ordering::Relaxed) => {
                let (ctrl, alt) = mouse_modifiers();
                hook.emit_mouse(MouseEvent::Move { point, ctrl, alt });
            }
            WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN => {
                hook.emit_mouse(MouseEvent::Down { point });
            }
            WM_MOUSEWHEEL => {
                // The high word of `mouseData` is the signed wheel delta; the sign is the
                // notch's direction (a high-resolution wheel may send less than 120 at once).
                let delta = i32::from((info.mouseData >> 16) as u16 as i16);
                hook.emit_mouse(MouseEvent::Wheel { point, rotation: delta.signum(), ctrl: mouse_modifiers().0 });
            }
            _ => {}
        });
    }
    // SAFETY: the hook contract: everything not consumed goes to the next hook in the chain.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// Whether Ctrl and Alt are held, as `uiohook`'s `ctrlKey`/`altKey` report them.
fn mouse_modifiers() -> (bool, bool) {
    // SAFETY: a plain Win32 query usable from a low-level hook callback.
    unsafe {
        let down = |key: VIRTUAL_KEY| GetAsyncKeyState(key.0 as i32) < 0;
        (down(VK_CONTROL), down(VK_MENU))
    }
}

/// Whether `hwnd` is the foreground window, or the root of the window that really has the
/// GUI focus. The reference asks MSAA (`AccessibleObjectFromEvent`, `STATE_SYSTEM_FOCUSED`)
/// because `EVENT_SYSTEM_FOREGROUND` can name a window that did not end up focused; the GUI
/// thread's active window answers the same question without an accessibility client.
fn is_really_focused(hwnd: HWND) -> bool {
    if hwnd.is_invalid() {
        return false;
    }
    // SAFETY: plain Win32 queries.
    unsafe {
        if GetForegroundWindow() == hwnd {
            return true;
        }
        let thread = GetWindowThreadProcessId(hwnd, None);
        let mut info = GUITHREADINFO { cbSize: std::mem::size_of::<GUITHREADINFO>() as u32, ..Default::default() };
        if GetGUIThreadInfo(thread, &mut info).is_ok() && info.hwndActive == hwnd {
            return true;
        }
        GetAncestor(GetForegroundWindow(), GA_ROOT) == hwnd
    }
}

/// The window's title as text, or `None` when it cannot be read.
fn window_title(hwnd: HWND) -> Option<String> {
    // SAFETY: `GetWindowTextW` writes at most the slice's length and returns the count.
    unsafe {
        let length = GetWindowTextLengthW(hwnd);
        if length <= 0 {
            return None;
        }
        let mut buffer = vec![0u16; length as usize + 1];
        let written = GetWindowTextW(hwnd, &mut buffer);
        if written <= 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&buffer[..written as usize]))
    }
}

/// The window's class name (`GetClassNameW`), for the log and a class-qualified target.
fn window_class(hwnd: HWND) -> Option<String> {
    // SAFETY: `GetClassNameW` writes at most the slice's length and returns the count.
    unsafe {
        let mut buffer = [0u16; 256];
        let written = GetClassNameW(hwnd, &mut buffer);
        if written <= 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&buffer[..written as usize]))
    }
}

/// The client area an overlay can be placed over, or `None` when the window has none (it is
/// minimised, or the queries failed).
fn drawn_bounds(hwnd: HWND) -> Option<Bounds> {
    client_bounds(hwnd).filter(|bounds| bounds.width > 0.0 && bounds.height > 0.0)
}

/// The window's client area in physical screen pixels: what the overlay covers. The rectangle
/// may be empty (a minimised window); `None` means the window is gone.
fn client_bounds(hwnd: HWND) -> Option<Bounds> {
    // SAFETY: both calls write into live locals; `ClientToScreen` converts the client origin.
    unsafe {
        let mut rect = RECT::default();
        if GetClientRect(hwnd, &mut rect).is_err() {
            return None;
        }
        let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
        let mut origin = POINT { x: rect.left, y: rect.top };
        if !ClientToScreen(hwnd, &mut origin).as_bool() {
            return None;
        }
        Some(Bounds {
            x: f64::from(origin.x),
            y: f64::from(origin.y),
            width: f64::from(width),
            height: f64::from(height),
        })
    }
}

/// Whether this process may post to `hwnd`: an elevated game refuses the post with
/// `ERROR_ACCESS_DENIED`, which is upstream's "no access" case.
fn has_uipi_access(hwnd: HWND, message: u32) -> bool {
    // SAFETY: a thread-safe Win32 probe; the game has no handler for the message.
    unsafe {
        SetLastError(ERROR_SUCCESS);
        let _ = PostMessageW(Some(hwnd), message, WPARAM(0), LPARAM(0));
        GetLastError() != ERROR_ACCESS_DENIED
    }
}

/// What the hook thread reports back when it starts and its message queue exists.
enum Startup {
    Ready(u32),
}

/// The hook thread: hooks, the timer, the first look at the foreground, then the message
/// loop until `WM_QUIT` (posted by [`Watcher::drop`]).
fn hook_thread(
    target: WindowTarget,
    events: Sender<GameWindowEvents>,
    mouse: Sender<MouseEvent>,
    mouse_moves: Arc<AtomicBool>,
    wake: WakeFn,
    game: Arc<Mutex<Option<isize>>>,
    startup: Sender<Startup>,
) {
    // SAFETY: every call here is a plain Win32 call owned by this thread, which also
    // unhooks everything before it ends.
    unsafe {
        let uipi_message = RegisterWindowMessageW(w!("WRAEX_UIPI_TEST"));
        HOOK.with(|cell| {
            *cell.borrow_mut() = Some(Hook {
                target,
                events,
                mouse,
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
                uipi_message,
            });
        });
        // System-wide: who came to the foreground, and who came back from minimised.
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(event_proc),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );
        SetWinEventHook(
            EVENT_SYSTEM_MINIMIZEEND,
            EVENT_SYSTEM_MINIMIZEEND,
            None,
            Some(event_proc),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );
        // The global mouse hook, observed only (`mouse_proc` hands every event on).
        let mouse_hook = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), None, 0);
        match mouse_hook {
            Ok(hook) => with_hook(|state| state.mouse_hook = hook),
            Err(error) => eprintln!("wraex: the global mouse hook could not be installed: {error}"),
        }
        let timer = SetTimer(None, 0, FOREGROUND_TIMER_MS, Some(foreground_timer));
        // Create this thread's message queue before reporting ready, so a stop posted right
        // after `start` returns cannot be lost.
        let mut message = MSG::default();
        let _ = PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE);

        // A game that already runs (and already has the foreground) is found here.
        let foreground = GetForegroundWindow();
        with_hook(|hook| {
            if !foreground.is_invalid() {
                hook.handle_new_foreground(foreground);
            }
        });
        let _ = startup.send(Startup::Ready(GetCurrentThreadId()));

        while GetMessageW(&mut message, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }

        // WM_QUIT: the watcher is being dropped.
        let _ = KillTimer(None, timer);
        with_hook(|hook| {
            hook.unhook_target();
            if !hook.name_hook.is_invalid() {
                let _ = UnhookWinEvent(hook.name_hook);
            }
            if !hook.mouse_hook.is_invalid() {
                let _ = UnhookWindowsHookEx(hook.mouse_hook);
                hook.mouse_hook = HHOOK::default();
            }
        });
        HOOK.with(|cell| {
            let _ = cell.borrow_mut().take();
        });
    }
}

/// The Win32 window hook. See the module's documentation.
pub struct Watcher {
    events: Receiver<GameWindowEvents>,
    mouse: Receiver<MouseEvent>,
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
    pub fn start(target: WindowTarget, wake: WakeFn) -> Result<Self, WatchError> {
        let (events, receiver) = mpsc::channel();
        let (mouse, mouse_receiver) = mpsc::channel();
        let mouse_moves: Arc<AtomicBool> = Arc::default();
        let game: Arc<Mutex<Option<isize>>> = Arc::default();
        let (startup, started) = mpsc::channel();
        let thread_game = game.clone();
        let thread_target = target.clone();
        let thread_mouse_moves = mouse_moves.clone();
        let thread = std::thread::Builder::new()
            .name("wraex-window-hook".to_owned())
            .spawn(move || hook_thread(thread_target, events, mouse, thread_mouse_moves, wake, thread_game, startup))
            .map_err(|error| WatchError::Thread(error.to_string()))?;
        match started.recv() {
            Ok(Startup::Ready(thread_id)) => Ok(Self {
                events: receiver,
                mouse: mouse_receiver,
                mouse_moves,
                game,
                target,
                thread: Some(thread),
                thread_id,
            }),
            Err(_) => {
                let _ = thread.join();
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
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Put the overlay window over the game's client area, in physical pixels (the reference's
/// `setBounds`, which Electron does in DIPs; the hook reports physical pixels, so this does
/// too). `false` when the window is not there any more.
pub fn place_overlay(hwnd: isize, bounds: Bounds) -> bool {
    // SAFETY: `hwnd` belongs to this process' Rune window for as long as the app runs.
    unsafe {
        SetWindowPos(
            HWND(hwnd as *mut _),
            None,
            bounds.x.round() as i32,
            bounds.y.round() as i32,
            bounds.width.round() as i32,
            bounds.height.round() as i32,
            SWP_NOACTIVATE | SWP_NOZORDER,
        )
        .is_ok()
    }
}

/// Hide the overlay window from the taskbar and Alt+Tab, as Electron's `skipTaskbar` does:
/// `WS_EX_TOOLWINDOW`, which Rune's window options do not expose.
pub fn configure_overlay(hwnd: isize) {
    // SAFETY: `hwnd` is this process' Rune window; the style change is applied with
    // `SWP_FRAMECHANGED`.
    unsafe {
        let window = HWND(hwnd as *mut _);
        let style = GetWindowLongPtrW(window, GWL_EXSTYLE);
        let wanted = style | WS_EX_TOOLWINDOW.0 as isize;
        if wanted != style {
            let _ = SetWindowLongPtrW(window, GWL_EXSTYLE, wanted);
            let _ = SetWindowPos(
                window,
                None,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
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
    let window = HWND(hwnd as *mut _);
    // SAFETY: both calls take a window handle of this desktop.
    unsafe {
        if window.is_invalid() {
            return false;
        }
        if SetForegroundWindow(window).as_bool() {
            return true;
        }
        let foreground = GetForegroundWindow();
        let our_thread = GetCurrentThreadId();
        let their_thread = if foreground.is_invalid() { 0 } else { GetWindowThreadProcessId(foreground, None) };
        if their_thread == 0 || their_thread == our_thread {
            return false;
        }
        let attached = AttachThreadInput(our_thread, their_thread, true).as_bool();
        let focused = SetForegroundWindow(window).as_bool();
        if attached {
            let _ = AttachThreadInput(our_thread, their_thread, false);
        }
        focused
    }
}

/// The window's DPI scale (1.0 at 96 DPI): converts the hook's physical pixels to the logical
/// points Rune lays out in.
pub fn dpi_scale(hwnd: isize) -> f64 {
    // SAFETY: a plain Win32 query; `0` means the window has no DPI (not shown yet).
    let dpi = unsafe { GetDpiForWindow(HWND(hwnd as *mut _)) };
    if dpi == 0 { 1.0 } else { f64::from(dpi) / 96.0 }
}
