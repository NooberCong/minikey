"""End-to-end test: real keystrokes, through the real hook, into real apps.

Runs on a private Windows desktop. Low-level hooks are per desktop, so other
input methods cannot interfere, and no test keystroke can reach the user's
windows. The screen shows that desktop while the test runs; a separate watchdog
process switches back even if this script dies.

Usage: python tools/e2e.py <minikey.exe> [group ...]
Groups: native chrome edge word console terminal system stall. Default: all of them.
"""
import ctypes
import ctypes.wintypes as wt
import json
import os
import subprocess
import sys
import tempfile
import threading
import time
import urllib.parse
import urllib.request
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path

DESKTOP = "minikey-e2e"
WATCHDOG_SECONDS = 240
REG_KEY = r"Software\minikey"
RUN_KEY = r"Software\Microsoft\Windows\CurrentVersion\Run"

user32 = ctypes.WinDLL("user32", use_last_error=True)
kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
ntdll = ctypes.WinDLL("ntdll")

ULONG_PTR = ctypes.c_size_t


class KEYBDINPUT(ctypes.Structure):
    _fields_ = [("wVk", wt.WORD), ("wScan", wt.WORD), ("dwFlags", wt.DWORD),
                ("time", wt.DWORD), ("dwExtraInfo", ULONG_PTR)]


class MOUSEINPUT(ctypes.Structure):
    _fields_ = [("dx", wt.LONG), ("dy", wt.LONG), ("mouseData", wt.DWORD), ("dwFlags", wt.DWORD),
                ("time", wt.DWORD), ("dwExtraInfo", ULONG_PTR)]


class _U(ctypes.Union):
    _fields_ = [("ki", KEYBDINPUT), ("mi", MOUSEINPUT)]


class INPUT(ctypes.Structure):
    _anonymous_ = ("u",)
    _fields_ = [("type", wt.DWORD), ("u", _U)]


class STARTUPINFO(ctypes.Structure):
    _fields_ = [("cb", wt.DWORD), ("lpReserved", wt.LPWSTR), ("lpDesktop", wt.LPWSTR),
                ("lpTitle", wt.LPWSTR), ("dwX", wt.DWORD), ("dwY", wt.DWORD), ("dwXSize", wt.DWORD),
                ("dwYSize", wt.DWORD), ("dwXCountChars", wt.DWORD), ("dwYCountChars", wt.DWORD),
                ("dwFillAttribute", wt.DWORD), ("dwFlags", wt.DWORD), ("wShowWindow", wt.WORD),
                ("cbReserved2", wt.WORD), ("lpReserved2", ctypes.c_void_p),
                ("hStdInput", wt.HANDLE), ("hStdOutput", wt.HANDLE), ("hStdError", wt.HANDLE)]


class PROCESS_INFORMATION(ctypes.Structure):
    _fields_ = [("hProcess", wt.HANDLE), ("hThread", wt.HANDLE),
                ("dwProcessId", wt.DWORD), ("dwThreadId", wt.DWORD)]


user32.CreateDesktopW.restype = wt.HANDLE
user32.CreateDesktopW.argtypes = [wt.LPCWSTR, wt.LPCWSTR, ctypes.c_void_p, wt.DWORD, wt.DWORD, ctypes.c_void_p]
user32.OpenDesktopW.restype = wt.HANDLE
user32.OpenDesktopW.argtypes = [wt.LPCWSTR, wt.DWORD, wt.BOOL, wt.DWORD]
user32.SwitchDesktop.argtypes = [wt.HANDLE]
user32.SetThreadDesktop.argtypes = [wt.HANDLE]
user32.SendInput.argtypes = [wt.UINT, ctypes.c_void_p, ctypes.c_int]
user32.FindWindowW.restype = wt.HWND
user32.FindWindowW.argtypes = [wt.LPCWSTR, wt.LPCWSTR]
user32.FindWindowExW.restype = wt.HWND
user32.FindWindowExW.argtypes = [wt.HWND, wt.HWND, wt.LPCWSTR, wt.LPCWSTR]
user32.GetForegroundWindow.restype = wt.HWND
user32.SendMessageW.restype = ctypes.c_ssize_t
user32.SendMessageW.argtypes = [wt.HWND, wt.UINT, ctypes.c_size_t, ctypes.c_void_p]
user32.PostMessageW.argtypes = [wt.HWND, wt.UINT, ctypes.c_size_t, ctypes.c_ssize_t]
user32.VkKeyScanW.restype = ctypes.c_short
user32.VkKeyScanW.argtypes = [wt.WCHAR]
user32.GetKeyState.restype = ctypes.c_short
user32.SetThreadDpiAwarenessContext.restype = ctypes.c_void_p
user32.SetThreadDpiAwarenessContext.argtypes = [ctypes.c_void_p]
user32.GetMenuItemRect.argtypes = [wt.HWND, ctypes.c_void_p, wt.UINT, ctypes.POINTER(wt.RECT)]
kernel32.OpenProcess.restype = wt.HANDLE
kernel32.TerminateProcess.argtypes = [wt.HANDLE, wt.UINT]
kernel32.GetProcessTimes.argtypes = [wt.HANDLE] + [ctypes.POINTER(wt.FILETIME)] * 4
ntdll.NtSuspendProcess.argtypes = [wt.HANDLE]
ntdll.NtResumeProcess.argtypes = [wt.HANDLE]

