//! minikey: a Vietnamese input method that lives in the notification area.
//!
//! Everything runs on one thread: the message loop delivers the low-level hook
//! callbacks and the tray messages, so no state needs a lock.

#![windows_subsystem = "windows"]

mod hook;
mod inject;
mod settings;
mod tray;

use std::mem::zeroed;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicU32, Ordering};

use settings::Settings;
use tray::Command;
use windows_sys::core::w;
use windows_sys::Win32::Foundation::{
    GetLastError, ERROR_ALREADY_EXISTS, HWND, LPARAM, LRESULT, WPARAM,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::{
    CreateMutexW, GetCurrentProcess, GetCurrentThread, SetProcessWorkingSetSize,
    SetThreadPriority, THREAD_PRIORITY_HIGHEST,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, MessageBoxW,
    PostQuitMessage, RegisterClassW, RegisterWindowMessageW, SetTimer, MB_ICONERROR, MSG,
    WM_APP, WM_DESTROY, WM_TIMER, WNDCLASSW,
};

/// The tray icon's callback message.
pub const WM_TRAY: u32 = WM_APP;
/// Posted by the hook when Ctrl+Shift switches language: the hook must return
/// quickly, so the slow work of updating the tray and registry happens here.
pub const WM_SETTINGS_CHANGED: u32 = WM_APP + 1;

/// How often to renew the keyboard hook (see [`hook::renew`]) and trim memory.
const MAINTENANCE_MS: u32 = 60_000;

/// Broadcast by the shell when the taskbar is (re)created.
static WM_TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

fn main() {
    unsafe {
        // Held until the process exits. The name is per session, like the hook.
        CreateMutexW(null(), 0, w!("Local\\minikey"));
        if GetLastError() == ERROR_ALREADY_EXISTS {
            return;
        }

        // Every keystroke on the system waits for the hook, so it must never
        // queue behind other work. It runs for microseconds at a time.
        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST);

        WM_TASKBAR_CREATED.store(RegisterWindowMessageW(w!("TaskbarCreated")), Ordering::Relaxed);
        let window = create_window();
        let settings = settings::load();

        if !hook::start(window, settings) {
            MessageBoxW(null_mut(), w!("Could not install the keyboard hook."), w!("minikey"), MB_ICONERROR);
            return;
        }
        tray::add(window, settings);
        SetTimer(window, 1, MAINTENANCE_MS, None);
        trim_memory();

        let mut message: MSG = zeroed();
        while GetMessageW(&mut message, null_mut(), 0, 0) > 0 {
            DispatchMessageW(&message);
        }

        tray::remove(window);
        hook::stop();
    }
}

/// Hands back pages that nothing is using right now. The few the hook needs
/// stay cached by the system and fault back in cheaply on the next keystroke.
fn trim_memory() {
    unsafe { SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX) };
}

/// Creates the hidden window that receives tray messages. It is a normal
/// top-level window because message-only windows miss `TaskbarCreated`.
unsafe fn create_window() -> HWND {
    let class = w!("minikey");
    let instance = GetModuleHandleW(null());
    RegisterClassW(&WNDCLASSW {
        lpfnWndProc: Some(window_proc),
        hInstance: instance,
        lpszClassName: class,
        ..zeroed()
    });
    CreateWindowExW(0, class, class, 0, 0, 0, 0, 0, null_mut(), null_mut(), instance, null())
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_TRAY => {
            match tray::handle_event(window, wparam, lparam, hook::settings()) {
                Some(Command::Set(settings)) => apply(window, settings),
                Some(Command::ToggleAutostart) => settings::set_autostart(!settings::autostart()),
                Some(Command::Exit) => {
                    DestroyWindow(window);
                }
                None => {}
            }
            trim_memory();
            0
        }
        WM_SETTINGS_CHANGED => {
            persist(window, hook::settings());
            0
        }
        WM_TIMER => {
            hook::renew();
            trim_memory();
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ if message == WM_TASKBAR_CREATED.load(Ordering::Relaxed) => {
            tray::add(window, hook::settings());
            0
        }
        _ => DefWindowProcW(window, message, wparam, lparam),
    }
}

fn apply(window: HWND, settings: Settings) {
    hook::set_settings(settings);
    persist(window, settings);
}

fn persist(window: HWND, settings: Settings) {
    tray::update(window, settings);
    settings::save(settings);
}
