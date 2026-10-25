# AutoClicker: Design plan

The implementation refers to these sections as **[D§n]**.

## D§1 Design principles
- **Native and plain.** Standard Windows 11 controls in the system theme. The one custom-drawn control is the Start/Stop button.
- **Show only what applies.** Each setting is a dropdown, and only the fields for the chosen option are shown below it.
- **Always clear what state it's in.** The Start button's colour and text, the status line and the window title all reflect the state.
- **Usable with the keyboard alone.** Tab order goes top to bottom, every label has an Alt+letter shortcut, and Esc cancels pick and capture.

## D§2 Window and type
- **Title:** `AutoClicker`. The title changes per state, see D§4.
- **Style:** caption, close and minimize. No resize, no maximize.
- **Client size:** **300 × 286** at 96 DPI (100%). Everything is scaled by `dpi/96`. Moving the window to a monitor with different scaling re-lays it out for that monitor.
- **Position:** Windows default (`CW_USEDEFAULT`).
- **Font:** Segoe UI 9pt on every control.
- **Colors:**
  - Everything uses system colors except the Start button.
  - Start is the system accent (`COLOR_HIGHLIGHT`) with white text while idle, red `#C42B2B` while running, and light grey with grey text when disabled. It's 20% darker while pressed.
- **Grid:** 16 px outer margin. Labels go in a left column (x 16, 76 wide) and controls in a right column (x 96, 188 wide).

## D§3 Layout
All values are in 96-DPI pixels, measured from the client area's top-left. Controls are listed in **creation order**, which is also the Tab order. Rows that share a y value are alternatives: only the one matching the dropdown choice is visible.

```
 ┌ AutoClicker ──────────────────────── _ ✕ ┐
 │  Interval    [ Fixed               ▾ ]   │
 │              [   100 ] ms                │   (Random: [ 80 ] – [ 150 ] ms)
 │                                          │
 │  Click at    [ Cursor              ▾ ]   │
 │              Clicks wherever the mouse is │   (Fixed spot: [ Pick… ]  1234, 567)
 │                                          │
 │  Hotkey      [    F6    ]  [ Change ]    │
 │                                          │
 │  [               Start                ]  │
 │             Stopped · 0 clicks           │
 └──────────────────────────────────────────┘
```

| # | Control | Text | x | y | w | h | Shown when |
|---|---|---|---|---|---|---|---|
| 1 | Label | `&Interval` | 16 | 19 | 76 | 20 | always |
| 2 | Dropdown (interval mode) | `Fixed` / `Random` | 96 | 16 | 188 | — | always |
| 3 | Number (fixed ms) | `100` | 96 | 48 | 72 | 23 | Fixed |
| 4 | Label | `ms` | 176 | 51 | 24 | 20 | Fixed |
| 5 | Number (min ms) | `80` | 96 | 48 | 72 | 23 | Random |
| 6 | Label | `–` | 168 | 51 | 20 | 20 | Random |
| 7 | Number (max ms) | `150` | 188 | 48 | 72 | 23 | Random |
| 8 | Label | `ms` | 266 | 51 | 24 | 20 | Random |
| 9 | Label | `Click &at` | 16 | 95 | 76 | 20 | always |
| 10 | Dropdown (location) | `Cursor` / `Fixed spot` | 96 | 92 | 188 | — | always |
| 11 | Hint (grey) | `Clicks wherever the mouse is` | 96 | 127 | 188 | 20 | Cursor |
| 12 | Button | `&Pick…` | 96 | 122 | 80 | 26 | Fixed spot |
| 13 | Label (spot) | `not set` (grey) / `1234, 567` | 184 | 127 | 100 | 20 | Fixed spot |
| 14 | Label | `&Hotkey` | 16 | 171 | 76 | 20 | always |
| 15 | Read-only box | `F6` (centred) | 96 | 168 | 104 | 25 | always |
| 16 | Button | `&Change` | 208 | 167 | 76 | 27 | always |
| 17 | Start/Stop (custom drawn) | `Start` | 16 | 210 | 268 | 36 | always |
| 18 | Status (centred) | `Stopped · 0 clicks` | 16 | 256 | 268 | 18 | always |

