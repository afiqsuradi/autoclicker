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
mod pick;
mod state;
mod ui;
mod win;

use clicker::{Config, Worker};
use hotkey::{HOTKEY_ID, Hotkey, is_modifier_vk};
use pick::{WM_APP_PICKED, on_picked, overlay_proc, start_pick};
use state::{Mode, app};
use ui::{
    CLIENT_H, CLIENT_W, ID_CHANGE, ID_FIXED_MS, ID_LOC, ID_MAX_MS, ID_MIN_MS, ID_MODE, ID_PICK,
    ID_START, ID_STATUS, MAIN_STYLE, MSG_BAREKEY, MSG_FIXED, MSG_MINMAX, MSG_NOSPOT, MSG_RANGE,
    apply_state, create_controls, draw_start, layout, show_msg, window_size,
};
use win::{combo, item, set_text, wide};

const TIMER_STATUS: usize = 1;

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

        let pick_class = wide(pick::CLASS);
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
                on_picked(hwnd, wp == 1, lp);
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
