//! Dev-only test target (not part of the app): a full-screen, topmost window that logs every
//! mouse click it receives, so autoclicker tests can't click anything else on the desktop.
//! Usage: clicktest <log-file>   Lines: "<L|R> <elapsed_ms> <x> <y>" (screen coordinates).

use std::fs::File;
use std::io::Write;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use std::sync::OnceLock;
use std::sync::Mutex;
use std::time::Instant;

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

static LOG: OnceLock<Mutex<(File, Instant, i32, i32)>> = OnceLock::new();

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn log(button: char, lp: LPARAM) {
    let (x, y) = ((lp & 0xFFFF) as i16 as i32, ((lp >> 16) & 0xFFFF) as i16 as i32);
    let mut g = LOG.get().unwrap().lock().unwrap();
    let (ox, oy) = (g.2, g.3);
    let ms = g.1.elapsed().as_secs_f64() * 1000.0;
    let _ = writeln!(g.0, "{button} {ms:.2} {} {}", x + ox, y + oy);
}

extern "system" fn proc_(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_LBUTTONDOWN => { log('L', lp); 0 }
            WM_RBUTTONDOWN => { log('R', lp); 0 }
            WM_PAINT => {
                let mut ps: PAINTSTRUCT = zeroed();
                let dc = BeginPaint(hwnd, &mut ps);
                let mut rc: RECT = zeroed();
                GetClientRect(hwnd, &mut rc);
                SetBkMode(dc, TRANSPARENT as i32);
                SetTextColor(dc, 0x00FFFFFF);
                let mut t = wide("AutoClicker test target - logging clicks, closes by itself");
                DrawTextW(dc, t.as_mut_ptr(), -1, &mut rc, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
                EndPaint(hwnd, &ps);
                0
            }
            WM_DESTROY => { PostQuitMessage(0); 0 }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

fn main() {
    let path = std::env::args().nth(1).expect("usage: clicktest <log-file>");
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let (x, y) = (GetSystemMetrics(SM_XVIRTUALSCREEN), GetSystemMetrics(SM_YVIRTUALSCREEN));
        let (w, h) = (GetSystemMetrics(SM_CXVIRTUALSCREEN), GetSystemMetrics(SM_CYVIRTUALSCREEN));
        LOG.set(Mutex::new((File::create(path).unwrap(), Instant::now(), x, y))).ok();

        let hinst = GetModuleHandleW(null());
        let class = wide("AutoClickerTest");
        let mut wc: WNDCLASSEXW = zeroed();
        wc.cbSize = size_of::<WNDCLASSEXW>() as u32;
        wc.lpfnWndProc = Some(proc_);
        wc.hInstance = hinst;
        wc.hCursor = LoadCursorW(null_mut(), IDC_ARROW);
        wc.hbrBackground = GetStockObject(DKGRAY_BRUSH) as HBRUSH;
        wc.lpszClassName = class.as_ptr();
        RegisterClassExW(&wc);
        CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW, class.as_ptr(), class.as_ptr(), WS_POPUP | WS_VISIBLE,
            x, y, w, h, null_mut(), null_mut(), hinst, null(),
        );
        let mut msg: MSG = zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