KEYUP, EXTENDED = 0x2, 0x1
VK = dict(shift=0x10, ctrl=0x11, alt=0x12, back=0x08, enter=0x0D, tab=0x09, left=0x25, right=0x27,
          up=0x26, down=0x28, home=0x24, end=0x23, caps=0x14, esc=0x1B, space=0x20)
EXTENDED_VKS = {0x25, 0x26, 0x27, 0x28, 0x24, 0x23}


def key_event(vk, up=False):
    event = INPUT(type=1)
    event.ki.wVk = vk
    event.ki.wScan = user32.MapVirtualKeyW(vk, 0)
    event.ki.dwFlags = (KEYUP if up else 0) | (EXTENDED if vk in EXTENDED_VKS else 0)
    return event


def send(*events):
    array = (INPUT * len(events))(*events)
    sent = user32.SendInput(len(events), array, ctypes.sizeof(INPUT))
    assert sent == len(events), f"SendInput blocked ({ctypes.get_last_error()}): is the desktop active?"


def tap(vk, delay=0.004):
    send(key_event(vk))
    send(key_event(vk, up=True))
    time.sleep(delay)


def chord(*names):
    vks = [VK.get(n) or user32.VkKeyScanW(n) & 0xFF for n in names]
    for vk in vks:
        send(key_event(vk))
        time.sleep(0.01)
    for vk in reversed(vks):
        send(key_event(vk, up=True))
        time.sleep(0.01)
    time.sleep(0.05)


def press_char(char, delay):
    scan = user32.VkKeyScanW(char)
    vk, shift = scan & 0xFF, bool(scan & 0x100)
    if shift:
        send(key_event(VK["shift"]))
    tap(vk, 0)
    if shift:
        send(key_event(VK["shift"], up=True))
    time.sleep(delay)


def type_keys(text, delay=0.004):
    """`<` is Backspace; everything else is typed as the character it is."""
    for char in text:
        if char == "<":
            tap(VK["back"], delay)
        else:
            press_char(char, delay)
    time.sleep(0.12)


def type_held_shift(text, delay=0.004):
    """Types a word with Shift physically held throughout, as people do."""
    send(key_event(VK["shift"]))
    for char in text:
        tap(user32.VkKeyScanW(char.lower()) & 0xFF, delay)
    send(key_event(VK["shift"], up=True))
    time.sleep(0.12)


def click(x, y):
    width, height = user32.GetSystemMetrics(0), user32.GetSystemMetrics(1)
    move = INPUT(type=0)
    move.mi.dx, move.mi.dy = x * 65535 // width, y * 65535 // height
    move.mi.dwFlags = 0x8001  # MOVE | ABSOLUTE
    down, up = INPUT(type=0), INPUT(type=0)
    down.mi.dwFlags, up.mi.dwFlags = 0x2, 0x4
    send(move, down, up)
    time.sleep(0.1)


def launch(command, cwd=None):
    """Starts a process on the test desktop."""
    info = STARTUPINFO(cb=ctypes.sizeof(STARTUPINFO), lpDesktop=f"WinSta0\\{DESKTOP}")
    process = PROCESS_INFORMATION()
    buffer = ctypes.create_unicode_buffer(command)
    ok = kernel32.CreateProcessW(None, buffer, None, None, False, 0, None, cwd,
                                 ctypes.byref(info), ctypes.byref(process))
    assert ok, f"CreateProcess failed: {ctypes.get_last_error()}: {command}"
    return process


def stop(process):
    kernel32.TerminateProcess(process.hProcess, 0)
    time.sleep(0.15)


def registry(key, name):
    import winreg
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, key) as handle:
            return winreg.QueryValueEx(handle, name)[0]
    except OSError:
        return None


def set_method(vni, vietnamese=True):
    import winreg
    with winreg.CreateKey(winreg.HKEY_CURRENT_USER, REG_KEY) as handle:
        winreg.SetValueEx(handle, "Method", 0, winreg.REG_DWORD, int(vni))
        winreg.SetValueEx(handle, "Vietnamese", 0, winreg.REG_DWORD, int(vietnamese))


def clear_settings():
    import winreg
    try:
        winreg.DeleteKey(winreg.HKEY_CURRENT_USER, REG_KEY)
    except OSError:
        pass


results = []


def check(group, name, actual, expected):
    passed = actual == expected
    results.append((group, name, passed))
    detail = "" if passed else f"  got {actual!r}, want {expected!r}"
    print(f"[{'PASS' if passed else 'FAIL'}] {group}: {name}{detail}", flush=True)
    return passed


