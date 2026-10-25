#![windows_subsystem = "windows"]

use std::cell::RefCell;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::*;
use windows_sys::Win32::UI::HiDpi::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

// ---- Layout [D§2], [D§3] -------------------------------------------------

const CLIENT_W: i32 = 300;
const CLIENT_H: i32 = 286;
const MAIN_STYLE: WINDOW_STYLE = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;

// IDs start at 100: IsDialogMessage sends 1 (IDOK) on Enter and 2 (IDCANCEL) on Esc.
const ID_MODE: i32 = 101; // interval dropdown: 0 = Fixed, 1 = Random
const ID_FIXED_MS: i32 = 103;
const ID_MIN_MS: i32 = 104;
const ID_MAX_MS: i32 = 105;
const ID_FIXED_UNIT: i32 = 106;
const ID_DASH: i32 = 107;
const ID_RANDOM_UNIT: i32 = 108;
const ID_LOC: i32 = 111; // click-at dropdown: 0 = Cursor, 1 = Fixed spot
const ID_CURSOR_HINT: i32 = 112;
const ID_PICK: i32 = 113;
const ID_SPOT_LBL: i32 = 114;
const ID_HOTKEY: i32 = 122; // read-only box showing the hotkey
const ID_CHANGE: i32 = 123;
const ID_START: i32 = 130;
const ID_STATUS: i32 = 131;

struct Ctl {
    id: i32,
    class: &'static str,
    text: &'static str,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    style: u32,
    ex: u32,
}

const DROPDOWN: u32 = CBS_DROPDOWNLIST as u32 | WS_VSCROLL | WS_TABSTOP;
const NUMBER: u32 = (ES_NUMBER | ES_RIGHT | ES_AUTOHSCROLL) as u32 | WS_TABSTOP;
const PUSH: u32 = BS_PUSHBUTTON as u32 | WS_TABSTOP;
const LABEL: u32 = 0; // SS_LEFT (lives behind an extra windows-sys feature)
const CENTER: u32 = 1; // SS_CENTER (same)
const EDGE: u32 = WS_EX_CLIENTEDGE;

// Creation order = Tab order, exactly as the [D§3] table. Rows that share a y are alternatives:
// only the ones matching the dropdown choice are shown.
#[rustfmt::skip]
const CONTROLS: [Ctl; 18] = [
    Ctl { id: 100, class: "STATIC", text: "&Interval", x: 16, y: 19, w: 76, h: 20, style: LABEL, ex: 0 },
    Ctl { id: ID_MODE, class: "COMBOBOX", text: "", x: 96, y: 16, w: 188, h: 120, style: DROPDOWN, ex: 0 },
    Ctl { id: ID_FIXED_MS, class: "EDIT", text: "100", x: 96, y: 48, w: 72, h: 23, style: NUMBER, ex: EDGE },
    Ctl { id: ID_FIXED_UNIT, class: "STATIC", text: "ms", x: 176, y: 51, w: 24, h: 20, style: LABEL, ex: 0 },
    Ctl { id: ID_MIN_MS, class: "EDIT", text: "80", x: 96, y: 48, w: 72, h: 23, style: NUMBER, ex: EDGE },
    Ctl { id: ID_DASH, class: "STATIC", text: "–", x: 168, y: 51, w: 20, h: 20, style: CENTER, ex: 0 },
    Ctl { id: ID_MAX_MS, class: "EDIT", text: "150", x: 188, y: 48, w: 72, h: 23, style: NUMBER, ex: EDGE },
    Ctl { id: ID_RANDOM_UNIT, class: "STATIC", text: "ms", x: 266, y: 51, w: 24, h: 20, style: LABEL, ex: 0 },
    Ctl { id: 110, class: "STATIC", text: "Click &at", x: 16, y: 95, w: 76, h: 20, style: LABEL, ex: 0 },
    Ctl { id: ID_LOC, class: "COMBOBOX", text: "", x: 96, y: 92, w: 188, h: 120, style: DROPDOWN, ex: 0 },
    Ctl { id: ID_CURSOR_HINT, class: "STATIC", text: "Clicks wherever the mouse is", x: 96, y: 127, w: 188, h: 20, style: LABEL, ex: 0 },
    Ctl { id: ID_PICK, class: "BUTTON", text: "&Pick…", x: 96, y: 122, w: 80, h: 26, style: PUSH, ex: 0 },
    Ctl { id: ID_SPOT_LBL, class: "STATIC", text: "not set", x: 184, y: 127, w: 100, h: 20, style: LABEL, ex: 0 },
    Ctl { id: 120, class: "STATIC", text: "&Hotkey", x: 16, y: 171, w: 76, h: 20, style: LABEL, ex: 0 },
    Ctl { id: ID_HOTKEY, class: "EDIT", text: "F6", x: 96, y: 168, w: 104, h: 25, style: (ES_READONLY | ES_CENTER) as u32, ex: EDGE },
    Ctl { id: ID_CHANGE, class: "BUTTON", text: "&Change", x: 208, y: 167, w: 76, h: 27, style: PUSH, ex: 0 },
    Ctl { id: ID_START, class: "BUTTON", text: "Start", x: 16, y: 210, w: 268, h: 36, style: BS_OWNERDRAW as u32 | WS_TABSTOP, ex: 0 },
    Ctl { id: ID_STATUS, class: "STATIC", text: "Stopped · 0 clicks", x: 16, y: 256, w: 268, h: 18, style: CENTER, ex: 0 },
];

