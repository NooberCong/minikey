//! Applies edits to the focused app by synthesizing input, and works out how
//! each app needs that done.

use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};

use minikey::{Edit, CAPACITY};
use windows_sys::core::w;
use windows_sys::Win32::Foundation::{
    CloseHandle, GENERIC_READ, GENERIC_WRITE, HWND, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::Console::{
    AttachConsole, FreeConsole, WriteConsoleInputW, INPUT_RECORD, KEY_EVENT,
};
use windows_sys::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetKeyboardLayout, SendInput, HKL, INPUT, INPUT_KEYBOARD,
    KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, VK_BACK, VK_LEFT, VK_SHIFT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetForegroundWindow, GetWindowThreadProcessId,
};

/// Marks our own synthesized keys so the hook can ignore them.
pub const TAG: usize = 0x4D4B_4559; // "MKEY"

/// How an app wants its text replaced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Strategy {
    /// Select with Shift+Left and type over the selection. Backspace is not
    /// reliable in browser address bars, where it first eats the autocomplete.
    Select,
    /// Delete with Backspace, for apps where Shift+Left misbehaves.
    Backspace,
    /// Write straight into the input buffer of the console owned by this
    /// process. Windows PowerShell's line editor ignores synthesized Unicode
    /// keys, so a classic console cannot be reached through `SendInput`.
    Console(u32),
    /// Leave all keys alone. Remote and virtual machines forward raw keys to a
    /// guest, which needs its own input method.
    Bypass,
}

/// Apps that do not work with the default strategy.
const APP_STRATEGIES: &[(&str, Strategy)] = &[
    ("mstsc.exe", Strategy::Bypass),
    ("msrdc.exe", Strategy::Bypass),
    ("vmconnect.exe", Strategy::Bypass),
    ("vmware.exe", Strategy::Bypass),
    ("vmplayer.exe", Strategy::Bypass),
    ("vmware-vmx.exe", Strategy::Bypass),
    ("VirtualBox.exe", Strategy::Bypass),
    ("VirtualBoxVM.exe", Strategy::Bypass),
    ("vncviewer.exe", Strategy::Bypass),
    ("WINWORD.EXE", Strategy::Backspace),
    ("EXCEL.EXE", Strategy::Backspace),
    ("POWERPNT.EXE", Strategy::Backspace),
    ("wps.exe", Strategy::Backspace),
    ("wpp.exe", Strategy::Backspace),
    ("et.exe", Strategy::Backspace),
    ("Notion.exe", Strategy::Backspace),
];

/// Windows Terminal's window class. Shift+Left would start a selection there.
const TERMINAL_CLASS: &str = "CASCADIA_HOSTING_WINDOW_CLASS";
/// The window class of a classic console.
const CONSOLE_CLASS: &str = "ConsoleWindowClass";

/// The app with keyboard focus, cached until the foreground window changes.
pub struct Target {
    window: HWND,
    thread: u32,
    strategy: Strategy,
}

impl Target {
    pub const fn new() -> Self {
        Self { window: null_mut(), thread: 0, strategy: Strategy::Select }
    }

    /// Follows the foreground window and returns its strategy.
    pub fn refresh(&mut self) -> Strategy {
        let window = unsafe { GetForegroundWindow() };
        if window != self.window {
            self.window = window;
            let mut process = 0;
            self.thread = unsafe { GetWindowThreadProcessId(window, &mut process) };
            self.strategy = strategy_for(window, process);
        }
        self.strategy
    }

    pub fn layout(&self) -> HKL {
        unsafe { GetKeyboardLayout(self.thread) }
    }
}

fn strategy_for(window: HWND, process: u32) -> Strategy {
    let mut class = [0u16; 64];
    let len = unsafe { GetClassNameW(window, class.as_mut_ptr(), class.len() as i32) };
    let class = &class[..len.max(0) as usize];
    if equals(class, CONSOLE_CLASS) {
        return Strategy::Console(process);
    }
    if equals(class, TERMINAL_CLASS) {
        return Strategy::Backspace;
    }

    let mut path = [0u16; 1024];
    let path = image_path(process, &mut path);
    let file = path.rsplit(|c| *c == u16::from(b'\\')).next().unwrap_or(path);
    APP_STRATEGIES
        .iter()
        .find(|(name, _)| equals(file, name))
        .map_or(Strategy::Select, |(_, strategy)| *strategy)
}

/// The executable path of a process, or nothing if it is off limits to us.
fn image_path(process: u32, buffer: &mut [u16]) -> &[u16] {
    let mut len = buffer.len() as u32;
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process);
        if handle.is_null() {
            return &[];
        }
        let ok = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, buffer.as_mut_ptr(), &mut len);
        CloseHandle(handle);
        if ok == 0 {
            return &[];
        }
    }
    &buffer[..len as usize]
}

