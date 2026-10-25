// Thin wrappers over the Win32 calls the UI uses all the time.

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

pub fn scale(v: i32, dpi: u32) -> i32 {
    v * dpi as i32 / 96
}

pub fn item(hwnd: HWND, id: i32) -> HWND {
    unsafe { GetDlgItem(hwnd, id) }
}

pub fn set_text(hwnd: HWND, s: &str) {
    unsafe { SetWindowTextW(hwnd, wide(s).as_ptr()) };
}

pub fn enable(hwnd: HWND, id: i32, on: bool) {
    unsafe { EnableWindow(item(hwnd, id), on as i32) };
}

pub fn show(hwnd: HWND, id: i32, on: bool) {
    unsafe { ShowWindow(item(hwnd, id), if on { SW_SHOW } else { SW_HIDE }) };
}

// Selected index of a dropdown.
pub fn combo(hwnd: HWND, id: i32) -> usize {
    unsafe { SendMessageW(item(hwnd, id), CB_GETCURSEL, 0, 0) as usize }
}