// ---- State ------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Idle,
    Running,
    Picking,
    Capturing,
}

struct App {
    mode: Mode,
    spot: Option<POINT>,
    hotkey: Option<(u32, u32)>, // (MOD_* flags, virtual key); None = not registered
    clicks: Arc<AtomicU64>,
    worker: Option<(JoinHandle<()>, Arc<AtomicBool>)>, // (thread, stop flag)
    msgbox_open: bool,
    font: HFONT, // current UI font, replaced when the window moves to a monitor with other scaling
}

const TIMER_STATUS: usize = 1;
const HOTKEY_ID: i32 = 1;
const WM_APP_PICKED: u32 = WM_APP + 1; // wParam: 1 = picked, 0 = cancelled; lParam: packed point
const OVERLAY_ALPHA: u8 = 60; // [D§6]

// Messages [D§5].
const MSG_FIXED: &str = "Enter an interval of at least 1 ms.";
const MSG_RANGE: &str = "Enter a minimum and maximum of at least 1 ms.";
const MSG_MINMAX: &str = "Minimum can't be larger than maximum.";
const MSG_NOSPOT: &str = "Pick a spot first (click \"Pick…\").";
const MSG_BAREKEY: &str =
    "Use an F-key, or add Ctrl or Alt. A plain key (or Shift+key) would stop working everywhere else.";

/// Everything the click thread needs; copied in so it never touches UI state.
#[derive(Clone, Copy)]
struct Config {
    min: u32,
    max: u32,
    spot: Option<POINT>,
    main: isize, // our HWND as an integer (HWND is a raw pointer, not Send)
}

thread_local! {
    static APP: RefCell<App> = RefCell::new(App {
        mode: Mode::Idle,
        spot: None,
        hotkey: Some((0, VK_F6 as u32)),
        clicks: Arc::new(AtomicU64::new(0)),
        worker: None,
        msgbox_open: false,
        font: null_mut(),
    });
}

// Never call anything that can re-enter the window procedure inside `f` (borrow would panic).
fn app<R>(f: impl FnOnce(&mut App) -> R) -> R {
    APP.with_borrow_mut(f)
}

// ---- Small helpers ------------------------------------------------------------

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn scale(v: i32, dpi: u32) -> i32 {
    v * dpi as i32 / 96
}

fn item(hwnd: HWND, id: i32) -> HWND {
    unsafe { GetDlgItem(hwnd, id) }
}

fn set_text(hwnd: HWND, s: &str) {
    unsafe { SetWindowTextW(hwnd, wide(s).as_ptr()) };
}

fn enable(hwnd: HWND, id: i32, on: bool) {
    unsafe { EnableWindow(item(hwnd, id), on as i32) };
}

fn show(hwnd: HWND, id: i32, on: bool) {
    unsafe { ShowWindow(item(hwnd, id), if on { SW_SHOW } else { SW_HIDE }) };
}

// Selected index of a dropdown.
fn combo(hwnd: HWND, id: i32) -> usize {
    unsafe { SendMessageW(item(hwnd, id), CB_GETCURSEL, 0, 0) as usize }
}

fn key_name(vk: u32) -> String {
    // Extended keys need bit 24, or GetKeyNameText returns the numpad names ("Num 4" for Left).
    let ext = matches!(
        vk as u16,
        VK_LEFT | VK_RIGHT | VK_UP | VK_DOWN | VK_INSERT | VK_DELETE | VK_HOME | VK_END
            | VK_PRIOR | VK_NEXT | VK_DIVIDE | VK_NUMLOCK | VK_RCONTROL | VK_RMENU
    );
    let mut buf = [0u16; 64];
    let n = unsafe {
        let sc = MapVirtualKeyW(vk, MAPVK_VK_TO_VSC);
        GetKeyNameTextW(((sc << 16) | if ext { 1 << 24 } else { 0 }) as i32, buf.as_mut_ptr(), 64)
    };
    if n > 0 { String::from_utf16_lossy(&buf[..n as usize]) } else { format!("Key {vk}") }
}

