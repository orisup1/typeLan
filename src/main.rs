mod dictionary;
#[cfg(target_os = "linux")]
mod gui;
mod keymap;
mod layout;
mod platform;
mod types;

use std::sync::Arc;

use dictionary::parse_dictionary;
use types::AppControl;

// Dictionaries are baked into the binary at compile time so the executable is
// self-contained — it loads identically no matter what the current working
// directory is when launched.
const EN_DICT_TXT: &str = include_str!("../en_dict.txt");
const HE_DICT_TXT: &str = include_str!("../he_dict.txt");

fn main() {
    let with_gui = std::env::args().skip(1).any(|a| a == "-g" || a == "--gui");

    let en_dict = parse_dictionary(EN_DICT_TXT);
    let he_dict = parse_dictionary(HE_DICT_TXT);

    let control = Arc::new(AppControl::new());

    #[cfg(target_os = "linux")]
    {
        if with_gui {
            // Keyboard listener runs in a background thread; the GUI must own
            // the main thread because eframe drives the windowing event loop.
            let listener_control = Arc::clone(&control);
            std::thread::spawn(move || {
                platform::linux::run(en_dict, he_dict, listener_control);
            });
            if let Err(e) = gui::run(control) {
                eprintln!("GUI error: {}", e);
            }
            return;
        }
        platform::linux::run(en_dict, he_dict, control);
    }

    #[cfg(target_os = "macos")]
    {
        let _ = with_gui; // GUI flag is currently Linux-only.
        let listener_control = Arc::clone(&control);
        std::thread::spawn(move || {
            platform::macos::run(en_dict, he_dict, listener_control);
        });
        // Menubar event loop must own the main thread (NSApp requirement).
        platform::tray::run(control);
    }

    #[cfg(target_os = "windows")]
    {
        let _ = with_gui;
        let listener_control = Arc::clone(&control);
        std::thread::spawn(move || {
            platform::windows::run(en_dict, he_dict, listener_control);
        });
        // Tray event loop must own the main thread (Win32 message pump).
        platform::tray::run(control);
    }
}