/// ASCII-case-insensitive comparison of a UTF-16 string with an ASCII one.
fn equals(wide: &[u16], ascii: &str) -> bool {
    wide.len() == ascii.len()
        && wide.iter().zip(ascii.bytes()).all(|(w, a)| {
            u8::try_from(*w).is_ok_and(|byte| byte.eq_ignore_ascii_case(&a))
        })
}

pub fn is_held(vk: u16) -> bool {
    unsafe { GetAsyncKeyState(i32::from(vk)) < 0 }
}

/// A fixed-size list of synthesized events: deletion and insertion, each a key
/// press and release per character, plus Shift around the selection.
struct Events {
    inputs: [INPUT; 4 * CAPACITY + 2],
    len: usize,
}

impl Events {
    fn new() -> Self {
        Self { inputs: unsafe { zeroed() }, len: 0 }
    }

    fn push(&mut self, vk: u16, scan: u16, flags: u32) {
        let input = &mut self.inputs[self.len];
        input.r#type = INPUT_KEYBOARD;
        input.Anonymous.ki.wVk = vk;
        input.Anonymous.ki.wScan = scan;
        input.Anonymous.ki.dwFlags = flags;
        input.Anonymous.ki.dwExtraInfo = TAG;
        self.len += 1;
    }

    fn tap(&mut self, vk: u16, scan: u16, flags: u32) {
        self.push(vk, scan, flags);
        self.push(vk, scan, flags | KEYEVENTF_KEYUP);
    }

    fn send(&self) {
        unsafe { SendInput(self.len as u32, self.inputs.as_ptr(), size_of::<INPUT>() as i32) };
    }
}

const SCAN_BACKSPACE: u16 = 0x0E;
const SCAN_SHIFT: u16 = 0x2A;
const SCAN_LEFT: u16 = 0x4B;

/// Writes an edit to a console's input buffer as one batch, so that the
/// deletions and text cannot be reordered. Returns false if the console is out
/// of reach. The attachment is brief because closing an attached console would
/// take this process down with it.
fn write_to_console(edit: &Edit, process: u32) -> bool {
    let mut records: [INPUT_RECORD; 4 * CAPACITY] = unsafe { zeroed() };
    let mut len = 0;
    let mut tap = |vk: u16, scan: u16, unit: u16| {
        for down in [1, 0] {
            let record = &mut records[len];
            record.EventType = KEY_EVENT as u16;
            let key = unsafe { &mut record.Event.KeyEvent };
            key.bKeyDown = down;
            key.wRepeatCount = 1;
            key.wVirtualKeyCode = vk;
            key.wVirtualScanCode = scan;
            key.uChar.UnicodeChar = unit;
            len += 1;
        }
    };
    for _ in 0..edit.backspaces {
        tap(VK_BACK, SCAN_BACKSPACE, 8);
    }
    for unit in edit.text() {
        tap(0, 0, *unit);
    }

    unsafe {
        if AttachConsole(process) == 0 {
            return false;
        }
        let input = CreateFileW(
            w!("CONIN$"),
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            null(),
            OPEN_EXISTING,
            0,
            null_mut(),
        );
        let mut written = 0;
        let ok = input != INVALID_HANDLE_VALUE
            && WriteConsoleInputW(input, records.as_ptr(), len as u32, &mut written) != 0;
        if input != INVALID_HANDLE_VALUE {
            CloseHandle(input);
        }
        FreeConsole();
        ok
    }
}

/// Performs an edit in the foreground app, in a single atomic batch.
pub fn replace(edit: &Edit, strategy: Strategy) {
    if let Strategy::Console(process) = strategy {
        if write_to_console(edit, process) {
            return;
        }
    }

    let mut events = Events::new();
    let text = edit.text();

    if strategy == Strategy::Select && !text.is_empty() {
        // If the user is holding Shift, as for a capital, then it is already
        // down, and releasing it here would turn their next letters lowercase.
        let press_shift = !is_held(VK_SHIFT);
        if press_shift {
            events.push(VK_SHIFT, SCAN_SHIFT, 0);
        }
        for _ in 0..edit.backspaces {
            events.tap(VK_LEFT, SCAN_LEFT, KEYEVENTF_EXTENDEDKEY);
        }
        if press_shift {
            events.push(VK_SHIFT, SCAN_SHIFT, KEYEVENTF_KEYUP);
        }
    } else {
        for _ in 0..edit.backspaces {
            events.tap(VK_BACK, SCAN_BACKSPACE, 0);
        }
    }

    for unit in text {
        events.tap(0, *unit, KEYEVENTF_UNICODE);
    }
    events.send();
}
