// Pick (eyedropper) [D§6]: a dimmed overlay over every monitor; the next click picks the spot.

use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::state::{Mode, app};
use crate::ui::{ID_LOC, ID_PICK, apply_state};
use crate::win::{item, wide};

pub const CLASS: &str = "AutoClickerPick";
pub const WM_APP_PICKED: u32 = WM_APP + 1; // wParam: 1 = picked, 0 = cancelled; lParam: packed point
const OVERLAY_ALPHA: u8 = 60; // [D§6]

pub fn start_pick(hwnd: HWND) {
    if app(|a| a.mode) != Mode::Idle {
        return;
    }
    app(|a| a.mode = Mode::Picking);
    apply_state(hwnd);
    unsafe {
        // Dimmed, topmost, crosshair window over every monitor; it eats the pick click.
        let class = wide(CLASS);
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

pub extern "system" fn overlay_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
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

// The overlay posted WM_APP_PICKED back to the main window.
pub fn on_picked(hwnd: HWND, picked: bool, lp: LPARAM) {
    if app(|a| a.mode) != Mode::Picking {
        return;
    }
    if picked {
        app(|a| a.spot = Some(unpack_point(lp)));
        unsafe { SendMessageW(item(hwnd, ID_LOC), CB_SETCURSEL, 1, 0) };
    }
    app(|a| a.mode = Mode::Idle);
    apply_state(hwnd);
    unsafe { SetFocus(item(hwnd, ID_PICK)) };
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
