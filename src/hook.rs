//! The low-level keyboard hook: decides, for every key on the system, whether
//! it passes through untouched or becomes an edit of the word being typed.
//!
//! Hook callbacks sit in the path of all typing, so `Typing` only ever does
//! table lookups and cheap queries. Its `RefCell` is never held across
//! `SendInput`, which re-enters the hook with our own injected keys.

use std::cell::RefCell;
use std::mem::replace;
use std::ptr::null_mut;

use minikey::{Edit, Engine, Method};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, ToUnicodeEx, VK_BACK, VK_CAPITAL, VK_CONTROL, VK_LCONTROL, VK_LMENU, VK_LSHIFT,
    VK_LWIN, VK_MENU, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT, VK_SPACE,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, PostMessageW, SetWindowsHookExW, UnhookWindowsHookEx, HC_ACTION, HHOOK,
    KBDLLHOOKSTRUCT, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_LBUTTONDOWN, WM_MBUTTONDOWN,
    WM_RBUTTONDOWN, WM_SYSKEYDOWN,
};

use crate::inject::{self, is_held, Strategy, Target};
use crate::settings::Settings;
use crate::WM_SETTINGS_CHANGED;

/// `ToUnicodeEx` flag: leave the keyboard's dead-key state alone.
const KEEP_KEYBOARD_STATE: u32 = 1 << 2;

thread_local! {
    static TYPING: RefCell<Typing> = const { RefCell::new(Typing::new()) };
}

/// What to do with a key.
enum Outcome {
    Pass,
    Swallow,
    Replace(Edit, Strategy),
}

struct Typing {
    engine: Engine,
    vietnamese: bool,
    window: HWND,
    keyboard_hook: HHOOK,
    /// Only installed while a word is in progress; see `sync_mouse_hook`.
    mouse_hook: HHOOK,
    /// Ctrl and Shift are down with nothing else pressed since.
    chord: bool,
    /// Keys whose key-down was swallowed, so their key-up must be too.
    swallowed: [bool; 256],
    target: Target,
}

impl Typing {
    const fn new() -> Self {
        Self {
            engine: Engine::new(Method::Telex),
            vietnamese: true,
            window: null_mut(),
            keyboard_hook: null_mut(),
            mouse_hook: null_mut(),
            chord: false,
            swallowed: [false; 256],
            target: Target::new(),
        }
    }

    fn on_key(&mut self, vk: u32, scan: u32, down: bool) -> Outcome {
        let vk16 = vk as u16;
        let index = (vk & 0xFF) as usize;

        if !down {
            if replace(&mut self.chord, false) && is_ctrl(vk16) | is_shift(vk16) {
                self.toggle();
            }
            return if replace(&mut self.swallowed[index], false) {
                Outcome::Swallow
            } else {
                Outcome::Pass
            };
        }

        if is_ctrl(vk16) || is_shift(vk16) {
            let other = if is_ctrl(vk16) { VK_SHIFT } else { VK_CONTROL };
            self.chord = is_held(other) && !is_held(VK_MENU) && !win_held();
            return Outcome::Pass;
        }
        self.chord = false;
        if is_alt_or_win(vk16) || !self.vietnamese {
            return Outcome::Pass;
        }

        let outcome = self.on_typing_key(vk, scan);
        self.sync_mouse_hook();
        if !matches!(outcome, Outcome::Pass) {
            self.swallowed[index] = true;
        }
        outcome
    }

    fn on_typing_key(&mut self, vk: u32, scan: u32) -> Outcome {
        let strategy = self.target.refresh();
        if strategy == Strategy::Bypass || is_held(VK_CONTROL) || is_held(VK_MENU) || win_held() {
            self.engine.reset();
            return Outcome::Pass;
        }

        let edit = match vk as u16 {
            VK_SPACE => {
                self.engine.end_word();
                None
            }
            VK_BACK => self.engine.backspace(),
            _ => match self.ascii(vk, scan) {
                Some(key) => self.engine.key(key),
                None => {
                    // Arrows, Enter, Home and the like: the caret has moved on.
                    self.engine.reset();
                    None
                }
            },
        };
        edit.map_or(Outcome::Pass, |edit| Outcome::Replace(edit, strategy))
    }

