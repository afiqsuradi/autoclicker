# AutoClicker

A tiny Windows autoclicker: one 146 KB exe, about 2 MB of RAM, and 0% CPU when idle.
It's written in Rust on plain Win32, and its only dependency is Microsoft's `windows-sys`.

## Features
- **Interval:** fixed (e.g. every 100 ms) or random between a min and max.
- **Start/stop hotkey:** F6 by default. Click the hotkey button and press any combo to change it.
- **Click location:** at the cursor, or at a fixed spot chosen with **Pick…**. Pick works like an eyedropper: the screen dims, you click where you want, and that click isn't passed to the app underneath.
- It never clicks its own window, and Stop takes effect within about 20 ms.

## Download
Grab `AutoClicker.exe` from [Releases](../../releases) and double-click it. Nothing to install.
The exe is unsigned, so Windows SmartScreen may say "unknown publisher". Click **More info → Run anyway**.

## Build from source
Needs Rust (stable, `x86_64-pc-windows-msvc`) and the MSVC Build Tools.
```
cargo build --release --locked
```
The output is `target\release\autoclicker.exe`. Run `cargo test` for the unit tests.

## Code layout
- `main.rs`: entry point, window classes, message loop and window procedure.
- `ui.rs`: the control table, DPI layout, `apply_state` and message boxes.
- `state.rs`: the app state (`Mode`, `App`) behind `app()`.
- `runner.rs`: reads the settings, starts and stops clicking.
- `clicker.rs`: the click thread (`Worker`) and timing.
- `hotkey.rs` / `capture.rs`: hotkey rules, and the Change flow that captures a new one.
- `pick.rs`: the Pick overlay.
- `win.rs`: small Win32 helpers.

## Notes
- Windows blocks clicks into apps running as administrator unless AutoClicker also runs as admin.
- Online games with anti-cheat may ban autoclickers. Use at your own risk.
- The UI spec is in [DESIGN.md](DESIGN.md).
- `examples/clicktest.rs` is a dev-only, full-screen test window that logs the clicks it receives.
