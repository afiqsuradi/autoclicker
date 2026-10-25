# AutoClicker: Design plan

The implementation refers to these sections as **[D§n]**.

## D§1 Design principles
- **Native and plain.** Standard Windows 11 controls, with the system colors and theme. No custom drawing, so it looks right and uses almost nothing.
- **One small window and one screen.** Every setting is visible at once, with no tabs or menus.
- **Always clear what state it's in.** The Start button text, the status line and the window title all reflect the state.
- **Usable with the keyboard alone.** Tab order goes top to bottom, every label has an Alt+letter shortcut, and Esc cancels pick and capture.

## D§2 Window and type
- **Title:** `AutoClicker`. The title changes per state, see D§4.
- **Style:** caption, close and minimize. No resize, no maximize.
- **Client size:** **320 × 286** at 96 DPI (100%). Everything is scaled by `dpi/96`, so at 150% it is 480 × 429 physical pixels. Moving the window to a monitor with different scaling re-lays it out for that monitor.
- **Position:** Windows default (`CW_USEDEFAULT`).
- **Font:** Segoe UI 9pt on every control.
- **Colors:** system only. The window is `COLOR_BTNFACE` and the text is `COLOR_WINDOWTEXT`. No custom colors.
- **Spacing grid:** 12 px outer margin, 8 px between groups, 26 px between rows inside a group.

## D§3 Layout
All values are in 96-DPI pixels, measured from the client area's top-left. Controls are listed in **creation order**, which is also the Tab order and the radio grouping.

```
 ┌ AutoClicker ─────────────────────────────── _ ✕ ┐
 │ ┌ Interval ──────────────────────────────────┐  │
 │ │ (•) Fixed    [  100 ] ms                   │  │
 │ │ ( ) Random   [   80 ] to [  150 ] ms       │  │
 │ └────────────────────────────────────────────┘  │
 │ ┌ Click location ────────────────────────────┐  │
 │ │ (•) At cursor                              │  │
 │ │ ( ) Fixed spot  [ Pick… ]   not set        │  │
 │ └────────────────────────────────────────────┘  │
 │ ┌ Hotkey ────────────────────────────────────┐  │
 │ │ Start / stop   [          F6           ]   │  │
 │ └────────────────────────────────────────────┘  │
 │ [      Start      ]    Stopped · 0 clicks       │
 └─────────────────────────────────────────────────┘
```

| # | Control | Text | x | y | w | h | Styles |
|---|---|---|---|---|---|---|---|
| 1 | Group box | `Interval` | 12 | 8 | 296 | 80 | WS_GROUP |
| 2 | Radio | `&Fixed` | 24 | 30 | 76 | 20 | WS_GROUP, WS_TABSTOP, checked |
| 3 | Radio | `&Random` | 24 | 56 | 76 | 20 | |
| 4 | Edit (fixed ms) | `100` | 104 | 28 | 64 | 23 | WS_GROUP, WS_TABSTOP, ES_NUMBER, right-aligned |
| 5 | Edit (min ms) | `80` | 104 | 54 | 64 | 23 | WS_TABSTOP, ES_NUMBER, right-aligned |
| 6 | Edit (max ms) | `150` | 196 | 54 | 64 | 23 | WS_TABSTOP, ES_NUMBER, right-aligned |
| 7 | Label | `ms` | 174 | 31 | 24 | 20 | |
| 8 | Label | `to` | 174 | 57 | 18 | 20 | |
| 9 | Label | `ms` | 266 | 57 | 24 | 20 | |
| 10 | Group box | `Click location` | 12 | 96 | 296 | 80 | WS_GROUP |
| 11 | Radio | `At &cursor` | 24 | 118 | 120 | 20 | WS_GROUP, WS_TABSTOP, checked |
| 12 | Radio | `Fixed &spot` | 24 | 144 | 84 | 20 | |
| 13 | Button | `&Pick…` | 112 | 141 | 72 | 26 | WS_GROUP, WS_TABSTOP |
| 14 | Label (spot) | `not set` | 192 | 146 | 108 | 20 | grey until set |
| 15 | Group box | `Hotkey` | 12 | 184 | 296 | 54 | WS_GROUP |
| 16 | Label | `Start / stop` | 24 | 207 | 76 | 20 | |
| 17 | Button (hotkey) | `F6` | 104 | 202 | 192 | 26 | WS_GROUP, WS_TABSTOP |
| 18 | Button (start) | `Start` | 12 | 248 | 120 | 30 | WS_TABSTOP |
| 19 | Label (status) | `Stopped · 0 clicks` | 144 | 255 | 164 | 20 | |

