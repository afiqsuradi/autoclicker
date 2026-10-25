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

mod capture;
mod clicker;
mod hotkey;
mod pick;
mod runner;
mod state;
mod ui;
mod win;

use capture::{finish_capture, on_capture_key, start_capture};
use hotkey::{HOTKEY_ID, Hotkey};
use pick::{WM_APP_PICKED, on_picked, overlay_proc, start_pick};
use runner::{TIMER_STATUS, stop, toggle};
use state::{Mode, app};
use ui::{
    CLIENT_H, CLIENT_W, ID_CHANGE, ID_LOC, ID_MODE, ID_PICK, ID_START, ID_STATUS, MAIN_STYLE,
    apply_state, create_controls, draw_start, layout, window_size,
};
use win::{item, set_text, wide};

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