fn hotkey_name(mods: u32, vk: u32) -> String {
    let mut s = String::new();
    if mods & MOD_CONTROL != 0 { s += "Ctrl+"; }
    if mods & MOD_ALT != 0 { s += "Alt+"; }
    if mods & MOD_SHIFT != 0 { s += "Shift+"; }
    s + &key_name(vk)
}

// ---- Clicking -----------------------------------------------------------------------

fn next_delay(min: u32, max: u32, r: u64) -> u64 {
    min as u64 + r % (max as u64 - min as u64 + 1)
}

// std's RandomState is seeded from the OS and every new instance has different keys:
// plenty for click jitter, no extra crate.
fn rand_u64() -> u64 {
    use std::hash::BuildHasher;
    std::collections::hash_map::RandomState::new().hash_one(())
}

// Sleeps `ms` but checks `stop` at least every 15 ms. std::thread::sleep uses a high-resolution
// timer on Windows 10 1803+, so the total wait stays accurate to ~1 ms. Returns false if stopped.
fn sleep_until_or_stop(ms: u64, stop: &AtomicBool) -> bool {
    let deadline = Instant::now() + Duration::from_millis(ms);
    loop {
        if stop.load(Ordering::Relaxed) {
            return false;
        }
        let now = Instant::now();
        if now >= deadline {
            return true;
        }
        thread::sleep((deadline - now).min(Duration::from_millis(15)));
    }
}

fn send_click() -> bool {
    // With swapped mouse buttons, the "primary" click is the physical right button.
    let swapped = unsafe { GetSystemMetrics(SM_SWAPBUTTON) } != 0;
    let (down, up) = if swapped {
        (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP)
    } else {
        (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP)
    };
    let mut inputs = [INPUT::default(), INPUT::default()];
    for (input, flags) in inputs.iter_mut().zip([down, up]) {
        input.r#type = INPUT_MOUSE;
        input.Anonymous.mi.dwFlags = flags;
    }
    unsafe { SendInput(2, inputs.as_ptr(), size_of::<INPUT>() as i32) == 2 }
}

fn worker(cfg: Config, stop: Arc<AtomicBool>, clicks: Arc<AtomicU64>) {
    let main = cfg.main as HWND;
    while sleep_until_or_stop(next_delay(cfg.min, cfg.max, rand_u64()), &stop) {
        unsafe {
            let mut pt = POINT { x: 0, y: 0 };
            match cfg.spot {
                Some(p) => pt = p,
                None => { GetCursorPos(&mut pt); }
            }
            // Never click our own window (it would hit Stop), but do click whatever covers it.
            // WindowFromPoint only sends hit-test messages to windows of the calling thread, and
            // this thread owns none, so it can't deadlock with the UI thread waiting in join().
            if GetAncestor(WindowFromPoint(pt), GA_ROOT) == main {
                continue;
            }
            if let Some(p) = cfg.spot {
                SetCursorPos(p.x, p.y);
            }
        }
        if send_click() {
            clicks.fetch_add(1, Ordering::Relaxed);
        }
    }
}

fn read_ms(hwnd: HWND, id: i32) -> Option<u32> {
    let mut buf = [0u16; 16];
    let n = unsafe { GetWindowTextW(item(hwnd, id), buf.as_mut_ptr(), buf.len() as i32) };
    String::from_utf16_lossy(&buf[..n as usize]).parse().ok().filter(|&v| v >= 1)
}

fn read_config(hwnd: HWND) -> Result<Config, &'static str> {
    let (min, max) = if combo(hwnd, ID_MODE) == 0 {
        let v = read_ms(hwnd, ID_FIXED_MS).ok_or(MSG_FIXED)?;
        (v, v)
    } else {
        let (a, b) = read_ms(hwnd, ID_MIN_MS).zip(read_ms(hwnd, ID_MAX_MS)).ok_or(MSG_RANGE)?;
        if a > b {
            return Err(MSG_MINMAX);
        }
        (a, b)
    };
    let spot = if combo(hwnd, ID_LOC) == 1 { Some(app(|a| a.spot).ok_or(MSG_NOSPOT)?) } else { None };
    Ok(Config { min, max, spot, main: hwnd as isize })
}

fn show_msg(hwnd: HWND, text: &str) {
    app(|a| a.msgbox_open = true);
    unsafe {
        MessageBoxW(hwnd, wide(text).as_ptr(), wide("AutoClicker").as_ptr(), MB_OK | MB_ICONWARNING);
    }
    app(|a| a.msgbox_open = false);
}

