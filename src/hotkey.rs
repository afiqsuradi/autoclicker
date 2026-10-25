// Hotkey names and the rules for which combos are allowed.

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;

pub const HOTKEY_ID: i32 = 1;

/// A global hotkey: MOD_* flags plus a virtual key.
#[derive(Clone, Copy)]
pub struct Hotkey {
    pub mods: u32,
    pub vk: u32,
}

impl Hotkey {
    pub const F6: Hotkey = Hotkey { mods: 0, vk: VK_F6 as u32 };

    pub fn name(self) -> String {
        let mut s = String::new();
        if self.mods & MOD_CONTROL != 0 { s += "Ctrl+"; }
        if self.mods & MOD_ALT != 0 { s += "Alt+"; }
        if self.mods & MOD_SHIFT != 0 { s += "Shift+"; }
        s + &key_name(self.vk)
    }

    // A bare letter/digit hotkey would swallow that key in every app, so plain keys must be F1–F24.
    // Shift alone doesn't count: a Shift+A hotkey would stop capital A from typing anywhere.
    pub fn is_allowed(self) -> bool {
        self.mods & (MOD_CONTROL | MOD_ALT) != 0 || (VK_F1 as u32..=VK_F24 as u32).contains(&self.vk)
    }

    /// Registers this as our global hotkey on `hwnd`. False if another app or Windows has it.
    pub fn register(self, hwnd: HWND) -> bool {
        unsafe { RegisterHotKey(hwnd, HOTKEY_ID, self.mods | MOD_NOREPEAT, self.vk) != 0 }
    }
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

pub fn is_modifier_vk(vk: u32) -> bool {
    matches!(
        vk as u16,
        VK_SHIFT | VK_CONTROL | VK_MENU | VK_LSHIFT | VK_RSHIFT | VK_LCONTROL | VK_RCONTROL
            | VK_LMENU | VK_RMENU | VK_LWIN | VK_RWIN
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hotkey_rules() {
        let hotkey_allowed = |mods, vk| Hotkey { mods, vk }.is_allowed();
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
}
