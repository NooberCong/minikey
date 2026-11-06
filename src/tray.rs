//! The notification area icon and its menu.

use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};

use minikey::Method;
use windows_sys::core::{w, PCWSTR};
use windows_sys::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE,
    NIM_MODIFY, NIM_SETVERSION, NOTIFYICONDATAW, NOTIFYICON_VERSION_4,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CheckMenuRadioItem, CreatePopupMenu, DestroyIcon, DestroyMenu, GetSystemMetrics,
    LoadImageW, PostMessageW, SetForegroundWindow, TrackPopupMenuEx, HICON, IMAGE_ICON,
    LR_DEFAULTCOLOR, MF_BYCOMMAND, MF_CHECKED, MF_SEPARATOR, MF_STRING, SM_CXSMICON, SM_CYSMICON,
    TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_CONTEXTMENU, WM_NULL, WM_USER,
};

use crate::settings::{self, Settings};
use crate::WM_TRAY;

const ICON_VIETNAMESE: usize = 1;
const ICON_ENGLISH: usize = 2;

/// Tray notifications that are missing from the bindings.
const NIN_SELECT: u32 = WM_USER;
const NIN_KEYSELECT: u32 = WM_USER + 1;

const ID_VIETNAMESE: u32 = 1;
const ID_ENGLISH: u32 = 2;
const ID_TELEX: u32 = 3;
const ID_VNI: u32 = 4;
const ID_AUTOSTART: u32 = 5;
const ID_EXIT: u32 = 6;

/// What the user asked for through the tray.
pub enum Command {
    Set(Settings),
    ToggleAutostart,
    Exit,
}

fn notify_data(window: HWND) -> NOTIFYICONDATAW {
    let mut data: NOTIFYICONDATAW = unsafe { zeroed() };
    data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    data.hWnd = window;
    data.uID = 1;
    data
}

fn load_icon(id: usize) -> HICON {
    unsafe {
        LoadImageW(
            GetModuleHandleW(null()),
            id as PCWSTR,
            IMAGE_ICON,
            GetSystemMetrics(SM_CXSMICON),
            GetSystemMetrics(SM_CYSMICON),
            LR_DEFAULTCOLOR,
        )
    }
}

/// Fills in the parts of the icon that show the current mode, and returns the
/// icon handle for the caller to destroy once the shell has copied it.
fn show_mode(data: &mut NOTIFYICONDATAW, settings: Settings) -> HICON {
    let language = if settings.vietnamese { "Tiếng Việt" } else { "English" };
    let method = if settings.method == Method::Telex { "Telex" } else { "VNI" };
    let tip = ["minikey: ", language, " (", method, ")"];
    let units = tip.iter().flat_map(|part| part.encode_utf16());
    for (slot, unit) in data.szTip.iter_mut().zip(units) {
        *slot = unit;
    }
    data.hIcon = load_icon(if settings.vietnamese { ICON_VIETNAMESE } else { ICON_ENGLISH });
    data.hIcon
}

pub fn add(window: HWND, settings: Settings) {
    let mut data = notify_data(window);
    data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP;
    data.uCallbackMessage = WM_TRAY;
    let icon = show_mode(&mut data, settings);
    data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
    unsafe {
        Shell_NotifyIconW(NIM_ADD, &data);
        Shell_NotifyIconW(NIM_SETVERSION, &data);
        DestroyIcon(icon);
    }
}

pub fn update(window: HWND, settings: Settings) {
    let mut data = notify_data(window);
    data.uFlags = NIF_ICON | NIF_TIP | NIF_SHOWTIP;
    let icon = show_mode(&mut data, settings);
    unsafe {
        Shell_NotifyIconW(NIM_MODIFY, &data);
        DestroyIcon(icon);
    }
}

pub fn remove(window: HWND) {
    unsafe { Shell_NotifyIconW(NIM_DELETE, &notify_data(window)) };
}

/// Interprets a tray callback message.
pub fn handle_event(window: HWND, wparam: WPARAM, lparam: LPARAM, settings: Settings) -> Option<Command> {
    match lparam as u32 & 0xFFFF {
        NIN_SELECT | NIN_KEYSELECT => {
            Some(Command::Set(Settings { vietnamese: !settings.vietnamese, ..settings }))
        }
        WM_CONTEXTMENU => {
            let (x, y) = (wparam as i16 as i32, (wparam >> 16) as i16 as i32);
            command_for(show_menu(window, x, y, settings), settings)
        }
        _ => None,
    }
}

fn command_for(id: u32, settings: Settings) -> Option<Command> {
    let set = |settings| Some(Command::Set(settings));
    match id {
        ID_VIETNAMESE => set(Settings { vietnamese: true, ..settings }),
        ID_ENGLISH => set(Settings { vietnamese: false, ..settings }),
        ID_TELEX => set(Settings { method: Method::Telex, ..settings }),
        ID_VNI => set(Settings { method: Method::Vni, ..settings }),
        ID_AUTOSTART => Some(Command::ToggleAutostart),
        ID_EXIT => Some(Command::Exit),
        _ => None,
    }
}

/// Runs the menu and returns the chosen item's id, or 0.
fn show_menu(window: HWND, x: i32, y: i32, settings: Settings) -> u32 {
    unsafe {
        let menu = CreatePopupMenu();
        AppendMenuW(menu, MF_STRING, ID_VIETNAMESE as usize, w!("Tiếng Việt\tCtrl+Shift"));
        AppendMenuW(menu, MF_STRING, ID_ENGLISH as usize, w!("English"));
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        AppendMenuW(menu, MF_STRING, ID_TELEX as usize, w!("Telex"));
        AppendMenuW(menu, MF_STRING, ID_VNI as usize, w!("VNI"));
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        let autostart = if settings::autostart() { MF_CHECKED } else { 0 };
        AppendMenuW(menu, MF_STRING | autostart, ID_AUTOSTART as usize, w!("Start with Windows"));
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        AppendMenuW(menu, MF_STRING, ID_EXIT as usize, w!("Exit"));

        let language = if settings.vietnamese { ID_VIETNAMESE } else { ID_ENGLISH };
        CheckMenuRadioItem(menu, ID_VIETNAMESE, ID_ENGLISH, language, MF_BYCOMMAND);
        let method = if settings.method == Method::Telex { ID_TELEX } else { ID_VNI };
        CheckMenuRadioItem(menu, ID_TELEX, ID_VNI, method, MF_BYCOMMAND);

        // Without these two steps the menu would not close on an outside click.
        SetForegroundWindow(window);
        let flags = TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON;
        let id = TrackPopupMenuEx(menu, flags, x, y, window, null_mut());
        PostMessageW(window, WM_NULL, 0, 0);

        DestroyMenu(menu);
        id as u32
    }
}