def wait_for(condition, timeout=15):
    deadline = time.time() + timeout
    while time.time() < deadline:
        value = condition()
        if value:
            return value
        time.sleep(0.1)
    return None


class Minikey:
    def __init__(self, exe, vni=False, vietnamese=True):
        set_method(vni, vietnamese)
        self.process = launch(f'"{exe}"')
        self.window = wait_for(lambda: user32.FindWindowW("minikey", None))
        assert self.window, "minikey window did not appear"
        time.sleep(0.2)

    def stop(self):
        stop(self.process)


# ---- Native controls -------------------------------------------------------

TARGET_SOURCE = r'''
import ctypes, ctypes.wintypes as wt, sys
u = ctypes.WinDLL("user32"); k = ctypes.WinDLL("kernel32")
k.LoadLibraryW("msftedit.dll")
k.GetModuleHandleW.restype = wt.HMODULE
WNDPROC = ctypes.WINFUNCTYPE(ctypes.c_ssize_t, wt.HWND, wt.UINT, ctypes.c_size_t, ctypes.c_ssize_t)
u.DefWindowProcW.restype = ctypes.c_ssize_t
u.DefWindowProcW.argtypes = [wt.HWND, wt.UINT, ctypes.c_size_t, ctypes.c_ssize_t]
class WC(ctypes.Structure):
    _fields_ = [("style", wt.UINT), ("proc", WNDPROC), ("a", ctypes.c_int), ("b", ctypes.c_int),
                ("inst", wt.HMODULE), ("icon", wt.HANDLE), ("cur", wt.HANDLE), ("br", wt.HANDLE),
                ("menu", wt.LPCWSTR), ("name", wt.LPCWSTR)]
proc = WNDPROC(lambda h, m, w, l: u.DefWindowProcW(h, m, w, l))
inst = k.GetModuleHandleW(None)
u.RegisterClassW(ctypes.byref(WC(proc=proc, inst=inst, br=6, name="minikey-e2e-target")))
u.CreateWindowExW.restype = wt.HWND
u.CreateWindowExW.argtypes = [wt.DWORD, wt.LPCWSTR, wt.LPCWSTR, wt.DWORD] + [ctypes.c_int]*4 + [wt.HWND, wt.HMENU, wt.HMODULE, ctypes.c_void_p]
top = u.CreateWindowExW(0, "minikey-e2e-target", "e2e", 0x10CF0000, 40, 40, 800, 500, None, None, inst, None)
style = 0x50A00004
u.CreateWindowExW(0, "EDIT", "", style, 10, 10, 760, 200, top, 1, inst, None)
u.CreateWindowExW(0, "RICHEDIT50W", "", style, 10, 230, 760, 200, top, 2, inst, None)
msg = wt.MSG()
while u.GetMessageW(ctypes.byref(msg), None, 0, 0) > 0:
    u.TranslateMessage(ctypes.byref(msg)); u.DispatchMessageW(ctypes.byref(msg))
'''


def text_of(window):
    buffer = ctypes.create_unicode_buffer(4096)
    user32.SendMessageW(window, 0x0D, 4096, ctypes.cast(buffer, ctypes.c_void_p))  # WM_GETTEXT
    return buffer.value.replace("\r\n", "\n").replace("\r", "\n")


class Native:
    """Types in a control, reading back through WM_GETTEXT."""

    def __init__(self, control, point):
        self.control, self.point = control, point

    def clear(self):
        user32.SendMessageW(self.control, 0x0C, 0, ctypes.cast(ctypes.c_wchar_p(""), ctypes.c_void_p))
        click(*self.point)

    def text(self):
        return text_of(self.control)


def typing_cases():
    """(name, Telex keys, expected text)."""
    return [
        ("sentence", "Xin chaof cacs banj", "Xin chào các bạn"),
        ("full tones", "tieengs Vieejt raats hay", "tiếng Việt rất hay"),
        ("horns", "nguowif dduowngf thuowr", "người đường thuở"),
        ("restore", "windows system", "windows system"),
        ("undo marks", "aaa ass ddd", "aa as dd"),
        ("punctuation", "chaof, banj! ddaay.", "chào, bạn! đây."),
        ("backspace tone", "tuays<", "túa"),
        ("backspace word", "tieengs<<", "tiế"),
        ("resume after space", "tieeng <s", "tiếng"),
    ]


def run_typing_suite(group, field, slow=0.004):
    for name, keys, expected in typing_cases():
        field.clear()
        type_keys(keys, slow)
        check(group, name, field.text(), expected)


def open_target():
    source = Path(tempfile.gettempdir()) / "minikey_e2e_target.py"
    source.write_text(TARGET_SOURCE, encoding="utf-8")
    target = launch(f'"{sys.executable}" "{source}"')
    top = wait_for(lambda: user32.FindWindowW("minikey-e2e-target", None))
    assert top, "target window did not appear"
    user32.SetForegroundWindow(top)
    return target, top


