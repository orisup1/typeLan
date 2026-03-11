# typeLan

typeLan is a small background helper that watches what you type, checks each finished word
against language dictionaries, and automatically switches the keyboard layout between
English and Hebrew when it looks like you are typing in the wrong layout.

## How it works

- Listens to global keyboard events using the `rdev` crate.
- Builds up the current word from key presses.
- When you press Space or Enter, it:
  - Looks up the word in both `en_dict.txt` and `he_dict.txt`.
  - If the word is not in the current language's dictionary but is in the other one,
    it triggers a layout switch using a macOS `osascript` command.

## Setup

1. Ensure you are on macOS and have Rust installed (`rustup`, `cargo`).
2. Create or edit `en_dict.txt` and `he_dict.txt` in the project root.
   Each file should contain one word per line, in lowercase.
3. In `src/main.rs`, adjust the `switch_to_en_cmd` and `switch_to_he_cmd` strings
   so they match keyboard shortcuts you have configured in System Settings for
   switching input source to English / Hebrew.

## Run

```bash
cargo run --release
```

The process will start listening for keyboard events. Leave it running in the background.

