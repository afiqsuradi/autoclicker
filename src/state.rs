// App state: lives on the UI thread only, reached through `app()`.

use std::cell::RefCell;
use std::ptr::null_mut;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use windows_sys::Win32::Foundation::POINT;
use windows_sys::Win32::Graphics::Gdi::HFONT;

use crate::clicker::Worker;
use crate::hotkey::Hotkey;

#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    Idle,
    Running,
    Picking,
    Capturing,
}

pub struct App {
    pub mode: Mode,
    pub spot: Option<POINT>,
    pub hotkey: Option<Hotkey>, // None = not registered
    pub clicks: Arc<AtomicU64>,
    pub worker: Option<Worker>,
    pub msgbox_open: bool,
    pub font: HFONT, // current UI font, replaced when the window moves to a monitor with other scaling
}

thread_local! {
    static APP: RefCell<App> = RefCell::new(App {
        mode: Mode::Idle,
        spot: None,
        hotkey: Some(Hotkey::F6),
        clicks: Arc::new(AtomicU64::new(0)),
        worker: None,
        msgbox_open: false,
        font: null_mut(),
    });
}

// Never call anything that can re-enter the window procedure inside `f` (borrow would panic).
pub fn app<R>(f: impl FnOnce(&mut App) -> R) -> R {
    APP.with_borrow_mut(f)
}