fn start(hwnd: HWND) {
    let cfg = match read_config(hwnd) {
        Ok(c) => c,
        Err(text) => return show_msg(hwnd, text),
    };
    let stop = Arc::new(AtomicBool::new(false));
    let clicks = app(|a| {
        a.clicks.store(0, Ordering::Relaxed);
        a.clicks.clone()
    });
    let stop2 = stop.clone();
    let handle = thread::spawn(move || worker(cfg, stop2, clicks));
    app(|a| {
        a.worker = Some((handle, stop));
        a.mode = Mode::Running;
    });
    unsafe { SetTimer(hwnd, TIMER_STATUS, 250, None) };
    apply_state(hwnd);
    unsafe { SetFocus(item(hwnd, ID_START)) }; // don't leave focus on a now-disabled control
}

fn stop(hwnd: HWND) {
    let worker = app(|a| {
        a.mode = Mode::Idle;
        a.worker.take()
    });
    if let Some((handle, flag)) = worker {
        flag.store(true, Ordering::Relaxed);
        let _ = handle.join(); // returns within ~15 ms
    }
    unsafe { KillTimer(hwnd, TIMER_STATUS) };
    apply_state(hwnd);
}

fn toggle(hwnd: HWND) {
    let (mode, busy) = app(|a| (a.mode, a.msgbox_open));
    match mode {
        _ if busy => {}
        Mode::Running => stop(hwnd),
        Mode::Idle => start(hwnd),
        Mode::Picking | Mode::Capturing => {}
    }
}

// ---- Hotkey capture -------------------------------------------------------------------

fn is_modifier_vk(vk: u32) -> bool {
    matches!(
        vk as u16,
        VK_SHIFT | VK_CONTROL | VK_MENU | VK_LSHIFT | VK_RSHIFT | VK_LCONTROL | VK_RCONTROL
            | VK_LMENU | VK_RMENU | VK_LWIN | VK_RWIN
    )
}

// A bare letter/digit hotkey would swallow that key in every app, so plain keys must be F1–F24.
// Shift alone doesn't count: a Shift+A hotkey would stop capital A from typing anywhere.
fn hotkey_allowed(mods: u32, vk: u32) -> bool {
    mods & (MOD_CONTROL | MOD_ALT) != 0 || (VK_F1 as u32..=VK_F24 as u32).contains(&vk)
}

fn start_capture(hwnd: HWND) {
    if app(|a| a.mode) != Mode::Idle {
        return;
    }
    // Unregister so pressing the current combo is captured instead of toggling.
    unsafe { UnregisterHotKey(hwnd, HOTKEY_ID) };
    app(|a| a.mode = Mode::Capturing);
    apply_state(hwnd);
    unsafe { SetFocus(item(hwnd, ID_CHANGE)) };
}

// `new` = None cancels. Falls back to the old hotkey if the new one can't be registered.
fn finish_capture(hwnd: HWND, new: Option<(u32, u32)>) {
    let old = app(|a| a.hotkey);
    let register = |(m, vk): (u32, u32)| unsafe { RegisterHotKey(hwnd, HOTKEY_ID, m | MOD_NOREPEAT, vk) } != 0;
    let mut err = None;
    let mut hotkey = new.filter(|&h| register(h));
    if let (Some((m, vk)), None) = (new, hotkey) {
        err = Some(format!(
            "\"{}\" is already taken by another app or by Windows. Try a different one.",
            hotkey_name(m, vk)
        ));
    }
    if hotkey.is_none() {
        hotkey = old.filter(|&h| register(h));
    }
    app(|a| {
        a.hotkey = hotkey;
        a.mode = Mode::Idle;
    });
    apply_state(hwnd);
    if let Some(e) = err {
        show_msg(hwnd, &e);
    }
}

fn on_capture_key(hwnd: HWND, vk: u32) {
    if vk == VK_ESCAPE as u32 {
        return finish_capture(hwnd, None);
    }
    if is_modifier_vk(vk) {
        return; // wait for the main key
    }
    let down = |k: VIRTUAL_KEY| unsafe { GetKeyState(k as i32) } < 0;
    let mods = [(VK_CONTROL, MOD_CONTROL), (VK_MENU, MOD_ALT), (VK_SHIFT, MOD_SHIFT)]
        .into_iter()
        .filter(|&(k, _)| down(k))
        .fold(0, |m, (_, f)| m | f);
    if mods == MOD_ALT && vk == VK_F4 as u32 {
        // Alt+F4 means "close", not "use Alt+F4 as the hotkey".
        finish_capture(hwnd, None);
        unsafe { PostMessageW(hwnd, WM_CLOSE, 0, 0) };
        return;
    }
    if hotkey_allowed(mods, vk) {
        finish_capture(hwnd, Some((mods, vk)));
    } else {
        show_msg(hwnd, MSG_BAREKEY); // stay in capture mode
    }
}