def test_native(exe):
    target, top = open_target()
    edit = user32.FindWindowExW(top, None, "EDIT", None)
    rich = user32.FindWindowExW(top, None, "RICHEDIT50W", None)
    field = Native(edit, (200, 120))
    richfield = Native(rich, (200, 340))

    app = Minikey(exe)
    run_typing_suite("edit", field)
    run_typing_suite("richedit", richfield)

    field.clear()
    type_held_shift("VIEETJ")
    check("edit", "held Shift all-caps word", field.text(), "VIỆT")
    field.clear()
    type_held_shift("DDUWOWNGF")
    check("edit", "held Shift keeps capitals after an edit", field.text(), "ĐƯỜNG")

    field.clear()
    tap(VK["caps"])
    time.sleep(0.05)
    type_keys("vieejt")
    tap(VK["caps"])
    check("edit", "Caps Lock", field.text(), "VIỆT")

    field.clear()
    type_keys("tieng")
    tap(VK["left"])
    tap(VK["right"])
    type_keys("s")
    check("edit", "arrow key ends the word", field.text(), "tiengs")

    field.clear()
    type_keys("tieng")
    time.sleep(0.7)  # Outlast the double-click time, which would select the word.
    click(*field.point)
    type_keys("s")
    check("edit", "mouse click ends the word", field.text(), "tiengs")

    field.clear()
    for char in "ddaay laf tieengs Vieejt":
        press_char(char, 0)
    time.sleep(0.3)
    check("edit", "burst with no delay between keys", field.text(), "đây là tiếng Việt")

    field.clear()
    type_keys("as")
    chord("ctrl", "a")
    type_keys("as")
    check("edit", "Ctrl shortcut ends the word", field.text(), "á")

    # The language switch.
    field.clear()
    chord("ctrl", "shift")
    time.sleep(0.2)
    type_keys("tieengs ")
    check("toggle", "Ctrl+Shift switches to English", field.text(), "tieengs ")
    check("toggle", "switch is persisted", registry(REG_KEY, "Vietnamese"), 0)
    chord("ctrl", "shift")
    time.sleep(0.2)
    type_keys("tieengs")
    check("toggle", "Ctrl+Shift switches back", field.text(), "tieengs tiếng")
    check("toggle", "switch back is persisted", registry(REG_KEY, "Vietnamese"), 1)
    field.clear()
    chord("ctrl", "shift", "n")
    time.sleep(0.2)
    check("toggle", "Ctrl+Shift+key is not a switch", registry(REG_KEY, "Vietnamese"), 1)
    user32.SetForegroundWindow(top)
    click(*field.point)
    app.stop()

    app = Minikey(exe, vni=True)
    for name, keys, expected in [
        ("words", "tie61ng Vie65t", "tiếng Việt"),
        ("horns", "ngu7o72i d9u7o7ng2", "người đường"),
        ("breve and undo", "a8 a11", "ă a1"),
        ("digits stay", "123 45", "123 45"),
    ]:
        field.clear()
        type_keys(keys)
        check("vni", name, field.text(), expected)
    app.stop()
    stop(target)


# ---- Browsers ---------------------------------------------------------------

PAGE = (b"<!doctype html><title>T:</title><body style='margin:40px'>"
        b"<textarea id=t autofocus style='width:700px;height:300px' spellcheck=false "
        b"oninput=\"document.title='T:'+this.value\"></textarea>")
SERVER_PORT, DEBUG_PORT = 9444, 9333


