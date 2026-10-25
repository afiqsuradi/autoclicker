#![windows_subsystem = "windows"]

use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use std::sync::atomic::Ordering;

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::*;
use windows_sys::Win32::UI::HiDpi::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

mod clicker;
mod hotkey;
mod state;
mod ui;
mod win;

use clicker::{Config, Worker};
use hotkey::{HOTKEY_ID, Hotkey, is_modifier_vk};
use state::{Mode, app};
use ui::{
    CLIENT_H, CLIENT_W, ID_CHANGE, ID_CURSOR_HINT, ID_DASH, ID_FIXED_MS, ID_FIXED_UNIT, ID_HOTKEY,
    ID_LOC, ID_MAX_MS, ID_MIN_MS, ID_MODE, ID_PICK, ID_RANDOM_UNIT, ID_SPOT_LBL, ID_START, ID_STATUS,
    MAIN_STYLE, create_controls, layout, window_size,
};
use win::{combo, enable, item, set_text, show, wide};

const TIMER_STATUS: usize = 1;
const WM_APP_PICKED: u32 = WM_APP + 1; // wParam: 1 = picked, 0 = cancelled; lParam: packed point
const OVERLAY_ALPHA: u8 = 60; // [D§6]

// Messages [D§5].
const MSG_FIXED: &str = "Enter an interval of at least 1 ms.";
const MSG_RANGE: &str = "Enter a minimum and maximum of at least 1 ms.";
const MSG_MINMAX: &str = "Minimum can't be larger than maximum.";
const MSG_NOSPOT: &str = "Pick a spot first (click \"Pick…\").";
const MSG_BAREKEY: &str =
    "Use an F-key, or add Ctrl or Alt. A plain key (or Shift+key) would stop working everywhere else.";

// ---- Start / stop -------------------------------------------------------------------

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
    let clicks = app(|a| {
        a.clicks.store(0, Ordering::Relaxed);
        a.clicks.clone()
    });
    let worker = Worker::spawn(cfg, clicks);
    app(|a| {
        a.worker = Some(worker);
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
    if let Some(w) = worker {
        w.stop();
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
fn finish_capture(hwnd: HWND, new: Option<Hotkey>) {
    let old = app(|a| a.hotkey);
    let mut err = None;
    let mut hotkey = new.filter(|h| h.register(hwnd));
    if let (Some(h), None) = (new, hotkey) {
        err = Some(format!(
            "\"{}\" is already taken by another app or by Windows. Try a different one.",
            h.name()
        ));
    }
    if hotkey.is_none() {
        hotkey = old.filter(|h| h.register(hwnd));
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
    let new = Hotkey { mods, vk };
    if new.is_allowed() {
        finish_capture(hwnd, Some(new));
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
        (_, Some(h)) => h.name(),
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
        if !Hotkey::F6.register(hwnd) {
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
    fn point_packing_round_trips_negative_coords() {
        for (x, y) in [(0, 0), (1234, 567), (-1920, -40), (i32::MAX, i32::MIN)] {
            let p = unpack_point(pack_point(POINT { x, y }));
            assert_eq!((p.x, p.y), (x, y));
        }
    }
}