Notes:
- The number boxes accept digits only, up to 7 (9,999,999 ms), right-aligned.
- The hotkey box is read-only and not a Tab stop. It only displays the combo; **Change** sets it.

## D§4 States
Every state change calls `apply_state()`, and that function follows this table only.

| | Idle | Running | Picking | Capturing hotkey |
|---|---|---|---|---|
| **Window title** | `AutoClicker` | `AutoClicker – running` | `AutoClicker – pick a spot` | `AutoClicker – set hotkey` |
| **Start button** | `Start`, accent, on | `Stop`, red, on | `Start`, grey, off | `Start`, grey, off |
| **Status label** | `Stopped · N clicks` | `Running · N clicks` (updated every 250 ms) | `Click anywhere · Esc cancels` | `Press a combo · Esc cancels` |
| **Dropdowns, number boxes, Pick** | on | off | off | off |
| **Hotkey box** | hotkey name, e.g. `Ctrl+F8` | hotkey name | hotkey name | `Press a key…` |
| **Change button** | `Change`, on | off | off | `Cancel`, on and focused |
| **Global hotkey** | registered | registered | ignored | unregistered (re-registered on finish or cancel) |

- The N clicks count stays visible after stopping, so you can see how many were made.
- Picking a spot switches the location dropdown to `Fixed spot`.

## D§5 Messages
All messages are `MessageBoxW` with an OK button and a warning icon, and the title is `AutoClicker`.

| When | Text |
|---|---|
| Fixed interval is empty or 0 | `Enter an interval of at least 1 ms.` |
| Random min or max is empty or 0 | `Enter a minimum and maximum of at least 1 ms.` |
| min > max | `Minimum can't be larger than maximum.` |
| Fixed spot with no pick | `Pick a spot first (click "Pick…").` |
| A key that isn't F1–F24, with no Ctrl or Alt (Shift alone doesn't count) | `Use an F-key, or add Ctrl or Alt. A plain key (or Shift+key) would stop working everywhere else.` |
| Hotkey taken by another app or reserved by Windows (e.g. F12) | `"F12" is already taken by another app or by Windows. Try a different one.` |
| Alt+F4 pressed while capturing | No message. Capture is cancelled and the app closes, as Alt+F4 normally does. |
| F6 unavailable at startup | No message box. The hotkey box shows `(none – click to set)` and the status shows `Set a hotkey to begin`. |

## D§6 Pick overlay (eyedropper)
- A borderless window that covers **all monitors**, black at **alpha 60/255** (about 25% dim). It shows clearly that pick mode is on while you can still see what's underneath.
- **Cursor:** the system crosshair (`IDC_CROSS`).
- **No text on the overlay**, because text would be dimmed too. Instructions appear in the main window's status label and title (D§4).
- **Left-click:** picks the point and the overlay disappears at once. The app underneath doesn't receive the click.
- **Right-click, Esc, Alt+F4, or switching away:** cancels.
- **After picking:** the location dropdown switches to `Fixed spot` and label 13 shows the coordinates.

## D§7 Keyboard
- **Tab order** follows the D§3 table and skips hidden controls: Interval dropdown → its number box(es) → Click-at dropdown → Pick (if shown) → Change → Start.
- Up/Down on a focused dropdown changes the choice, and the rows below follow.
- **Mnemonics** (Alt+letter): **I**nterval, Click **a**t, **H**otkey (each focuses the control next to it), **P**ick, **C**hange.
- **Enter** does nothing special: no surprise starts.
- **Esc** cancels Pick or hotkey capture. Otherwise it does nothing (it doesn't close the window).
