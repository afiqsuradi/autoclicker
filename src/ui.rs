// The main window's controls: the [D§3] table, creating them and laying them out per DPI.

use std::ptr::null;

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::*;
use windows_sys::Win32::UI::HiDpi::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::state::app;
use crate::win::{item, scale, wide};

// ---- Layout [D§2], [D§3] -------------------------------------------------

pub const CLIENT_W: i32 = 300;
pub const CLIENT_H: i32 = 286;
pub const MAIN_STYLE: WINDOW_STYLE = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;

// IDs start at 100: IsDialogMessage sends 1 (IDOK) on Enter and 2 (IDCANCEL) on Esc.
pub const ID_MODE: i32 = 101; // interval dropdown: 0 = Fixed, 1 = Random
pub const ID_FIXED_MS: i32 = 103;
pub const ID_MIN_MS: i32 = 104;
pub const ID_MAX_MS: i32 = 105;
pub const ID_FIXED_UNIT: i32 = 106;
pub const ID_DASH: i32 = 107;
pub const ID_RANDOM_UNIT: i32 = 108;
pub const ID_LOC: i32 = 111; // click-at dropdown: 0 = Cursor, 1 = Fixed spot
pub const ID_CURSOR_HINT: i32 = 112;
pub const ID_PICK: i32 = 113;
pub const ID_SPOT_LBL: i32 = 114;
pub const ID_HOTKEY: i32 = 122; // read-only box showing the hotkey
pub const ID_CHANGE: i32 = 123;
pub const ID_START: i32 = 130;
pub const ID_STATUS: i32 = 131;

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

pub fn create_controls(hwnd: HWND) {
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
pub fn window_size(dpi: u32) -> (i32, i32) {
    let mut rc = RECT { left: 0, top: 0, right: scale(CLIENT_W, dpi), bottom: scale(CLIENT_H, dpi) };
    unsafe { AdjustWindowRectExForDpi(&mut rc, MAIN_STYLE, 0, 0, dpi) };
    (rc.right - rc.left, rc.bottom - rc.top)
}

// Positions every control and sets a font for `dpi`. Called at startup and on WM_DPICHANGED.
pub fn layout(hwnd: HWND, dpi: u32) {
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
