# typeLan

typeLan is a small background helper that watches what you type, checks each finished word
against language dictionaries, and automatically switches the keyboard layout between
English and Hebrew when it looks like you are typing in the wrong layout — then retypes the
mistyped word in the correct layout.

## How it works

- Captures global key events on every supported platform.
- Builds up the current word from key presses; resets the buffer on cursor / focus-shifting
  keys (Tab, Escape, arrows, Home/End, PgUp/PgDn, Insert, Delete) and on mouse clicks
  (macOS / Windows).
- When you press Space or Enter, it interprets the typed key sequence as both an English
  and a Hebrew word and looks each up in the matching dictionary.
- If exactly one dictionary contains the word, it switches the OS keyboard layout to that
  language, erases the mistyped word, and retypes it (followed by the original Space/Enter).

## Supported platforms

| OS      | Capture       | Layout switch                                    |
| ------- | ------------- | ------------------------------------------------ |
| Linux   | `evdev`       | `hyprctl switchxkblayout` (Hyprland only)        |
| macOS   | `rdev`        | Carbon `TISSelectInputSource`                    |
| Windows | `rdev`        | `LoadKeyboardLayoutW` + `WM_INPUTLANGCHANGEREQUEST` |

Linux additionally requires the user to be in the `input` group (for `evdev` read access)
and creates a `uinput` virtual device named `typeLan-injector` to replay corrected words.

## Setup

1. Install Rust (`rustup`, `cargo`).
2. Place `en_dict.txt` and `he_dict.txt` in the working directory you launch from.
   One word per line, lowercase.
3. Make sure both English and Hebrew layouts are installed in your OS keyboard settings.
   On Linux/Hyprland the xkb config must list English as layout 0 and Hebrew as layout 1.

## Run

```bash
cargo run --release
```

Pass `-g` (Linux only) to launch a small control window with an enable/disable
switch and a counter of words fixed since startup:

```bash
cargo run --release -- -g
```

Set `TYPELAN_DEBUG=1` to print every word check and switch decision:

```bash
TYPELAN_DEBUG=1 cargo run --release
```

Leave the process running in the background.