// ---- Pick (eyedropper) [D§6] ------------------------------------------------------------

fn start_pick(hwnd: HWND) {
    if app(|a| a.mode) != Mode::Idle {
        return;
    }
    app(|a| a.mode = Mode::Picking);
    apply_state(hwnd);
    unsafe {
        // Dimmed, topmost, crosshair window over every monitor; it eats the pick click.
        let class = wide("AutoClickerPick");
        let ov = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW, class.as_ptr(), null(), WS_POPUP,
            GetSystemMetrics(SM_XVIRTUALSCREEN), GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN), GetSystemMetrics(SM_CYVIRTUALSCREEN),
            hwnd, null_mut(), GetModuleHandleW(null()), null(),
        );
        if ov.is_null() {
            app(|a| a.mode = Mode::Idle);
            return apply_state(hwnd);
        }
        SetLayeredWindowAttributes(ov, 0, OVERLAY_ALPHA, LWA_ALPHA);
        ShowWindow(ov, SW_SHOW);
        SetForegroundWindow(ov);
        SetFocus(ov);
    }
}

fn pack_point(p: POINT) -> LPARAM {
    ((p.y as u32 as u64) << 32 | p.x as u32 as u64) as LPARAM
}

fn unpack_point(lp: LPARAM) -> POINT {
    POINT { x: lp as u64 as u32 as i32, y: (lp as u64 >> 32) as u32 as i32 }
}

