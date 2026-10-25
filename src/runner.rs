// Start/stop: reads the settings from the window and runs or stops the click worker.

use std::sync::atomic::Ordering;

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::clicker::{Config, Worker};
use crate::state::{Mode, app};
use crate::ui::{
    ID_FIXED_MS, ID_LOC, ID_MAX_MS, ID_MIN_MS, ID_MODE, ID_START, MSG_FIXED, MSG_MINMAX, MSG_NOSPOT,
    MSG_RANGE, apply_state, show_msg,
};
use crate::win::{combo, item};

pub const TIMER_STATUS: usize = 1;

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

pub fn stop(hwnd: HWND) {
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

pub fn toggle(hwnd: HWND) {
    let (mode, busy) = app(|a| (a.mode, a.msgbox_open));
    match mode {
        _ if busy => {}
        Mode::Running => stop(hwnd),
        Mode::Idle => start(hwnd),
        Mode::Picking | Mode::Capturing => {}
    }
}