class Page(BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.end_headers()
        self.wfile.write(PAGE)

    def log_message(self, *args):
        pass


def browser_targets():
    with urllib.request.urlopen(f"http://127.0.0.1:{DEBUG_PORT}/json", timeout=5) as response:
        return [t for t in json.load(response) if t.get("type") == "page"]


class BrowserField:
    def clear(self):
        click(400, 300)
        chord("ctrl", "a")
        tap(0x2E)  # Delete
        time.sleep(0.1)

    def text(self):
        time.sleep(0.15)
        title = browser_targets()[0]["title"]
        return title[2:] if title.startswith("T:") else title


def page_ready():
    try:
        return any("vietnam" in t.get("url", "") for t in browser_targets())
    except OSError:
        return False


def kill_tree(process):
    subprocess.run(["taskkill", "/PID", str(process.dwProcessId), "/T", "/F"], capture_output=True)
    time.sleep(0.4)


def test_browser(exe, name, path):
    server = HTTPServer(("127.0.0.1", SERVER_PORT), Page)
    server.handle_error = lambda *args: None  # The browser dies mid-request at the end.
    threading.Thread(target=server.serve_forever, daemon=True).start()
    profile = tempfile.mkdtemp(prefix="minikey-e2e-")
    url = f"http://vietnam.localhost:{SERVER_PORT}/"
    process = launch(
        f'"{path}" --user-data-dir="{profile}" --remote-debugging-port={DEBUG_PORT} '
        "--no-first-run --no-default-browser-check --disable-sync --disable-features=Translate "
        "--window-position=0,0 --window-size=1000,700 "
        '"--host-resolver-rules=MAP * ~NOTFOUND, EXCLUDE localhost, EXCLUDE *.localhost" '
        f"{url}")
    assert wait_for(page_ready, 25), "browser page did not load"
    time.sleep(1.0)
    app = Minikey(exe)
    field = BrowserField()
    run_typing_suite(name, field)

    # Teach the omnibox a URL, so typing "vie" gets an inline autocompletion
    # that a plain Backspace would eat.
    chord("ctrl", "shift")
    time.sleep(0.2)
    for _ in range(3):
        chord("ctrl", "l")
        type_keys(f"vietnam.localhost:{SERVER_PORT}/", 0.01)
        tap(VK["enter"])
        time.sleep(0.8)
    chord("ctrl", "shift")
    time.sleep(0.2)

    for keys, expected in [("vieejt", "việt"), ("tieengs Vieejt", "tiếng Việt")]:
        chord("ctrl", "l")
        time.sleep(0.2)
        type_keys(keys, 0.06)
        time.sleep(0.3)
        tap(VK["enter"])
        time.sleep(1.0)
        location = browser_targets()[0]["url"]
        query = urllib.parse.parse_qs(urllib.parse.urlsplit(location).query).get("q", [location])[0]
        check(name, f"address bar with autocomplete: {keys}", query, expected)
    app.stop()
    kill_tree(process)
    server.shutdown()
    server.server_close()


def test_chrome(exe):
    test_browser(exe, "chrome", r"C:\Program Files\Google\Chrome\Application\chrome.exe")


def test_edge(exe):
    test_browser(exe, "edge", r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe")


# ---- Word -------------------------------------------------------------------

WORD_SCRIPT = r"""
param($out, $stop)
$w = New-Object -ComObject Word.Application
$w.Visible = $true
$d = $w.Documents.Add()
$w.Activate()
while (-not (Test-Path $stop)) {
    try { [IO.File]::WriteAllText($out, $d.Content.Text, [Text.Encoding]::UTF8) } catch {}
    Start-Sleep -Milliseconds 100
}
$d.Close(0)
$w.Quit()
"""


def test_word(exe):
    listing = subprocess.run(["tasklist", "/FI", "IMAGENAME eq WINWORD.EXE"], capture_output=True, text=True)
    if "WINWORD" in listing.stdout:
        check("word", "skipped: Word is already open, leaving it alone", False, True)
        return
    work = Path(tempfile.mkdtemp(prefix="minikey-e2e-"))
    script, out, flag = work / "word.ps1", work / "out.txt", work / "stop"
    script.write_text(WORD_SCRIPT, encoding="utf-8")
    process = launch(f'pwsh -NoProfile -File "{script}" "{out}" "{flag}"')
    assert wait_for(out.exists, 40), "Word did not start"
    time.sleep(2.0)
    app = Minikey(exe)

    def text():
        time.sleep(0.5)
        return out.read_text(encoding="utf-8-sig").rstrip("\r\n")

    for keys, expected in [("Xin chaof cacs banj", "Xin chào các bạn"),
                           ("Tieengs Vieejt raats hay", "Tiếng Việt rất hay"),
                           ("Nguowif dduowngf thuowr", "Người đường thuở"),
                           ("Windows system", "Windows system"),
                           ("Tuays<", "Túa")]:
        chord("ctrl", "a")
        tap(0x2E)
        time.sleep(0.4)
        type_keys(keys, 0.03)
        check("word", keys, text(), expected)
    app.stop()
    flag.write_text("")
    wait_for(lambda: kernel32.WaitForSingleObject(process.hProcess, 0) == 0, 15)


# ---- Consoles ---------------------------------------------------------------

CONSOLE_CASES = [
    ("words", "tieengs Vieejt", "tiếng Việt"),
    ("horns", "nguowif dduowngf", "người đường"),
    ("restore", "windows", "windows"),
    ("backspace tone", "tuays<", "túa"),
]


def run_console_cases(group, out, prefix):
    for name, keys, expected in CONSOLE_CASES:
        if out.exists():
            out.unlink()
        type_keys(prefix + keys, 0.03)
        tap(VK["enter"])
        wait_for(lambda: out.exists() and out.stat().st_size > 0, 3)
        actual = out.read_text(encoding="utf-8-sig").strip() if out.exists() else "<nothing>"
        check(group, name, actual, expected)

    # Fast typing: native keys and console writes must stay in order.
    if out.exists():
        out.unlink()
    for char in prefix + "ddaay laf tieengs Vieejt":
        press_char(char, 0.002)
    time.sleep(0.2)
    tap(VK["enter"])
    wait_for(lambda: out.exists() and out.stat().st_size > 0, 3)
    actual = out.read_text(encoding="utf-8-sig").strip() if out.exists() else "<nothing>"
    check(group, "fast burst", actual, "đây là tiếng Việt")


def powershell_profile(work, out):
    profile = work / "p.ps1"
    profile.write_text(
        f"function x {{ [IO.File]::WriteAllText('{out}', ($args -join ' '), [Text.Encoding]::UTF8) }}\n",
        encoding="utf-8")
    return profile


def powershell_console(exe, group, shell):
    work = Path(tempfile.mkdtemp(prefix="minikey-e2e-"))
    out = work / "out.txt"
    profile = powershell_profile(work, out)
    process = launch(f'conhost.exe {shell} -NoProfile -NoExit -File "{profile}"')
    time.sleep(3.0)
    app = Minikey(exe)
    run_console_cases(group, out, "x ")
    app.stop()
    kill_tree(process)


def test_console(exe):
    powershell_console(exe, "conhost pwsh7", "pwsh")
    powershell_console(exe, "conhost powershell5", "powershell")

    work = Path(tempfile.mkdtemp(prefix="minikey-e2e-"))
    out = work / "out.txt"
    batch = work / "loop.cmd"
    batch.write_text(
        "@echo off\nchcp 65001 >nul\n:again\nset /p v=\n"
        f"(echo %v%)>\"{out}\"\ngoto again\n", encoding="ascii")
    process = launch(f'conhost.exe cmd /c "{batch}"')
    time.sleep(2.0)
    app = Minikey(exe)
    run_console_cases("conhost cmd", out, "")
    app.stop()
    kill_tree(process)


def test_terminal(exe):
    terminal_shell(exe, "terminal pwsh7", "pwsh")
    terminal_shell(exe, "terminal powershell5", "powershell")


def terminal_shell(exe, group, shell):
    work = Path(tempfile.mkdtemp(prefix="minikey-e2e-"))
    out = work / "out.txt"
    profile = powershell_profile(work, out)
    launch(f'wt.exe -w new {shell} -NoProfile -NoExit -File "{profile}"')
    window = wait_for(lambda: user32.FindWindowW("CASCADIA_HOSTING_WINDOW_CLASS", None), 10)
    if not window:
        check(group, "did not open on the test desktop", False, True)
        return
    user32.SetForegroundWindow(window)
    time.sleep(3.0)
    app = Minikey(exe)
    run_console_cases(group, out, "x ")
    app.stop()
    user32.PostMessageW(window, 0x10, 0, 0)  # WM_CLOSE
    wait_for(lambda: not user32.FindWindowW("CASCADIA_HOSTING_WINDOW_CLASS", None), 5)


# ---- System behaviour --------------------------------------------------------

WM_APP, WM_CONTEXTMENU, NIN_SELECT = 0x8000, 0x7B, 0x400


def menu(app, shot=None):
    """Opens the tray menu as a right click does, and waits for it."""
    user32.PostMessageW(app.window, WM_APP, 300 | (300 << 16), WM_CONTEXTMENU)
    assert wait_for(lambda: user32.FindWindowW("#32768", None), 5), "menu did not open"
    time.sleep(0.5)
    if shot:
        from PIL import ImageGrab
        ImageGrab.grab(bbox=(250, 120, 620, 420)).save(shot)


# Positions in the menu, counting separators.
ITEM_VIETNAMESE, ITEM_ENGLISH, ITEM_TELEX, ITEM_VNI, ITEM_AUTOSTART, ITEM_EXIT = 0, 1, 3, 4, 6, 8


def choose(position):
    """Clicks a menu item with the real mouse. minikey is DPI aware and this
    script is not, so this works in physical pixels."""
    popup = user32.FindWindowW("#32768", None)
    menu_handle = user32.SendMessageW(popup, 0x01E1, 0, None)  # MN_GETHMENU
    previous = user32.SetThreadDpiAwarenessContext(ctypes.c_void_p(-4))
    rect = wt.RECT()
    assert user32.GetMenuItemRect(popup, ctypes.c_void_p(menu_handle), position, ctypes.byref(rect))
    click((rect.left + rect.right) // 2, (rect.top + rect.bottom) // 2)
    user32.SetThreadDpiAwarenessContext(previous)
    time.sleep(0.4)


def process_cpu_ms(process):
    times = [wt.FILETIME() for _ in range(4)]
    kernel32.GetProcessTimes(process.hProcess, *[ctypes.byref(t) for t in times])
    return sum((t.dwHighDateTime << 32 | t.dwLowDateTime) / 10_000 for t in times[2:])


def move_mouse(count):
    event = INPUT(type=0)
    event.mi.dwFlags = 0x1  # MOVE
    for i in range(count):
        event.mi.dx = 1 if i % 2 else -1
        send(event)


def key_latency(samples=400):
    timings = []
    vk = 0x51  # q
    for _ in range(samples):
        start = time.perf_counter()
        send(key_event(vk))
        send(key_event(vk, up=True))
        timings.append((time.perf_counter() - start) * 1e6)
        time.sleep(0.001)
    timings.sort()
    return timings[len(timings) // 2], timings[len(timings) * 99 // 100]


def memory(process):
    script = (f"$p=Get-Process -Id {process.dwProcessId}; "
              "\"ws=$([int]($p.WorkingSet64/1KB))KB commit=$([int]($p.PrivateMemorySize64/1KB))KB "
              "threads=$($p.Threads.Count) handles=$($p.HandleCount)\"")
    return subprocess.run(["pwsh", "-NoProfile", "-Command", script], capture_output=True, text=True).stdout.strip()


def test_system(exe):
    target, top = open_target()
    try:
        system_checks(exe, target, top)
    finally:
        stop(target)
        subprocess.run(["taskkill", "/IM", Path(exe).name, "/F"], capture_output=True)


def system_checks(exe, target, top):
    shots = Path(os.environ.get("E2E_SHOTS", tempfile.gettempdir()))
    field = Native(user32.FindWindowExW(top, None, "EDIT", None), (200, 120))

    field.clear()
    base = key_latency()
    print(f"latency no hook: median {base[0]:.0f}us p99 {base[1]:.0f}us", flush=True)

    app = Minikey(exe)
    print("memory idle after start:", memory(app.process), flush=True)

    # A second launch must leave the first in charge.
    second = launch(f'"{exe}"')
    check("system", "second instance exits", kernel32.WaitForSingleObject(second.hProcess, 3000), 0)

    # The mouse hook must exist only while a word is in progress.
    field.clear()
    time.sleep(0.2)
    start = process_cpu_ms(app.process)
    move_mouse(4000)
    idle_cpu = process_cpu_ms(app.process) - start
    type_keys("tieng")
    start = process_cpu_ms(app.process)
    move_mouse(4000)
    armed_cpu = process_cpu_ms(app.process) - start
    time.sleep(0.7)
    click(*field.point)
    start = process_cpu_ms(app.process)
    move_mouse(4000)
    disarmed_cpu = process_cpu_ms(app.process) - start
    print(f"cpu for 4000 mouse moves: idle {idle_cpu:.1f}ms, mid-word {armed_cpu:.1f}ms, "
          f"after click {disarmed_cpu:.1f}ms", flush=True)
    check("system", "mouse moves cost nothing when idle", idle_cpu, 0)
    check("system", "mouse hook is armed mid-word", armed_cpu > 0, True)
    check("system", "click disarms the mouse hook", disarmed_cpu, 0)

    # Latency through the hook.
    field.clear()
    viet = key_latency()
    print(f"latency vietnamese: median {viet[0]:.0f}us p99 {viet[1]:.0f}us", flush=True)
    chord("ctrl", "shift")
    time.sleep(0.2)
    english = key_latency()
    print(f"latency english: median {english[0]:.0f}us p99 {english[1]:.0f}us", flush=True)
    chord("ctrl", "shift")
    time.sleep(0.2)
    print("memory after typing:", memory(app.process), flush=True)

    # Tray: left click toggles.
    user32.PostMessageW(app.window, WM_APP, 0, NIN_SELECT)
    time.sleep(0.3)
    check("tray", "click switches to English", registry(REG_KEY, "Vietnamese"), 0)
    user32.PostMessageW(app.window, WM_APP, 0, NIN_SELECT)
    time.sleep(0.3)
    check("tray", "click switches back", registry(REG_KEY, "Vietnamese"), 1)

    menu(app, shots / "menu-telex.png")
    choose(ITEM_VNI)
    check("tray", "menu selects VNI", registry(REG_KEY, "Method"), 1)
    user32.SetForegroundWindow(top)
    field.clear()
    type_keys("a1")
    check("tray", "VNI is live", field.text(), "á")
    menu(app)
    choose(ITEM_TELEX)
    check("tray", "menu selects Telex", registry(REG_KEY, "Method"), 0)
    menu(app)
    choose(ITEM_ENGLISH)
    check("tray", "menu selects English", registry(REG_KEY, "Vietnamese"), 0)
    menu(app)
    choose(ITEM_VIETNAMESE)
    check("tray", "menu selects Vietnamese", registry(REG_KEY, "Vietnamese"), 1)

    # Start with Windows.
    menu(app)
    choose(ITEM_AUTOSTART)
    command = registry(RUN_KEY, "minikey")
    check("autostart", "menu writes the Run entry", command, f'"{exe}"')
    menu(app, shots / "menu-autostart.png")
    click(700, 30)  # Outside the menu, which closes it.
    app.stop()
    check("autostart", "old process gone", user32.FindWindowW("minikey", None), None)
    app.process = launch(command)
    app.window = wait_for(lambda: user32.FindWindowW("minikey", None), 5)
    check("autostart", "the Run command starts minikey", bool(app.window), True)
    time.sleep(0.3)
    user32.SetForegroundWindow(top)
    field.clear()
    type_keys("tieengs")
    check("autostart", "and it types", field.text(), "tiếng")
    menu(app)
    choose(ITEM_AUTOSTART)
    check("autostart", "menu removes the Run entry", registry(RUN_KEY, "minikey"), None)

    # Settings survive a restart.
    menu(app)
    choose(ITEM_VNI)
    app.stop()
    app.process = launch(f'"{exe}"')
    app.window = wait_for(lambda: user32.FindWindowW("minikey", None), 5)
    time.sleep(0.3)
    user32.SetForegroundWindow(top)
    field.clear()
    type_keys("a1")
    check("settings", "method survives a restart", field.text(), "á")
    print("memory after menus:", memory(app.process), flush=True)

    # Exit from the menu.
    menu(app)
    choose(ITEM_EXIT)
    check("tray", "Exit ends the process", kernel32.WaitForSingleObject(app.process.hProcess, 3000), 0)
    user32.SetForegroundWindow(top)
    field.clear()
    type_keys("as")
    check("tray", "keys are untouched after exit", field.text(), "as")


def test_stall(exe):
    """Stalls the hook until Windows drops it, then waits for the renewal."""
    target, top = open_target()
    try:
        field = Native(user32.FindWindowExW(top, None, "EDIT", None), (200, 120))
        app = Minikey(exe, vni=True)
        field.clear()
        type_keys("a1")
        check("hook", "typing before the stall", field.text(), "á")

        start = time.time()
        ntdll.NtSuspendProcess(app.process.hProcess)
        stalled_keys = 0
        while time.time() - start < 45:
            tap(0x51, 0)
            stalled_keys += 1
        ntdll.NtResumeProcess(app.process.hProcess)
        print(f"stalled {stalled_keys} keys over {time.time() - start:.0f}s", flush=True)
        time.sleep(0.5)

        field.clear()
        type_keys("a1")
        dropped = field.text() == "a1"
        print(f"right after the stall: {field.text()!r}: "
              f"Windows {'dropped' if dropped else 'kept'} the hook", flush=True)
        time.sleep(62)
        field.clear()
        start = time.perf_counter()
        send(key_event(0x41))
        first_key = (time.perf_counter() - start) * 1e6
        send(key_event(0x41, up=True))
        print(f"first key after a trim: {first_key:.0f}us", flush=True)
        field.clear()
        type_keys("a1")
        check("hook", "typing works after the renewal", field.text(), "á")
        print("memory after a trim:", memory(app.process), flush=True)
        app.stop()
    finally:
        stop(target)
        subprocess.run(["taskkill", "/IM", Path(exe).name, "/F"], capture_output=True)


GROUPS = {"stall": test_stall, "native": test_native, "chrome": test_chrome, "edge": test_edge, "word": test_word,
          "console": test_console, "terminal": test_terminal, "system": test_system}


# ---- Harness ---------------------------------------------------------------

def main():
    sys.stdout.reconfigure(encoding="utf-8")
    exe = str(Path(sys.argv[1]).resolve())
    groups = sys.argv[2:] or list(GROUPS)
    before = registry(RUN_KEY, "minikey")
    existing_settings = registry(REG_KEY, "Method")
    assert existing_settings is None, "minikey settings exist; a copy may be in use"

    desktop = user32.CreateDesktopW(DESKTOP, None, None, 0, 0x10000000, None)
    assert desktop, ctypes.get_last_error()
    default = user32.OpenDesktopW("Default", 0, False, 0x0100)  # DESKTOP_SWITCHDESKTOP
    assert default, ctypes.get_last_error()
    watchdog = subprocess.Popen([sys.executable, "-c", (
        "import ctypes, time\n"
        f"time.sleep({WATCHDOG_SECONDS})\n"
        "u = ctypes.WinDLL('user32')\n"
        "u.OpenDesktopW.restype = ctypes.c_void_p\n"
        "u.SwitchDesktop.argtypes = [ctypes.c_void_p]\n"
        "u.SwitchDesktop(u.OpenDesktopW('Default', 0, False, 0x100))\n")])
    assert user32.SetThreadDesktop(desktop), ctypes.get_last_error()
    assert user32.SwitchDesktop(desktop), ctypes.get_last_error()
    try:
        time.sleep(0.3)
        for group in groups:
            try:
                GROUPS[group](exe)
            except Exception as error:  # A broken group must not leave the desktop up.
                check(group, f"group crashed: {error!r}", False, True)
    finally:
        user32.SwitchDesktop(default)
        watchdog.kill()
        clear_settings()

    print(f"Run key unchanged: {registry(RUN_KEY, 'minikey') == before}")
    failed = [r for r in results if not r[2]]
    print(f"{len(results) - len(failed)} passed, {len(failed)} failed")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
