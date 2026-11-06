//! What survives a restart, all of it in the registry under HKCU.

use std::ptr::null_mut;

use minikey::Method;
use windows_sys::core::{w, PCWSTR};
use windows_sys::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows_sys::Win32::System::Registry::{
    RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_DWORD, REG_SZ,
    RRF_RT_REG_DWORD, RRF_RT_REG_SZ,
};

const SETTINGS_KEY: PCWSTR = w!("Software\\minikey");
const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const RUN_VALUE: PCWSTR = w!("minikey");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub method: Method,
    pub vietnamese: bool,
}

fn read_dword(name: PCWSTR) -> Option<u32> {
    let mut data = 0u32;
    let mut size = 4u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            SETTINGS_KEY,
            name,
            RRF_RT_REG_DWORD,
            null_mut(),
            (&raw mut data).cast(),
            &mut size,
        )
    };
    (status == 0).then_some(data)
}

fn write_dword(name: PCWSTR, value: u32) {
    unsafe {
        RegSetKeyValueW(HKEY_CURRENT_USER, SETTINGS_KEY, name, REG_DWORD, (&raw const value).cast(), 4);
    }
}

pub fn load() -> Settings {
    Settings {
        method: if read_dword(w!("Method")) == Some(1) { Method::Vni } else { Method::Telex },
        vietnamese: read_dword(w!("Vietnamese")) != Some(0),
    }
}

pub fn save(settings: Settings) {
    write_dword(w!("Method"), u32::from(settings.method == Method::Vni));
    write_dword(w!("Vietnamese"), u32::from(settings.vietnamese));
}

/// Longest command line we handle: a quoted path and its terminator.
const COMMAND_CAPACITY: usize = 1024;

/// This executable's path in quotes, as a Run entry needs it. Returns the
/// buffer and the length of the command, excluding the terminator.
fn run_command() -> ([u16; COMMAND_CAPACITY], usize) {
    let mut command = [0u16; COMMAND_CAPACITY];
    command[0] = u16::from(b'"');
    let path = &mut command[1..COMMAND_CAPACITY - 2];
    let len = unsafe { GetModuleFileNameW(null_mut(), path.as_mut_ptr(), path.len() as u32) } as usize;
    command[len + 1] = u16::from(b'"');
    (command, len + 2)
}

/// Whether Windows will start this copy of minikey at sign-in.
pub fn autostart() -> bool {
    let (expected, len) = run_command();
    let mut stored = [0u16; COMMAND_CAPACITY];
    let mut size = (COMMAND_CAPACITY * 2) as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            RUN_VALUE,
            RRF_RT_REG_SZ,
            null_mut(),
            stored.as_mut_ptr().cast(),
            &mut size,
        )
    };
    status == 0 && stored[..=len] == expected[..=len]
}

pub fn set_autostart(enabled: bool) {
    unsafe {
        if enabled {
            let (command, len) = run_command();
            let size = ((len + 1) * 2) as u32;
            RegSetKeyValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_VALUE, REG_SZ, command.as_ptr().cast(), size);
        } else {
            RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_VALUE);
        }
    }
}