Notes:
- The edits that belong to the radios sit on the same row as their radio. Edit 4 goes with Fixed; edits 5 and 6 go with Random.
- Typing in an interval edit auto-selects its radio, so you don't have to click both.
- Label 14 starts out greyed with "not set". Once a spot is picked it shows `1234, 567` in normal text.

## D§4 States
Every state change calls `apply_state()`, and that function follows this table only.

| | Idle | Running | Picking | Capturing hotkey |
|---|---|---|---|---|
| **Window title** | `AutoClicker` | `AutoClicker – running` | `AutoClicker – pick a spot` | `AutoClicker – set hotkey` |
| **Start button** | `Start`, on | `Stop`, on | `Start`, off | `Start`, off |
| **Status label** | `Stopped · N clicks` | `Running · N clicks` (updated every 250 ms) | `Click anywhere · Esc cancels` | `Press a combo · Esc cancels` |
| **Interval radios and edits** | on | off | off | off |
| **Location radios** | on | off | off | off |
| **Pick button** | on | off | off | off |
| **Hotkey button** | hotkey name, e.g. `Ctrl+F8`, on | hotkey name, off | hotkey name, off | `Press a key…`, on and focused |
| **Global hotkey** | registered | registered | ignored | unregistered (re-registered on finish or cancel) |

- The N clicks count stays visible after stopping, so you can see how many were made.
- Selecting the "Random" radio doesn't disable the Fixed edit and vice versa. The radio decides which values are used, which keeps the logic simple.

## D§5 Messages
All messages are `MessageBoxW` with an OK button and a warning icon, and the title is `AutoClicker`.

| When | Text |
|---|---|
| Fixed interval is empty or 0 | `Enter an interval of at least 1 ms.` |
| Random min or max is empty or 0 | `Enter a minimum and maximum of at least 1 ms.` |
| min > max | `Minimum can't be larger than maximum.` |
| Fixed spot with no pick | `Pick a spot first (click "Pick…").` |
| A key that isn't F1–F24, with no Ctrl or Alt (Shift alone doesn't count) | `Use an F-key, or add Ctrl or Alt. A plain key (or Shift+key) would stop working everywhere else.` |
| Alt+F4 pressed while capturing | No message. Capture is cancelled and the app closes, as Alt+F4 normally does. |
| Hotkey taken by another app or reserved by Windows (e.g. F12) | `"F12" is already taken by another app or by Windows. Try a different one.` |
| F6 unavailable at startup | No message box. The hotkey button shows `(none – click to set)` and the status shows `Set a hotkey to begin`. |

## D§6 Pick overlay (eyedropper)
- A borderless window that covers **all monitors**, black at **alpha 60/255** (about 25% dim). It shows clearly that pick mode is on while you can still see what's underneath.
- **Cursor:** the system crosshair (`IDC_CROSS`).
- **No text on the overlay**, because text would be dimmed too. Instructions appear in the main window's status label and title (D§4).
- **Left-click:** picks the point and the overlay disappears at once. The app underneath doesn't receive the click.
- **Right-click, Esc, or switching away:** cancels.
- **After picking:** "Fixed spot" is selected automatically and label 14 shows the coordinates.

## D§7 Keyboard
- **Tab order** follows the D§3 table: Fixed/Random → interval edits → At cursor/Fixed spot → Pick → hotkey button → Start. Arrow keys move between the radios in a group.
- **Mnemonics** (Alt+letter): **F**ixed, **R**andom, At **c**ursor, Fixed **s**pot, **P**ick.
- **Enter** does nothing special: no surprise starts.
- **Esc** cancels Pick or hotkey capture. Otherwise it does nothing (it doesn't close the window).
