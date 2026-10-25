// Hotkey capture: the Change button waits for the next key combo and registers it.

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::hotkey::{HOTKEY_ID, Hotkey, is_modifier_vk};
use crate::state::{Mode, app};
use crate::ui::{ID_CHANGE, MSG_BAREKEY, apply_state, show_msg};
use crate::win::item;

pub fn start_capture(hwnd: HWND) {
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
pub fn finish_capture(hwnd: HWND, new: Option<Hotkey>) {
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

pub fn on_capture_key(hwnd: HWND, vk: u32) {
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