extern "system" fn overlay_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        let finish = |picked: bool| {
            // Hiding deactivates us and re-enters with WM_ACTIVATE; the visibility check makes that a no-op.
            if IsWindowVisible(hwnd) == 0 {
                return;
            }
            ShowWindow(hwnd, SW_HIDE);
            let mut pt = POINT { x: 0, y: 0 };
            GetCursorPos(&mut pt);
            PostMessageW(GetWindow(hwnd, GW_OWNER), WM_APP_PICKED, picked as usize, pack_point(pt));
            DestroyWindow(hwnd);
        };
        match msg {
            // Act on button *up* so both down and up land here and the app below gets nothing.
            WM_LBUTTONDOWN | WM_RBUTTONDOWN => 0,
            WM_LBUTTONUP => { finish(true); 0 }
            WM_RBUTTONUP => { finish(false); 0 }
            WM_KEYDOWN if wp == VK_ESCAPE as usize => { finish(false); 0 }
            WM_CLOSE => { finish(false); 0 } // Alt+F4 on the overlay
            WM_ACTIVATE if (wp & 0xFFFF) as u32 == WA_INACTIVE => { finish(false); 0 }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

// ---- UI ---------------------------------------------------------------------------

fn create_controls(hwnd: HWND) {
    unsafe {
        let hinst = GetModuleHandleW(null());
        for c in &CONTROLS {
            CreateWindowExW(
                c.ex, wide(c.class).as_ptr(), wide(c.text).as_ptr(), WS_CHILD | WS_VISIBLE | c.style,
                0, 0, 0, 0, hwnd, c.id as usize as HMENU, hinst, null(),
            );
        }
        for id in [ID_FIXED_MS, ID_MIN_MS, ID_MAX_MS] {
            SendMessageW(item(hwnd, id), EM_SETLIMITTEXT, 7, 0);
        }
        for (id, choices) in [(ID_MODE, ["Fixed", "Random"]), (ID_LOC, ["Cursor", "Fixed spot"])] {
            for c in choices {
                let w = wide(c);
                SendMessageW(item(hwnd, id), CB_ADDSTRING, 0, w.as_ptr() as LPARAM);
            }
            SendMessageW(item(hwnd, id), CB_SETCURSEL, 0, 0);
        }
    }
}

// Outer window size that gives a CLIENT_W x CLIENT_H client area at `dpi` [D§2].
fn window_size(dpi: u32) -> (i32, i32) {
    let mut rc = RECT { left: 0, top: 0, right: scale(CLIENT_W, dpi), bottom: scale(CLIENT_H, dpi) };
    unsafe { AdjustWindowRectExForDpi(&mut rc, MAIN_STYLE, 0, 0, dpi) };
    (rc.right - rc.left, rc.bottom - rc.top)
}

// Positions every control and sets a font for `dpi`. Called at startup and on WM_DPICHANGED.
fn layout(hwnd: HWND, dpi: u32) {
    unsafe {
        let face = wide("Segoe UI");
        let font = CreateFontW(
            -((9 * dpi as i32 + 36) / 72), 0, 0, 0, FW_NORMAL as i32, 0, 0, 0,
            DEFAULT_CHARSET as u32, OUT_DEFAULT_PRECIS as u32, CLIP_DEFAULT_PRECIS as u32,
            CLEARTYPE_QUALITY as u32, DEFAULT_PITCH as u32, face.as_ptr(),
        );
        for c in &CONTROLS {
            let h = item(hwnd, c.id);
            MoveWindow(h, scale(c.x, dpi), scale(c.y, dpi), scale(c.w, dpi), scale(c.h, dpi), 1);
            SendMessageW(h, WM_SETFONT, font as usize, 1);
        }
        let old = app(|a| std::mem::replace(&mut a.font, font));
        if !old.is_null() {
            DeleteObject(old);
        }
    }
}

// Sets every enabled flag and text from the current mode, per the [D§4] table.
fn apply_state(hwnd: HWND) {
    use Mode::*;
    let (mode, spot, hotkey, clicks) = app(|a| (a.mode, a.spot, a.hotkey, a.clicks.load(Ordering::Relaxed)));
    let (title, status) = match mode {
        Idle if hotkey.is_none() => ("AutoClicker", "Set a hotkey to begin".to_string()),
        Idle => ("AutoClicker", format!("Stopped · {clicks} clicks")),
        Running => ("AutoClicker – running", format!("Running · {clicks} clicks")),
        Picking => ("AutoClicker – pick a spot", "Click anywhere · Esc cancels".to_string()),
        Capturing => ("AutoClicker – set hotkey", "Press a combo · Esc cancels".to_string()),
    };
    set_text(hwnd, title);
    set_text(item(hwnd, ID_STATUS), &status);
    set_text(item(hwnd, ID_START), if mode == Running { "Stop" } else { "Start" });
    enable(hwnd, ID_START, matches!(mode, Idle | Running));
    unsafe { InvalidateRect(item(hwnd, ID_START), null(), 1) }; // owner-drawn: colour follows mode
    for id in [ID_MODE, ID_FIXED_MS, ID_MIN_MS, ID_MAX_MS, ID_LOC, ID_PICK] {
        enable(hwnd, id, mode == Idle);
    }
    // Only the row matching each dropdown's choice is visible [D§3].
    let fixed = combo(hwnd, ID_MODE) == 0;
    let at_spot = combo(hwnd, ID_LOC) == 1;
    for id in [ID_FIXED_MS, ID_FIXED_UNIT] { show(hwnd, id, fixed); }
    for id in [ID_MIN_MS, ID_DASH, ID_MAX_MS, ID_RANDOM_UNIT] { show(hwnd, id, !fixed); }
    show(hwnd, ID_CURSOR_HINT, !at_spot);
    for id in [ID_PICK, ID_SPOT_LBL] { show(hwnd, id, at_spot); }
    enable(hwnd, ID_CURSOR_HINT, false); // disabled static = grey hint text
    let hk = match (mode, hotkey) {
        (Capturing, _) => "Press a key…".to_string(),
        (_, Some((m, vk))) => hotkey_name(m, vk),
        (_, None) => "(none – click to set)".to_string(),
    };
    set_text(item(hwnd, ID_HOTKEY), &hk);
    set_text(item(hwnd, ID_CHANGE), if mode == Capturing { "Cancel" } else { "&Change" });
    enable(hwnd, ID_CHANGE, matches!(mode, Idle | Capturing));
    let spot_text = spot.map_or("not set".to_string(), |p| format!("{}, {}", p.x, p.y));
    set_text(item(hwnd, ID_SPOT_LBL), &spot_text);
    enable(hwnd, ID_SPOT_LBL, spot.is_some()); // disabled static = grey "not set"
}

// Big Start/Stop button [D§3]: accent fill when idle, red while running, grey when disabled.
fn draw_start(d: &DRAWITEMSTRUCT) {
    let running = app(|a| a.mode) == Mode::Running;
    let font = app(|a| a.font);
    unsafe {
        let (dc, r) = (d.hDC, d.rcItem);
        FillRect(dc, &r, GetSysColorBrush(COLOR_BTNFACE));
        let disabled = d.itemState & ODS_DISABLED != 0;
        let base = if running { 0x002B_2BC4 } else { GetSysColor(COLOR_HIGHLIGHT) }; // 0x00BBGGRR
        let fill = if disabled {
            0x00D6_D6D6
        } else if d.itemState & ODS_SELECTED != 0 {
            darken(base)
        } else {
            base
        };
        let (brush, pen) = (CreateSolidBrush(fill), CreatePen(PS_SOLID, 1, fill));
        let (old_brush, old_pen) = (SelectObject(dc, brush), SelectObject(dc, pen));
        let radius = (r.bottom - r.top) / 4;
        RoundRect(dc, r.left, r.top, r.right, r.bottom, radius * 2, radius * 2);
        SelectObject(dc, old_brush);
        SelectObject(dc, old_pen);
        DeleteObject(brush);
        DeleteObject(pen);

        let mut buf = [0u16; 16];
        let n = GetWindowTextW(d.hwndItem, buf.as_mut_ptr(), buf.len() as i32);
        let old_font = SelectObject(dc, font);
        SetBkMode(dc, TRANSPARENT as i32);
        SetTextColor(dc, if disabled { GetSysColor(COLOR_GRAYTEXT) } else { 0x00FF_FFFF });
        let mut tr = r;
        DrawTextW(dc, buf.as_ptr(), n, &mut tr, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
        SelectObject(dc, old_font);
        if d.itemState & ODS_FOCUS != 0 && d.itemState & ODS_NOFOCUSRECT == 0 {
            let inset = RECT { left: r.left + 3, top: r.top + 3, right: r.right - 3, bottom: r.bottom - 3 };
            SetTextColor(dc, 0x00FF_FFFF);
            DrawFocusRect(dc, &inset);
        }
    }
}

fn darken(c: u32) -> u32 {
    let ch = |shift: u32| (((c >> shift) & 0xFF) * 4 / 5) << shift;
    ch(0) | ch(8) | ch(16)
}

fn main() {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let hinst = GetModuleHandleW(null());

        let class = wide("AutoClickerMain");
        let mut wc: WNDCLASSEXW = zeroed();
        wc.cbSize = size_of::<WNDCLASSEXW>() as u32;
        wc.lpfnWndProc = Some(main_proc);
        wc.hInstance = hinst;
        wc.hCursor = LoadCursorW(null_mut(), IDC_ARROW);
        wc.hbrBackground = (COLOR_BTNFACE + 1) as usize as HBRUSH;
        wc.lpszClassName = class.as_ptr();
        RegisterClassExW(&wc);

        let pick_class = wide("AutoClickerPick");
        wc.lpfnWndProc = Some(overlay_proc);
        wc.hCursor = LoadCursorW(null_mut(), IDC_CROSS);
        wc.hbrBackground = GetStockObject(BLACK_BRUSH) as HBRUSH;
        wc.lpszClassName = pick_class.as_ptr();
        RegisterClassExW(&wc);

        let title = wide("AutoClicker");
        let hwnd = CreateWindowExW(
            0, class.as_ptr(), title.as_ptr(), MAIN_STYLE,
            CW_USEDEFAULT, CW_USEDEFAULT, CLIENT_W, CLIENT_H,
            null_mut(), null_mut(), hinst, null(),
        );

        let dpi = GetDpiForWindow(hwnd);
        let (w, h) = window_size(dpi);
        SetWindowPos(hwnd, null_mut(), 0, 0, w, h, SWP_NOMOVE | SWP_NOZORDER);
        create_controls(hwnd);
        layout(hwnd, dpi);
        if RegisterHotKey(hwnd, HOTKEY_ID, MOD_NOREPEAT, VK_F6 as u32) == 0 {
            app(|a| a.hotkey = None); // F6 taken by another app [D§5]
        }
        apply_state(hwnd);
        ShowWindow(hwnd, SW_SHOW);
        SetFocus(item(hwnd, ID_START));

        let mut msg: MSG = zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            // While capturing a hotkey, keys go to the capture logic, never to the controls.
            if matches!(msg.message, WM_KEYDOWN | WM_SYSKEYDOWN | WM_KEYUP | WM_SYSKEYUP | WM_CHAR | WM_SYSCHAR)
                && app(|a| a.mode) == Mode::Capturing
            {
                if matches!(msg.message, WM_KEYDOWN | WM_SYSKEYDOWN) {
                    on_capture_key(hwnd, msg.wParam as u32);
                }
                continue;
            }
            if IsDialogMessageW(hwnd, &msg) == 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        DeleteObject(app(|a| a.font));
    }
}

extern "system" fn main_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_COMMAND => {
                let id = (wp & 0xFFFF) as i32;
                let code = ((wp >> 16) & 0xFFFF) as u32;
                match (id, code) {
                    (ID_MODE | ID_LOC, CBN_SELCHANGE) => apply_state(hwnd),
                    (ID_START, BN_CLICKED) => toggle(hwnd),
                    (ID_CHANGE, BN_CLICKED) if app(|a| a.mode) == Mode::Capturing => finish_capture(hwnd, None),
                    (ID_CHANGE, BN_CLICKED) => start_capture(hwnd),
                    (ID_PICK, BN_CLICKED) => start_pick(hwnd),
                    _ => {}
                }
                0
            }
            WM_APP_PICKED => {
                if app(|a| a.mode) == Mode::Picking {
                    if wp == 1 {
                        app(|a| a.spot = Some(unpack_point(lp)));
                        SendMessageW(item(hwnd, ID_LOC), CB_SETCURSEL, 1, 0);
                    }
                    app(|a| a.mode = Mode::Idle);
                    apply_state(hwnd);
                    SetFocus(item(hwnd, ID_PICK));
                }
                0
            }
            WM_HOTKEY => {
                toggle(hwnd);
                0
            }
            WM_DRAWITEM if wp == ID_START as usize => {
                draw_start(&*(lp as *const DRAWITEMSTRUCT));
                1
            }
            // Moved to a monitor with different scaling: resize and re-lay out for the new DPI.
            WM_DPICHANGED => {
                let dpi = (wp & 0xFFFF) as u32;
                let suggested = &*(lp as *const RECT);
                let (w, h) = window_size(dpi);
                SetWindowPos(hwnd, null_mut(), suggested.left, suggested.top, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
                layout(hwnd, dpi);
                0
            }
            // Switching away cancels capture (but not our own message box popping up).
            WM_ACTIVATE if (wp & 0xFFFF) as u32 == WA_INACTIVE
                && app(|a| a.mode == Mode::Capturing && !a.msgbox_open) =>
            {
                finish_capture(hwnd, None);
                DefWindowProcW(hwnd, msg, wp, lp)
            }
            WM_TIMER if wp == TIMER_STATUS => {
                let n = app(|a| a.clicks.load(Ordering::Relaxed));
                set_text(item(hwnd, ID_STATUS), &format!("Running · {n} clicks"));
                0
            }
            WM_DESTROY => {
                if app(|a| a.mode) == Mode::Running {
                    stop(hwnd);
                }
                UnregisterHotKey(hwnd, HOTKEY_ID);
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_interval_is_exact() {
        for r in [0, 1, 12345, u64::MAX] {
            assert_eq!(next_delay(200, 200, r), 200);
        }
    }

    #[test]
    fn random_interval_stays_in_range_and_hits_both_ends() {
        let (mut lo, mut hi) = (false, false);
        for _ in 0..10_000 {
            let d = next_delay(3, 7, rand_u64());
            assert!((3..=7).contains(&d));
            lo |= d == 3;
            hi |= d == 7;
        }
        assert!(lo && hi);
    }

    #[test]
    fn hotkey_rules() {
        let a = b'A' as u32;
        assert!(hotkey_allowed(0, VK_F6 as u32));
        assert!(hotkey_allowed(0, VK_F24 as u32));
        assert!(!hotkey_allowed(0, a));
        assert!(!hotkey_allowed(0, 0x20)); // space
        assert!(hotkey_allowed(MOD_CONTROL, a));
        assert!(hotkey_allowed(MOD_ALT, b'1' as u32));
        assert!(!hotkey_allowed(MOD_SHIFT, a)); // would block typing capital A
        assert!(hotkey_allowed(MOD_CONTROL | MOD_SHIFT, a));
        assert!(hotkey_allowed(MOD_SHIFT, VK_F6 as u32));
    }

    #[test]
    fn modifier_keys() {
        for vk in [VK_SHIFT, VK_CONTROL, VK_MENU, VK_LSHIFT, VK_RCONTROL, VK_LMENU, VK_LWIN, VK_RWIN] {
            assert!(is_modifier_vk(vk as u32));
        }
        assert!(!is_modifier_vk(VK_F6 as u32));
        assert!(!is_modifier_vk(b'A' as u32));
    }

    #[test]
    fn point_packing_round_trips_negative_coords() {
        for (x, y) in [(0, 0), (1234, 567), (-1920, -40), (i32::MAX, i32::MIN)] {
            let p = unpack_point(pack_point(POINT { x, y }));
            assert_eq!((p.x, p.y), (x, y));
        }
    }

    #[test]
    fn no_overflow_at_limits() {
        assert_eq!(next_delay(1, u32::MAX, u64::MAX), 1 + u64::MAX % u32::MAX as u64);
        assert!(next_delay(u32::MAX, u32::MAX, u64::MAX) == u32::MAX as u64);
    }
}
