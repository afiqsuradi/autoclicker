// The click engine: runs on its own thread and never touches UI state.

use std::mem::size_of;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{HWND, POINT};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

/// Everything the click thread needs; copied in so it never touches UI state.
#[derive(Clone, Copy)]
pub struct Config {
    pub min: u32,
    pub max: u32,
    pub spot: Option<POINT>,
    pub main: isize, // our HWND as an integer (HWND is a raw pointer, not Send)
}

fn next_delay(min: u32, max: u32, r: u64) -> u64 {
    min as u64 + r % (max as u64 - min as u64 + 1)
}

// std's RandomState is seeded from the OS and every new instance has different keys:
// plenty for click jitter, no extra crate.
fn rand_u64() -> u64 {
    use std::hash::BuildHasher;
    std::collections::hash_map::RandomState::new().hash_one(())
}

// Sleeps `ms` but checks `stop` at least every 15 ms. std::thread::sleep uses a high-resolution
// timer on Windows 10 1803+, so the total wait stays accurate to ~1 ms. Returns false if stopped.
fn sleep_until_or_stop(ms: u64, stop: &AtomicBool) -> bool {
    let deadline = Instant::now() + Duration::from_millis(ms);
    loop {
        if stop.load(Ordering::Relaxed) {
            return false;
        }
        let now = Instant::now();
        if now >= deadline {
            return true;
        }
        thread::sleep((deadline - now).min(Duration::from_millis(15)));
    }
}

fn send_click() -> bool {
    // With swapped mouse buttons, the "primary" click is the physical right button.
    let swapped = unsafe { GetSystemMetrics(SM_SWAPBUTTON) } != 0;
    let (down, up) = if swapped {
        (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP)
    } else {
        (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP)
    };
    let mut inputs = [INPUT::default(), INPUT::default()];
    for (input, flags) in inputs.iter_mut().zip([down, up]) {
        input.r#type = INPUT_MOUSE;
        input.Anonymous.mi.dwFlags = flags;
    }
    unsafe { SendInput(2, inputs.as_ptr(), size_of::<INPUT>() as i32) == 2 }
}

pub fn worker(cfg: Config, stop: Arc<AtomicBool>, clicks: Arc<AtomicU64>) {
    let main = cfg.main as HWND;
    while sleep_until_or_stop(next_delay(cfg.min, cfg.max, rand_u64()), &stop) {
        unsafe {
            let mut pt = POINT { x: 0, y: 0 };
            match cfg.spot {
                Some(p) => pt = p,
                None => { GetCursorPos(&mut pt); }
            }
            // Never click our own window (it would hit Stop), but do click whatever covers it.
            // WindowFromPoint only sends hit-test messages to windows of the calling thread, and
            // this thread owns none, so it can't deadlock with the UI thread waiting in join().
            if GetAncestor(WindowFromPoint(pt), GA_ROOT) == main {
                continue;
            }
            if let Some(p) = cfg.spot {
                SetCursorPos(p.x, p.y);
            }
        }
        if send_click() {
            clicks.fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_interval_is_exact() {
        for r in [0, 1, 12345, u64::MAX] {
            assert_eq!(next_delay(200, 200, r), 200);
        }
    }

    #[test]
    fn random_interval_stays_in_range_and_hits_both_ends() {
        let (mut lo, mut hi) = (false, false);
        for _ in 0..10_000 {
            let d = next_delay(3, 7, rand_u64());
            assert!((3..=7).contains(&d));
            lo |= d == 3;
            hi |= d == 7;
        }
        assert!(lo && hi);
    }

    #[test]
    fn no_overflow_at_limits() {
        assert_eq!(next_delay(1, u32::MAX, u64::MAX), 1 + u64::MAX % u32::MAX as u64);
        assert!(next_delay(u32::MAX, u32::MAX, u64::MAX) == u32::MAX as u64);
    }
}