    /// The printable ASCII character this key types, if any.
    fn ascii(&self, vk: u32, scan: u32) -> Option<u8> {
        let shift = is_held(VK_SHIFT);
        let caps = unsafe { GetKeyState(i32::from(VK_CAPITAL)) } & 1 != 0;

        if (0x41..=0x5A).contains(&vk) {
            return Some(if shift ^ caps { vk as u8 } else { vk as u8 + 32 });
        }
        if !matches!(vk, 0x30..=0x39 | 0x60..=0x6F | 0xBA..=0xC0 | 0xDB..=0xDF | 0xE2) {
            return None;
        }

        // Digits and punctuation depend on the layout of the app being typed in.
        let mut keys = [0u8; 256];
        keys[usize::from(VK_SHIFT)] = u8::from(shift) << 7;
        keys[usize::from(VK_CAPITAL)] = u8::from(caps);
        let mut units = [0u16; 4];
        let layout = self.target.layout();
        let written = unsafe {
            ToUnicodeEx(vk, scan, keys.as_ptr(), units.as_mut_ptr(), 4, KEEP_KEYBOARD_STATE, layout)
        };
        (written == 1 && (0x21..0x7F).contains(&units[0])).then_some(units[0] as u8)
    }

    fn toggle(&mut self) {
        self.vietnamese = !self.vietnamese;
        self.reset_word();
        unsafe { PostMessageW(self.window, WM_SETTINGS_CHANGED, 0, 0) };
    }

    fn reset_word(&mut self) {
        self.engine.reset();
        self.sync_mouse_hook();
    }

    /// A click can move the caret, which must end the current word. But a mouse
    /// hook wakes this process on every mouse movement, so it only exists while
    /// there is a word to end: idle or in English, the mouse costs nothing.
    fn sync_mouse_hook(&mut self) {
        let wanted = self.engine.has_state();
        unsafe {
            if wanted && self.mouse_hook.is_null() {
                self.mouse_hook = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), null_mut(), 0);
            } else if !wanted && !self.mouse_hook.is_null() {
                UnhookWindowsHookEx(replace(&mut self.mouse_hook, null_mut()));
            }
        }
    }
}

const fn is_ctrl(vk: u16) -> bool {
    matches!(vk, VK_CONTROL | VK_LCONTROL | VK_RCONTROL)
}

const fn is_shift(vk: u16) -> bool {
    matches!(vk, VK_SHIFT | VK_LSHIFT | VK_RSHIFT)
}

const fn is_alt_or_win(vk: u16) -> bool {
    matches!(vk, VK_MENU | VK_LMENU | VK_RMENU | VK_LWIN | VK_RWIN)
}

fn win_held() -> bool {
    is_held(VK_LWIN) || is_held(VK_RWIN)
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let event = &*(lparam as *const KBDLLHOOKSTRUCT);
        if event.dwExtraInfo != inject::TAG {
            let down = matches!(wparam as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
            let outcome =
                TYPING.with_borrow_mut(|typing| typing.on_key(event.vkCode, event.scanCode, down));
            match outcome {
                Outcome::Pass => {}
                Outcome::Swallow => return 1,
                Outcome::Replace(edit, strategy) => {
                    // Sent from inside the callback so that it lands ahead of
                    // whatever the user types next.
                    inject::replace(&edit, strategy);
                    return 1;
                }
            }
        }
    }
    CallNextHookEx(null_mut(), code, wparam, lparam)
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let result = CallNextHookEx(null_mut(), code, wparam, lparam);
    if code == HC_ACTION as i32
        && matches!(wparam as u32, WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN)
    {
        TYPING.with_borrow_mut(Typing::reset_word);
    }
    result
}

fn install_keyboard_hook() -> HHOOK {
    unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), null_mut(), 0) }
}

/// Installs the hook. Returns false if Windows refused it.
pub fn start(window: HWND, settings: Settings) -> bool {
    TYPING.with_borrow_mut(|typing| {
        typing.window = window;
        typing.vietnamese = settings.vietnamese;
        typing.engine.set_method(settings.method);
        typing.keyboard_hook = install_keyboard_hook();
        !typing.keyboard_hook.is_null()
    })
}

pub fn stop() {
    TYPING.with_borrow_mut(|typing| {
        typing.engine.reset();
        typing.sync_mouse_hook();
        unsafe { UnhookWindowsHookEx(typing.keyboard_hook) };
    });
}

/// Replaces the keyboard hook with a fresh one. Windows silently removes a hook
/// that was too slow to respond, as can happen on a stalled system, and offers
/// no way to detect it. The new hook goes in before the old one comes out, and
/// nothing runs in between on this thread, so no keystroke can be missed.
pub fn renew() {
    TYPING.with_borrow_mut(|typing| {
        let fresh = install_keyboard_hook();
        if !fresh.is_null() {
            unsafe { UnhookWindowsHookEx(replace(&mut typing.keyboard_hook, fresh)) };
        }
    });
}

pub fn settings() -> Settings {
    TYPING.with_borrow(|typing| Settings {
        method: typing.engine.method(),
        vietnamese: typing.vietnamese,
    })
}

pub fn set_settings(settings: Settings) {
    TYPING.with_borrow_mut(|typing| {
        typing.vietnamese = settings.vietnamese;
        typing.engine.set_method(settings.method);
        typing.sync_mouse_hook();
    });
}
